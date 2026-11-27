import { randomUUID } from "node:crypto";
import { and, eq, gt, isNull, sql } from "drizzle-orm";
import { Effect } from "effect";
import { type Database } from "./db";
import {
  effectiveRole,
  type IssuerAuthority,
  issuerRoleSource,
  pairedSubject,
  pairedSubjectPrefix,
} from "./identity";
import { mintOneTimeToken, readOneTimeToken } from "./one-time-tokens";
import { agentCredentials, memberIdentities, organisations, pairingInvites, users } from "./schema";
import { allocateUsername } from "./usernames";
import { dateTime } from "./validation";

/**
 * Pairing invites, and the people they are issued for: the one sequence that the owner bootstrap
 * command on the API host (`pairing-owner.ts`) and the API's invite routes both follow. Each
 * creates a paired-only person with `insertPairedPerson` if it needs one, issues the invite with
 * `issueInvite`, and records the audit fields these return as its own actor: the host command as
 * an integration, the routes as the member who asked. A device redeems the invite in
 * `pairing.ts`. An invite is `spair1.` and the unpadded base64url encoding of 48 bytes, its 16-byte
 * ID and 32 random bytes; only the SHA-256 of the whole text is stored.
 */
export const pairingInvitePrefix = "spair1.";
export const defaultInviteLifetime = 3600;
export const minInviteLifetime = 300;
export const maxInviteLifetime = 24 * 3600;

type Db = (typeof Database)["Service"];

/** A new invite for the row `id`, and the hash to store. */
export const mintPairingInvite = (id: string) => mintOneTimeToken(pairingInvitePrefix, id);

/** The row ID an invite names and its hash, if it has an invite's shape. */
export const readPairingInvite = (invite: string) => readOneTimeToken(pairingInvitePrefix, invite);

/**
 * Creates a person of the organisation who signs in only with paired devices until they link a
 * provider account, with a username allocated from their name and e-mail. Returns the row and
 * the audit fields of their registration, with `summary`, for the caller to record as its actor.
 */
export const insertPairedPerson = (
  tx: Db,
  person: {
    organisationId: string;
    displayName: string;
    email: string | null;
    role: "admin" | "member";
    reserved: ReadonlySet<string>;
    summary: string;
  },
) =>
  Effect.gen(function* () {
    const id = randomUUID();
    const { organisationId, displayName, email, role } = person;
    const username = yield* allocateUsername(
      tx,
      organisationId,
      { name: displayName, email },
      person.reserved,
    );
    const [member] = yield* tx
      .insert(users)
      .values({
        id,
        organisationId,
        subject: pairedSubject(id),
        displayName,
        email,
        role,
        username,
      })
      .returning();
    return {
      member: member!,
      registered: {
        action: "memberRegistered",
        entityType: "Member",
        entityId: id,
        summary: person.summary,
        changes: { provider: "pairing", role, username },
      },
    };
  });

/** The database's clock, read in `tx` once it holds the organisation's lock. */
export const databaseNow = (tx: Db, organisationId: string) =>
  Effect.map(
    tx
      .select({ now: sql`clock_timestamp()`.mapWith(pairingInvites.expiresAt) })
      .from(organisations)
      .where(eq(organisations.id, organisationId)),
    (rows) => (rows as [{ now: Date }])[0].now,
  );

/**
 * Issues a pairing invite for a person, in a transaction that holds the organisation's lock, and
 * revokes the person's earlier pending invites, so that only the newest one works. `now`, by
 * default the database's clock, which redemption judges expiry by, dates both. Returns the
 * invite's row, its text, which is shown once, and the revoked invites' IDs, for the caller to
 * audit.
 */
export const issueInvite = (
  tx: Db,
  values: {
    organisationId: string;
    userId: string;
    lifetime: number;
    createdById?: string | null;
    createdByCredentialId?: string | null;
    issuerAuthority?: IssuerAuthority | null;
    now?: Date;
  },
) =>
  Effect.gen(function* () {
    const { organisationId, userId } = values;
    const now = values.now ?? (yield* databaseNow(tx, organisationId));
    const revokedInvites = yield* tx
      .update(pairingInvites)
      .set({ revokedAt: now })
      .where(
        and(
          eq(pairingInvites.organisationId, organisationId),
          eq(pairingInvites.userId, userId),
          isNull(pairingInvites.redeemedAt),
          isNull(pairingInvites.revokedAt),
          gt(pairingInvites.expiresAt, now),
        ),
      )
      .returning({ id: pairingInvites.id });
    const id = randomUUID();
    const { token, hash } = mintPairingInvite(id);
    const [invite] = yield* tx
      .insert(pairingInvites)
      .values({
        id,
        organisationId,
        userId,
        tokenHash: hash,
        expiresAt: new Date(now.valueOf() + values.lifetime * 1000),
        createdById: values.createdById ?? null,
        createdByCredentialId: values.createdByCredentialId ?? null,
        issuerAuthority: values.issuerAuthority ?? null,
      })
      .returning();
    return { invite: invite!, token, revokedInvites };
  });

