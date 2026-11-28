import { and, asc, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { lockOrganisation } from "./organisation-lock";
import { type AuditFields } from "./audit";
import { Database } from "./db";
import { revokeDevices } from "./device-revocation";
import { pairedSubjectPrefix } from "./identity";
import { lockInitiativeWrites } from "./initiatives";
import {
  databaseNow,
  insertPairedPerson,
  inviteCreated,
  inviteReplaced,
  issueInvite,
} from "./pairing-invites";
import { agentCredentials, auditLogs, organisations, users } from "./schema";

/**
 * The owner bootstrap command's work, on the API host (`scripts/pairing-owner.ts`): it creates a
 * pairing organisation and its first owner, or finds them again, and issues the owner an invite
 * (see `pairing-invites.ts`), auditing it all as an integration rather than a member.
 */

type Db = (typeof Database)["Service"];

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

export const PairingOrganisationId = Schema.String.check(
  Schema.isPattern(/^[a-z0-9][a-z0-9.-]{0,62}$/u),
);

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

/**
 * The pairing organisation's ID, created if it is new, and locked: `FOR NO KEY UPDATE` by default,
 * as every write to its members takes it, or `FOR UPDATE` for a transaction that will also write
 * initiatives (`lockInitiativeWrites`), so that it takes the stronger lock first and never
 * upgrades it while holding other rows.
 */
export const ensureOrganisation = (
  tx: Db,
  externalId: string,
  lock: "no key update" | "update" = "no key update",
) =>
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
    yield* lock === "update" ? lockInitiativeWrites(tx, id) : lockOrganisation(tx, id);
    return { id, created: Boolean(created) };
  });

/**
 * Settles the owner's device credentials that are not revoked, oldest first: `revoke` revokes
 * them, and what they issued, as the device routes do (see `revokeDevices`), and `keep` lets a
 * reactivated owner's sign in again. Reactivating an owner who has any needs one of the two: they
 * would sign in again, and the owner may have been deactivated because one was compromised. The
 * organisation is locked, so no device is paired or revoked until the bootstrap commits, and a
 * refusal rolls the whole bootstrap back.
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
      .select({
        id: agentCredentials.id,
        userId: agentCredentials.userId,
        fingerprint: agentCredentials.fingerprint,
        revokedAt: agentCredentials.revokedAt,
      })
      .from(agentCredentials)
      .where(
        and(
          eq(agentCredentials.organisationId, owner.organisationId),
          eq(agentCredentials.userId, owner.id),
          isNull(agentCredentials.revokedAt),
        ),
      )
      .orderBy(asc(agentCredentials.createdAt), asc(agentCredentials.id))
      .for("update");
    const ids = devices.map((device) => device.id);
    const none = { devices: 0, invites: 0 };
    if (reactivated && ids.length && !choice)
      return yield* Effect.fail(
        new BootstrapRefused(
          `The owner ${owner.displayName} has ${ids.length} device credential(s) that would sign in again. Rerun with --revoke-devices to revoke them, or --keep-devices to let them sign in.`,
        ),
      );
    if (choice !== "revoke")
      return {
        ids,
        kept: reactivated && choice === "keep" ? ids.length : 0,
        revoked: 0,
        cascade: none,
      };
    const revoker = {
      organisationId: owner.organisationId,
      memberId: null,
      audit: (fields: AuditFields) =>
        tx.insert(auditLogs).values(hostAudit(owner.organisationId, fields)),
    };
    const cascade = yield* revokeDevices(
      tx,
      revoker,
      devices,
      () => "Revoked the owner's device credential on the API host.",
      now,
    );
    return { ids, kept: 0, revoked: ids.length, cascade };
  });

/**
 * The organisation's owner, its earliest administrator with a paired-only subject, created with
 * the request's name if there is none. A deactivated owner is never replaced, and is restored only
 * by `auth:owner` with `reactivate`. The caller holds the organisation lock acquired by
 * `ensureOrganisation` and attributes any new owner registration to its own CLI audit actor.
 */
export const ensureOwner = (
  tx: Db,
  organisationId: string,
  request: Pick<OwnerBootstrap, "displayName" | "email" | "reserved"> &
    (
      | { readonly command: "auth:owner"; readonly reactivate?: boolean }
      | { readonly command: "auth:provision-fleet" }
    ),
) =>
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
    if (existing?.active) return { owner: existing, reactivated: false };
    if (existing) {
      if (request.command === "auth:provision-fleet")
        return yield* Effect.fail(
          new BootstrapRefused(
            `The owner ${existing.displayName} is deactivated. Use auth:owner with --reactivate to restore them before provisioning.`,
          ),
        );
      if (!request.reactivate)
        return yield* Effect.fail(
          new BootstrapRefused(
            `The owner ${existing.displayName} is deactivated. Rerun with --reactivate to restore them.`,
          ),
        );
      yield* tx.update(users).set({ active: true }).where(eq(users.id, existing.id));
      return { owner: { ...existing, active: true }, reactivated: true };
    }
    if (request.command === "auth:owner" && request.reactivate)
      return yield* Effect.fail(
        new BootstrapRefused(
          "There is no owner to reactivate. Run without --reactivate to create one.",
        ),
      );
    if (!request.displayName)
      return yield* Effect.fail(
        new BootstrapRefused(
          request.command === "auth:owner"
            ? "Give the owner's name with --name."
            : "Give the owner's name in the fleet specification.",
        ),
      );
    const { member, registered } = yield* insertPairedPerson(tx, {
      organisationId,
      displayName: request.displayName,
      email: request.email ?? null,
      role: "admin",
      reserved: request.reserved,
      summary: "Registered the organisation's owner on the API host.",
    });
    return { owner: member, registered, reactivated: false };
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
          registered,
          reactivated: ownerReactivated,
        } = yield* ensureOwner(tx, organisationId, { ...request, command: "auth:owner" });
        const ownerId = owner.id;

        // The database's clock, which redemption judges expiry by: with this process's, a host
        // clock ahead of it would skip revoking a pending invite that redemption still accepts.
        const now = yield* databaseNow(tx, organisationId);
        const devices = yield* settleDevices(tx, owner, ownerReactivated, request.devices, now);

        const { invite, token, revokedInvites } = yield* issueInvite(tx, {
          organisationId,
          userId: ownerId,
          lifetime: request.lifetime,
          now,
        });
        yield* tx.insert(auditLogs).values([
          ...(registered ? [hostAudit(organisationId, registered)] : []),
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
            hostAudit(organisationId, inviteReplaced(row.id, ownerId)),
          ),
          hostAudit(
            organisationId,
            inviteCreated(
              invite,
              ownerId,
              "Issued a pairing invite for the owner on the API host.",
            ),
          ),
        ]);
        return {
          organisationId,
          organisationCreated: organisation.created,
          owner: { id: ownerId, displayName: owner.displayName, username: owner.username },
          ownerCreated: Boolean(registered),
          ownerReactivated,
          invite: { id: invite.id, token, expiresAt: invite.expiresAt },
          revokedInvites: revokedInvites.length,
          revokedDevices: devices.revoked,
          keptDevices: devices.kept,
          /** What the owner's revoked devices had issued, revoked with them. */
          cascade: devices.cascade,
        };
      }),
    );
  });
