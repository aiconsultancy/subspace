import { and, eq, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { lockOrganisation } from "./agent-credentials";
import { recordAudit } from "./audit";
import { type PrincipalIdentity, type SignInClaims, type VerifiedIdentity } from "./auth";
import { type Provider } from "./config";
import { Database } from "./db";
import {
  findSignedInMember,
  isKeyOnlySubject,
  issuerInviteRevoked,
  type Member,
  otherSignInRoleSource,
  revokeIssuedInvites,
  signedInAs,
} from "./identity";
import { providerOrganisationClaim } from "./provider-organisations";
import { auditLogs, organisations, users } from "./schema";
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
 *
 * Returns the member the account signs in as and whether it is through a linked account, as
 * `findSignedInMember` does: a link committed since the caller's own lookup is found here, under
 * the provider organisation's lock, and is returned as linked rather than registered over.
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
        // Linking and registration take the claim's lock, so it sees a link or a registration
        // committed since the caller's own lookup. The account comes before the binding: the
        // account that a link into another organisation names signs in as its member.
        const claim = yield* providerOrganisationClaim(tx, identity);
        if (claim.signedIn) return claim.signedIn;
        // A provider organisation whose accounts are linked into another Subspace organisation
        // gets no organisation of its own, which would split its people in two: they are invited
        // to that organisation and link their accounts instead.
        if (claim.boundTo) return undefined;
        yield* tx
          .insert(organisations)
          .values({ provider: identity.provider, externalId: identity.externalOrganisation })
          .onConflictDoNothing();
        const [org] = yield* tx
          .select({ id: organisations.id })
          .from(organisations)
          .where(organisation)
          .for("no key update");
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
        if (!created)
          return yield* findSignedInMember(identity).pipe(Effect.provideService(Database, tx));
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
        return { member: created, linked: false };
      }),
    );
  });

/**
 * Records the role a person's Entra token gives them, writing and auditing only when it changes.
 * The provisioned role in `role` is left as it is. Agent records are never changed, and the
 * database refuses them a token role, so they are never administrators. A person whose token no
 * longer makes them an administrator loses the pairing invites they issued for others.
 */
export const applyTokenRole = (member: Member, tokenRole: "admin" | "member") =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        // The organisation's row first, as every write to a member and their invites takes it:
        // redemption holds an invite and then reads its issuer, so taking the member's row and
        // then their invites without it could deadlock with a redemption.
        yield* lockOrganisation(tx, member.organisationId);
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
        if (tokenRole !== "admin")
          for (const invite of yield* revokeIssuedInvites(tx, updated))
            yield* recordAudit(tx, updated, issuerInviteRevoked(invite, updated.id));
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
 * The member a verified sign-in acts as: the member registered under the account, or the one
 * that linked it. An unknown person is registered when their provider allows it, unless their
 * provider organisation is linked into another organisation; a known one, active or not, is
 * returned as found, so registration never reactivates anyone. An active person without a
 * username is given one.
 *
 * The returned `role` is the effective role, and `roleSource` where it comes from (see
 * `RoleSource`). With ENTRA_ADMIN_ROLE configured, an Entra person's is the one their token gives,
 * which is recorded in `tokenRole`; otherwise it is the provisioned `role`, so removing the setting
 * withdraws every grant it made. Agents are never administrators.
 */
