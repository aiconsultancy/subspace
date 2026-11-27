import { randomUUID } from "node:crypto";
import { and, desc, eq, sql } from "drizzle-orm";
import { Context, Effect, Layer, Redacted, Result, Schema } from "effect";
import { HttpServerRequest, HttpServerResponse } from "effect/http";
import {
  HttpApi,
  HttpApiEndpoint,
  HttpApiGroup,
  HttpApiMiddleware,
  HttpApiSchema,
  HttpApiSecurity,
  OpenApi,
} from "effect/http-api";
import {
  activeCredential,
  insertCredential,
  lockOrganisation,
  readCertificate,
} from "./agent-credentials";
import { CertificatePem, CredentialScopes } from "./agent-keys";
import {
  AgentName,
  checkAgentNames,
  findFleetOfficer,
  IrcNick,
  registerFleetOfficer,
} from "./agents";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { findFleet, FleetRef, ShipName } from "./fleets";
import { CurrentMember, keyOnlySubject, type Member } from "./identity";
import { mintOneTimeToken, readOneTimeToken, sameHash } from "./one-time-tokens";
import {
  BearerAuth,
  databaseFailure,
  handleDb,
  ValidationErrors,
  versionedApi,
} from "./middleware";
import {
  endpointErrors,
  InternalError,
  problem,
  Unauthorised,
  validationProblem,
} from "./problems";
import { auditLogs, enrolmentTokens, fleets, ships } from "./schema";
import { dateTime, dateTimeOrNull, Line, Timestamp, Uuid } from "./validation";

/**
 * Fleet enrolment. An administrator issues a token for one fleet; the fleet's gateway, with no
 * other login, registers that fleet's rear admiral and captains and their key credentials with
 * it. A token is `senr1.` and the unpadded base64url encoding of 48 bytes: its 16-byte ID and 32
 * random bytes. Only the SHA-256 of the whole text is stored, and it is compared in constant time.
 */
export const enrolmentTokenPrefix = "senr1.";
export const defaultEnrolmentLifetime = 3600;
export const maxEnrolmentLifetime = 7 * 24 * 3600;
/** Enough for a rear admiral and the installer's twelve captains, with room to spare. */
export const defaultEnrolmentUses = 16;
export const maxEnrolmentUses = 100;

type TokenRow = typeof enrolmentTokens.$inferSelect;

/** A new token for the row `id`, and the hash to store. */
export const mintEnrolmentToken = (id: string) => mintOneTimeToken(enrolmentTokenPrefix, id);

/** The row ID a token names and its hash, if it has the token's shape. */
export const readEnrolmentToken = (token: string) => readOneTimeToken(enrolmentTokenPrefix, token);

/** Why a token whose secret is right no longer registers anything, if it does not. */
function spent(row: TokenRow, now = Date.now()) {
  if (row.revokedAt) return "This enrolment token was revoked.";
  if (row.expiresAt.valueOf() <= now) return "This enrolment token has expired.";
  return undefined;
}

/** The fleet and organisation an accepted enrolment token registers agents in. */
export class EnrolmentGrant extends Context.Service<
  EnrolmentGrant,
  { readonly tokenId: string; readonly organisationId: string; readonly fleetId: string }
>()("subspace/EnrolmentGrant") {}

/** Authorises the enrolment route by an enrolment token alone, never by a member's token. */
export class EnrolmentAuth extends HttpApiMiddleware.Service<
  EnrolmentAuth,
  { provides: EnrolmentGrant }
>()("subspace/EnrolmentAuth", {
  security: { enrolmentToken: HttpApiSecurity.bearer },
  error: [Unauthorised, InternalError],
}) {}

const invalidToken = "A valid enrolment token is required.";

