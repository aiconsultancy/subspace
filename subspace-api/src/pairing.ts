import { randomUUID } from "node:crypto";
import { and, asc, eq, gt, inArray, isNull, sql } from "drizzle-orm";
import { Effect, Layer, Schema } from "effect";
import { HttpServerRequest, HttpServerResponse } from "effect/http";
import {
  HttpApi,
  HttpApiEndpoint,
  HttpApiError,
  HttpApiGroup,
  HttpApiMiddleware,
  HttpApiSchema,
  OpenApi,
} from "effect/http-api";
import { insertCredential, lockOrganisation, readCertificate } from "./agent-credentials";
import { agentTokenMaxLifetime, CertificatePem, deviceTokenType, KeyAlgorithm } from "./agent-keys";
import { recordAudit } from "./audit";
import { deviceTokenIssuer, externalClockTolerance } from "./auth";
import { type Provider } from "./config";
import { Database } from "./db";
import { hasConstraint } from "./fleets";
import { pairedSubject, pairedSubjectPrefix, type Member } from "./identity";
import { handleDb, versionedApi } from "./middleware";
import { mintOneTimeToken, readOneTimeToken, sameHash } from "./one-time-tokens";
import {
  BadRequest,
  endpointErrors,
  InternalError,
  isProblem,
  problem,
  problemTypes,
  typedProblem,
  Unauthorised,
} from "./problems";
import { agentCredentials, auditLogs, organisations, pairingInvites, users } from "./schema";
import { allocateUsername } from "./usernames";
import { dateTime, Line, Uuid } from "./validation";

/**
 * Workstation pairing. A person's device generates its own key and self-signed certificate and
 * registers the certificate as one of that member's device credentials by redeeming a single-use
 * invite; it then signs device tokens with the key, as an agent signs agent tokens (see
 * `agent-keys.ts`). An invite is `spair1.` and the unpadded base64url encoding of 48 bytes, its
 * 16-byte ID and 32 random bytes; only the SHA-256 of the whole text is stored.
 */
export const pairingInvitePrefix = "spair1.";
export const defaultInviteLifetime = 3600;
export const minInviteLifetime = 300;
export const maxInviteLifetime = 24 * 3600;
export const pairingPath = `${versionedApi}/pairing`;
export const redeemPath = `${pairingPath}/redeem`;
/** A paired device signs requests for the API alone, until device credentials can sign in to IRC. */
const deviceScopes = ["api"] as const;

/** A new invite for the row `id`, and the hash to store. */
export const mintPairingInvite = (id: string) => mintOneTimeToken(pairingInvitePrefix, id);

/** The row ID an invite names and its hash, if it has an invite's shape. */
export const readPairingInvite = (invite: string) => readOneTimeToken(pairingInvitePrefix, invite);

/** What paired devices sign their tokens for: the audience, and whether pairing is on at all. */
export interface PairingPolicy {
  /** The device token audience, or null when pairing is off (AUTH_PROVIDERS lacks pairing). */
  readonly audience: string | null;
  /** The providers of the organisations this deployment serves. */
  readonly providers: ReadonlySet<Provider>;
}

/** An audit row written by the owner bootstrap command on the API host, not by a member. */
const hostAudit = (
  organisationId: string,
  fields: Pick<
    typeof auditLogs.$inferInsert,
    "action" | "entityType" | "entityId" | "summary" | "changes"
  >,
) => ({
  ...fields,
  organisationId,
  actorType: "integration" as const,
  actorId: null,
  actorDisplayName: "Owner bootstrap",
  source: "cli" as const,
});

/** Why the bootstrap command refused, in words safe to print on the API host. */
export class BootstrapRefused extends Error {}

export interface OwnerBootstrap {
  /** The pairing organisation's external ID, which IRC_AUTH_ORGANISATION names after `pairing:`. */
  readonly externalOrganisation: string;
  /** The owner's name and e-mail, needed only to create the owner. */
  readonly displayName?: string;
  readonly email?: string | null;
  /** The invite's lifetime in seconds. */
  readonly lifetime: number;
  /** Whether to restore an existing deactivated owner before issuing an invite. */
  readonly reactivate?: boolean;
  /**
   * What becomes of the owner's device credentials that are not revoked: `revoke` them, for a lost
   * or stolen workstation, or `keep` them, letting a reactivated owner's devices sign in again.
   * Reactivating an owner who has any needs one of the two.
   */
  readonly devices?: "revoke" | "keep";
  readonly reserved: ReadonlySet<string>;
}

