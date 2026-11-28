import { and, asc, eq } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApi, HttpApiEndpoint, HttpApiGroup, HttpApiSchema, OpenApi } from "effect/http-api";
import { lockOrganisation } from "./organisation-lock";
import { recordAudit } from "./audit";
import { type VerifiedIdentity } from "./auth";
import { Database } from "./db";
import { hasConstraint } from "./fleets";
import {
  CurrentMember,
  isAdministrator,
  isKeyOnlySubject,
  isLinkProvider,
  linkProviders,
  type LinkProvider,
} from "./identity";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { confirmCaller, managedPerson, type Caller } from "./people";
import {
  endpointErrors,
  identityConflict,
  identityTokenUnusable,
  problem,
  problemTypes,
} from "./problems";
import {
  lockProviderOrganisation,
  providerOrganisationClaim,
  type ProviderOrganisationClaim,
} from "./provider-organisations";
import { memberIdentities } from "./schema";
import { dateTime, Timestamp, Uuid } from "./validation";

/**
 * Provider accounts linked to a member. A person who signs in with paired devices links a WorkOS
 * or Entra account from one of those devices, giving a fresh access token of that account, so
 * that one request proves both who the member is (the device's signature) and that they hold the
 * account (the provider's signature). From then on the account signs in as that member, keeping
 * their authorship and IRC account, beside their devices.
 *
 * What stops an account being linked to the wrong member:
 * - the account names at most one member in the database: linking refuses an account that is
 *   linked already, or that a member is registered under in the provider organisation's own
 *   Subspace organisation;
 * - the provider token must have been issued in the last ten minutes, which narrows the use of a
 *   leaked token, and the request must be signed by one of the member's own devices;
 * - linking the first account of a provider organisation into an organisation (other than the
 *   provider organisation's own) lets that provider organisation's accounts be linked there by
 *   anyone holding one of its accounts and a device, so an administrator does it, with an account
 *   whose Entra token says it is a member of the tenant (`acct` 0); after that, members link their own accounts of that provider
 *   organisation, and the host's operator can release it with the provider bindings command
 *   (`releaseProviderOrganisation` in `provider-organisations.ts`);
 * - a provider organisation is linked into at most one Subspace organisation, and never into
 *   another one while its own Subspace organisation has members; once linked, its unknown people
 *   are not registered in an organisation of its own (see `registration.ts`).
 */

type Verify = (token: string) => Promise<VerifiedIdentity | null>;

/** How long after its issue a provider token links an account, in seconds. */
export const linkTokenMaxAge = 600;

export const LinkedIdentity = Schema.Struct({
  id: Uuid,
  memberId: Uuid,
  provider: Schema.Literals(linkProviders),
  /** The WorkOS organisation ID or the Entra tenant ID. */
  externalOrganisation: Schema.String,
  /** The WorkOS user ID or the Entra object ID. */
  subject: Schema.String,
  createdAt: Timestamp,
}).annotate({ identifier: "LinkedIdentity" });

const IdentityLink = Schema.Struct({
  /** A WorkOS or Entra access token for this API, issued within the last ten minutes. */
  token: Schema.String.check(Schema.isMinLength(1), Schema.isMaxLength(16_384)),
}).annotate({ identifier: "IdentityLink" });

const identityDto = (row: typeof memberIdentities.$inferSelect) => ({
  id: row.id,
  memberId: row.userId,
  provider: row.provider,
  externalOrganisation: row.externalOrganisation,
  subject: row.subject,
  createdAt: dateTime(row.createdAt),
});

const unusableToken = () =>
  identityTokenUnusable(
    `This token cannot link an account. Sign in with WorkOS or Entra again and send the new access token within ${linkTokenMaxAge / 60} minutes.`,
  );

/** The provider account a token proves, if it is a fresh WorkOS or Entra token for this API. */
const verifiedAccount = (verify: Verify, token: string) =>
  Effect.gen(function* () {
    const identity = yield* Effect.promise(() => verify(token));
    const now = Math.floor(Date.now() / 1000);
    if (
      !identity ||
      !isLinkProvider(identity.provider) ||
      isKeyOnlySubject(identity.subject) ||
      identity.issuedAt === undefined ||
      identity.issuedAt > now + 60 ||
      now - identity.issuedAt > linkTokenMaxAge
    )
      return yield* Effect.fail(unusableToken());
    return { ...identity, provider: identity.provider };
  });

/**
 * The refusal, if any, of linking `account` to the caller, from its provider organisation's claim
 * (see `providerOrganisationClaim`). An account linked or registered already is refused, unless it
 * is the caller's own link. A provider organisation bound to another organisation, or with its own
 * organisation's members, is refused. Its first link into an organisation other than its own binds
 * it there, so it needs an administrator, with an account that is the provider organisation's own:
 * for Entra, one whose token's `acct` claim says it is a member of the tenant, never a guest's,
 * whose tenant is not theirs to bind.
 */
