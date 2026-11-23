import { and, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { recordAudit } from "./audit";
import { type PrincipalIdentity, type SignInClaims, type VerifiedIdentity } from "./auth";
import { type Provider } from "./config";
import { Database } from "./db";
import { findMember, type Member } from "./identity";
import { organisations, users } from "./schema";
import { allocateUsername, lockUsernames } from "./usernames";
import { Email } from "./validation";

/** How a verified sign-in becomes a member: first-sign-in registration and the admin role. */
export interface SignInPolicy {
  /** Providers whose first valid token registers an unknown person as a member. */
  readonly autoRegister: ReadonlySet<Provider>;
  /** The Entra app role that makes a person an administrator, or null to leave roles alone. */
  readonly entraAdminRole: string | null;
  /** The operator's reserved usernames, in lower case. */
  readonly reservedUsernames: ReadonlySet<string>;
  /** Whether registration may register an Entra guest. */
  readonly registerGuests: boolean;
}

const isEmail = Schema.is(Email);

const isControl = (character: string) =>
  character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127;

/** A claim as one line of at most `max` characters, or nothing. */
function line(value: string | undefined, max: number) {
  if (value === undefined) return undefined;
  const text = Array.from(value, (character) => (isControl(character) ? " " : character))
    .join("")
    .replace(/\s+/gu, " ")
    .trim()
    .slice(0, max)
    .trim();
  return text || undefined;
}

/**
 * A new person's profile from their token's claims. The e-mail is the `email` claim when it has
 * the form of an address, and nothing else: `preferred_username` is often a sign-in name, not a
 * mailbox. The display name is `name`, else the given and family names, else the e-mail, else
 * `preferred_username`. Claims only fill in the profile; they never identify the person.
 */
export function profileFromClaims(claims: Omit<SignInClaims, "roles">) {
  const email = claims.email !== undefined && isEmail(claims.email) ? claims.email : null;
  const displayName =
    line(claims.name, 120) ??
    line([claims.givenName, claims.familyName].filter(Boolean).join(" "), 120) ??
    line(email ?? undefined, 120) ??
    line(claims.preferredUsername, 120) ??
    "New member";
  return { displayName, email };
}

/**
 * Registers a person on their first valid sign-in: the organisation row if it is missing, then
 * an active member with no initiative memberships and a username, audited, in one transaction.
 * Registrations in an organisation are serialised on its row, as agent registration changes are,
 * so two first requests from the same person create one member, the second request returns it,
 * and two people with the same first name receive different usernames.
 */
export const registerMember = (
  identity: PrincipalIdentity,
  claims: SignInClaims,
  tokenRole: Member["tokenRole"],
  reserved: ReadonlySet<string>,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const organisation = and(
      eq(organisations.provider, identity.provider),
      eq(organisations.externalId, identity.externalOrganisation),
    );
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* tx
          .insert(organisations)
          .values({ provider: identity.provider, externalId: identity.externalOrganisation })
          .onConflictDoNothing();
        const [org] = yield* tx
          .select({ id: organisations.id })
          .from(organisations)
          .where(organisation)
          .for("no key update");
        const existing = yield* findMember(identity).pipe(Effect.provideService(Database, tx));
        if (existing) return existing;
        const profile = profileFromClaims(claims);
        const username = yield* allocateUsername(
          tx,
          org!.id,
          {
            givenName: claims.givenName,
            familyName: claims.familyName,
            name: claims.name,
            // Only when no name is given, the sign-in name stands in for an e-mail.
            email:
              profile.email ??
              (claims.preferredUsername && isEmail(claims.preferredUsername)
                ? claims.preferredUsername
                : null),
          },
          reserved,
        );
        const [created] = yield* tx
          .insert(users)
          .values({
            organisationId: org!.id,
            subject: identity.subject,
            ...profile,
            role: "member",
            tokenRole,
            username,
          })
          .onConflictDoNothing({ target: [users.organisationId, users.subject] })
          .returning();
        // A writer that does not take the lock, such as an older API instance, added it first.
        if (!created) return yield* findMember(identity).pipe(Effect.provideService(Database, tx));
        yield* recordAudit(tx, created, {
          action: "memberRegistered",
          entityType: "Member",
          entityId: created.id,
          summary: "Registered on first sign-in.",
          changes: {
            provider: identity.provider,
            role: "member",
            ...(tokenRole ? { tokenRole } : {}),
            username,
          },
        });
        return created;
      }),
    );
  });