/** The bootstrap command's device flags as `OwnerBootstrap.devices`, or the refusal of their mix. */
export function deviceChoice(flags: {
  readonly reactivate?: boolean;
  readonly revokeDevices?: boolean;
  readonly keepDevices?: boolean;
}): OwnerBootstrap["devices"] {
  if (flags.keepDevices && flags.revokeDevices)
    throw new BootstrapRefused("Give --keep-devices or --revoke-devices, not both.");
  if (flags.keepDevices && !flags.reactivate)
    throw new BootstrapRefused("--keep-devices applies only with --reactivate.");
  return flags.revokeDevices ? "revoke" : flags.keepDevices ? "keep" : undefined;
}

type Db = (typeof Database)["Service"];

/** The pairing organisation's ID, created if it is new, and locked. */
const ensureOrganisation = (tx: Db, externalId: string) =>
  Effect.gen(function* () {
    const [created] = yield* tx
      .insert(organisations)
      .values({ provider: "pairing", externalId })
      .onConflictDoNothing()
      .returning({ id: organisations.id });
    // Nothing is inserted only when the organisation exists, so the select then finds it.
    const id =
      created?.id ??
      (yield* tx
        .select({ id: organisations.id })
        .from(organisations)
        .where(
          and(eq(organisations.provider, "pairing"), eq(organisations.externalId, externalId)),
        ))[0]!.id;
    yield* lockOrganisation(tx, id);
    return { id, created: Boolean(created) };
  });

/**
 * Settles the owner's device credentials that are not revoked, oldest first: `revoke` revokes
 * them, and `keep` lets a reactivated owner's sign in again. Reactivating an owner who has any
 * needs one of the two: they would sign in again, and the owner may have been deactivated because
 * one was compromised. The organisation is locked, so no device is paired or revoked until the
 * bootstrap commits, and a refusal rolls the whole bootstrap back.
 */
const settleDevices = (
  tx: Db,
  owner: { readonly id: string; readonly organisationId: string; readonly displayName: string },
  reactivated: boolean,
  choice: OwnerBootstrap["devices"],
  now: Date,
) =>
  Effect.gen(function* () {
    const devices = yield* tx
      .select({ id: agentCredentials.id, fingerprint: agentCredentials.fingerprint })
      .from(agentCredentials)
      .where(
        and(
          eq(agentCredentials.organisationId, owner.organisationId),
          eq(agentCredentials.userId, owner.id),
          isNull(agentCredentials.revokedAt),
        ),
      )
      .orderBy(asc(agentCredentials.createdAt), asc(agentCredentials.id));
    const ids = devices.map((device) => device.id);
    if (reactivated && ids.length && !choice)
      return yield* Effect.fail(
        new BootstrapRefused(
          `The owner ${owner.displayName} has ${ids.length} device credential(s) that would sign in again. Rerun with --revoke-devices to revoke them, or --keep-devices to let them sign in.`,
        ),
      );
    if (choice !== "revoke")
      return { ids, kept: reactivated && choice === "keep" ? ids.length : 0, revoked: [] };
    if (ids.length)
      yield* tx
        .update(agentCredentials)
        .set({ revokedAt: now })
        .where(inArray(agentCredentials.id, ids));
    return { ids, kept: 0, revoked: devices };
  });

/**
 * The organisation's owner, its earliest administrator with a paired-only subject, created with
 * the request's name if there is none. A deactivated owner is never replaced, and is restored only
 * with `reactivate`.
 */