export const enrolmentAuth = Layer.effect(
  EnrolmentAuth,
  Effect.map(Database, (db) =>
    EnrolmentAuth.of({
      enrolmentToken: (next, { credential }) =>
        Effect.gen(function* () {
          const request = yield* HttpServerRequest.HttpServerRequest;
          if (!/^Bearer [^\s]+$/iu.test(request.headers.authorization ?? ""))
            return yield* Effect.fail(problem(401, invalidToken));
          const read = readEnrolmentToken(Redacted.value(credential));
          if (!read) return yield* Effect.fail(problem(401, invalidToken));
          const [row] = yield* db
            .select()
            .from(enrolmentTokens)
            .where(eq(enrolmentTokens.id, read.id))
            .pipe(
              Effect.tapError((error) =>
                Effect.logError("The enrolment token could not be read", databaseFailure(error)),
              ),
              Effect.mapError(() => problem(500, "The enrolment token could not be checked.")),
            );
          if (!row || !sameHash(read.hash, row.tokenHash))
            return yield* Effect.fail(problem(401, invalidToken));
          const reason = spent(row);
          if (reason) return yield* Effect.fail(problem(401, reason));
          return yield* next.pipe(
            Effect.provideService(EnrolmentGrant, {
              tokenId: row.id,
              organisationId: row.organisationId,
              fleetId: row.fleetId,
            }),
            Effect.map(HttpServerResponse.setHeader("Cache-Control", "no-store")),
          );
        }),
    }),
  ),
);

const EnrolmentTokenSummary = Schema.Struct({
  id: Uuid,
  fleetId: Uuid,
  expiresAt: Timestamp,
  maxUses: Schema.Int,
  uses: Schema.Int,
  createdAt: Timestamp,
  createdByUserId: Uuid,
  revokedAt: Schema.NullOr(Timestamp),
  revokedByUserId: Schema.NullOr(Uuid),
}).annotate({ identifier: "EnrolmentToken" });

const IssuedEnrolmentToken = Schema.Struct({
  id: Uuid,
  token: Schema.String,
  expiresAt: Timestamp,
  maxUses: Schema.Int,
}).annotate({ identifier: "IssuedEnrolmentToken" });

const NewEnrolmentToken = Schema.Struct({
  expiresInSeconds: Schema.optional(
    Schema.Int.check(Schema.isBetween({ minimum: 60, maximum: maxEnrolmentLifetime })),
  ),
  maxUses: Schema.optional(
    Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: maxEnrolmentUses })),
  ),
}).annotate({ identifier: "NewEnrolmentToken" });

const tokenDto = (row: TokenRow) => ({
  id: row.id,
  fleetId: row.fleetId,
  expiresAt: dateTime(row.expiresAt),
  maxUses: row.maxUses,
  uses: row.uses,
  createdAt: dateTime(row.createdAt),
  createdByUserId: row.createdById,
  revokedAt: dateTimeOrNull(row.revokedAt),
  revokedByUserId: row.revokedById,
});

const adminOnly = (member: Member) =>
  member.role !== "admin" || member.agentRank
    ? Effect.fail(problem(403, "Enrolment tokens are managed by an organisation administrator."))
    : Effect.void;

const fleetNotFound = () => problem(404, "Fleet not found.");

type Db = (typeof Database)["Service"];

/** Insert a fleet grant within the caller's transaction. The caller audits its own provenance. */
export const insertEnrolmentToken = (
  tx: Db,
  input: Pick<TokenRow, "organisationId" | "fleetId" | "createdById" | "maxUses"> & {
    lifetime: number;
  },
) =>
  Effect.gen(function* () {
    const { lifetime, ...fields } = input;
    const id = randomUUID();
    const { token, hash } = mintEnrolmentToken(id);
    const [row] = yield* tx
      .insert(enrolmentTokens)
      .values({ ...fields, id, tokenHash: hash, expiresAt: new Date(Date.now() + lifetime * 1000) })
      .returning();
    return { id, token, expiresAt: dateTime(row!.expiresAt), maxUses: row!.maxUses };
  });