export const signedInMember = (identity: VerifiedIdentity, policy: SignInPolicy) =>
  Effect.gen(function* () {
    // No provider account is an enrolled agent, and none is registered under such a subject.
    if (isKeyOnlySubject(identity.subject)) return undefined;
    const claims = identity.claims ?? { roles: [] };
    const adminRole = identity.provider === "entra" ? policy.entraAdminRole : null;
    const tokenRole =
      adminRole === null ? null : claims.roles.includes(adminRole) ? "admin" : "member";
    // A guest of the tenant is registered only when the operator allows it; an existing guest
    // member, provisioned by the membership command, still signs in.
    const registers =
      policy.autoRegister.has(identity.provider) && (!claims.guest || policy.registerGuests);
    const found =
      (yield* findSignedInMember(identity)) ??
      (registers
        ? yield* registerMember(identity, claims, tokenRole, policy.reservedUsernames)
        : undefined);
    if (!found) return undefined;
    // A linked account signs in as the member that linked it, with that member's own role: the
    // provider's administrator role applies only in the provider organisation's own Subspace
    // organisation, to the people registered there. That holds too for a link that registration
    // finds, committed after the lookup above. While ENTRA_ADMIN_ROLE is configured, that role is
    // never above the member's recorded token role.
    const other = otherSignInRoleSource(policy);
    if (found.linked) return signedInAs(found.member, other);
    let member = found.member;
    if (!member.active || member.agentRank) return signedInAs(member, other);
    if (tokenRole !== null && member.tokenRole !== tokenRole)
      member = yield* applyTokenRole(member, tokenRole);
    if (member.username === null) member = yield* ensureUsername(member, policy.reservedUsernames);
    // The role this request's own token granted travels with it, as the ceiling whenever its row
    // is read again: another sign-in may record a different one meanwhile.
    return tokenRole === null ? signedInAs(member, other) : signedInAs(member, "token", tokenRole);
  });

interface Profile {
  email: string | null;
  displayName: string;
}

/**
 * Saves a person's membership from the membership command. A new person, or one saved before
 * usernames existed, is allocated a username; an existing username never changes. It never adds
 * a person to the provider organisation's own Subspace organisation while that provider
 * organisation's accounts are linked into another one, as registration does not: that would split
 * its people in two. A person already saved there can still be changed or deactivated; one who
 * is deactivated or is no longer an administrator loses the invites they issued for others.
 */
export const provisionMember = (
  identity: PrincipalIdentity,
  profile: Profile,
  role: Member["role"],
  active = true,
  reserved: ReadonlySet<string> = new Set(),
) =>
  Effect.gen(function* () {
    if (isKeyOnlySubject(identity.subject))
      return yield* Effect.fail(
        new Error("This form of subject is kept for members who sign in only with a key."),
      );
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const claim = yield* providerOrganisationClaim(tx, identity);
        // An account linked to a member signs in as that member: registering it as well would
        // make it name two.
        if (claim.signedIn?.linked)
          return yield* Effect.fail(
            new Error(
              "This account is linked to a member, as whom it signs in. Unlink it from that member first.",
            ),
          );
        yield* tx
          .insert(organisations)
          .values({ provider: identity.provider, externalId: identity.externalOrganisation })
          .onConflictDoNothing();
        const org = (yield* tx
          .select()
          .from(organisations)
          .where(
            and(
              eq(organisations.provider, identity.provider),
              eq(organisations.externalId, identity.externalOrganisation),
            ),
          )
          .for("no key update"))[0]!;
        const [existing] = yield* tx
          .select()
          .from(users)
          .where(and(eq(users.organisationId, org.id), eq(users.subject, identity.subject)));
        if (existing?.agentRank)
          return yield* Effect.fail(new Error("This subject is registered as an agent."));
        // Under the provider organisation's lock, which linking takes too, so no link can land
        // between this check and the insert.
        if (!existing && claim.boundTo)
          return yield* Effect.fail(
            new Error(
              "Accounts of this provider organisation are linked into another organisation. Invite this person there instead, where they link their account from a paired device.",
            ),
          );
        const username =
          existing?.username ??
          (yield* allocateUsername(
            tx,
            org.id,
            { name: profile.displayName, email: profile.email },
            reserved,
          ));
        if (existing && (!active || role !== "admin"))
          for (const invite of yield* revokeIssuedInvites(tx, existing))
            yield* tx.insert(auditLogs).values({
              ...issuerInviteRevoked(invite, existing.id),
              organisationId: org.id,
              actorType: "integration",
              actorId: null,
              actorDisplayName: "Membership command",
              source: "cli",
            });
        const [member] = existing
          ? yield* tx
              .update(users)
              .set(active ? { ...profile, role, active, username } : { active, username })
              .where(eq(users.id, existing.id))
              .returning()
          : yield* tx
              .insert(users)
              .values({
                organisationId: org.id,
                subject: identity.subject,
                ...profile,
                role,
                active,
                username,
              })
              .returning();
        return member!;
      }),
    );
  });