const ensureOwner = (tx: Db, organisationId: string, request: OwnerBootstrap) =>
  Effect.gen(function* () {
    // Locked before `active` is read, so a concurrent change to the owner, such as a direct
    // deactivation, either commits first and is seen here or waits for this invite.
    const [existing] = yield* tx
      .select()
      .from(users)
      .where(
        and(
          eq(users.organisationId, organisationId),
          isNull(users.agentRank),
          eq(users.role, "admin"),
          sql`starts_with(${users.subject}, ${pairedSubjectPrefix})`,
        ),
      )
      .orderBy(asc(users.createdAt), asc(users.id))
      .limit(1)
      .for("no key update");
    if (existing?.active) return { owner: existing, created: false, reactivated: false };
    if (existing) {
      if (!request.reactivate)
        return yield* Effect.fail(
          new BootstrapRefused(
            `The owner ${existing.displayName} is deactivated. Rerun with --reactivate to restore them.`,
          ),
        );
      yield* tx.update(users).set({ active: true }).where(eq(users.id, existing.id));
      return { owner: { ...existing, active: true }, created: false, reactivated: true };
    }
    if (request.reactivate)
      return yield* Effect.fail(
        new BootstrapRefused(
          "There is no owner to reactivate. Run without --reactivate to create one.",
        ),
      );
    if (!request.displayName)
      return yield* Effect.fail(new BootstrapRefused("Give the owner's name with --name."));
    const id = randomUUID();
    const email = request.email ?? null;
    const username = yield* allocateUsername(
      tx,
      organisationId,
      { name: request.displayName, email },
      request.reserved,
    );
    const [owner] = yield* tx
      .insert(users)
      .values({
        id,
        organisationId,
        subject: pairedSubject(id),
        displayName: request.displayName,
        email,
        role: "admin",
        username,
      })
      .returning();
    yield* tx.insert(auditLogs).values(
      hostAudit(organisationId, {
        action: "memberRegistered",
        entityType: "Member",
        entityId: id,
        summary: "Registered the organisation's owner on the API host.",
        changes: { provider: "pairing", role: "admin", username },
      }),
    );
    return { owner: owner!, created: true, reactivated: false };
  });

/**
 * Creates a pairing organisation and its first owner, an administrator who signs in only with
 * paired devices, or finds them again, and issues one invite that pairs a device of the owner.
 * Earlier pending invites of the owner are revoked, so only the newest one works; with `devices`
 * `revoke`, so are the owner's device credentials. With `reactivate`, a deactivated owner is
 * restored; if they have devices that were never revoked, `devices` must say what becomes of them.
 * Run from the root-only command on the API host.
 */
export const bootstrapOwner = (request: OwnerBootstrap) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const organisation = yield* ensureOrganisation(tx, request.externalOrganisation);
        const organisationId = organisation.id;
        const {
          owner,
          created: ownerCreated,
          reactivated: ownerReactivated,
        } = yield* ensureOwner(tx, organisationId, request);
        const ownerId = owner.id;

        // The database's clock, which redemption judges expiry by: with this process's, a host
        // clock ahead of it would skip revoking a pending invite that redemption still accepts.
        const [{ now }] = (yield* tx
          .select({ now: sql`clock_timestamp()`.mapWith(pairingInvites.expiresAt) })
          .from(organisations)
          .where(eq(organisations.id, organisationId))) as [{ now: Date }];
        const revokedInvites = yield* tx
          .update(pairingInvites)
          .set({ revokedAt: now })
          .where(
            and(
              eq(pairingInvites.organisationId, organisationId),
              eq(pairingInvites.userId, ownerId),
              isNull(pairingInvites.redeemedAt),
              isNull(pairingInvites.revokedAt),
              gt(pairingInvites.expiresAt, now),
            ),
          )
          .returning({ id: pairingInvites.id });
        const devices = yield* settleDevices(tx, owner, ownerReactivated, request.devices, now);

        const id = randomUUID();
        const { token, hash } = mintPairingInvite(id);
        const [invite] = yield* tx
          .insert(pairingInvites)
          .values({
            id,
            organisationId,
            userId: ownerId,
            tokenHash: hash,
            expiresAt: new Date(now.valueOf() + request.lifetime * 1000),
          })
          .returning();
        yield* tx.insert(auditLogs).values([
          ...(ownerReactivated
            ? [
                hostAudit(organisationId, {
                  action: "memberReactivated",
                  entityType: "Member",
                  entityId: ownerId,
                  summary: "Reactivated the organisation's owner on the API host.",
                  changes: {
                    active: true,
                    devices: devices.ids.length ? (devices.kept ? "kept" : "revoked") : "none",
                    deviceCount: devices.ids.length,
                    deviceCredentialIds: devices.ids,
                  },
                }),
              ]
            : []),
          ...revokedInvites.map((row) =>
            hostAudit(organisationId, {
              action: "pairingInviteRevoked",
              entityType: "PairingInvite",
              entityId: row.id,
              summary: "Revoked a pending pairing invite: a newer one replaces it.",
              changes: { memberId: ownerId },
            }),
          ),
          ...devices.revoked.map((row) =>
            hostAudit(organisationId, {
              action: "deviceCredentialRevoked",
              entityType: "DeviceCredential",
              entityId: row.id,
              summary: "Revoked the owner's device credential on the API host.",
              changes: { memberId: ownerId, fingerprint: row.fingerprint },
            }),
          ),
          hostAudit(organisationId, {
            action: "pairingInviteCreated",
            entityType: "PairingInvite",
            entityId: id,
            summary: "Issued a pairing invite for the owner on the API host.",
            changes: { memberId: ownerId, expiresAt: dateTime(invite!.expiresAt) },
          }),
        ]);
        return {
          organisationId,
          organisationCreated: organisation.created,
          owner: { id: ownerId, displayName: owner.displayName, username: owner.username },
          ownerCreated,
          ownerReactivated,
          invite: { id, token, expiresAt: invite!.expiresAt },
          revokedInvites: revokedInvites.length,
          revokedDevices: devices.revoked.length,
          keptDevices: devices.kept,
        };
      }),
    );
  });

