import { and, eq, inArray, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { AgentRank, CurrentMember, memberKind, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { addActivity, lockIssues, readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { readProject } from "./projects";
import {
  decisionIssues,
  decisionRevisions,
  decisions,
  decisionStatuses,
  evidenceKinds,
  issues,
  projects,
  users,
} from "./schema";
import {
  DateOnly,
  dateTime,
  dateTimeOrNull,
  Line,
  NullableString,
  PageQuery,
  Paragraph,
  Ref,
  Timestamp,
  Uuid,
  pageOf,
} from "./validation";

const Status = Schema.Literals(decisionStatuses);
const Maker = Schema.Literals(["human", "agent"]);
const Evidence = Schema.Literals(evidenceKinds);
const DecisionIssue = Schema.Struct({ id: Uuid, identifier: Ref, title: Line(200) });
const Provenance = Schema.Struct({
  madeByType: Maker,
  madeByUserId: Schema.NullOr(Uuid),
  madeByAgentName: NullableString,
  madeByAgentRank: Schema.NullOr(AgentRank),
  madeByAgentModel: NullableString,
  evidenceKind: Schema.NullOr(Evidence),
  evidenceReference: NullableString,
  evidenceExcerpt: NullableString,
}).annotate({ identifier: "DecisionProvenance" });
const DecisionSummary = Schema.Struct({ id: Uuid, title: Line(200), status: Status });
const DecisionMember = Schema.Struct({
  userId: Uuid,
  displayName: Line(120),
  available: Schema.Boolean,
  kind: Maker,
});
const Decision = Schema.Struct({
  id: Uuid,
  title: Line(200),
  status: Status,
  decidedOn: DateOnly,
  projectId: Uuid,
  projectKey: Ref,
  project: Schema.Struct({ id: Uuid, name: Line(120), archived: Schema.Boolean }),
  supersedesId: Schema.NullOr(Uuid),
  supersededById: Schema.NullOr(Uuid),
  supersedes: Schema.NullOr(DecisionSummary),
  supersededBy: Schema.NullOr(DecisionSummary),
  issues: Schema.Struct({ items: Schema.Array(DecisionIssue), count: Schema.Int }),
  tags: Schema.Array(Schema.String),
  ...Provenance.fields,
  recordedByUserId: Uuid,
  version: Version,
  revisionNumber: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
}).annotate({ identifier: "Decision" });
const Detail = Schema.Struct({
  ...Decision.fields,
  decision: Schema.String,
  context: Schema.String,
  why: Schema.String,
  implications: Schema.String,
  alternatives: Schema.String,
  acceptedAt: Schema.NullOr(Timestamp),
  rejectedAt: Schema.NullOr(Timestamp),
  acceptedByUserId: Schema.NullOr(Uuid),
  rejectedByUserId: Schema.NullOr(Uuid),
  provenance: Provenance,
  recordedBy: DecisionMember,
}).annotate({ identifier: "DecisionDetail" });
export const Query = Schema.Struct({
  ...PageQuery,
  issue: Schema.optional(Ref),
  project: Schema.optional(Ref),
  status: Schema.optional(Schema.Array(Status)),
  madeByType: Schema.optional(Maker),
  tag: Schema.optional(Line(80)),
  q: Schema.optional(Paragraph(200)),
});
export const NewDecision = Schema.Struct({
  projectId: Uuid,
  title: Line(200),
  decidedOn: DateOnly,
  decision: Paragraph(50_000).check(Schema.isPattern(/\S/u)),
  context: Schema.optional(Paragraph(50_000)),
  why: Schema.optional(Paragraph(50_000)),
  implications: Schema.optional(Paragraph(50_000)),
  alternatives: Schema.optional(Paragraph(50_000)),
  status: Schema.optional(Schema.Literals(["proposed", "accepted", "rejected"])),
  tags: Schema.optional(Schema.Array(Line(80)).check(Schema.isMaxLength(50))),
  issueIds: Schema.optional(Schema.Array(Uuid).check(Schema.isMaxLength(100))),
  supersedesId: Schema.optional(Schema.NullOr(Uuid)),
  madeByType: Schema.optional(Maker),
  madeByUserId: Schema.optional(Uuid),
  evidenceKind: Schema.optional(Schema.NullOr(Evidence)),
  evidenceReference: Schema.optional(Schema.NullOr(Paragraph(2000))),
  evidenceExcerpt: Schema.optional(Schema.NullOr(Paragraph(4000))),
});

export const DecisionEdit = Schema.Struct({
  title: Schema.optional(NewDecision.fields.title),
  decidedOn: Schema.optional(DateOnly),
  decision: Schema.optional(NewDecision.fields.decision),
  context: NewDecision.fields.context,
  why: NewDecision.fields.why,
  implications: NewDecision.fields.implications,
  alternatives: NewDecision.fields.alternatives,
  tags: NewDecision.fields.tags,
  issueIds: NewDecision.fields.issueIds,
  supersedesId: NewDecision.fields.supersedesId,
  version: Schema.optional(Version),
});
export const StatusChange = Schema.Struct({
  status: Schema.Literals(["accepted", "rejected"]),
  version: Schema.optional(Version),
});
const RevisionMetadata = Schema.Struct({
  id: Uuid,
  number: Version,
  savedAt: Timestamp,
  savedBy: DecisionMember,
  changedFields: Schema.Array(Schema.String),
});
const RevisionDetail = Schema.Struct({
  revision: RevisionMetadata,
  snapshot: Detail,
  currentVersion: Version,
  currentRevisionNumber: Version,
  previousRevisionId: Schema.NullOr(Uuid),
  nextRevisionId: Schema.NullOr(Uuid),
});

type Db = (typeof Database)["Service"];
type Select = typeof decisions.$inferSelect;
const orgScope = (member: Member) => eq(decisions.organisationId, member.organisationId);

function rowData(db: Db, member: Member, id?: string, project?: string) {
  const org = member.organisationId;
  return db
    .select({ decision: decisions, project: projects, actor: users })
    .from(decisions)
    .innerJoin(
      projects,
      and(eq(projects.organisationId, org), eq(projects.id, decisions.projectId)),
    )
    .innerJoin(users, and(eq(users.organisationId, org), eq(users.id, decisions.recordedById)))
    .where(
      and(
        orgScope(member),
        id ? eq(decisions.id, id) : undefined,
        project ? eq(decisions.projectId, project) : undefined,
      ),
    );
}

type Joined = Effect.Success<ReturnType<typeof rowData>>[number];

function linkedIssues(db: Db, member: Member, ids: readonly string[]) {
  if (!ids.length) return Effect.succeed(new Map<string, (typeof DecisionIssue.Type)[]>());
  return Effect.gen(function* () {
    const org = member.organisationId;
    const rows = yield* db
      .select({
        decisionId: decisionIssues.decisionId,
        id: issues.id,
        identifier: issues.identifier,
        title: issues.title,
      })
      .from(decisionIssues)
      .innerJoin(issues, and(eq(issues.organisationId, org), eq(issues.id, decisionIssues.issueId)))
      .where(
        and(eq(decisionIssues.organisationId, org), inArray(decisionIssues.decisionId, [...ids])),
      )
      .orderBy(issues.identifier, issues.id);
    return Map.groupBy(rows, ({ decisionId }) => decisionId);
  });
}

function decisionMember(actor: Member) {
  return {
    userId: actor.id,
    displayName: actor.displayName,
    available: actor.active,
    kind: memberKind(actor),
  };
}

function relatedSummaries(db: Db, member: Member, rows: Joined[]) {
  const ids = [
    ...new Set(
      rows
        .flatMap(({ decision }) => [decision.supersedesId, decision.supersededById])
        .filter((id) => id !== null),
    ),
  ];
  return ids.length
    ? Effect.map(
        db
          .select({ id: decisions.id, title: decisions.title, status: decisions.status })
          .from(decisions)
          .where(and(orgScope(member), inArray(decisions.id, ids))),
        (found) => new Map(found.map((row) => [row.id, row])),
      )
    : Effect.succeed(new Map<string, typeof DecisionSummary.Type>());
}

function decisionDto(
  row: Joined,
  links: readonly (typeof DecisionIssue.Type)[],
  related: Map<string, typeof DecisionSummary.Type>,
) {
  const { decision: record, project, actor } = row;
  const provenance: typeof Provenance.Type = {
    madeByType: memberKind(actor),
    madeByUserId: actor.agentRank ? null : actor.id,
    madeByAgentName: actor.agentRank ? actor.displayName : null,
    madeByAgentModel: actor.agentModel,
    madeByAgentRank: actor.agentRank,
    evidenceKind: record.evidenceKind,
    evidenceReference: record.evidenceReference,
    evidenceExcerpt: record.evidenceExcerpt,
  };
  return {
    id: record.id,
    title: record.title,
    decidedOn: record.decidedOn,
    status: record.status,
    projectId: project.id,
    projectKey: project.key,
    project: { id: project.id, name: project.name, archived: project.archivedAt !== null },
    supersedesId: record.supersedesId,
    supersededById: record.supersededById,
    supersedes: record.supersedesId ? (related.get(record.supersedesId) ?? null) : null,
    supersededBy: record.supersededById ? (related.get(record.supersededById) ?? null) : null,
    issues: { items: links.slice(0, 50), count: links.length },
    tags: record.tags,
    ...provenance,
    recordedByUserId: actor.id,
    version: record.version,
    revisionNumber: record.version,
    createdAt: dateTime(record.createdAt),
    updatedAt: dateTime(record.updatedAt),
    decision: record.decision,
    context: record.context,
    why: record.why,
    implications: record.implications,
    alternatives: record.alternatives,
    acceptedAt: dateTimeOrNull(record.acceptedAt),
    rejectedAt: dateTimeOrNull(record.rejectedAt),
    acceptedByUserId: record.acceptedByUserId,
    rejectedByUserId: record.rejectedByUserId,
    provenance,
    recordedBy: decisionMember(actor),
  };
}

export function getDecision(db: Db, member: Member, id: string) {
  return Effect.gen(function* () {
    const [row] = yield* rowData(db, member, id);
    if (!row) return yield* Effect.fail(problem(404, "Decision not found."));
    const links = yield* linkedIssues(db, member, [id]);
    return decisionDto(row, links.get(id) ?? [], yield* relatedSummaries(db, member, [row]));
  });
}

export const listDecisions = (member: Member, query: typeof Query.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const { cursor, limit, ...filters } = query;
          const project = filters.project
            ? (yield* readProject(tx, member, filters.project)).project.id
            : undefined;
          const issue = filters.issue
            ? (yield* readIssue(tx, member, filters.issue)).id
            : undefined;
          const rows = yield* rowData(tx, member, undefined, project);
          const links = yield* linkedIssues(
            tx,
            member,
            rows.map(({ decision }) => decision.id),
          );
          const search = filters.q?.trim().toLowerCase();
          const selected = rows.filter(
            ({ decision, actor }) =>
              (!filters.madeByType || filters.madeByType === memberKind(actor)) &&
              (!filters.status?.length || filters.status.includes(decision.status)) &&
              (!filters.tag ||
                decision.tags.some((tag) => tag.toLowerCase() === filters.tag?.toLowerCase())) &&
              (!issue || links.get(decision.id)?.some(({ id }) => id === issue)) &&
              (!search ||
                [decision.title, decision.decision, decision.context].some((field) =>
                  field.toLowerCase().includes(search),
                )),
          );
          const key = ({ decision }: Joined) => {
            const day = Number(decision.decidedOn.replaceAll("-", ""));
            return `${String(99_991_231 - day).padStart(8, "0")}:${decision.id}`;
          };
          const result = yield* page(
            selected.sort(byKey(key)),
            { org: member.organisationId, decisions: filters },
            key,
            limit ?? 50,
            cursor,
          );
          const related = yield* relatedSummaries(tx, member, result.items);
          return {
            ...result,
            items: result.items.map((row) =>
              decisionDto(row, links.get(row.decision.id) ?? [], related),
            ),
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    ),
  );

function statusFields(status: Select["status"], userId: string) {
  return {
    acceptedAt: status === "accepted" ? new Date() : null,
    acceptedByUserId: status === "accepted" ? userId : null,
    rejectedAt: status === "rejected" ? new Date() : null,
    rejectedByUserId: status === "rejected" ? userId : null,
  };
}

function auditDecision(
  db: Db,
  member: Member,
  decision: typeof Detail.Type,
  action: string,
  changes: Record<string, unknown>,
) {
  return recordAudit(db, member, {
    action,
    entityType: "Decision",
    entityId: decision.id,
    projectId: decision.projectId,
    summary: "Changed a project decision.",
    changes,
  });
}

function saveRevision(
  db: Db,
  member: Member,
  decision: typeof Detail.Type,
  changedFields: string[],
  existing = false,
) {
  return db
    .insert(decisionRevisions)
    .values({
      id: decision.revisionNumber === 1 ? decision.id : undefined,
      organisationId: member.organisationId,
      decisionId: decision.id,
      number: decision.revisionNumber,
      snapshot: decision,
      savedBy: existing ? decision.recordedBy : decisionMember(member),
      savedAt: new Date(decision.updatedAt),
      changedFields,
    })
    .onConflictDoNothing({
      target: [
        decisionRevisions.organisationId,
        decisionRevisions.decisionId,
        decisionRevisions.number,
      ],
    });
}

function replaceIssueLinks(db: Db, member: Member, record: Select, issueIds: readonly string[]) {
  return Effect.gen(function* () {
    const ids = [...new Set(issueIds)];
    for (const issueId of ids) {
      const issue = yield* readIssue(db, member, issueId);
      if (issue.projectId !== record.projectId)
        return yield* Effect.fail(problem(400, "Decision issues must belong to its project."));
    }
    yield* db
      .delete(decisionIssues)
      .where(
        and(
          eq(decisionIssues.organisationId, member.organisationId),
          eq(decisionIssues.decisionId, record.id),
        ),
      );
    if (ids.length)
      yield* db.insert(decisionIssues).values(
        ids.map((issueId) => ({
          organisationId: member.organisationId,
          decisionId: record.id,
          issueId,
        })),
      );
    return ids;
  });
}

function predecessor(
  db: Db,
  member: Member,
  projectId: string,
  targetId?: string | null,
  self?: string,
) {
  return Effect.gen(function* () {
    if (!targetId) return null;
    if (self && targetId === self)
      return yield* Effect.fail(problem(400, "A decision cannot supersede itself."));
    const [target] = yield* rowData(db, member, targetId, projectId);
    if (!target)
      return yield* Effect.fail(
        problem(404, "The decision to supersede was not found in this project."),
      );
    if (target.decision.status !== "accepted" || target.decision.supersededById)
      return yield* Effect.fail(problem(409, "Choose a current accepted decision to supersede."));
    return target.decision;
  });
}

function supersede(db: Db, member: Member, target: Select, replacementId: string) {
  return Effect.gen(function* () {
    yield* saveRevision(db, member, yield* getDecision(db, member, target.id), [], true);
    yield* db
      .update(decisions)
      .set({
        status: "superseded",
        supersededById: replacementId,
        updatedAt: sql`now()`,
        version: target.version + 1,
      })
      .where(and(orgScope(member), eq(decisions.id, target.id)));
    const updated = yield* getDecision(db, member, target.id);
    yield* saveRevision(db, member, updated, ["status", "supersededById"]);
    yield* auditDecision(db, member, updated, "decisionSuperseded", {
      supersededById: replacementId,
    });
  });
}

function notifyDecision(
  db: Db,
  member: Member,
  decisionId: string,
  issueIds: readonly string[],
  kind: string,
) {
  return Effect.forEach(
    new Set(issueIds),
    (issueId) =>
      Effect.gen(function* () {
        yield* db
          .update(issues)
          .set({ lastActivityAt: sql`greatest(${issues.lastActivityAt}, now())` })
          .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, issueId)));
        yield* addActivity(db, member, issueId, kind, { decisionId });
      }),
    { discard: true },
  );
}

