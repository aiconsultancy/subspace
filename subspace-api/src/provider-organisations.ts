import { and, asc, eq, ne, sql } from "drizzle-orm";
import { Effect } from "effect";
import { lockOrganisation } from "./agent-credentials";
import { type PrincipalIdentity } from "./auth";
import { type Provider } from "./config";
import { Database } from "./db";
import {
  findSignedInMember,
  isLinkProvider,
  type LinkProvider,
  linkOf,
  type Member,
} from "./identity";
import { auditLogs, memberIdentities, organisations, users } from "./schema";

/**
 * Which Subspace organisation a provider organisation's accounts sign in to. They sign in to the
 * provider organisation's own, or are linked into one other: linking the first account into an
 * organisation other than its own binds the provider organisation to it (see
 * `linked-identities.ts`), and its unknown people are then registered nowhere, which would split
 * them in two. Registration, the membership command and linking all read that with
 * `providerOrganisationClaim`; only unlinking every account there releases it, by its members or
 * by the host's operator with the provider bindings command (`scripts/provider-bindings.ts`).
 */

/**
 * Serialises, across organisations, every change to which Subspace organisation a provider
 * organisation's accounts sign in to: linking or unlinking an account, releasing the provider
 * organisation, and registering a person of it. Taken before any organisation's row lock, so that
 * it never waits while holding one.
 */
export const lockProviderOrganisation = (
  db: (typeof Database)["Service"],
  identity: Pick<PrincipalIdentity, "provider" | "externalOrganisation">,
) =>
  db.execute(
    sql`select pg_advisory_xact_lock(hashtextextended(${`subspace-provider-organisation:${identity.provider}:${identity.externalOrganisation}`}, 0))`,
  );

/**
 * Where a provider account, and its provider organisation, stand: the one reading that
 * registration, the membership command and linking each turn into their own answer.
 */
export interface ProviderOrganisationClaim {
  /** The account's own link, if a member linked it. */
  readonly link: typeof memberIdentities.$inferSelect | undefined;
  /** The member the account signs in as, registered or through its link: see `findSignedInMember`. */
  readonly signedIn: { readonly member: Member; readonly linked: boolean } | undefined;
  /** The Subspace organisation, other than its own, that the provider organisation is bound to. */
  readonly boundTo: string | null;
  /** The provider organisation's own Subspace organisation, if it exists, and whether it has members. */
  readonly own: { readonly id: string; readonly hasMembers: boolean } | null;
}

/**
 * Takes the provider organisation's lock, then reads its claim (see `ProviderOrganisationClaim`)
 * in the caller's transaction, so that nothing it reports can change until the transaction ends.
 */
export const providerOrganisationClaim = (
  tx: (typeof Database)["Service"],
  identity: PrincipalIdentity,
) =>
  Effect.gen(function* () {
    yield* lockProviderOrganisation(tx, identity);
    const signedIn = yield* findSignedInMember(identity).pipe(Effect.provideService(Database, tx));
    const [own] = yield* tx
      .select({
        id: organisations.id,
        hasMembers: sql<boolean>`exists (select 1 from ${users} where ${users.organisationId} = ${organisations.id})`,
      })
      .from(organisations)
      .where(
        and(
          eq(organisations.provider, identity.provider),
          eq(organisations.externalId, identity.externalOrganisation),
        ),
      );
    // Only WorkOS and Entra accounts are linked, so only their provider organisations are bound.
    const provider = isLinkProvider(identity.provider) ? identity.provider : null;
    const [linked] = provider
      ? yield* tx.select().from(memberIdentities).where(linkOf(identity)!)
      : [];
    const [bound] = provider
      ? yield* tx
          .select({ organisationId: memberIdentities.organisationId })
          .from(memberIdentities)
          .where(
            and(
              eq(memberIdentities.provider, provider),
              eq(memberIdentities.externalOrganisation, identity.externalOrganisation),
              own ? ne(memberIdentities.organisationId, own.id) : undefined,
            ),
          )
          .limit(1)
      : [];
    return {
      link: linked,
      signedIn,
      boundTo: bound?.organisationId ?? null,
      own: own ?? null,
    } satisfies ProviderOrganisationClaim;
  });

