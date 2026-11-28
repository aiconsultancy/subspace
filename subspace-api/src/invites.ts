import { and, desc, eq, gt, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApi, HttpApiEndpoint, HttpApiGroup, HttpApiSchema, OpenApi } from "effect/http-api";
import { lockOrganisation } from "./organisation-lock";
import { recordAudit } from "./audit";
import { Database } from "./db";
import {
  adminOnly,
  type Caller,
  confirmCaller,
  type ConfirmedCaller,
  findPerson,
  managedPerson,
  memberNotFound,
} from "./people";
import { CurrentMember, issuerAuthorityOf } from "./identity";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import {
  defaultInviteLifetime,
  insertPairedPerson,
  inviteCreated,
  inviteReplaced,
  issueInvite,
  issuerWithdrawal,
  maxInviteLifetime,
  minInviteLifetime,
  pairingInvitePrefix,
  signsIn,
} from "./pairing-invites";
import {
  endpointErrors,
  memberSignsIn,
  pairingUnavailable,
  problem,
  problemTypes,
} from "./problems";
import { agentCredentials, pairingInvites, users } from "./schema";
import { dateTime, dateTimeOrNull, Email, Line, Timestamp, Uuid } from "./validation";

/**
 * Pairing invites issued through the API. An administrator invites a new person, or a person who
 * has no other way to sign in; anyone who signs in pairs their own further devices. Each invite
 * records who issued it and, when a device signed the request, which device: revoking that device
 * revokes what it issued (see `devices.ts`). Invites are issued, here and by the owner bootstrap
 * command on the API host, with `pairing-invites.ts`.
 */

type Db = (typeof Database)["Service"];

export interface InvitePolicy {
  /** The device token audience, or null when pairing is off and no invite can be issued. */
  readonly audience: string | null;
  /** The operator's reserved usernames, in lower case, for a member created with an invite. */
  readonly reservedUsernames: ReadonlySet<string>;
  /** The Entra app role that makes a person an administrator, or null (ENTRA_ADMIN_ROLE). */
  readonly entraAdminRole: string | null;
}

const Lifetime = Schema.Int.check(
  Schema.isBetween({ minimum: minInviteLifetime, maximum: maxInviteLifetime }),
);

const NewPairingInvite = Schema.Union([
  Schema.Struct({ memberId: Uuid, expiresInSeconds: Schema.optional(Lifetime) }),
  Schema.Struct({
    displayName: Line(120),
    email: Schema.optional(Schema.NullOr(Email)),
    role: Schema.optional(Schema.Literals(["admin", "member"])),
    expiresInSeconds: Schema.optional(Lifetime),
  }),
]).annotate({ identifier: "NewPairingInvite" });

const NewOwnPairingInvite = Schema.Struct({
  expiresInSeconds: Schema.optional(Lifetime),
}).annotate({ identifier: "NewOwnPairingInvite" });

const PairingInviteIssued = Schema.Struct({
  inviteId: Uuid,
  memberId: Uuid,
  memberCreated: Schema.Boolean,
  invite: Schema.String,
  expiresAt: Timestamp,
}).annotate({ identifier: "PairingInviteIssued" });

/** An invite's record, never its text, which only the answer that issued it shows. */
const PairingInvite = Schema.Struct({
  id: Uuid,
  memberId: Uuid,
  createdAt: Timestamp,
  expiresAt: Timestamp,
  redeemedAt: Schema.NullOr(Timestamp),
  revokedAt: Schema.NullOr(Timestamp),
  /** The member who issued it; null for the owner bootstrap command on the API host. */
  createdByUserId: Schema.NullOr(Uuid),
  /** The device that signed the request issuing it, if one did. */
  createdByDeviceId: Schema.NullOr(Uuid),
}).annotate({ identifier: "PairingInvite" });

const inviteDto = (row: typeof pairingInvites.$inferSelect) => ({
  id: row.id,
  memberId: row.userId,
  createdAt: dateTime(row.createdAt),
  expiresAt: dateTime(row.expiresAt),
  redeemedAt: dateTimeOrNull(row.redeemedAt),
  revokedAt: dateTimeOrNull(row.revokedAt),
  createdByUserId: row.createdById,
  createdByDeviceId: row.createdByCredentialId,
});

/** Whether a stored invite still pairs a device, by the database's clock. */
const pending = and(
  isNull(pairingInvites.redeemedAt),
  isNull(pairingInvites.revokedAt),
  gt(pairingInvites.expiresAt, sql`clock_timestamp()`),
);

/**
 * The person an administrator's invite pairs a device for: a new person, who signs in only with
 * paired devices until they link a provider account, or an active person who has no other way to
 * sign in, such as one whose only device was lost. An invite never pairs a device for a person who
 * signs in, which would let the administrator act as them.
 */
