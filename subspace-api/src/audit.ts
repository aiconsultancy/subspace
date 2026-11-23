import { and, eq, gte, inArray, isNotNull, lte } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup } from "effect/http-api";
import { RelayedCrew } from "./crew";
import { Database } from "./db";
import { AgentRank, CurrentMember, memberKind, type Member } from "./identity";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { readProject } from "./projects";
import { auditLogs } from "./schema";
import {
  dateTime,
  Line,
  NullableString,
  PageQuery,
  pageOf,
  Ref,
  Timestamp,
  Uuid,
} from "./validation";

type Db = (typeof Database)["Service"];
type AuditFields = Pick<
  typeof auditLogs.$inferInsert,
  "action" | "entityType" | "entityId" | "projectId" | "summary" | "changes"
>;

export function recordAudit(db: Db, member: Member, fields: AuditFields) {
  return db.insert(auditLogs).values({
    ...fields,
    organisationId: member.organisationId,
    actorId: member.id,
    actorType: memberKind(member),
    actorDisplayName: member.displayName,
    actorRank: member.agentRank,
    source: member.source ?? "rest",
  });
}

export const AuditQuery = Schema.Struct({
  ...PageQuery,
  project: Schema.optional(Ref),
  entityType: Schema.optional(Line(80)),
  entityId: Schema.optional(Ref),
  action: Schema.optional(Schema.Array(Line(80)).check(Schema.isMaxLength(30))),
  actorId: Schema.optional(Uuid),
  from: Schema.optional(Schema.DateTimeUtcFromString),
  to: Schema.optional(Schema.DateTimeUtcFromString),
});
const Audit = Schema.Struct({
  id: Uuid,
  occurredAt: Timestamp,
  action: Schema.String,
  actorType: Schema.Literals(["human", "agent", "integration"]),
  actorId: Schema.NullOr(Uuid),
  actorDisplayName: NullableString,
  actorRank: Schema.NullOr(AgentRank),
  relayedCrewName: NullableString,
  relayedCrewModel: NullableString,
  source: Schema.Literals(["rest", "mcp", "github", "cli"]),
  entityType: Schema.String,
  entityId: NullableString,
  projectId: Schema.NullOr(Uuid),
  correlationId: Schema.String,
  summary: Schema.String,
  changes: Schema.Record(Schema.String, Schema.Unknown),
});

export function listAudit(member: Member, input: typeof AuditQuery.Type) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { project, entityType, entityId, action, actorId, from, to } = input;
    if (from && to && from.epochMilliseconds > to.epochMilliseconds)
      return yield* Effect.fail(problem(400, "The audit start must be on or before its end."));
    const projectId = project ? (yield* readProject(db, member, project)).project.id : undefined;
    const rows = yield* db
      .select()
      .from(auditLogs)
      .where(
        and(
          eq(auditLogs.organisationId, member.organisationId),
          member.agentRank ? isNotNull(auditLogs.projectId) : undefined,
          projectId ? eq(auditLogs.projectId, projectId) : undefined,
          entityType ? eq(auditLogs.entityType, entityType) : undefined,
          entityId ? eq(auditLogs.entityId, entityId) : undefined,
          action?.length ? inArray(auditLogs.action, action) : undefined,
          actorId ? eq(auditLogs.actorId, actorId) : undefined,
          from ? gte(auditLogs.occurredAt, new Date(from.epochMilliseconds)) : undefined,
          to ? lte(auditLogs.occurredAt, new Date(to.epochMilliseconds)) : undefined,
        ),
      );
    const key = (row: typeof auditLogs.$inferSelect) =>
      `${String(Number.MAX_SAFE_INTEGER - row.occurredAt.valueOf()).padStart(16, "0")}:${row.id}`;
    const scope = {
      org: member.organisationId,
      member: member.id,
      projectId,
      entityType,
      entityId,
      action,
      actorId,
      from: from?.epochMilliseconds,
      to: to?.epochMilliseconds,
    };
    const result = yield* page(rows.sort(byKey(key)), scope, key, input.limit, input.cursor);
    return {
      ...result,
      items: result.items.map((row) => {
        const attribution = row.changes.relayedCrew;
        const crew = Schema.is(RelayedCrew)(attribution) ? attribution : null;
        return {
          ...row,
          occurredAt: dateTime(row.occurredAt),
          correlationId: row.id,
          relayedCrewName: crew?.name ?? null,
          relayedCrewModel: crew?.model ?? null,
        };
      }),
    };
  });
}

export const auditGroup = HttpApiGroup.make("audit")
  .add(
    HttpApiEndpoint.get("listAudit", "/audit", {
      query: AuditQuery,
      success: pageOf(Audit),
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const auditHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof auditGroup>,
) =>
  handlers.handle("listAudit", ({ query }) =>
    handleDb(Effect.flatMap(CurrentMember, (member) => listAudit(member, query))),
  );