export const createDecision = (member: Member, input: typeof NewDecision.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { issueIds, madeByType, madeByUserId, ...fields } = input;
        const { project } = yield* readProject(tx, member, fields.projectId);
        if (project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the project before logging decisions."),
          );
        if (
          (madeByType && madeByType !== memberKind(member)) ||
          (madeByUserId && (member.agentRank || madeByUserId !== member.id))
        )
          return yield* Effect.fail(problem(403, "Record decisions under your own identity."));
        const status = fields.status ?? "proposed";
        const target = yield* predecessor(tx, member, project.id, fields.supersedesId);
        const [row] = yield* tx
          .insert(decisions)
          .values({
            ...fields,
            ...statusFields(status, member.id),
            organisationId: member.organisationId,
            recordedById: member.id,
            tags: [...new Set(fields.tags)],
            status,
          })
          .returning();
        if (!row) return yield* Effect.fail(problem(500, "The decision could not be saved."));
        const targets = yield* replaceIssueLinks(tx, member, row, issueIds ?? []);
        if (status === "accepted" && target) yield* supersede(tx, member, target, row.id);
        const result = yield* getDecision(tx, member, row.id);
        yield* saveRevision(tx, member, result, ["created"]);
        yield* auditDecision(tx, member, result, "decisionLogged", { status });
        yield* notifyDecision(tx, member, row.id, targets, "decision.logged");
        return result;
      }),
    ),
  );