/** What a device signs its tokens with, so that a client need not hard-code it. */
const DeviceTokenProfile = Schema.Struct({
  type: Schema.Literal(deviceTokenType),
  issuer: Schema.Literal(deviceTokenIssuer),
  audience: Schema.String,
  algorithms: Schema.Array(KeyAlgorithm),
  maxLifetimeSeconds: Schema.Int,
  clockToleranceSeconds: Schema.Int,
}).annotate({ identifier: "DeviceTokenProfile" });

const deviceTokenProfile = (audience: string) =>
  ({
    type: deviceTokenType,
    issuer: deviceTokenIssuer,
    audience,
    // Ed25519 first: it is the algorithm every client implements.
    algorithms: ["EdDSA", "ES256"],
    maxLifetimeSeconds: agentTokenMaxLifetime,
    clockToleranceSeconds: externalClockTolerance,
  }) as const;

const PairingCapabilities = Schema.Struct({
  enabled: Schema.Boolean,
  inviteVersion: Schema.Literal("spair1"),
  redeemPath: Schema.Literal(redeemPath),
  deviceToken: Schema.NullOr(DeviceTokenProfile),
}).annotate({ identifier: "PairingCapabilities" });

const maxInviteLength = 256;
const Redemption = Schema.Struct({
  /** The invite, as one line: never in a URL, a query or a header that a proxy logs. */
  invite: Schema.String.check(Schema.isMaxLength(maxInviteLength)),
  certificate: CertificatePem,
  label: Schema.optional(Schema.NullOr(Line(120))),
}).annotate({ identifier: "PairingRedemption" });

const DevicePaired = Schema.Struct({
  memberId: Uuid,
  organisationId: Uuid,
  credentialId: Uuid,
  fingerprint: Schema.String,
  scopes: Schema.Array(Schema.Literal("api")),
  deviceToken: DeviceTokenProfile,
}).annotate({ identifier: "DevicePaired" });

const unusableDetail = "This pairing invite cannot be used. Ask for a new invite.";

/**
 * The one refusal of an invite, whatever the reason, so that its answer tells nothing about
 * which invites exist. The reason goes to a debug log line; the invite never does.
 */
const unusable = (reason: string) =>
  Effect.logDebug("Pairing invite refused", { reason }).pipe(
    Effect.andThen(
      Effect.fail(typedProblem(401, problemTypes.pairingInviteUnusable, unusableDetail)),
    ),
  );

/**
 * Why a stored invite no longer pairs anything, if it does not. Whether it has expired is the
 * database's answer, not this process's clock: see `redeem`.
 */
function spent(row: typeof pairingInvites.$inferSelect, expired: boolean) {
  if (row.revokedAt) return "revoked";
  if (row.redeemedAt) return "used";
  if (expired) return "expired";
  return undefined;
}

/**
 * Registers the certificate as a device credential of the invite's member and spends the invite,
 * in one transaction under the organisation's lock and the invite's row lock, so that however
 * many requests race, an invite registers one credential. The member must be an active person of
 * an organisation this deployment serves. An invite whose secret does not match is refused on an
 * unlocked read, before any transaction or lock: a caller who knows only an invite's row ID can
 * neither learn from a lock wait that the row exists nor contend with real writes.
 */