/** A provider organisation's accounts linked in a Subspace organisation. */
export interface ProviderBinding {
  readonly organisation: { readonly provider: Provider; readonly externalId: string };
  readonly provider: LinkProvider;
  readonly externalOrganisation: string;
  readonly accounts: number;
  /** Whether the Subspace organisation is the provider organisation's own, which binds nothing. */
  readonly own: boolean;
}

/** Why the provider bindings command refused, in words safe to print on the API host. */
export class ReleaseRefused extends Error {}

/** Every provider organisation with accounts linked in a Subspace organisation. */
export const providerOrganisationBindings = Effect.gen(function* () {
  const db = yield* Database;
  const rows = yield* db
    .select({
      organisationProvider: organisations.provider,
      organisationId: organisations.externalId,
      provider: memberIdentities.provider,
      externalOrganisation: memberIdentities.externalOrganisation,
      accounts: sql<number>`count(*)::int`,
    })
    .from(memberIdentities)
    .innerJoin(organisations, eq(organisations.id, memberIdentities.organisationId))
    .groupBy(
      organisations.provider,
      organisations.externalId,
      memberIdentities.provider,
      memberIdentities.externalOrganisation,
    )
    .orderBy(
      asc(organisations.provider),
      asc(organisations.externalId),
      asc(memberIdentities.provider),
      asc(memberIdentities.externalOrganisation),
    );
  return rows.map((row): ProviderBinding => ({
    organisation: { provider: row.organisationProvider, externalId: row.organisationId },
    provider: row.provider,
    externalOrganisation: row.externalOrganisation,
    accounts: row.accounts,
    own:
      row.organisationProvider === row.provider && row.organisationId === row.externalOrganisation,
  }));
});

/**
 * Releases a provider organisation from a Subspace organisation: unlinks every account of it
 * there, auditing each, so that its accounts sign in to nothing there and its unknown people are
 * registered in its own organisation again. Takes the provider organisation's lock and then the
 * Subspace organisation's, as linking does. Returns the unlinked accounts' member IDs.
 */
export const releaseProviderOrganisation = (
  organisation: { provider: Provider; externalId: string },
  account: { provider: LinkProvider; externalOrganisation: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockProviderOrganisation(tx, account);
        const [org] = yield* tx
          .select({ id: organisations.id })
          .from(organisations)
          .where(
            and(
              eq(organisations.provider, organisation.provider),
              eq(organisations.externalId, organisation.externalId),
            ),
          );
        if (!org)
          return yield* Effect.fail(
            new ReleaseRefused(
              `There is no organisation ${organisation.provider}:${organisation.externalId}.`,
            ),
          );
        yield* lockOrganisation(tx, org.id);
        const unlinked = yield* tx
          .delete(memberIdentities)
          .where(
            and(
              eq(memberIdentities.organisationId, org.id),
              eq(memberIdentities.provider, account.provider),
              eq(memberIdentities.externalOrganisation, account.externalOrganisation),
            ),
          )
          .returning();
        if (unlinked.length)
          yield* tx.insert(auditLogs).values(
            unlinked.map((row) => ({
              organisationId: org.id,
              actorType: "integration" as const,
              actorId: null,
              actorDisplayName: "Provider bindings command",
              source: "cli" as const,
              action: "memberIdentityUnlinked",
              entityType: "MemberIdentity",
              entityId: row.id,
              summary:
                "Unlinked a provider account on the API host, releasing its provider organisation from this organisation.",
              changes: {
                memberId: row.userId,
                provider: row.provider,
                externalOrganisation: row.externalOrganisation,
                subject: row.subject,
              },
            })),
          );
        return unlinked.map((row) => row.userId);
      }),
    );
  });