const linkRefusal = (
  claim: ProviderOrganisationClaim,
  caller: Caller,
  account: VerifiedIdentity & { provider: LinkProvider },
) => {
  if (claim.signedIn)
    return identityConflict(
      claim.signedIn.linked
        ? "This account is linked to another member."
        : claim.signedIn.member.id === caller.id
          ? "This account is already your sign-in: it signs you in as this member, so there is nothing to link."
          : "This account is registered as a member already. Sign in with it there instead.",
    );
  if (claim.boundTo && claim.boundTo !== caller.organisationId)
    return identityConflict(
      "Accounts of this provider organisation are linked into another organisation.",
    );
  // The caller's organisation is the provider organisation's own: its members link freely.
  if (claim.own?.id === caller.organisationId) return undefined;
  if (claim.own?.hasMembers)
    return identityConflict(
      "This provider organisation has its own Subspace organisation with members. Sign in there instead.",
    );
  // Bound to the caller's organisation already: its members link their own accounts.
  if (claim.boundTo) return undefined;
  if (!isAdministrator(caller))
    return problem(
      403,
      "An administrator links the first account of this provider organisation, which lets its accounts be linked here.",
    );
  // Only a token that says outright that its account is the tenant's member binds the tenant:
  // without `acct`, a guest whose home tenant is this one looks like a member.
  if (account.provider === "entra" && !account.claims?.tenantMember)
    return problem(
      403,
      "Only a member of the tenant links its first account here, not a guest, and the access token must say so with the optional acct claim. Add acct to the API's access token and use a member's account.",
    );
  return undefined;
};

/**
 * Links the provider account a fresh token proves to the caller, who must sign the request with
 * one of their paired devices, still unrevoked under the organisation's lock. Returns the existing
 * link when the caller has linked that account already.
 */
const linkIdentity = (signedIn: Caller, token: string, verify: Verify) =>
  Effect.gen(function* () {
    if (!signedIn.keyCredential || signedIn.agentRank)
      return yield* Effect.fail(
        problem(403, "Link a provider account from one of your paired devices."),
      );
    const account = yield* verifiedAccount(verify, token);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const claim = yield* providerOrganisationClaim(tx, account);
        yield* lockOrganisation(tx, signedIn.organisationId);
        // Under the lock that device revocation takes, so a device revoked before this commits
        // links nothing. A link committed earlier is the member's, not the device's: revoking the
        // device leaves it, and only unlinking it ends it.
        const caller = yield* confirmCaller(tx, signedIn);
        if (claim.link?.userId === caller.id) return identityDto(claim.link);
        const refusal = linkRefusal(claim, caller, account);
        if (refusal) return yield* Effect.fail(refusal);
        const [row] = yield* tx
          .insert(memberIdentities)
          .values({
            organisationId: caller.organisationId,
            userId: caller.id,
            provider: account.provider,
            externalOrganisation: account.externalOrganisation,
            subject: account.subject,
          })
          .returning()
          .pipe(
            Effect.catchIf(
              (error) => hasConstraint(error, "member_identities_one_per_provider"),
              () =>
                Effect.fail(
                  identityConflict(
                    `You have linked another ${account.provider === "entra" ? "Entra" : "WorkOS"} account. Unlink it first.`,
                  ),
                ),
            ),
            Effect.catchIf(
              (error) => hasConstraint(error, "member_identities_account"),
              () => Effect.fail(identityConflict("This account is linked to another member.")),
            ),
          );
        yield* recordAudit(tx, caller, {
          action: "memberIdentityLinked",
          entityType: "MemberIdentity",
          entityId: row!.id,
          summary: "Linked a provider account from a paired device.",
          changes: {
            memberId: caller.id,
            provider: row!.provider,
            externalOrganisation: row!.externalOrganisation,
            subject: row!.subject,
            credentialId: caller.keyCredential!.id,
          },
        });
        return identityDto(row!);
      }),
    );
  });

const listIdentities = (caller: Caller, memberId: string | null) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const owner = yield* managedPerson(db, caller, memberId);
    const rows = yield* db
      .select()
      .from(memberIdentities)
      .where(
        and(
          eq(memberIdentities.organisationId, caller.organisationId),
          eq(memberIdentities.userId, owner),
        ),
      )
      .orderBy(asc(memberIdentities.provider), asc(memberIdentities.id));
    return { items: rows.map(identityDto) };
  });

/**
 * Unlinks a provider account: it no longer signs in as the member, from its next request. The
 * member's devices and other accounts keep working. Unlinking the last account of a provider
 * organisation releases its binding, so it takes the provider organisation's lock, which linking
 * holds while it reads the claim, before the organisation's: a link never acts on a claim that an
 * unlink changed under it.
 */