const redeem = (policy: PairingPolicy, payload: typeof Redemption.Type) =>
  Effect.gen(function* () {
    const { audience } = policy;
    if (!audience)
      return yield* Effect.fail(
        typedProblem(404, problemTypes.pairingUnavailable, "This API does not pair devices."),
      );
    const read = readPairingInvite(payload.invite);
    if (!read) return yield* unusable("shape");
    const certificate = yield* readCertificate(payload.certificate).pipe(
      Effect.mapError((refusal) => ({ ...refusal, type: problemTypes.invalidRequest })),
    );
    const db = yield* Database;
    const [found] = yield* db
      .select({
        organisationId: pairingInvites.organisationId,
        tokenHash: pairingInvites.tokenHash,
      })
      .from(pairingInvites)
      .where(eq(pairingInvites.id, read.id));
    if (!found || !sameHash(read.hash, found.tokenHash)) return yield* unusable("unknown");
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        // Read again under the locks: the invite may have been spent or revoked since. Expiry is
        // judged by the database's clock once the organisation is locked, never by this process's,
        // so replicas whose clocks differ agree: once one refuses an invite as expired, no
        // redemption that takes the lock later can pair with it.
        yield* lockOrganisation(tx, found.organisationId);
        const [locked] = yield* tx
          .select({
            invite: pairingInvites,
            expired: sql<boolean>`${pairingInvites.expiresAt} <= clock_timestamp()`,
          })
          .from(pairingInvites)
          .where(
            and(
              eq(pairingInvites.organisationId, found.organisationId),
              eq(pairingInvites.id, read.id),
            ),
          )
          .for("update");
        if (!locked || !sameHash(read.hash, locked.invite.tokenHash))
          return yield* unusable("unknown");
        const { invite } = locked;
        const reason = spent(invite, locked.expired);
        if (reason) return yield* unusable(reason);
        const [row] = yield* tx
          .select({ member: users, provider: organisations.provider })
          .from(users)
          .innerJoin(organisations, eq(organisations.id, users.organisationId))
          .where(and(eq(users.organisationId, invite.organisationId), eq(users.id, invite.userId)))
          .for("share", { of: users });
        if (!row || row.member.agentRank || !row.member.active) return yield* unusable("member");
        if (!policy.providers.has(row.provider)) return yield* unusable("organisation");
        const member: Member = row.member;

        const credential = yield* insertCredential(tx, {
          organisationId: invite.organisationId,
          userId: member.id,
          certificate,
          scopes: deviceScopes,
          label: payload.label ?? null,
          pairingInviteId: invite.id,
        }).pipe(
          // Its only problem: the certificate is already registered, or was revoked (409).
          Effect.mapError((error) =>
            isProblem(error) ? { ...error, type: problemTypes.credentialConflict } : error,
          ),
          Effect.catchIf(
            (error) => hasConstraint(error, "agent_credentials_pairing_invite"),
            () => unusable("used"),
          ),
        );
        yield* tx
          .update(pairingInvites)
          .set({ redeemedAt: new Date() })
          .where(eq(pairingInvites.id, invite.id));
        yield* recordAudit(tx, member, {
          action: "deviceCredentialAdded",
          entityType: "DeviceCredential",
          entityId: credential.id,
          summary: "Paired a device with a pairing invite.",
          changes: {
            memberId: member.id,
            fingerprint: credential.fingerprint,
            scopes: credential.scopes,
            pairingInviteId: invite.id,
          },
        });
        yield* recordAudit(tx, member, {
          action: "pairingInviteRedeemed",
          entityType: "PairingInvite",
          entityId: invite.id,
          summary: "Redeemed a pairing invite.",
          changes: { memberId: member.id, credentialId: credential.id },
        });
        return {
          memberId: member.id,
          organisationId: member.organisationId,
          credentialId: credential.id,
          fingerprint: credential.fingerprint,
          scopes: deviceScopes,
          deviceToken: deviceTokenProfile(audience),
        };
      }),
    );
  });

class PairingProblems extends HttpApiMiddleware.Service<PairingProblems>()(
  "subspace/PairingProblems",
  { error: [BadRequest, InternalError] },
) {}

