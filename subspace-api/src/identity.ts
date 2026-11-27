import { and, eq, gt, isNull, ne, sql } from "drizzle-orm";
import { Context, Effect, Schema } from "effect";
import { type PrincipalIdentity } from "./auth";
import { type Config } from "./config";
import { Database } from "./db";
import {
  fleets,
  initiativeMemberships,
  initiatives,
  memberIdentities,
  organisations,
  pairingInvites,
  ships,
  users,
} from "./schema";
import { allocateUsername, lockUsernames, type NameSource } from "./usernames";

export const AgentRank = Schema.Literals(["rearAdmiral", "captain", "crew"]);
export const memberKind = (member: { agentRank: string | null }) =>
  member.agentRank ? ("agent" as const) : ("human" as const);
export type Member = typeof users.$inferSelect & {
  source?: "rest" | "mcp";
  relayedCrew?: { name: string; model: string } | null;
};
/**
 * The key credential an agent or device token was signed with, when the caller signed in that way.
 */
export interface KeyCredentialUse {
  readonly id: string;
  readonly scopes: readonly ("api" | "irc")[];
}
/** Whether a member administers their organisation: a person with the admin role, never an agent. */
export const isAdministrator = (member: Pick<Member, "role" | "agentRank">) =>
  member.role === "admin" && !member.agentRank;

/**
 * Where a signed-in request's role comes from, decided once at sign-in from how it signed in, and
 * applied again by `requestRole` whenever the caller's row is read again under a lock:
 * - `token`: a person's own Entra token with ENTRA_ADMIN_ROLE configured, whose role sign-in
 *   records in `tokenRole`. A request that signed in this way also carries the role its own token
 *   granted (`tokenGrant`), which caps it when its row is read again (`requestRole`): another
 *   sign-in recording the app role meanwhile never lends it to a token without it;
 * - `lowerOfBoth`: any other sign-in, such as a paired device or a linked account, while
 *   ENTRA_ADMIN_ROLE is configured: the provisioned role, but never above a recorded token role,
 *   so a person whom the provider no longer makes an administrator is not one by another route;
 * - `provisioned`: any sign-in without ENTRA_ADMIN_ROLE, which uses the provisioned role.
 */
export type RoleSource = "token" | "lowerOfBoth" | "provisioned";

/**
 * A member's role from `source`, as their row records it: see `RoleSource`. For a request, which
 * also has its own token's grant, use `requestRole`.
 */
export const effectiveRole = (
  member: Pick<Member, "role" | "tokenRole">,
  source: RoleSource,
): Member["role"] => {
  if (source === "token") return member.tokenRole ?? "member";
  if (source === "lowerOfBoth" && member.tokenRole === "member") return "member";
  return member.role;
};

/**
 * The role source of every sign-in but a person's own Entra token, such as a paired device's or a
 * linked account's, given the deployment's ENTRA_ADMIN_ROLE: see `RoleSource`.
 */
export const otherSignInRoleSource = (policy: {
  readonly entraAdminRole: string | null;
}): RoleSource => (policy.entraAdminRole === null ? "provisioned" : "lowerOfBoth");

/**
 * Where the administrator role came from that let a member invite another person, as an invite
 * records it (`pairing_invites.issuer_authority`): their own Entra token's app role, or the
 * provisioned role. Redemption checks that the issuer still holds the role from there.
 */
export type IssuerAuthority = "entraAppRole" | "provisioned";

/**
 * The authority an administrator's invite records, from the source the request signed in with
 * and the issuer's provisioned role read under the lock: the provisioned role whenever it makes
 * them an administrator, even if their token's app role does too, so that removing
 * ENTRA_ADMIN_ROLE leaves their invites working, as it leaves their role.
 */
export const issuerAuthorityOf = (
  source: RoleSource,
  provisionedRole: Member["role"],
): IssuerAuthority =>
  source === "token" && provisionedRole !== "admin" ? "entraAppRole" : "provisioned";

/**
 * The role source redemption applies to an invite's issuer for the authority it records, under
 * the deployment's ENTRA_ADMIN_ROLE as it is now, or null once that authority no longer exists:
 * the app role with the setting removed. An invite recorded before the column, with none, is
 * judged as the provisioned role is. Never above what the issuer's own sign-in from that source
 * gives them now, and with the provisioned role never above their recorded token role.
 */
export const issuerRoleSource = (
  authority: IssuerAuthority | null,
  entraAdminRole: string | null,
): RoleSource | null =>
  authority === "entraAppRole"
    ? entraAdminRole === null
      ? null
      : "token"
    : otherSignInRoleSource({ entraAdminRole });