const unlinkIdentity = (signedIn: Caller, memberId: string | null, identityId: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        // The link's provider organisation, read without a lock to know which lock to take: the
        // link is read again, and deleted, under both locks.
        const [candidate] = yield* tx
          .select({
            provider: memberIdentities.provider,
            externalOrganisation: memberIdentities.externalOrganisation,
          })
          .from(memberIdentities)
          .where(
            and(
              eq(memberIdentities.organisationId, signedIn.organisationId),
              eq(memberIdentities.id, identityId),
            ),
          );
        if (candidate) yield* lockProviderOrganisation(tx, candidate);
        yield* lockOrganisation(tx, signedIn.organisationId);
        const caller = yield* confirmCaller(tx, signedIn);
        const owner = yield* managedPerson(tx, caller, memberId);
        const [row] = yield* tx
          .delete(memberIdentities)
          .where(
            and(
              eq(memberIdentities.organisationId, caller.organisationId),
              eq(memberIdentities.userId, owner),
              eq(memberIdentities.id, identityId),
            ),
          )
          .returning();
        if (!row)
          return yield* Effect.fail(
            problem(404, "Linked account was not found. Use an ID from the member's accounts."),
          );
        yield* recordAudit(tx, caller, {
          action: "memberIdentityUnlinked",
          entityType: "MemberIdentity",
          entityId: row.id,
          summary:
            owner === caller.id
              ? "Unlinked one of their provider accounts."
              : "Unlinked a member's provider account.",
          changes: {
            memberId: owner,
            provider: row.provider,
            externalOrganisation: row.externalOrganisation,
            subject: row.subject,
          },
        });
        return identityDto(row);
      }),
    );
  });

const identities = Schema.Struct({ items: Schema.Array(LinkedIdentity) });
const memberParams = { memberId: Uuid };
const untyped = "Its problems are about:blank.";

/** Linked provider accounts, on the main listener only. */
export const linkedIdentityGroup = HttpApiGroup.make("linkedIdentities")
  .add(
    HttpApiEndpoint.post("linkIdentity", "/me/identities", {
      payload: IdentityLink,
      success: LinkedIdentity.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `Links the WorkOS or Entra account that token proves to the caller, so that it signs in as the caller from then on, keeping their authorship and IRC account (201; the existing link if the caller has linked it already). The request must be signed by one of the caller's paired devices, and token must be an access token for this API issued within the last ${linkTokenMaxAge / 60} minutes (400 with type ${problemTypes.identityTokenUnusable} otherwise). The first account of a provider organisation linked into an organisation other than its own needs an administrator, with an account that is not an Entra guest's: for Entra, the token must carry the optional acct claim, 0, for a member of the tenant (403). 409 with type ${problemTypes.identityConflict} when the account is linked to or registered as another member, the caller has linked another account of that provider, or the provider organisation is linked into another organisation or has its own organisation with members. The other problems, including 400 for a malformed body, are about:blank.`,
    ),
    HttpApiEndpoint.get("listMyIdentities", "/me/identities", {
      success: identities,
      error: endpointErrors,
    }).annotate(OpenApi.Description, `The provider accounts linked to the caller. ${untyped}`),
    HttpApiEndpoint.delete("unlinkMyIdentity", "/me/identities/:identityId", {
      params: { identityId: Uuid },
      success: LinkedIdentity,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `Unlinks one of the caller's provider accounts: from its next request it no longer signs in as the caller. Their devices and other accounts keep working. ${untyped}`,
    ),
    HttpApiEndpoint.get("listMemberIdentities", "/members/:memberId/identities", {
      params: memberParams,
      success: identities,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator lists the provider accounts linked to a person. ${untyped}`,
    ),
    HttpApiEndpoint.delete("unlinkMemberIdentity", "/members/:memberId/identities/:identityId", {
      params: { ...memberParams, identityId: Uuid },
      success: LinkedIdentity,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator unlinks a provider account from a person, such as one linked by mistake: from its next request it no longer signs in as them. ${untyped}`,
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const linkedIdentityHandlers =
  (verify: Verify) =>
  (
    handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<
      typeof linkedIdentityGroup
    >,
  ) =>
    handlers
      .handle("linkIdentity", ({ payload }) =>
        handleDb(
          Effect.flatMap(CurrentMember, (caller) => linkIdentity(caller, payload.token, verify)),
        ),
      )
      .handle("listMyIdentities", () =>
        handleDb(Effect.flatMap(CurrentMember, (caller) => listIdentities(caller, null))),
      )
      .handle("unlinkMyIdentity", ({ params }) =>
        handleDb(
          Effect.flatMap(CurrentMember, (caller) =>
            unlinkIdentity(caller, null, params.identityId),
          ),
        ),
      )
      .handle("listMemberIdentities", ({ params }) =>
        handleDb(
          Effect.flatMap(CurrentMember, (caller) => listIdentities(caller, params.memberId)),
        ),
      )
      .handle("unlinkMemberIdentity", ({ params }) =>
        handleDb(
          Effect.flatMap(CurrentMember, (caller) =>
            unlinkIdentity(caller, params.memberId, params.identityId),
          ),
        ),
      );
