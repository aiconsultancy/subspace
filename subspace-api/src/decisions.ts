import { and, desc, eq, exists, inArray, isNull, or, sql } from "drizzle-orm";
import { alias } from "drizzle-orm/pg-core";
import { Effect, Schema, SchemaGetter } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { AgentRank, CurrentMember, memberKind, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { addActivity, lockIssues, readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { cursorTime, encodeCursor, page, readCursor } from "./paging";
import { endpointErrors, problem, validationProblem } from "./problems";
import { readProject } from "./projects";
import {
  comments,
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
  isUuid,
  Slug,
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

const decisionNotFound =
  "Decision was not found. Use an identifier or ID returned by list decisions.";
const decisionIdentifierPattern = /^[Dd]-[0-9]{5,10}-[A-Za-z][A-Za-z0-9]{1,9}$/u;
const DecisionIdentifier = Schema.String.check(Schema.isPattern(decisionIdentifierPattern));
export const DecisionRef = Schema.Union([Uuid, DecisionIdentifier]);
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
const decisionSummary = {
  id: decisions.id,
  identifier: decisions.identifier,
  title: decisions.title,
  status: decisions.status,
};
const DecisionSummary = Schema.Struct({
  id: Uuid,
  identifier: DecisionIdentifier,
  title: Line(200),
  status: Status,
});
const DecisionMember = Schema.Struct({
  userId: Uuid,
  displayName: Line(120),
  available: Schema.Boolean,
  kind: Maker,
});
const Decision = Schema.Struct({
  id: Uuid,
  identifier: DecisionIdentifier,
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
  madeBy: Schema.NullOr(DecisionMember),
  recordedByUserId: Uuid,
  recordedBy: DecisionMember,
  version: Version,
  revisionNumber: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
}).annotate({ identifier: "Decision" });
const Detail = Schema.Struct({
  decision: Schema.String,
  context: Schema.String,
  why: Schema.String,
  implications: Schema.String,
  alternatives: Schema.String,
  acceptedAt: Schema.NullOr(Timestamp),
  acceptedByUserId: Schema.NullOr(Uuid),
  rejectedAt: Schema.NullOr(Timestamp),
  rejectedByUserId: Schema.NullOr(Uuid),
  provenance: Provenance,
  ...Decision.fields,
}).annotate({ identifier: "DecisionDetail" });
const filterText = Schema.optional(Schema.String.check(Schema.isPattern(/^[^\0]*$/u)));
const queryStatus = Schema.String.pipe(
  Schema.decodeTo(Status, {
    decode: SchemaGetter.transform(
      (text: string) => text.trim().toLowerCase() as typeof Status.Type,
    ),
    encode: SchemaGetter.transform((status: typeof Status.Type) => status),
  }),
);
const queryMaker = Schema.String.pipe(
  Schema.decodeTo(Maker, {
    decode: SchemaGetter.transform(
      (text: string) => text.trim().toLowerCase() as typeof Maker.Type,
    ),
    encode: SchemaGetter.transform((maker: typeof Maker.Type) => maker),
  }),
);
export const Query = Schema.Struct({
  ...PageQuery,
  issue: filterText,
  project: filterText,
  status: Schema.optional(Schema.Array(queryStatus)),
  madeByType: Schema.optional(queryMaker),
  tag: filterText,
  q: filterText,
});
const { status: _status, limit: _limit, cursor: _cursor, ...countFilters } = Query.fields;
export const CountQuery = Schema.Struct(countFilters);
const DecisionCounts = Schema.Struct({
  proposed: Schema.Int,
  accepted: Schema.Int,
  rejected: Schema.Int,
  superseded: Schema.Int,
  total: Schema.Int,
}).annotate({ identifier: "DecisionCounts" });
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
  tags: Schema.optional(
    Schema.Array(Slug.check(Schema.isMaxLength(40))).check(Schema.isMaxLength(20)),
  ),
  issueIds: Schema.optional(Schema.Array(Uuid).check(Schema.isMaxLength(100))),
  supersedesId: Schema.optional(Schema.NullOr(Uuid)),
  madeByType: Schema.optional(Maker),
  madeByUserId: Schema.optional(Schema.NullOr(Uuid)),
  madeByAgentName: Schema.optional(Schema.NullOr(Line(200))),
  madeByAgentRank: Schema.optional(Schema.NullOr(AgentRank)),
  madeByAgentModel: Schema.optional(Schema.NullOr(Line(100))),
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

const projectJoin = (member: Member) =>
  and(eq(projects.organisationId, member.organisationId), eq(projects.id, decisions.projectId));
const actorJoin = (member: Member) =>
  and(eq(users.organisationId, member.organisationId), eq(users.id, decisions.recordedById));

const maker = alias(users, "decision_maker");

function joinedDecisions(db: Db, member: Member) {
  return db
    .select({
      decision: decisions,
      project: projects,
      actor: users,
      maker,
      cursorTime: cursorTime(decisions.createdAt),
    })
    .from(decisions)
    .innerJoin(projects, projectJoin(member))
    .innerJoin(users, actorJoin(member))
    .leftJoin(
      maker,
      and(eq(maker.organisationId, member.organisationId), eq(maker.id, decisions.madeByUserId)),
    );
}

function rowData(db: Db, member: Member, reference: string, project?: string) {
  return joinedDecisions(db, member).where(
    and(
      orgScope(member),
      isUuid(reference)
        ? eq(decisions.id, reference)
        : eq(decisions.identifier, reference.toUpperCase()),
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
          .select(decisionSummary)
          .from(decisions)
          .where(and(orgScope(member), inArray(decisions.id, ids))),
        (found) => new Map(found.map((row) => [row.id, row])),
      )
    : Effect.succeed(new Map<string, typeof DecisionSummary.Type>());
}

export function decisionDto(
  row: Joined,
  links: readonly (typeof DecisionIssue.Type)[],
  related: Map<string, typeof DecisionSummary.Type>,
) {
  const { decision: record, project, actor } = row;
  const provenance: typeof Provenance.Type = {
    madeByType: record.madeByType,
    madeByUserId: record.madeByUserId,
    madeByAgentName: record.madeByAgentName,
    madeByAgentModel: record.madeByAgentModel,
    madeByAgentRank: record.madeByAgentRank,
    evidenceKind: record.evidenceKind,
    evidenceReference: record.evidenceReference,
    evidenceExcerpt: record.evidenceExcerpt,
  };
  return {
    id: record.id,
    identifier: record.identifier,
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
    madeBy: row.maker ? decisionMember(row.maker) : null,
  };
}

export function getDecision(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    const [row] = yield* rowData(db, member, reference);
    if (!row) return yield* Effect.fail(problem(404, decisionNotFound));
    const { id } = row.decision;
    const links = yield* linkedIssues(db, member, [id]);
    return decisionDto(row, links.get(id) ?? [], yield* relatedSummaries(db, member, [row]));
  });
}

function decisionSearch(search: string) {
  const reference = search.toUpperCase();
  if (decisionIdentifierPattern.test(reference)) return eq(decisions.identifier, reference);
  if (/^D-[0-9]{0,10}$/u.test(reference)) {
    // Five or more digits name a short reference. The delimiter keeps it
    // distinct from a longer number and matches the same number in any project.
    const prefix = reference.length >= 7 ? `${reference}-` : reference;
    return sql`starts_with(${decisions.identifier}, ${prefix})`;
  }
  if (/^D-[0-9]{5,10}-(?:[A-Z])?$/u.test(reference))
    return sql`starts_with(${decisions.identifier}, ${reference})`;
  return or(
    ...[
      decisions.title,
      decisions.decision,
      decisions.context,
      decisions.why,
      decisions.implications,
      decisions.alternatives,
    ].map((column) => sql`strpos(lower(${column}), ${search}) > 0`),
  );
}

function decisionFilters(db: Db, member: Member, filters: typeof CountQuery.Type) {
  const { project, issue, q, tag, madeByType } = filters;
  if (issue !== undefined && !isUuid(issue) && !/^.+-[1-9][0-9]*$/u.test(issue))
    return Effect.fail(validationProblem("issue", "Use an issue ID or identifier such as BIO-42."));
  const search = q?.trim().toLowerCase();
  return Effect.succeed(
    and(
      orgScope(member),
      project !== undefined
        ? isUuid(project)
          ? eq(decisions.projectId, project)
          : eq(projects.key, project.toUpperCase())
        : undefined,
      madeByType ? eq(decisions.madeByType, madeByType) : undefined,
      tag !== undefined ? sql`${decisions.tags} @> ${JSON.stringify([tag])}::jsonb` : undefined,
      issue
        ? exists(
            db
              .select({ id: decisionIssues.issueId })
              .from(decisionIssues)
              .innerJoin(
                issues,
                and(
                  eq(issues.organisationId, member.organisationId),
                  eq(issues.id, decisionIssues.issueId),
                ),
              )
              .where(
                and(
                  eq(decisionIssues.organisationId, member.organisationId),
                  eq(decisionIssues.decisionId, decisions.id),
                  isUuid(issue) ? eq(issues.id, issue) : eq(issues.identifier, issue.toUpperCase()),
                ),
              ),
          )
        : undefined,
      search ? decisionSearch(search) : undefined,
    ),
  );
}

export const listDecisions = (member: Member, query: typeof Query.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const { limit = 50, cursor, status, ...queryFilters } = query;
          const anchor = yield* readCursor(cursor);
          const filters = yield* decisionFilters(tx, member, queryFilters);
          const rows = yield* joinedDecisions(tx, member)
            .where(
              and(
                filters,
                status?.length ? inArray(decisions.status, [...status]) : undefined,
                anchor
                  ? sql`(${decisions.createdAt}, ${decisions.id}) < (${anchor.time}::timestamptz, ${anchor.id}::uuid)`
                  : undefined,
              ),
            )
            .orderBy(desc(decisions.createdAt), desc(decisions.id))
            .limit(limit + 1);
          const items = rows.slice(0, limit);
          const links = yield* linkedIssues(
            tx,
            member,
            items.map(({ decision }) => decision.id),
          );
          const related = yield* relatedSummaries(tx, member, items);
          const last = items.at(-1);
          return {
            items: items.map((row) => decisionDto(row, links.get(row.decision.id) ?? [], related)),
            nextCursor:
              last && rows.length > limit ? encodeCursor(last.cursorTime, last.decision.id) : null,
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    ),
  );

export const countDecisions = (member: Member, query: typeof CountQuery.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const filters = yield* decisionFilters(tx, member, query);
          const rows = yield* tx
            .select({ status: decisions.status, count: sql<number>`count(*)::int` })
            .from(decisions)
            .innerJoin(projects, projectJoin(member))
            .innerJoin(users, actorJoin(member))
            .where(filters)
            .groupBy(decisions.status);
          const counts = { proposed: 0, accepted: 0, rejected: 0, superseded: 0, total: 0 };
          for (const { status, count } of rows) {
            counts[status] = count;
            counts.total += count;
          }
          return counts;
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

function resolveMaker(db: Db, member: Member, input: typeof NewDecision.Type) {
  return Effect.gen(function* () {
    const madeByType = input.madeByType ?? memberKind(member);
    if (madeByType === "human") {
      if (input.madeByAgentName || input.madeByAgentRank || input.madeByAgentModel)
        return yield* Effect.fail(problem(400, "Remove agent provenance from a human decision."));
      const id = input.madeByUserId ?? (!input.madeByType && !member.agentRank ? member.id : null);
      const needsEvidence =
        input.madeByType !== undefined || id !== member.id || member.agentRank !== null;
      if (!id || (needsEvidence && (!input.evidenceKind || !input.evidenceReference?.trim())))
        return yield* Effect.fail(
          problem(400, "A human decision needs a human maker, an evidence kind and reference."),
        );
      const [maker] = yield* db
        .select({ id: users.id })
        .from(users)
        .where(
          and(
            eq(users.organisationId, member.organisationId),
            eq(users.id, id),
            isNull(users.agentRank),
          ),
        );
      if (!maker)
        return yield* Effect.fail(problem(400, "Choose an existing human user as the maker."));
      if (input.evidenceKind === "subspaceComment") {
        const reference = input.evidenceReference;
        const [comment] =
          reference && isUuid(reference)
            ? yield* db
                .select({ id: comments.id })
                .from(comments)
                .where(
                  and(
                    eq(comments.organisationId, member.organisationId),
                    eq(comments.id, reference),
                    eq(comments.authorId, id),
                    isNull(comments.deletedAt),
                  ),
                )
                .for("share")
            : [];
        if (!comment)
          return yield* Effect.fail(
            problem(
              400,
              "Use the ID of an undeleted comment written by the decision's human maker.",
            ),
          );
      }
      return { madeByType, madeByUserId: id };
    }

    if (!member.agentRank)
      return yield* Effect.fail(
        problem(
          403,
          "Decision provenance rule 3: humans cannot record agent decisions. Ask the agent to record its own decision.",
        ),
      );
    if (input.madeByUserId)
      return yield* Effect.fail(
        problem(400, "Do not supply madeByUserId for an agent decision; the server resolves it."),
      );
    const relayed = member.relayedCrew;
    const rank = input.madeByAgentRank ?? (relayed ? "crew" : member.agentRank);
    const relay = rank === "crew" && (input.madeByAgentRank === "crew" || relayed != null);
    if (relay && member.agentRank !== "captain" && member.agentRank !== "rearAdmiral")
      return yield* Effect.fail(
        problem(
          403,
          "Decision provenance rule 3 (crew): only a registered captain or rear admiral may record a crew decision. Ask a registered agent to relay it.",
        ),
      );
    const name = input.madeByAgentName ?? (relay ? relayed?.name : member.displayName);
    const model = input.madeByAgentModel ?? (relay ? relayed?.model : member.agentModel);
    if (
      !name ||
      !model ||
      (input.madeByType &&
        !relay &&
        (!input.madeByAgentName || !input.madeByAgentModel || !input.madeByAgentRank))
    )
      return yield* Effect.fail(problem(400, "Supply the agent maker's name, rank and model."));
    if (
      !relay &&
      (rank !== member.agentRank || name.toLowerCase() !== member.displayName.toLowerCase())
    )
      return yield* Effect.fail(
        problem(403, "Use your own registered agent name and rank when recording your decision."),
      );
    return {
      madeByType,
      madeByUserId: relay ? null : member.id,
      madeByAgentName: name,
      madeByAgentRank: rank,
      madeByAgentModel: model,
    };
  });
}

export const createDecision = (member: Member, input: typeof NewDecision.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { issueIds, ...fields } = input;
        const provenance = yield* resolveMaker(tx, member, input);
        const { project } = yield* readProject(tx, member, fields.projectId);
        if (project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the project before logging decisions."),
          );
        const status = fields.status ?? "proposed";
        const target = yield* predecessor(tx, member, project.id, fields.supersedesId);
        const number = project.nextDecisionNumber;
        if (number === 2_147_483_647)
          return yield* Effect.fail(problem(409, "This project has reached its decision limit."));
        yield* tx
          .update(projects)
          .set({ nextDecisionNumber: number + 1 })
          .where(
            and(eq(projects.organisationId, member.organisationId), eq(projects.id, project.id)),
          );
        const [row] = yield* tx
          .insert(decisions)
          .values({
            ...fields,
            ...provenance,
            ...statusFields(status, member.id),
            organisationId: member.organisationId,
            recordedById: member.id,
            identifier: `D-${String(number).padStart(5, "0")}-${project.key}`,
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
  reference: string,
  input: typeof DecisionEdit.Type | typeof StatusChange.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const [joined] = yield* rowData(tx, member, reference);
        if (!joined) return yield* Effect.fail(problem(404, decisionNotFound));
        const prior = joined.decision;
        const { id } = prior;
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

function revisions(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    const current = yield* getDecision(db, member, reference);
    const { id } = current;
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
    const { current, rows } = yield* revisions(db, member, id);
    const key = (row: (typeof rows)[number]) => String(2147483647 - row.number).padStart(10, "0");
    const result = yield* page(
      rows.reverse(),
      { org: member.organisationId, decisionId: current.id },
      key,
      query.limit,
      query.cursor,
    );
    return { ...result, items: yield* Effect.forEach(result.items, revisionMetadata) };
  });
}

// Snapshots saved before `madeBy` existed name a human maker only through `madeByUserId`,
// which was always the recording member, so their saved `recordedBy` is the maker.
export function withMadeBy(snapshot: unknown) {
  if (typeof snapshot !== "object" || snapshot === null || "madeBy" in snapshot) return snapshot;
  const legacy = snapshot as { madeByUserId?: unknown; recordedBy?: unknown };
  return {
    ...snapshot,
    madeBy: typeof legacy.madeByUserId === "string" ? legacy.recordedBy : null,
  };
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
      snapshot: yield* Schema.decodeUnknownEffect(Detail)(withMadeBy(row.snapshot)),
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
    HttpApiEndpoint.get("decisionCounts", "/decisions/counts", {
      query: CountQuery,
      success: DecisionCounts,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("decision", "/decisions/:id", {
      params: { id: DecisionRef },
      success: Detail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("updateDecision", "/decisions/:id", {
      params: { id: DecisionRef },
      payload: DecisionEdit,
      success: Detail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("changeStatus", "/decisions/:id/status", {
      params: { id: DecisionRef },
      payload: StatusChange,
      success: Detail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("listRevisions", "/decisions/:id/revisions", {
      params: { id: DecisionRef },
      query: PageQuery,
      success: pageOf(RevisionMetadata),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("getRevision", "/decisions/:id/revisions/:revisionId", {
      params: { id: DecisionRef, revisionId: Uuid },
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
    .handle("decisionCounts", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => countDecisions(member, query))),
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