const issueToken = (member: Member, reference: string, payload: typeof NewEnrolmentToken.Type) =>
  Effect.gen(function* () {
    yield* adminOnly(member);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const [fleet] = yield* findFleet(tx, member.organisationId, reference);
        if (!fleet) return yield* Effect.fail(fleetNotFound());
        const grant = yield* insertEnrolmentToken(tx, {
          organisationId: member.organisationId,
          fleetId: fleet.id,
          createdById: member.id,
          lifetime: payload.expiresInSeconds ?? defaultEnrolmentLifetime,
          maxUses: payload.maxUses ?? defaultEnrolmentUses,
        });
        yield* recordAudit(tx, member, {
          action: "enrolmentTokenCreated",
          entityType: "EnrolmentToken",
          entityId: grant.id,
          summary: "Issued an enrolment token for a fleet.",
          changes: {
            fleetId: fleet.id,
            expiresAt: grant.expiresAt,
            maxUses: grant.maxUses,
          },
        });
        return grant;
      }),
    );
  });

const listTokens = (member: Member, reference: string) =>
  Effect.gen(function* () {
    yield* adminOnly(member);
    const db = yield* Database;
    const [fleet] = yield* findFleet(db, member.organisationId, reference);
    if (!fleet) return yield* Effect.fail(fleetNotFound());
    const rows = yield* db
      .select()
      .from(enrolmentTokens)
      .where(
        and(
          eq(enrolmentTokens.organisationId, member.organisationId),
          eq(enrolmentTokens.fleetId, fleet.id),
        ),
      )
      .orderBy(desc(enrolmentTokens.createdAt), desc(enrolmentTokens.id));
    return { items: rows.map(tokenDto) };
  });

const revokeToken = (member: Member, reference: string, tokenId: string) =>
  Effect.gen(function* () {
    yield* adminOnly(member);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, member.organisationId);
        const [fleet] = yield* findFleet(tx, member.organisationId, reference);
        if (!fleet) return yield* Effect.fail(fleetNotFound());
        const [old] = yield* tx
          .select()
          .from(enrolmentTokens)
          .where(
            and(
              eq(enrolmentTokens.organisationId, member.organisationId),
              eq(enrolmentTokens.fleetId, fleet.id),
              eq(enrolmentTokens.id, tokenId),
            ),
          )
          .for("update");
        if (!old) return yield* Effect.fail(problem(404, "Enrolment token not found."));
        if (old.revokedAt) return tokenDto(old);
        const [row] = yield* tx
          .update(enrolmentTokens)
          .set({ revokedAt: new Date(), revokedById: member.id })
          .where(eq(enrolmentTokens.id, old.id))
          .returning();
        yield* recordAudit(tx, member, {
          action: "enrolmentTokenRevoked",
          entityType: "EnrolmentToken",
          entityId: row!.id,
          summary: "Revoked an enrolment token.",
          changes: { fleetId: fleet.id, uses: row!.uses },
        });
        return tokenDto(row!);
      }),
    );
  });

const Enrolment = Schema.Struct({
  name: AgentName,
  rank: Schema.Literals(["rearAdmiral", "captain"]),
  ship: Schema.optional(Schema.NullOr(ShipName)),
  model: Line(100),
  ircNick: Schema.optional(Schema.NullOr(IrcNick)),
  certificate: CertificatePem,
  scopes: CredentialScopes,
}).annotate({ identifier: "AgentEnrolment" });

const Enrolled = Schema.Struct({
  agentId: Uuid,
  credentialId: Uuid,
  fingerprint: Schema.String,
}).annotate({ identifier: "AgentEnrolled" });

/** An audit row written by an enrolment token rather than a member. */
const tokenAudit = (
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
  actorDisplayName: "Enrolment token",
  source: "rest" as const,
});

/** What an enrolment asks for, without its key: the name, rank, ship and IRC nick. */
type Wanted = Pick<typeof Enrolment.Type, "name" | "rank" | "ship" | "ircNick">;

/** The ship a rank takes: a captain's own (required), none for a rear admiral (400 otherwise). */
const shipOf = (payload: Wanted) => {
  const shipName = payload.ship?.trim() || null;
  if (payload.rank === "captain" && !shipName)
    return Effect.fail(validationProblem("ship", "A captain needs a ship."));
  if (payload.rank === "rearAdmiral" && shipName)
    return Effect.fail(validationProblem("ship", "A rear admiral must not supply a ship."));
  return Effect.succeed(shipName);
};