/**
 * Whether a person already signs in some way: with a provider account, registered or linked, or
 * with a device that was never revoked. Such a person pairs a further device from one they have.
 */
export const signsIn = (tx: Db, person: typeof users.$inferSelect) =>
  Effect.gen(function* () {
    if (!person.subject.startsWith(pairedSubjectPrefix)) return true;
    const [linked] = yield* tx
      .select({ id: memberIdentities.id })
      .from(memberIdentities)
      .where(
        and(
          eq(memberIdentities.organisationId, person.organisationId),
          eq(memberIdentities.userId, person.id),
        ),
      )
      .limit(1);
    if (linked) return true;
    const [device] = yield* tx
      .select({ id: agentCredentials.id })
      .from(agentCredentials)
      .where(
        and(
          eq(agentCredentials.organisationId, person.organisationId),
          eq(agentCredentials.userId, person.id),
          isNull(agentCredentials.revokedAt),
        ),
      )
      .limit(1);
    return device !== undefined;
  });

/** The audit fields of a pending invite that a newer one for the same person replaced. */
export const inviteReplaced = (inviteId: string, memberId: string) => ({
  action: "pairingInviteRevoked",
  entityType: "PairingInvite",
  entityId: inviteId,
  summary: "Revoked a pending pairing invite: a newer one replaces it.",
  changes: { memberId },
});

/** The audit fields of an invite that `issueInvite` issued for `memberId`, with `summary`. */
export const inviteCreated = (
  invite: typeof pairingInvites.$inferSelect,
  memberId: string,
  summary: string,
) => ({
  action: "pairingInviteCreated",
  entityType: "PairingInvite",
  entityId: invite.id,
  summary,
  changes: {
    memberId,
    expiresAt: dateTime(invite.expiresAt),
    createdByDeviceId: invite.createdByCredentialId,
  },
});

/**
 * Why an invite's issuer no longer vouches for it, if they do not, given the device that issued
 * it and its issuer as they stand now: the device is revoked, or, for another person's invite,
 * the issuer is deactivated or no longer an administrator from the source their role came from
 * when they issued it (`issuerAuthority`, see `issuerRoleSource`), under the deployment's
 * ENTRA_ADMIN_ROLE as it is now. So an invite issued with an Entra app role alone is refused once
 * the setting is removed, and one issued with the provisioned role once the provisioned or the
 * recorded token role is a member's, without the issuer signing in again. Redemption applies it
 * under the organisation's lock (`issuerRefusal`), and the pending list leaves out the invites it
 * refuses, so that the list shows only invites that still pair a device.
 */
export const issuerWithdrawal = (
  invite: Pick<
    typeof pairingInvites.$inferSelect,
    "userId" | "createdById" | "createdByCredentialId" | "issuerAuthority"
  >,
  device: { readonly revokedAt: Date | null } | null | undefined,
  issuer:
    | Pick<typeof users.$inferSelect, "active" | "agentRank" | "role" | "tokenRole">
    | null
    | undefined,
  entraAdminRole: string | null,
) => {
  if (invite.createdByCredentialId && (!device || device.revokedAt)) return "issuing device";
  if (!invite.createdById || invite.createdById === invite.userId) return undefined;
  if (!issuer?.active || issuer.agentRank) return "issuer";
  const source = issuerRoleSource(invite.issuerAuthority, entraAdminRole);
  return source && effectiveRole(issuer, source) === "admin" ? undefined : "issuer role";
};

/**
 * `issuerWithdrawal` for an invite at redemption, reading its issuing device and its issuer, whose
 * row it shares, in the transaction that holds the organisation's lock and the invite's row.
 */
export const issuerRefusal = (
  tx: Db,
  invite: typeof pairingInvites.$inferSelect,
  entraAdminRole: string | null,
) =>
  Effect.gen(function* () {
    const [device] = invite.createdByCredentialId
      ? yield* tx
          .select({ revokedAt: agentCredentials.revokedAt })
          .from(agentCredentials)
          .where(
            and(
              eq(agentCredentials.organisationId, invite.organisationId),
              eq(agentCredentials.id, invite.createdByCredentialId),
            ),
          )
      : [];
    const [issuer] =
      invite.createdById && invite.createdById !== invite.userId
        ? yield* tx
            .select()
            .from(users)
            .where(
              and(
                eq(users.organisationId, invite.organisationId),
                eq(users.id, invite.createdById),
              ),
            )
            .for("share")
        : [];
    return issuerWithdrawal(invite, device, issuer, entraAdminRole);
  });