export function changeDecision(
  member: Member,
  id: string,
  input: typeof DecisionEdit.Type | typeof StatusChange.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const [joined] = yield* rowData(tx, member, id);
        if (!joined) return yield* Effect.fail(problem(404, "Decision not found."));
        const prior = joined.decision;
        if (member.agentRank && prior.recordedById !== member.id)
          return yield* Effect.fail(problem(403, "Agents can change decisions they recorded."));
        if (input.version !== undefined && input.version !== prior.version)
          return yield* Effect.fail(problem(409, "The decision version is stale.", prior.version));
        if (prior.status !== "proposed" || joined.project.archivedAt)
          return yield* Effect.fail(
            problem(
              409,
              "Only proposed decisions on active projects can be changed.",
              prior.version,
            ),
          );
        const status = "status" in input ? input.status : undefined;
        const { version: _version, issueIds, ...fields } = input as typeof DecisionEdit.Type;
        const changedFields = Object.keys(input).filter((key) => key !== "version");
        if (!changedFields.length)
          return yield* Effect.fail(problem(400, "Supply a decision field to change."));
        const targetId =
          fields.supersedesId === undefined ? prior.supersedesId : fields.supersedesId;
        const target =
          status === "accepted" || fields.supersedesId !== undefined
            ? yield* predecessor(tx, member, prior.projectId, targetId, id)
            : null;
        const before = yield* getDecision(tx, member, id);
        yield* saveRevision(tx, member, before, [], true);
        if (issueIds) yield* replaceIssueLinks(tx, member, prior, issueIds);
        yield* tx
          .update(decisions)
          .set({
            ...fields,
            ...(status ? statusFields(status, member.id) : {}),
            tags: fields.tags ? [...new Set(fields.tags)] : undefined,
            version: prior.version + 1,
            updatedAt: sql`now()`,
          })
          .where(and(orgScope(member), eq(decisions.id, id)));
        if (status === "accepted" && target) yield* supersede(tx, member, target, id);
        const result = yield* getDecision(tx, member, id);
        yield* saveRevision(tx, member, result, changedFields);
        yield* auditDecision(
          tx,
          member,
          result,
          status ? "decisionStatusChanged" : "decisionUpdated",
          { fields: changedFields },
        );
        const links =
          issueIds ?? (yield* linkedIssues(tx, member, [id])).get(id)?.map((row) => row.id) ?? [];
        yield* notifyDecision(
          tx,
          member,
          id,
          links,
          status ? "decision.status-changed" : "decision.updated",
        );
        return result;
      }),
    ),
  );
}