const invalidRequest = (detail: string) => typedProblem(400, problemTypes.invalidRequest, detail);
const notJson = () => invalidRequest("Send an application/json body.");

/**
 * The pairing group's refusals of a request's shape, typed so that clients can branch on them; the
 * detail never repeats a field's value. A body is JSON only. Effect reads a body without a
 * Content-Type as JSON, so that is refused here; for another media type it answers 415, not a
 * schema failure, when it cannot select a body decoder.
 */
export const pairingProblems = Layer.succeed(PairingProblems, (handle, { endpoint }) =>
  Effect.gen(function* () {
    const request = yield* HttpServerRequest.HttpServerRequest;
    if (endpoint.payload.size && !request.headers["content-type"]?.trim())
      return yield* Effect.fail(notJson());
    const response = yield* handle;
    return response.status === 415 ? yield* Effect.fail(notJson()) : response;
  }).pipe(
    Effect.catch((error) =>
      Effect.fail(
        !HttpApiError.HttpApiSchemaError.is(error)
          ? error
          : error.kind === "ResponseHeaders"
            ? problem(500, "The response could not be serialised.")
            : invalidRequest("The request contains invalid fields."),
      ),
    ),
  ),
);

/**
 * Pairing on the main listener, outside the bearer middleware: the probe needs no token, and a
 * redemption is authorised by the invite in its body alone.
 */
export const pairingGroup = HttpApiGroup.make("pairing")
  .add(
    HttpApiEndpoint.get("pairingCapabilities", "/pairing", {
      success: PairingCapabilities,
    }).annotate(
      OpenApi.Description,
      "Whether this API pairs devices, and the device token profile a paired device signs with. It needs no token and tells nothing secret, so a client can check it before it sends an invite: an API that predates pairing answers 404, and one that does not pair devices answers enabled false.",
    ),
    HttpApiEndpoint.post("redeemPairingInvite", "/pairing/redeem", {
      payload: Redemption,
      success: DevicePaired.pipe(HttpApiSchema.status(201)),
      error: [...endpointErrors, Unauthorised],
    }).annotate(
      OpenApi.Description,
      [
        `A person's device registers its own certificate as one of the invite's member's device credentials, and spends the invite (201). The invite, which begins ${pairingInvitePrefix}, is sent in the JSON body only.`,
        `Problems carry a stable type: ${problemTypes.invalidRequest} (400) for a missing or unsupported content type, malformed body or fields (including an invite field longer than ${maxInviteLength} characters), or a certificate that is not Ed25519 or P-256;`,
        `${problemTypes.pairingInviteUnusable} (401) for an otherwise valid request with an unknown, malformed, expired, revoked or used invite, or one whose member can no longer pair, all with the same detail;`,
        `${problemTypes.credentialConflict} (409) for a certificate already registered or revoked; ${problemTypes.pairingUnavailable} (404) when this API does not pair devices.`,
        "A redemption retry with the same invite returns 401 if the first attempt committed. While the outcome is unknown, keep the pending key and certificate.",
        "While the invite may still be usable, retry with the same invite and certificate, then check with a freshly signed GET /api/v1/me. A 200 confirms pairing. Even a second /me 401 does not rule out an in-flight commit.",
        "Ask for a new invite only after the original invite is refused as unusable and a fresh /me still returns 401 with the audience and clock checked, or after an administrator confirms the original invite cannot be redeemed and the pending device is not active. Otherwise keep the pending record.",
        "Asking for a replacement invite never permits discarding a pending key whose outcome is still unknown; pair the replacement with a new key.",
        "Expiry is judged by the database's clock, so every API replica gives the same answer.",
      ].join(" "),
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(PairingProblems);

export const pairingHandlers =
  (policy: PairingPolicy) =>
  (handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof pairingGroup>) =>
    handlers
      .handle("pairingCapabilities", () =>
        Effect.succeed({
          enabled: policy.audience !== null,
          inviteVersion: "spair1" as const,
          redeemPath,
          deviceToken: policy.audience ? deviceTokenProfile(policy.audience) : null,
        }),
      )
      .handle("redeemPairingInvite", ({ payload }) =>
        handleDb(redeem(policy, payload)).pipe(
          Effect.map((body) =>
            HttpServerResponse.jsonUnsafe(body, {
              status: 201,
              headers: { "Cache-Control": "no-store" },
            }),
          ),
        ),
      );