/**
 * Registers an agent of the token's fleet, or a credential of an existing one, under the
 * organisation's lock and the token's row lock. The agent is the active agent of that name in the
 * token's fleet, with the requested rank (findFleetOfficer); a new agent's name and IRC nick must
 * be free in the organisation (checkAgentNames). The same certificate again for the same agent
 * and scopes is answered 200 with the same IDs and uses nothing; any other registration uses one
 * of the token's uses.
 */
const enrol = (grant: (typeof EnrolmentGrant)["Service"], payload: typeof Enrolment.Type) =>
  Effect.gen(function* () {
    const shipName = yield* shipOf(payload);
    const certificate = yield* readCertificate(payload.certificate);
    const scopes = [...payload.scopes].sort();
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, grant.organisationId);
        const [token] = yield* tx
          .select()
          .from(enrolmentTokens)
          .where(
            and(
              eq(enrolmentTokens.organisationId, grant.organisationId),
              eq(enrolmentTokens.id, grant.tokenId),
            ),
          )
          .for("update");
        if (!token) return yield* Effect.fail(problem(401, invalidToken));
        const reason = spent(token);
        if (reason) return yield* Effect.fail(problem(401, reason));
        const [fleet] = yield* tx
          .select()
          .from(fleets)
          .where(and(eq(fleets.organisationId, token.organisationId), eq(fleets.id, token.fleetId)))
          .for("share");
        if (!fleet) return yield* Effect.fail(problem(401, invalidToken));

        // Agent names are unique in an organisation, so at most one agent has this one.
        const existing = yield* findFleetOfficer(tx, fleet, payload);

        const active = yield* activeCredential(tx, certificate.fingerprint);
        if (active) {
          const same =
            existing &&
            active.organisationId === token.organisationId &&
            active.userId === existing.id;
          if (!same)
            return yield* Effect.fail(problem(409, "This certificate is already registered."));
          if (active.scopes.join(",") !== scopes.join(","))
            return yield* Effect.fail(
              problem(409, "This certificate is registered with other scopes."),
            );
          return HttpServerResponse.jsonUnsafe(
            { agentId: existing.id, credentialId: active.id, fingerprint: active.fingerprint },
            { status: 200 },
          );
        }
        if (token.uses >= token.maxUses)
          return yield* Effect.fail(problem(401, "This enrolment token has been used up."));

        let agentId = existing?.id;
        if (!existing) {
          let ship: typeof ships.$inferSelect | undefined;
          if (shipName) {
            yield* tx
              .insert(ships)
              .values({ organisationId: fleet.organisationId, fleetId: fleet.id, name: shipName })
              .onConflictDoNothing();
            [ship] = yield* tx
              .select()
              .from(ships)
              .where(
                and(eq(ships.fleetId, fleet.id), sql`lower(${ships.name}) = lower(${shipName})`),
              )
              .for("share");
            if (!ship)
              return yield* Effect.fail(
                problem(409, "The ship name changed. Retry the enrolment."),
              );
          }
          // The administrator who issued the token is the registered sponsor of new officers.
          agentId = yield* registerFleetOfficer(tx, fleet, ship, payload, token.createdById);
          yield* tx.insert(auditLogs).values(
            tokenAudit(token.organisationId, {
              action: "agentEnrolled",
              entityType: "Agent",
              entityId: agentId,
              summary: "Registered an agent with an enrolment token.",
              changes: { enrolmentTokenId: token.id, fleetId: fleet.id, rank: payload.rank },
            }),
          );
        }

        const credential = yield* insertCredential(tx, {
          organisationId: token.organisationId,
          userId: agentId!,
          certificate,
          scopes,
          enrolmentTokenId: token.id,
        });
        const [used] = yield* tx
          .update(enrolmentTokens)
          .set({ uses: sql`${enrolmentTokens.uses} + 1` })
          .where(eq(enrolmentTokens.id, token.id))
          .returning({ uses: enrolmentTokens.uses });
        yield* tx.insert(auditLogs).values([
          tokenAudit(token.organisationId, {
            action: "agentCredentialAdded",
            entityType: "AgentCredential",
            entityId: credential.id,
            summary: "Added a key credential to an agent with an enrolment token.",
            changes: {
              agentId: agentId!,
              fingerprint: credential.fingerprint,
              scopes: credential.scopes,
              enrolmentTokenId: token.id,
            },
          }),
          tokenAudit(token.organisationId, {
            action: "enrolmentTokenUsed",
            entityType: "EnrolmentToken",
            entityId: token.id,
            summary: "Used an enrolment token.",
            changes: {
              agentId: agentId!,
              credentialId: credential.id,
              agentCreated: !existing,
              uses: used!.uses,
            },
          }),
        ]);
        return {
          agentId: agentId!,
          credentialId: credential.id,
          fingerprint: credential.fingerprint,
        };
      }),
    );
  });