function revisions(db: Db, member: Member, id: string) {
  return Effect.gen(function* () {
    const current = yield* getDecision(db, member, id);
    const rows = yield* db
      .select()
      .from(decisionRevisions)
      .where(
        and(
          eq(decisionRevisions.organisationId, member.organisationId),
          eq(decisionRevisions.decisionId, id),
        ),
      )
      .orderBy(decisionRevisions.number);
    return {
      current,
      rows: rows.length
        ? rows
        : [
            {
              id,
              number: current.revisionNumber,
              savedAt: new Date(current.updatedAt),
              savedBy: current.recordedBy,
              snapshot: current,
              changedFields: [],
            },
          ],
    };
  });
}

function revisionMetadata(
  row: Pick<
    typeof decisionRevisions.$inferSelect,
    "id" | "number" | "savedAt" | "savedBy" | "changedFields"
  >,
) {
  return Effect.gen(function* () {
    const savedBy = yield* Schema.decodeUnknownEffect(DecisionMember)(row.savedBy);
    return {
      id: row.id,
      number: row.number,
      savedAt: dateTime(row.savedAt),
      changedFields: row.changedFields,
      savedBy,
    };
  });
}

export function listRevisions(
  member: Member,
  id: string,
  query: { limit?: number; cursor?: string },
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { rows } = yield* revisions(db, member, id);
    const key = (row: (typeof rows)[number]) => String(2147483647 - row.number).padStart(10, "0");
    const result = yield* page(
      rows.reverse(),
      { org: member.organisationId, decisionId: id },
      key,
      query.limit,
      query.cursor,
    );
    return { ...result, items: yield* Effect.forEach(result.items, revisionMetadata) };
  });
}