const inviteeOf = (
  tx: Db,
  caller: Caller,
  payload: typeof NewPairingInvite.Type,
  reserved: ReadonlySet<string>,
) =>
  Effect.gen(function* () {
    if ("memberId" in payload) {
      const person = yield* findPerson(tx, caller, payload.memberId);
      if (!person) return yield* Effect.fail(memberNotFound());
      if (!person.active)
        return yield* Effect.fail(
          problem(409, "This member is deactivated. Reactivate them before pairing a device."),
        );
      if (yield* signsIn(tx, person)) return yield* Effect.fail(memberSignsIn());
      return { member: person, created: false };
    }
    const { member, registered } = yield* insertPairedPerson(tx, {
      organisationId: caller.organisationId,
      displayName: payload.displayName,
      email: payload.email ?? null,
      role: payload.role ?? "member",
      reserved,
      summary: "Registered a member to pair their device.",
    });
    yield* recordAudit(tx, caller, registered);
    return { member, created: true };
  });

/**
 * Issues and audits an invite for `memberId` in the caller's transaction, revoking the person's
 * earlier pending invites. The invite is in the answer only: never stored, audited or logged.
 */
const issueAudited = (
  tx: Db,
  caller: ConfirmedCaller,
  member: { id: string; created: boolean },
  lifetime: number | undefined,
) =>
  Effect.gen(function* () {
    const { invite, token, revokedInvites } = yield* issueInvite(tx, {
      organisationId: caller.organisationId,
      userId: member.id,
      lifetime: lifetime ?? defaultInviteLifetime,
      createdById: caller.id,
      createdByCredentialId: caller.keyCredential?.id ?? null,
      // Another person's invite needs an administrator: record where that role came from, which
      // redemption checks again.
      issuerAuthority:
        member.id === caller.id
          ? null
          : issuerAuthorityOf(caller.roleSource, caller.provisionedRole),
    });
    for (const row of revokedInvites)
      yield* recordAudit(tx, caller, inviteReplaced(row.id, member.id));
    yield* recordAudit(
      tx,
      caller,
      inviteCreated(
        invite,
        member.id,
        member.id === caller.id
          ? "Issued a pairing invite for another of their devices."
          : "Issued a pairing invite.",
      ),
    );
    return {
      inviteId: invite.id,
      memberId: member.id,
      memberCreated: member.created,
      invite: token,
      expiresAt: dateTime(invite.expiresAt),
    };
  });

/**
 * An administrator issues a pairing invite (see `inviteeOf`), in one transaction under the
 * organisation's lock, in which their role and the device that signed the request are checked
 * again.
 */
const issuePairingInvite = (
  caller: Caller,
  payload: typeof NewPairingInvite.Type,
  policy: InvitePolicy,
) =>
  Effect.gen(function* () {
    yield* adminOnly(caller);
    if (!policy.audience) return yield* Effect.fail(pairingUnavailable());
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, caller.organisationId);
        const confirmed = yield* confirmCaller(tx, caller);
        yield* adminOnly(confirmed);
        const invitee = yield* inviteeOf(tx, confirmed, payload, policy.reservedUsernames);
        return yield* issueAudited(
          tx,
          confirmed,
          { id: invitee.member.id, created: invitee.created },
          payload.expiresInSeconds,
        );
      }),
    );
  });

/**
 * A person who signs in, with a provider account or a paired device, issues an invite to pair
 * another of their own devices.
 */
const issueOwnPairingInvite = (
  caller: Caller,
  payload: typeof NewOwnPairingInvite.Type,
  policy: InvitePolicy,
) =>
  Effect.gen(function* () {
    if (!policy.audience) return yield* Effect.fail(pairingUnavailable());
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, caller.organisationId);
        const confirmed = yield* confirmCaller(tx, caller);
        const self = yield* managedPerson(tx, confirmed, null);
        return yield* issueAudited(
          tx,
          confirmed,
          { id: self, created: false },
          payload.expiresInSeconds,
        );
      }),
    );
  });

/**
 * An administrator lists the organisation's pending invites, newest first: those that would pair
 * a device now. Stored invites whose issuer no longer vouches for them, which redemption refuses,
 * are left out by the same rule (`issuerWithdrawal`).
 */
const listPendingInvites = (caller: Caller, policy: InvitePolicy) =>
  Effect.gen(function* () {
    yield* adminOnly(caller);
    const db = yield* Database;
    const rows = yield* db
      .select({
        invite: pairingInvites,
        device: { id: agentCredentials.id, revokedAt: agentCredentials.revokedAt },
        issuer: {
          id: users.id,
          active: users.active,
          agentRank: users.agentRank,
          role: users.role,
          tokenRole: users.tokenRole,
        },
      })
      .from(pairingInvites)
      .leftJoin(
        agentCredentials,
        and(
          eq(agentCredentials.organisationId, pairingInvites.organisationId),
          eq(agentCredentials.id, pairingInvites.createdByCredentialId),
        ),
      )
      .leftJoin(
        users,
        and(
          eq(users.organisationId, pairingInvites.organisationId),
          eq(users.id, pairingInvites.createdById),
        ),
      )
      .where(and(eq(pairingInvites.organisationId, caller.organisationId), pending))
      .orderBy(desc(pairingInvites.createdAt), desc(pairingInvites.id));
    return {
      items: rows
        .filter(
          (row) => !issuerWithdrawal(row.invite, row.device, row.issuer, policy.entraAdminRole),
        )
        .map((row) => inviteDto(row.invite)),
    };
  });