/** At most this many agents in one enrolment check: a rear admiral and the installer's captains. */
export const maxEnrolmentChecks = 50;

const EnrolmentCheck = Schema.Struct({
  agents: Schema.Array(
    Schema.Struct({
      name: AgentName,
      rank: Schema.Literals(["rearAdmiral", "captain"]),
      ship: Schema.optional(Schema.NullOr(ShipName)),
      ircNick: Schema.optional(Schema.NullOr(IrcNick)),
    }),
  ).check(Schema.isMinLength(1), Schema.isMaxLength(maxEnrolmentChecks)),
}).annotate({ identifier: "AgentEnrolmentCheck" });

const EnrolmentCheckResult = Schema.Struct({
  items: Schema.Array(
    Schema.Struct({
      name: Schema.String,
      outcome: Schema.Literals(["new", "existing", "refused"]),
      detail: Schema.NullOr(Schema.String),
    }),
  ),
}).annotate({ identifier: "AgentEnrolmentCheckResult" });

/** A refusal an enrolment answers with a status and detail (400 or 409), as opposed to a failure. */
const refusal = (error: unknown) =>
  typeof error === "object" &&
  error !== null &&
  "status" in error &&
  (error.status === 400 || error.status === 409) &&
  "detail" in error &&
  typeof error.detail === "string"
    ? error.detail +
      ("errors" in error && error.errors && typeof error.errors === "object"
        ? Object.values(error.errors as Record<string, readonly string[]>)
            .flat()
            .map((message) => ` ${message}`)
            .join("")
        : "")
    : undefined;

/**
 * What enrolling each agent with this token would do, without a key and without changing
 * anything or using the token: add a key to the existing agent of that name in the fleet, make
 * a new agent, or refuse it, with the refusal's detail. The same rules as enrolment, so a
 * fleet's installer can check its officers' names before it registers anything.
 */
const checkEnrolment = (
  grant: (typeof EnrolmentGrant)["Service"],
  payload: typeof EnrolmentCheck.Type,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const [fleet] = yield* db
      .select()
      .from(fleets)
      .where(and(eq(fleets.organisationId, grant.organisationId), eq(fleets.id, grant.fleetId)));
    if (!fleet) return yield* Effect.fail(problem(401, invalidToken));
    const items = [];
    for (const wanted of payload.agents) {
      const decided = yield* Effect.gen(function* () {
        yield* shipOf(wanted);
        const existing = yield* findFleetOfficer(db, fleet, wanted);
        if (existing) return "existing" as const;
        yield* checkAgentNames(db, grant.organisationId, {
          subject: keyOnlySubject(randomUUID()),
          displayName: wanted.name,
          agentIrcNick: wanted.ircNick ?? null,
        });
        return "new" as const;
      }).pipe(Effect.result);
      if (Result.isSuccess(decided)) {
        items.push({ name: wanted.name, outcome: decided.success, detail: null });
        continue;
      }
      const detail = refusal(decided.failure);
      if (detail === undefined) return yield* Effect.fail(decided.failure);
      items.push({ name: wanted.name, outcome: "refused" as const, detail });
    }
    return { items };
  });

const fleetParams = { reference: FleetRef };