/** How a request signed in, for its role: see `RoleSource`. */
export interface RequestRoleSource {
  readonly roleSource: RoleSource;
  /**
   * For the `token` source, the role the request's own verified Entra token granted; null for
   * every other source. Never read back from the member's row, which another sign-in can change.
   */
  readonly tokenGrant: Member["role"] | null;
}

/**
 * A signed-in request's role from `member`'s row as it stands, at sign-in or read again under a
 * lock: `effectiveRole` from the source it signed in with and, for its own Entra token, never
 * above what that token granted. So a recorded withdrawal of the app role applies at once, and a
 * recorded grant by another sign-in does not lift a request whose token lacked it. A `token`
 * request without a grant acts as a member.
 */
export const requestRole = (
  member: Pick<Member, "role" | "tokenRole">,
  request: RequestRoleSource,
): Member["role"] => {
  const role = effectiveRole(member, request.roleSource);
  return request.roleSource === "token" && request.tokenGrant !== "admin" ? "member" : role;
};

/**
 * The member as a request that signed in from `source` acts: with its role (`requestRole`), and
 * for the `token` source the role its own token granted, `tokenGrant`.
 */
export const signedInAs = <M extends Member>(
  member: M,
  source: RoleSource,
  tokenGrant: Member["role"] | null = null,
) => {
  const request = { roleSource: source, tokenGrant: source === "token" ? tokenGrant : null };
  return { ...member, ...request, role: requestRole(member, request) };
};

export class CurrentMember extends Context.Service<
  CurrentMember,
  Member & RequestRoleSource & { tokenExpiresAt: number; keyCredential?: KeyCredentialUse }
>()("subspace/CurrentMember") {}

/**
 * An agent created by enrolment, and a person created to pair their devices, have no account with
 * an identity provider. Their subject is one of these prefixes and their own user ID, which no
 * provider issues, and a provider's token is never matched to such a subject: they sign in only
 * with their key credentials.
 */
export const keyOnlySubjectPrefix = "subspace-agent:";
export const pairedSubjectPrefix = "subspace-member:";
export const keyOnlySubject = (userId: string) => `${keyOnlySubjectPrefix}${userId}`;
export const pairedSubject = (userId: string) => `${pairedSubjectPrefix}${userId}`;
export const isKeyOnlySubject = (subject: string) =>
  subject.startsWith(keyOnlySubjectPrefix) || subject.startsWith(pairedSubjectPrefix);

/** The providers whose accounts a member can link, named in the schema: see `linked-identities.ts`. */
export const linkProviders = memberIdentities.provider.enumValues;
export type LinkProvider = (typeof linkProviders)[number];
export const isLinkProvider = (provider: string): provider is LinkProvider =>
  (linkProviders as readonly string[]).includes(provider);

/**
 * The condition that selects the link of a provider account, or undefined for a provider whose
 * accounts are never linked.
 */
export const linkOf = (identity: PrincipalIdentity) =>
  isLinkProvider(identity.provider)
    ? and(
        eq(memberIdentities.provider, identity.provider),
        eq(memberIdentities.externalOrganisation, identity.externalOrganisation),
        eq(memberIdentities.subject, identity.subject),
      )
    : undefined;

const withAgentPlacement = (row: {
  member: typeof users.$inferSelect;
  fleet: string | null;
  ship: string | null;
}) => ({
  ...row.member,
  agentFleet: row.fleet ?? row.member.agentFleet,
  agentShip: row.ship ?? row.member.agentShip,
});

/**
 * The member a provider account signs in as, and whether it is through a linked account: first
 * the member registered under the account's subject in the provider organisation's own Subspace
 * organisation, then the member that linked the account (see `linked-identities.ts`). Linking
 * refuses an account that a member is registered under, so at most one of the two matches.
 */
export const findSignedInMember = (identity: PrincipalIdentity) =>
  Effect.gen(function* () {
    if (isKeyOnlySubject(identity.subject)) return undefined;
    const db = yield* Database;
    const [registered] = yield* db
      .select({ member: users, fleet: fleets.slug, ship: ships.name })
      .from(users)
      .leftJoin(fleets, eq(fleets.id, users.agentFleetId))
      .leftJoin(ships, eq(ships.id, users.agentShipId))
      .innerJoin(organisations, eq(users.organisationId, organisations.id))
      .where(
        and(
          eq(users.subject, identity.subject),
          sql`not starts_with(${users.subject}, ${keyOnlySubjectPrefix})`,
          sql`not starts_with(${users.subject}, ${pairedSubjectPrefix})`,
          eq(organisations.provider, identity.provider),
          eq(organisations.externalId, identity.externalOrganisation),
        ),
      );
    if (registered) return { member: withAgentPlacement(registered), linked: false };
    const link = linkOf(identity);
    if (!link) return undefined;
    const [linked] = yield* db
      .select({ member: users, fleet: fleets.slug, ship: ships.name })
      .from(memberIdentities)
      .innerJoin(
        users,
        and(
          eq(users.organisationId, memberIdentities.organisationId),
          eq(users.id, memberIdentities.userId),
        ),
      )
      .leftJoin(fleets, eq(fleets.id, users.agentFleetId))
      .leftJoin(ships, eq(ships.id, users.agentShipId))
      .where(link);
    return linked ? { member: withAgentPlacement(linked), linked: true } : undefined;
  });