/**
 * An administrator revokes an invite. Revoking an invite that is spent, expired or revoked
 * returns it unchanged.
 */
const revokeInvite = (caller: Caller, inviteId: string) =>
  Effect.gen(function* () {
    yield* adminOnly(caller);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, caller.organisationId);
        const confirmed = yield* confirmCaller(tx, caller);
        yield* adminOnly(confirmed);
        const where = and(
          eq(pairingInvites.organisationId, caller.organisationId),
          eq(pairingInvites.id, inviteId),
        );
        const [revoked] = yield* tx
          .update(pairingInvites)
          .set({ revokedAt: new Date() })
          .where(and(where, pending))
          .returning();
        if (revoked) {
          yield* recordAudit(tx, confirmed, {
            action: "pairingInviteRevoked",
            entityType: "PairingInvite",
            entityId: revoked.id,
            summary: "Revoked a pending pairing invite.",
            changes: { memberId: revoked.userId },
          });
          return inviteDto(revoked);
        }
        const [row] = yield* tx.select().from(pairingInvites).where(where);
        if (!row)
          return yield* Effect.fail(
            problem(404, "Pairing invite was not found. Use an ID from the pending invites."),
          );
        return inviteDto(row);
      }),
    );
  });

const issued = PairingInviteIssued.pipe(HttpApiSchema.status(201));
const untyped = "Its problems are about:blank.";
const lifetimes = `The invite begins ${pairingInvitePrefix} and expires after expiresInSeconds, ${minInviteLifetime} to ${maxInviteLifetime} (default ${defaultInviteLifetime}); the person's earlier pending invites are revoked. It is shown in this answer only, and is revoked with the device that signed the request, if one did.`;

/** Pairing invites issued through the API, on the main listener only. */
export const inviteGroup = HttpApiGroup.make("invites")
  .add(
    HttpApiEndpoint.post("issuePairingInvite", "/pairing/invites", {
      payload: NewPairingInvite,
      success: issued,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator issues a single-use pairing invite for a new person named by displayName (with an optional email and role, member by default), who is created and signs in only with paired devices until they link a provider account, or for an active person named by memberId who has no other way to sign in: no provider account and no device that was never revoked (201). ${lifetimes} 404 when this API does not pair devices or the member is not a person of the organisation; 409 for a deactivated member, and for a person who signs in, who pairs another device from /me/pairing/invites. Unlike redemption's, only two of these problems carry a type: ${problemTypes.pairingUnavailable} (404) when this API does not pair devices, and ${problemTypes.memberSignsIn} (409) for a person who signs in. The others, including 400 for a malformed body, are about:blank.`,
    ),
    HttpApiEndpoint.post("issueOwnPairingInvite", "/me/pairing/invites", {
      payload: NewOwnPairingInvite,
      success: issued,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `A person who signs in, with a provider account or a paired device, issues a single-use invite that pairs another of their own devices (201). ${lifetimes} 403 for an agent; 404 when this API does not pair devices, with type ${problemTypes.pairingUnavailable}. The other problems, including 400 for a malformed body, are about:blank.`,
    ),
    HttpApiEndpoint.get("listPairingInvites", "/pairing/invites", {
      success: Schema.Struct({ items: Schema.Array(PairingInvite) }),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator lists the organisation's pending pairing invites, newest first: those neither redeemed, revoked nor expired, and whose issuer still vouches for them, as redemption checks: the device that issued one is unrevoked and, for another person's invite, its issuer is still an active administrator from the source their role came from when they issued it. ${untyped}`,
    ),
    HttpApiEndpoint.delete("revokePairingInvite", "/pairing/invites/:inviteId", {
      params: { inviteId: Uuid },
      success: PairingInvite,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator revokes a pairing invite, which then pairs nothing. An invite that is already redeemed, revoked or expired is returned unchanged. ${untyped}`,
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const inviteHandlers =
  (policy: InvitePolicy) =>
  (handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof inviteGroup>) =>
    handlers
      .handle("issuePairingInvite", ({ payload }) =>
        handleDb(
          Effect.flatMap(CurrentMember, (caller) => issuePairingInvite(caller, payload, policy)),
        ),
      )
      .handle("issueOwnPairingInvite", ({ payload }) =>
        handleDb(
          Effect.flatMap(CurrentMember, (caller) => issueOwnPairingInvite(caller, payload, policy)),
        ),
      )
      .handle("listPairingInvites", () =>
        handleDb(Effect.flatMap(CurrentMember, (caller) => listPendingInvites(caller, policy))),
      )
      .handle("revokePairingInvite", ({ params }) =>
        handleDb(Effect.flatMap(CurrentMember, (caller) => revokeInvite(caller, params.inviteId))),
      );