/**
 * Records the role a person's Entra token gives them, writing and auditing only when it changes.
 * The provisioned role in `role` is left as it is. Agent records are never changed, and the
 * database refuses them a token role, so they are never administrators.
 */
export const applyTokenRole = (member: Member, tokenRole: "admin" | "member") =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [updated] = yield* tx
          .update(users)
          .set({ tokenRole, updatedAt: sql`now()` })
          .where(
            and(
              eq(users.id, member.id),
              isNull(users.agentRank),
              sql`${users.tokenRole} is distinct from ${tokenRole}`,
            ),
          )
          .returning();
        // Another request recorded the same change first.
        if (!updated) return { ...member, tokenRole };
        yield* recordAudit(tx, updated, {
          action: "memberRoleChanged",
          entityType: "Member",
          entityId: updated.id,
          summary: "Applied the identity provider's administrator role.",
          changes: { from: member.tokenRole, to: tokenRole, source: "entraAppRole" },
        });
        return { ...member, tokenRole, updatedAt: updated.updatedAt };
      }),
    );
  });

/**
 * Allocates a username to an active person who has none, such as one an older API instance
 * added during a rolling deployment, by the backfill's rule. Once set it never changes here.
 */
export const ensureUsername = (member: Member, reserved: ReadonlySet<string>) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockUsernames(tx, member.organisationId);
        const [row] = yield* tx
          .select({ username: users.username })
          .from(users)
          .where(eq(users.id, member.id));
        // Another request allocated it first.
        if (row?.username) return { ...member, username: row.username };
        const username = yield* allocateUsername(
          tx,
          member.organisationId,
          { name: member.displayName, email: member.email },
          reserved,
        );
        const [updated] = yield* tx
          .update(users)
          .set({ username })
          .where(and(eq(users.id, member.id), isNull(users.username), isNull(users.agentRank)))
          .returning({ username: users.username });
        if (!updated) return member;
        yield* recordAudit(tx, member, {
          action: "usernameAllocated",
          entityType: "Member",
          entityId: member.id,
          summary: "Allocated a username.",
          changes: { username },
        });
        return { ...member, username };
      }),
    );
  });

/**
 * The member a verified sign-in acts as. An unknown person is registered when their provider
 * allows it; a known one, active or not, is returned as found, so registration never reactivates
 * anyone. An active person without a username is given one.
 *
 * The returned `role` is the effective role. With ENTRA_ADMIN_ROLE configured, an Entra person's
 * is the one their token gives, which is recorded in `tokenRole`; otherwise it is the provisioned
 * `role`, so removing the setting withdraws every grant it made. Agents are never administrators.
 */
export const signedInMember = (identity: VerifiedIdentity, policy: SignInPolicy) =>
  Effect.gen(function* () {
    const claims = identity.claims ?? { roles: [] };
    const adminRole = identity.provider === "entra" ? policy.entraAdminRole : null;
    const tokenRole =
      adminRole === null ? null : claims.roles.includes(adminRole) ? "admin" : "member";
    // A guest of the tenant is registered only when the operator allows it; an existing guest
    // member, provisioned by the membership command, still signs in.
    const registers =
      policy.autoRegister.has(identity.provider) && (!claims.guest || policy.registerGuests);
    let member =
      (yield* findMember(identity)) ??
      (registers
        ? yield* registerMember(identity, claims, tokenRole, policy.reservedUsernames)
        : undefined);
    if (!member?.active || member.agentRank) return member;
    if (tokenRole !== null && member.tokenRole !== tokenRole)
      member = yield* applyTokenRole(member, tokenRole);
    if (member.username === null) member = yield* ensureUsername(member, policy.reservedUsernames);
    if (tokenRole === null) return member;
    const effective: Member = { ...member, role: tokenRole };
    return effective;
  });