function getRevision(member: Member, id: string, revisionId: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { current, rows } = yield* revisions(db, member, id);
    const index = rows.findIndex((row) => row.id === revisionId);
    const row = rows[index];
    if (!row) return yield* Effect.fail(problem(404, "Decision revision not found."));
    return {
      revision: yield* revisionMetadata(row),
      snapshot: yield* Schema.decodeUnknownEffect(Detail)(row.snapshot),
      currentVersion: current.version,
      currentRevisionNumber: current.revisionNumber,
      previousRevisionId: rows[index - 1]?.id ?? null,
      nextRevisionId: rows[index + 1]?.id ?? null,
    };
  });
}

export const decisionGroup = HttpApiGroup.make("decisions")
  .add(
    HttpApiEndpoint.get("decisions", "/decisions", {
      query: Query,
      success: pageOf(Decision),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("decision", "/decisions/:id", {
      params: { id: Uuid },
      success: Detail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("updateDecision", "/decisions/:id", {
      params: { id: Uuid },
      payload: DecisionEdit,
      success: Detail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("changeStatus", "/decisions/:id/status", {
      params: { id: Uuid },
      payload: StatusChange,
      success: Detail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("listRevisions", "/decisions/:id/revisions", {
      params: { id: Uuid },
      query: PageQuery,
      success: pageOf(RevisionMetadata),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("getRevision", "/decisions/:id/revisions/:revisionId", {
      params: { id: Uuid, revisionId: Uuid },
      success: RevisionDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("logDecision", "/decisions", {
      payload: NewDecision,
      success: Detail.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const decisionHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof decisionGroup>,
) =>
  handlers
    .handle("decisions", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listDecisions(member, query))),
    )
    .handle("decision", ({ params }) =>
      handleDb(
        Effect.gen(function* () {
          const db = yield* Database;
          const member = yield* CurrentMember;
          return yield* getDecision(db, member, params.id);
        }),
      ),
    )
    .handle("updateDecision", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => changeDecision(member, params.id, payload)),
      ),
    )
    .handle("changeStatus", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => changeDecision(member, params.id, payload)),
      ),
    )
    .handle("listRevisions", ({ params, query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listRevisions(member, params.id, query))),
    )
    .handle("getRevision", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          getRevision(member, params.id, params.revisionId),
        ),
      ),
    )
    .handle("logDecision", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createDecision(member, payload))),
    );
