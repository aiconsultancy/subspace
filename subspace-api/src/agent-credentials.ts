import { and, asc, desc, eq, isNotNull, isNull } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApi, HttpApiEndpoint, HttpApiGroup, HttpApiSchema, OpenApi } from "effect/http-api";
import {
  certificateProblemDetail,
  CertificatePem,
  CredentialScope,
  CredentialScopes,
  KeyAlgorithm,
  parseCertificate,
  type CredentialScope as Scope,
  type ParsedCertificate,
} from "./agent-keys";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { hasConstraint } from "./fleets";
import { CurrentMember, type Member } from "./identity";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors, problem, validationProblem } from "./problems";
import { agentCredentials, organisations, users } from "./schema";
import { dateTime, dateTimeOrNull, Line, NullableString, Timestamp, Uuid } from "./validation";

type Db = (typeof Database)["Service"];
type CredentialRow = typeof agentCredentials.$inferSelect;

export const AgentCredential = Schema.Struct({
  id: Uuid,
  agentId: Uuid,
  fingerprint: Schema.String,
  algorithm: KeyAlgorithm,
  scopes: Schema.Array(CredentialScope),
  label: NullableString,
  certificate: Schema.String,
  createdAt: Timestamp,
  createdByUserId: Schema.NullOr(Uuid),
  enrolmentTokenId: Schema.NullOr(Uuid),
  lastUsedAt: Schema.NullOr(Timestamp),
  revokedAt: Schema.NullOr(Timestamp),
  revokedByUserId: Schema.NullOr(Uuid),
}).annotate({ identifier: "AgentCredential" });

const NewCredential = Schema.Struct({
  certificate: CertificatePem,
  scopes: CredentialScopes,
  label: Schema.optional(Schema.NullOr(Line(120))),
}).annotate({ identifier: "NewAgentCredential" });

export const credentialDto = (row: CredentialRow) => ({
  id: row.id,
  agentId: row.userId,
  fingerprint: row.fingerprint,
  algorithm: row.algorithm,
  scopes: row.scopes,
  label: row.label,
  certificate: row.certificate,
  createdAt: dateTime(row.createdAt),
  createdByUserId: row.createdById,
  enrolmentTokenId: row.enrolmentTokenId,
  lastUsedAt: dateTimeOrNull(row.lastUsedAt),
  revokedAt: dateTimeOrNull(row.revokedAt),
  revokedByUserId: row.revokedById,
});

/** A certificate read strictly, or a 400 naming the field. */
export const readCertificate = (pem: string, field = "certificate") =>
  Effect.flatMap(
    Effect.promise(() => parseCertificate(pem)),
    (check) =>
      check.ok
        ? Effect.succeed(check.certificate)
        : Effect.fail(validationProblem(field, certificateProblemDetail[check.reason])),
  );

/** Locks the organisation's row as every agent registration change does, before its users. */
export const lockOrganisation = (db: Db, organisationId: string) =>
  db
    .select({ id: organisations.id })
    .from(organisations)
    .where(eq(organisations.id, organisationId))
    .for("no key update");

/** The active credential with a fingerprint, in any organisation, if there is one. */
export const activeCredential = (db: Db, fingerprint: string) =>
  Effect.map(
    db
      .select()
      .from(agentCredentials)
      .where(
        and(eq(agentCredentials.fingerprint, fingerprint), isNull(agentCredentials.revokedAt)),
      ),
    (rows) => rows[0],
  );

/**
 * Registers a certificate as a credential of an agent. Call it in a transaction that holds the
 * organisation's lock, after checking that the agent belongs to it. A certificate that is active
 * anywhere, or was revoked in this organisation, is refused (409): a fingerprint names one active
 * credential, because an agent token names no organisation, and a revoked key stays revoked.
 */
export const insertCredential = (
  db: Db,
  values: {
    organisationId: string;
    userId: string;
    certificate: ParsedCertificate;
    scopes: readonly Scope[];
    label?: string | null;
    createdById?: string | null;
    enrolmentTokenId?: string | null;
  },
) =>
  Effect.gen(function* () {
    const { certificate } = values;
    if (yield* activeCredential(db, certificate.fingerprint))
      return yield* Effect.fail(problem(409, "This certificate is already registered."));
    const [revoked] = yield* db
      .select({ id: agentCredentials.id })
      .from(agentCredentials)
      .where(
        and(
          eq(agentCredentials.organisationId, values.organisationId),
          eq(agentCredentials.fingerprint, certificate.fingerprint),
          isNotNull(agentCredentials.revokedAt),
        ),
      )
      .limit(1);
    if (revoked)
      return yield* Effect.fail(
        problem(409, "This certificate was revoked. Generate a new key and certificate."),
      );
    const [row] = yield* db
      .insert(agentCredentials)
      .values({
        organisationId: values.organisationId,
        userId: values.userId,
        fingerprint: certificate.fingerprint,
        algorithm: certificate.algorithm,
        certificate: certificate.pem,
        publicKey: certificate.publicKey,
        scopes: [...new Set(values.scopes)].sort(),
        label: values.label ?? null,
        createdById: values.createdById ?? null,
        enrolmentTokenId: values.enrolmentTokenId ?? null,
      })
      .returning()
      .pipe(
        Effect.catchIf(
          (error) => hasConstraint(error, "agent_credentials_active_fingerprint"),
          () => Effect.fail(problem(409, "This certificate is already registered.")),
        ),
      );
    return row!;
  });