/** The member a provider account signs in as, registered or through a linked account. */
export const findMember = (identity: PrincipalIdentity) =>
  Effect.map(findSignedInMember(identity), (found) => found?.member);

type Db = (typeof Database)["Service"];

/**
 * Revokes the pending pairing invites a member issued for other people, once the member may no
 * longer issue them: deactivated, or no longer an administrator. Their invites for their own
 * devices stay. Returns the revoked invites, for the caller to audit with `issuerInviteRevoked`.
 */
export const revokeIssuedInvites = (tx: Db, member: Pick<Member, "id" | "organisationId">) =>
  tx
    .update(pairingInvites)
    .set({ revokedAt: new Date() })
    .where(
      and(
        eq(pairingInvites.organisationId, member.organisationId),
        eq(pairingInvites.createdById, member.id),
        ne(pairingInvites.userId, member.id),
        isNull(pairingInvites.redeemedAt),
        isNull(pairingInvites.revokedAt),
        gt(pairingInvites.expiresAt, sql`clock_timestamp()`),
      ),
    )
    .returning({ id: pairingInvites.id, userId: pairingInvites.userId });

/** The audit fields of an invite revoked by `revokeIssuedInvites`. */
export const issuerInviteRevoked = (invite: { id: string; userId: string }, issuerId: string) => ({
  action: "pairingInviteRevoked",
  entityType: "PairingInvite",
  entityId: invite.id,
  summary:
    "Revoked a pending pairing invite: the member who issued it was deactivated or is no longer an administrator.",
  changes: { memberId: invite.userId, issuerId },
});

/**
 * The development sign-in member for an e-mail and organisation, created on first sign-in. A new
 * member's username comes from `usernameFrom`, by default the display name and the e-mail.
 */
export const localMember = (
  email: string,
  displayName: string,
  externalId: string,
  dev: NonNullable<Config["dev"]>,
  reserved: ReadonlySet<string> = new Set(),
  usernameFrom: NameSource = { name: displayName, email },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [newOrg] = yield* tx
          .insert(organisations)
          .values({ provider: "dev", externalId })
          .onConflictDoNothing()
          .returning();
        const org =
          newOrg ??
          (yield* tx
            .select()
            .from(organisations)
            .where(
              and(eq(organisations.provider, "dev"), eq(organisations.externalId, externalId)),
            ))[0]!;
        yield* lockUsernames(tx, org.id);
        const member =
          (yield* tx
            .select()
            .from(users)
            .where(and(eq(users.organisationId, org.id), eq(users.subject, email))))[0] ??
          (yield* tx
            .insert(users)
            .values({
              organisationId: org.id,
              subject: email,
              email,
              displayName,
              role: newOrg ? "admin" : "member",
              username: yield* allocateUsername(tx, org.id, usernameFrom, reserved),
            })
            .returning())[0]!;

        if (newOrg) {
          const [fleet] = yield* tx
            .insert(fleets)
            .values({
              organisationId: org.id,
              slug: "local",
              name: "Local fleet",
              ircHost: dev.ircHost,
              ircPort: dev.ircPort,
              ircTls: false,
            })
            .returning();
          const [initiative] = yield* tx
            .insert(initiatives)
            .values({
              organisationId: org.id,
              name: "Local workspace",
              channels: dev.channels,
              ownerId: member.id,
              fleetId: fleet!.id,
              status: "active",
            })
            .returning();
          yield* tx.insert(initiativeMemberships).values({
            organisationId: org.id,
            initiativeId: initiative!.id,
            userId: member.id,
          });
        }

        return member;
      }),
    );
  });

export const publicProfile = (member: Member) => ({
  userId: member.id,
  kind: memberKind(member),
  displayName: member.displayName,
  email: member.email,
  username: member.username,
  agentName: member.agentRank ? member.displayName : null,
  agentRank: member.agentRank,
  fleet: member.agentFleet,
  ship: member.agentShip,
  fleetId: member.agentFleetId,
  shipId: member.agentShipId,
  isAdmin: isAdministrator(member),
});