/** An administrator's enrolment tokens for a fleet, on the main listener only. */
export const enrolmentTokenGroup = HttpApiGroup.make("enrolmentTokens")
  .add(
    HttpApiEndpoint.post("issueEnrolmentToken", "/fleets/:reference/enrolment-tokens", {
      params: fleetParams,
      // The body is optional: none, an empty one or null takes the defaults.
      payload: [NewEnrolmentToken, Schema.Null],
      success: IssuedEnrolmentToken.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      `An organisation administrator issues a token with which the fleet's gateway enrols the fleet's agents. The token is returned once, begins ${enrolmentTokenPrefix} and is stored only as a hash. The body may be left out. expiresInSeconds defaults to ${defaultEnrolmentLifetime} (60 to ${maxEnrolmentLifetime}) and maxUses to ${defaultEnrolmentUses} (1 to ${maxEnrolmentUses}).`,
    ),
    HttpApiEndpoint.get("listEnrolmentTokens", "/fleets/:reference/enrolment-tokens", {
      params: fleetParams,
      success: Schema.Struct({ items: Schema.Array(EnrolmentTokenSummary) }),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An organisation administrator lists the fleet's enrolment tokens, newest first, without their secrets.",
    ),
    HttpApiEndpoint.delete("revokeEnrolmentToken", "/fleets/:reference/enrolment-tokens/:tokenId", {
      params: { ...fleetParams, tokenId: Uuid },
      success: EnrolmentTokenSummary,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An organisation administrator revokes an enrolment token. The record is kept.",
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const enrolmentPath = `${versionedApi}/agents/enrol`;
export const enrolmentCheckPath = `${enrolmentPath}/check`;

/**
 * The fleet gateway's enrolment route, authorised by an enrolment token alone: outside the
 * administrator routes and on the main listener only.
 */
export const enrolmentGroup = HttpApiGroup.make("enrolment")
  .add(
    HttpApiEndpoint.post("enrolAgent", "/agents/enrol", {
      payload: Enrolment,
      success: [Enrolled.pipe(HttpApiSchema.status(201)), Enrolled],
      error: [...endpointErrors, Unauthorised],
    }).annotate(
      OpenApi.Description,
      "A fleet's gateway registers a rear admiral or captain of the enrolment token's fleet with a certificate, or adds the certificate to the active agent of that name and rank in that fleet (201). The same certificate with the same scopes again answers 200 with the same IDs. 401 for a missing, unknown, expired, revoked or used-up token; 409 for a name held in another fleet, by a deactivated agent or with another rank, a new agent's name or IRC nick that another agent or person holds (the detail names the agent and its fleet), or a certificate registered elsewhere or revoked.",
    ),
    HttpApiEndpoint.post("checkEnrolment", "/agents/enrol/check", {
      payload: EnrolmentCheck,
      success: EnrolmentCheckResult,
      error: [...endpointErrors, Unauthorised],
    }).annotate(
      OpenApi.Description,
      `A fleet's installer asks, with the fleet's enrolment token, what enrolling each of up to ${maxEnrolmentChecks} agents would do, by the same rules as enrolment: existing (a key is added to the active agent of that name and rank in the token's fleet), new (the name and IRC nick are free) or refused, with the reason, which names the agent and fleet that hold a name. It takes no key, changes nothing and uses none of the token's uses. 401 for a missing, unknown, expired or revoked token.`,
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(EnrolmentAuth)
  .middleware(ValidationErrors);

export const enrolmentTokenHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof enrolmentTokenGroup>,
) =>
  handlers
    .handle("issueEnrolmentToken", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          issueToken(member, params.reference, payload ?? {}),
        ),
      ),
    )
    .handle("listEnrolmentTokens", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listTokens(member, params.reference))),
    )
    .handle("revokeEnrolmentToken", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          revokeToken(member, params.reference, params.tokenId),
        ),
      ),
    );

export const enrolmentHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof enrolmentGroup>,
) =>
  handlers
    .handle("enrolAgent", ({ payload }) =>
      handleDb(Effect.flatMap(EnrolmentGrant, (grant) => enrol(grant, payload))),
    )
    .handle("checkEnrolment", ({ payload }) =>
      handleDb(Effect.flatMap(EnrolmentGrant, (grant) => checkEnrolment(grant, payload))),
    );