const adminOnly = (member: Member) =>
  member.role !== "admin" || member.agentRank
    ? Effect.fail(problem(403, "Agent credentials are managed by an organisation administrator."))
    : Effect.void;

const notFound = () =>
  problem(404, "Agent was not found. Use the userId returned by the agents list.");

/** The agent record with this ID in the member's organisation. */
const findAgent = (db: Db, member: Member, agentId: string) =>
  Effect.map(
    db
      .select({ id: users.id })
      .from(users)
      .where(
        and(
          eq(users.organisationId, member.organisationId),
          eq(users.id, agentId),
          isNotNull(users.agentRank),
        ),
      ),
    (rows) => rows[0],
  );

const listCredentials = (member: Member, agentId: string) =>
  Effect.gen(function* () {
    yield* adminOnly(member);
    const db = yield* Database;
    if (!(yield* findAgent(db, member, agentId))) return yield* Effect.fail(notFound());
    const rows = yield* db
      .select()
      .from(agentCredentials)
      .where(
        and(
          eq(agentCredentials.organisationId, member.organisationId),
          eq(agentCredentials.userId, agentId),
        ),
      )
      .orderBy(desc(agentCredentials.createdAt), asc(agentCredentials.id));
    return { items: rows.map(credentialDto) };
  });

const addCredential = (member: Member, agentId: string, payload: typeof NewCredential.Type) =>
  Effect.gen(function* () {
    yield* adminOnly(member);
    const certificate = yield* readCertificate(payload.certificate);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, member.organisationId);
        if (!(yield* findAgent(tx, member, agentId))) return yield* Effect.fail(notFound());
        const row = yield* insertCredential(tx, {
          organisationId: member.organisationId,
          userId: agentId,
          certificate,
          scopes: payload.scopes,
          label: payload.label,
          createdById: member.id,
        });
        yield* recordAudit(tx, member, {
          action: "agentCredentialAdded",
          entityType: "AgentCredential",
          entityId: row.id,
          summary: "Added a key credential to an agent.",
          changes: { agentId, fingerprint: row.fingerprint, scopes: row.scopes },
        });
        return credentialDto(row);
      }),
    );
  });

const revokeCredential = (member: Member, agentId: string, credentialId: string) =>
  Effect.gen(function* () {
    yield* adminOnly(member);
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockOrganisation(tx, member.organisationId);
        const [old] = yield* tx
          .select()
          .from(agentCredentials)
          .where(
            and(
              eq(agentCredentials.organisationId, member.organisationId),
              eq(agentCredentials.userId, agentId),
              eq(agentCredentials.id, credentialId),
            ),
          )
          .for("update");
        if (!old)
          return yield* Effect.fail(
            problem(404, "Credential was not found. Use an ID from the agent's credentials."),
          );
        if (old.revokedAt) return credentialDto(old);
        const [row] = yield* tx
          .update(agentCredentials)
          .set({ revokedAt: new Date(), revokedById: member.id })
          .where(eq(agentCredentials.id, old.id))
          .returning();
        yield* recordAudit(tx, member, {
          action: "agentCredentialRevoked",
          entityType: "AgentCredential",
          entityId: row!.id,
          summary: "Revoked an agent's key credential.",
          changes: { agentId, fingerprint: row!.fingerprint },
        });
        return credentialDto(row!);
      }),
    );
  });

const params = { agentId: Uuid };

/**
 * The administrator's management of agents' key credentials, on the main listener only: the
 * agent listener does not serve this group.
 */
export const agentCredentialGroup = HttpApiGroup.make("agentCredentials")
  .add(
    HttpApiEndpoint.get("listAgentCredentials", "/agents/:agentId/credentials", {
      params,
      success: Schema.Struct({ items: Schema.Array(AgentCredential) }),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An organisation administrator lists an agent's key credentials, revoked ones included, newest first.",
    ),
    HttpApiEndpoint.post("addAgentCredential", "/agents/:agentId/credentials", {
      params,
      payload: NewCredential,
      success: AgentCredential.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An organisation administrator registers a certificate with an Ed25519 or ECDSA P-256 key as one of the agent's key credentials, with scopes api, irc or both. 409 when the certificate is already registered, or was revoked in this organisation.",
    ),
    HttpApiEndpoint.delete("revokeAgentCredential", "/agents/:agentId/credentials/:credentialId", {
      params: { ...params, credentialId: Uuid },
      success: AgentCredential,
      error: endpointErrors,
    }).annotate(
      OpenApi.Description,
      "An organisation administrator revokes one of the agent's key credentials. Its tokens and IRC sign-ins are refused from then on; the record is kept.",
    ),
  )
  .annotate(HttpApi.PayloadParseOptions, { onExcessProperty: "error" })
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const agentCredentialHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<
    typeof agentCredentialGroup
  >,
) =>
  handlers
    .handle("listAgentCredentials", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listCredentials(member, params.agentId))),
    )
    .handle("addAgentCredential", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => addCredential(member, params.agentId, payload)),
      ),
    )
    .handle("revokeAgentCredential", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          revokeCredential(member, params.agentId, params.credentialId),
        ),
      ),
    );
