import { and, asc, desc, eq, inArray, isNull, or, sql, type SQL } from "drizzle-orm";
import { Effect } from "effect";
import { withCrew } from "./crew";
import { Database } from "./db";
import { estimatePoints } from "./estimates";
import { requireFleetOwnership } from "./fleet-ownership";
import { memberKind, type Member } from "./identity";
import {
  type CommentInput,
  type EditIssue,
  type GroupQuery,
  type IssueQuery,
  type NewIssue,
  type RelationList,
} from "./issue-contract";
import { issueConditions, type IssueFilter } from "./issue-filter";
import { issuePriorityOrder, issueTimeOrder } from "./issue-order";
import { localMedia } from "./issue-media";
import { markdownMentions, mentionPeople, mentionStatuses, validatedMentions } from "./mentions";
import { byKey, cursorAfter, cursorAnchor, page } from "./paging";
import { problem } from "./problems";
import { person, readProject, takeIssueNumber } from "./projects";
import {
  comments,
  decisions,
  pullRequests,
  pullRequestLinks,
  cycles,
  issueActivity,
  issueAliases,
  issueRelations,
  organisations,
  issueLabels,
  issues,
  labels,
  milestones,
  projects,
  states,
  todos,
  stateTypes,
  users,
} from "./schema";
import { todoDtos, todoRows } from "./todo-model";
import { trackerState } from "./trackers";
import { dateTime, dateTimeOrNull, isUuid } from "./validation";

type Db = (typeof Database)["Service"];
type IssueRow = typeof issues.$inferSelect;
type ActivityRow = typeof issueActivity.$inferSelect;
type StateRow = typeof states.$inferSelect;

const inOrg = (member: Pick<Member, "organisationId">) =>
  eq(issues.organisationId, member.organisationId);
const matchIssue = (value: string) =>
  isUuid(value) ? eq(issues.id, value) : eq(issues.identifier, value.toUpperCase());

export const readIssue = (db: Db, member: Pick<Member, "organisationId">, reference: string) =>
  Effect.gen(function* () {
    const id = isUuid(reference)
      ? undefined
      : (yield* db
          .select({ id: issueAliases.issueId })
          .from(issueAliases)
          .where(
            and(
              eq(issueAliases.organisationId, member.organisationId),
              eq(issueAliases.identifier, reference.toUpperCase()),
            ),
          ))[0]?.id;
    const [row] = yield* db
      .select()
      .from(issues)
      .where(and(inOrg(member), matchIssue(id ?? reference)));
    if (!row) return yield* Effect.fail(problem(404, "Issue not found."));
    return row;
  });

export const activityDto = (event: ActivityRow, actor: Member | null | undefined) => ({
  id: event.id,
  kind: event.kind,
  occurredAt: dateTime(event.createdAt),
  actor: actor
    ? { type: memberKind(actor), userId: actor.id, person: person(actor)!, label: null }
    : { type: "integration" as const, userId: null, person: null, label: "GitHub" },
  data: event.data,
});

function stateSummary(row: StateRow) {
  return { id: row.id, name: row.name, type: row.type };
}

function issueJoins(org: string) {
  return {
    project: and(eq(projects.organisationId, org), eq(projects.id, issues.projectId)),
    state: and(
      eq(states.organisationId, org),
      eq(states.id, issues.stateId),
      eq(states.projectId, issues.projectId),
    ),
    cycle: and(
      eq(cycles.organisationId, org),
      eq(cycles.id, issues.cycleId),
      eq(cycles.projectId, issues.projectId),
    ),
  };
}

function joinedIssues(db: Db, org: string) {
  const joins = issueJoins(org);
  return db
    .select({ issue: issues, project: projects, state: states, cycle: cycles })
    .from(issues)
    .innerJoin(projects, joins.project)
    .innerJoin(states, joins.state)
    .leftJoin(cycles, joins.cycle);
}

type Joined = Effect.Success<ReturnType<typeof joinedIssues>>[number];
type Progress = {
  issueId: string;
  total: number;
  done: number;
  canceled: number;
  duplicate: number;
  sample: { id: string; stateType: StateRow["type"] }[];
  hasMore: boolean;
};

function issueProgress(db: Db, org: string, ids: string[]) {
  // Traverse active descendants in the database and return at most 40 samples per issue.
  const maxSamples = 40;
  return db.execute<Progress>(sql`
    with recursive descendants(root_id, id, project_id, state_id, type, depth) as (
      select parent_id, id, project_id, state_id, type, 1
      from ${issues}
      where ${and(eq(issues.organisationId, org), inArray(issues.parentId, ids), isNull(issues.archivedAt))}
      union all
      select tree.root_id, child.id, child.project_id, child.state_id, child.type, tree.depth + 1
      from descendants tree
      join ${issues} child on child.parent_id = tree.id
      where child.organisation_id = ${org} and child.archived_at is null
    ) cycle id set cyclic using traversal,
    ranked as (
      select tree.root_id, tree.id, tree.type, workflow.type as state_type,
        row_number() over (partition by tree.root_id, tree.type order by tree.depth, tree.id) as rank
      from descendants tree
      join ${states} workflow on workflow.organisation_id = ${org}
        and workflow.project_id = tree.project_id and workflow.id = tree.state_id
      where not tree.cyclic
    )
    select root_id as "issueId",
      count(*) filter (where type = 'issue' and state_type not in ('canceled', 'duplicate'))::int as total,
      count(*) filter (where type = 'issue' and state_type = 'completed')::int as done,
      count(*) filter (where type = 'issue' and state_type = 'canceled')::int as canceled,
      count(*) filter (where type = 'issue' and state_type = 'duplicate')::int as duplicate,
      coalesce(json_agg(json_build_object('id', id, 'stateType', state_type) order by rank)
        filter (where type = 'issue' and rank <= ${maxSamples}), '[]'::json) as sample,
      count(*) filter (where type = 'issue') > ${maxSamples} as "hasMore"
    from ranked group by root_id
  `);
}

function issueSnapshot(db: Db, member: Member, rows: readonly Joined[], detail?: Joined) {
  const org = member.organisationId;
  const ids = [...new Set(rows.map(({ issue }) => issue.id))];
  return Effect.gen(function* () {
    const labelRows = ids.length
      ? yield* db
          .select({ id: issueLabels.issueId, label: labels })
          .from(issueLabels)
          .innerJoin(
            labels,
            and(eq(labels.organisationId, org), eq(labels.id, issueLabels.labelId)),
          )
          .where(and(eq(issueLabels.organisationId, org), inArray(issueLabels.issueId, ids)))
          .orderBy(asc(labels.name), asc(labels.id))
      : [];
    const labelsByIssue = Map.groupBy(labelRows, ({ id }) => id);
    const counting = ids.length
      ? yield* db
          .select({ id: comments.issueId, count: sql<number>`count(*)::int` })
          .from(comments)
          .where(
            and(
              eq(comments.organisationId, org),
              inArray(comments.issueId, ids),
              isNull(comments.deletedAt),
            ),
          )
          .groupBy(comments.issueId)
      : [];
    const commentsByIssue = new Map(counting.map(({ id, count }) => [id, count]));
    const todoStats = ids.length
      ? yield* db
          .select({
            issueId: todos.issueId,
            total: sql<number>`count(*)::int`,
            done: sql<number>`count(${todos.doneAt})::int`,
          })
          .from(todos)
          .where(and(eq(todos.organisationId, org), inArray(todos.issueId, ids)))
          .groupBy(todos.issueId)
      : [];
    const todoCounts = new Map(
      todoStats.map(({ issueId, total, done }) => [issueId, { total, done }]),
    );
    const heads = ids.length
      ? yield* db
          .selectDistinctOn([issueActivity.issueId])
          .from(issueActivity)
          .where(and(eq(issueActivity.organisationId, org), inArray(issueActivity.issueId, ids)))
          .orderBy(
            asc(issueActivity.issueId),
            desc(issueActivity.createdAt),
            desc(issueActivity.id),
          )
      : [];
    const headByIssue = new Map(heads.map((event) => [event.issueId, event]));
    const blocked = ids.length
      ? yield* db
          .selectDistinct({ id: issueRelations.targetId })
          .from(issueRelations)
          .innerJoin(
            issues,
            and(eq(issues.organisationId, org), eq(issues.id, issueRelations.sourceId)),
          )
          .innerJoin(
            states,
            and(
              eq(states.organisationId, org),
              eq(states.id, issues.stateId),
              eq(states.projectId, issues.projectId),
            ),
          )
          .where(
            and(
              eq(issueRelations.organisationId, org),
              inArray(issueRelations.targetId, ids),
              eq(issueRelations.kind, "blocks"),
              isNull(issues.archivedAt),
              sql`${states.type} not in ('completed', 'canceled', 'duplicate')`,
            ),
          )
      : [];
    const linkedPulls = ids.length
      ? yield* db
          .select({
            issueId: pullRequestLinks.issueId,
            kind: pullRequestLinks.kind,
            state: pullRequests.state,
            seen: pullRequests.lastObservedAt,
          })
          .from(pullRequestLinks)
          .innerJoin(
            pullRequests,
            and(
              eq(pullRequests.organisationId, org),
              eq(pullRequests.id, pullRequestLinks.pullRequestId),
            ),
          )
          .where(
            and(
              eq(pullRequestLinks.organisationId, org),
              inArray(pullRequestLinks.issueId, ids),
              eq(pullRequestLinks.active, true),
            ),
          )
      : [];
    const people = new Set<string>();
    for (const { issue } of rows) {
      if (issue.assigneeId) people.add(issue.assigneeId);
      if (issue.ownerId) people.add(issue.ownerId);
    }
    for (const head of heads) if (head.actorId) people.add(head.actorId);
    if (detail) {
      people.add(detail.issue.createdById);
      if (detail.issue.archivedById) people.add(detail.issue.archivedById);
      for (const { userId } of markdownMentions(detail.issue.description)) people.add(userId);
    }
    const members = yield* mentionPeople(db, member, [...people]);
    const progress = ids.length ? (yield* issueProgress(db, org, ids)).rows : [];
    return {
      membersById: new Map(members.map((user) => [user.id, user])),
      labelsByIssue,
      commentsByIssue,
      headByIssue,
      blocked: new Set(blocked.map(({ id }) => id)),
      pullSummary: Map.groupBy(linkedPulls, ({ issueId }) => issueId),
      todoCounts,
      progress: new Map(progress.map(({ issueId, ...entry }) => [issueId, entry])),
    };
  });
}

type Snapshot = Effect.Success<ReturnType<typeof issueSnapshot>>;

function issueDto(row: Joined, snapshot: Snapshot) {
  const issue = row.issue;
  const event = snapshot.headByIssue.get(issue.id);
  const pulls = snapshot.pullSummary.get(issue.id);
  const actor = event?.actorId ? snapshot.membersById.get(event.actorId) : undefined;
  return {
    id: issue.id,
    identifier: issue.identifier,
    projectId: issue.projectId,
    projectKey: row.project.key,
    type: issue.type,
    title: issue.title,
    summary: issue.summary,
    state: stateSummary(row.state),
    assignee: person(issue.assigneeId ? snapshot.membersById.get(issue.assigneeId) : null),
    readyForAgents: issue.readyForAgents,
    owner: person(issue.ownerId ? snapshot.membersById.get(issue.ownerId) : null),
    priority: issue.priority,
    estimate: issue.estimate,
    dueDate: issue.dueDate,
    labels: (snapshot.labelsByIssue.get(issue.id) ?? []).map(({ label }) => label),
    milestoneId: issue.milestoneId,
    parentId: issue.parentId,
    blocked: snapshot.blocked.has(issue.id),
    archivedAt: dateTimeOrNull(issue.archivedAt),
    createdAt: dateTime(issue.createdAt),
    updatedAt: dateTime(issue.updatedAt),
    version: issue.version,
    progress:
      snapshot.progress.get(issue.id) ??
      (issue.type === "tracker"
        ? { total: 0, done: 0, canceled: 0, duplicate: 0, sample: [], hasMore: false }
        : null),
    commentCount: snapshot.commentsByIssue.get(issue.id) ?? 0,
    todoCounts: snapshot.todoCounts.get(issue.id) ?? { total: 0, done: 0 },
    lastActivityAt: dateTime(issue.lastActivityAt),
    lastActivity: event ? activityDto(event, actor) : null,
    relayedCrew: issue.relayedCrew,
    provenance: null,
    cycle: row.cycle,
    cycleId: issue.cycleId,
    pullRequestSummary: pulls
      ? {
          total: pulls.length,
          open: pulls.filter(({ state }) => state === "open").length,
          closed: pulls.filter(({ state }) => state === "closed").length,
          merged: pulls.filter(({ state }) => state === "merged").length,
          closingPending: pulls.filter(({ state, kind }) => state === "open" && kind === "closes")
            .length,
          checksState: null,
          lastObservedAt: dateTime(
            new Date(pulls.reduce((latest, { seen }) => Math.max(latest, seen.valueOf()), 0)),
          ),
        }
      : null,
  };
}

function issueReference(row: Joined) {
  const { issue, state } = row;
  return {
    id: issue.id,
    identifier: issue.identifier,
    title: issue.title,
    summary: issue.summary,
    type: issue.type,
    state: stateSummary(state),
    archived: Boolean(issue.archivedAt),
  };
}

export function detailFrom(db: Db, member: Member, id: string) {
  return Effect.gen(function* () {
    const org = member.organisationId;
    const [row] = yield* joinedIssues(db, org).where(and(inOrg(member), eq(issues.id, id)));
    if (!row) return yield* Effect.fail(problem(404, "Issue not found."));
    const issue = row.issue;
    const [duplicateLink] = yield* db
      .select({ id: issueRelations.targetId })
      .from(issueRelations)
      .where(
        and(
          eq(issueRelations.organisationId, org),
          eq(issueRelations.kind, "duplicateOf"),
          eq(issueRelations.sourceId, id),
        ),
      )
      .limit(1);
    const linkedIds = [issue.parentId, duplicateLink?.id].filter((target): target is string =>
      Boolean(target),
    );
    const linked = linkedIds.length
      ? yield* joinedIssues(db, org).where(and(inOrg(member), inArray(issues.id, linkedIds)))
      : [];
    const parent = linked.find(({ issue: target }) => target.id === issue.parentId);
    const duplicate = linked.find(({ issue: target }) => target.id === duplicateLink?.id);
    const filters = { parent: id, order: "manual" } as const;
    const result = yield* issuePage(
      joinedIssues(db, org),
      and(inOrg(member), eq(issues.parentId, id), isNull(issues.archivedAt)),
      scopeCursor(member, filters),
      filters.order,
    );
    const snapshot = yield* issueSnapshot(db, member, [row, ...result.items], row);
    return {
      ...issueDto(row, snapshot),
      description: issue.description,
      mentions: mentionStatuses(issue.description, snapshot.membersById),
      todos: yield* todoDtos(db, member, yield* todoRows(db, member, issue.id)),
      createdBy: person(snapshot.membersById.get(issue.createdById)),
      startedAt: dateTimeOrNull(issue.startedAt),
      completedAt: dateTimeOrNull(issue.completedAt),
      canceledAt: dateTimeOrNull(issue.canceledAt),
      archivedBy: issue.archivedById ? person(snapshot.membersById.get(issue.archivedById)) : null,
      parent: parent ? issueReference(parent) : null,
      duplicateOf: duplicate ? issueReference(duplicate) : null,
      children: { ...result, items: result.items.map((child) => issueDto(child, snapshot)) },
      relations: null,
    };
  });
}

export function issueRowsById(db: Db, member: Member, ids: readonly string[]) {
  return Effect.gen(function* () {
    if (!ids.length) return [];
    const rows = yield* joinedIssues(db, member.organisationId).where(
      and(inOrg(member), inArray(issues.id, ids)),
    );
    const snapshot = yield* issueSnapshot(db, member, rows);
    const rowById = new Map(rows.map((row) => [row.issue.id, row]));
    return ids.flatMap((id) => {
      const row = rowById.get(id);
      return row ? [issueDto(row, snapshot)] : [];
    });
  });
}

export const getIssue = (member: Member, reference: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const row = yield* readIssue(tx, member, reference);
          return yield* detailFrom(tx, member, row.id);
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });

function issueKey(order: IssueFilter["order"] = "created") {
  return ({ issue }: Joined) => {
    const descendingTime = (time: Date) =>
      String(Number.MAX_SAFE_INTEGER - time.valueOf()).padStart(16, "0");
    const tie = `${descendingTime(issue.createdAt)}:${issue.id}`;
    if (order === "manual") return `${String(issue.number).padStart(12, "0")}:${issue.id}`;
    if (order === "priority") return `${issue.priority || 5}:${tie}`;
    if (order === "activity") return `${descendingTime(issue.lastActivityAt)}:${tie}`;
    return tie;
  };
}

function scopeCursor(member: Member, filters: IssueFilter) {
  return {
    org: member.organisationId,
    actor: member.id,
    ...filters,
    order: filters.order ?? "created",
    includeArchived: Boolean(filters.includeArchived),
  };
}

const createdOrder = issueTimeOrder(issues.createdAt);
const activityOrder = issueTimeOrder(issues.lastActivityAt);
const priorityOrder = issuePriorityOrder(issues.priority);

function issuePage(
  query: ReturnType<typeof joinedIssues>,
  where: SQL | undefined,
  scope: object,
  order: IssueFilter["order"] = "created",
  limit = 50,
  cursor?: string,
) {
  return Effect.gen(function* () {
    const anchor = yield* cursorAnchor(scope, cursor);
    const keys =
      order === "manual"
        ? [issues.number, issues.id]
        : order === "priority"
          ? [priorityOrder, createdOrder, issues.id]
          : order === "activity"
            ? [activityOrder, createdOrder, issues.id]
            : [createdOrder, issues.id];
    let after: SQL | undefined;
    if (anchor !== undefined) {
      const fields = anchor.split(":");
      const id = fields.pop();
      if (
        fields.length !== keys.length - 1 ||
        !id ||
        !isUuid(id) ||
        fields.some((field) => !/^\d{1,16}$/u.test(field) || !Number.isSafeInteger(Number(field)))
      ) {
        return yield* Effect.fail(problem(400, "The cursor position is invalid."));
      }
      const numbers = fields.map((field, index) => {
        const time = order !== "manual" && (order !== "priority" || index > 0);
        return sql`${time ? Number(field) - Number.MAX_SAFE_INTEGER : Number(field)}::bigint`;
      });
      after = sql`(${sql.join(keys, sql`, `)}) > (${sql.join([...numbers, sql`${id}::uuid`], sql`, `)})`;
    }
    const selected = yield* query
      .where(and(where, after))
      .orderBy(...keys)
      .limit(limit + 1);
    const items = selected.slice(0, limit);
    return {
      items,
      nextCursor:
        selected.length > limit ? cursorAfter(scope, issueKey(order)(items[limit - 1]!)) : null,
    };
  });
}

const resolveParent = (db: Db, member: Member, parent?: string) =>
  parent && parent !== "none"
    ? Effect.map(readIssue(db, member, parent), (row) => row.id)
    : Effect.succeed(parent);

export const listIssues = (member: Member, query: typeof IssueQuery.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const { cursor, limit, ...filters } = query;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const parent = yield* resolveParent(tx, member, filters.parent);
          const { where } = yield* issueConditions(tx, member, filters, parent);
          const result = yield* issuePage(
            joinedIssues(tx, member.organisationId),
            where,
            scopeCursor(member, filters),
            filters.order,
            limit,
            cursor,
          );
          const snapshot = yield* issueSnapshot(tx, member, result.items);
          return { ...result, items: result.items.map((row) => issueDto(row, snapshot)) };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });

export const memberQueue = (member: Member, query: { limit?: number; cursor?: string }) =>
  listIssues(member, {
    ...query,
    assignee: ["me"],
    stateType: ["triage", "backlog", "unstarted", "started"],
    order: "priority",
  });

export const groupIssues = (member: Member, query: typeof GroupQuery.Type) =>
  Effect.gen(function* () {
    const { groupBy, limitPerGroup, ...filters } = query;
    if (groupBy === "state" && !filters.project)
      return yield* Effect.fail(problem(400, "State groups require a project."));
    const db = yield* Database;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const org = member.organisationId;
          const parent = yield* resolveParent(tx, member, filters.parent);
          const { where, project } = yield* issueConditions(tx, member, filters, parent);
          const scopedProject = groupBy === "state" ? project : null;
          const dimension = scopedProject ? states.id : states.type;
          const joins = issueJoins(org);
          const counts = yield* tx
            .select({ group: dimension, count: sql<number>`count(*)::int` })
            .from(issues)
            .innerJoin(projects, joins.project)
            .innerJoin(states, joins.state)
            .leftJoin(cycles, joins.cycle)
            .where(where)
            .groupBy(dimension);
          const countsByGroup = new Map(counts.map(({ group, count }) => [group, count]));
          const scopedStates = scopedProject
            ? (yield* tx
                .select()
                .from(states)
                .where(
                  and(
                    eq(states.organisationId, member.organisationId),
                    eq(states.projectId, scopedProject.id),
                  ),
                )).sort(
                byKey(
                  (state) =>
                    `${stateTypes.indexOf(state.type)}:${String(state.position).padStart(8, "0")}`,
                ),
              )
            : [];
          const groups = scopedProject
            ? scopedStates.map((state) => ({ stateId: state.id, stateType: state.type }))
            : stateTypes.map((stateType) => ({ stateId: null, stateType }));
          const pages = yield* Effect.forEach(groups, ({ stateId, stateType }) =>
            Effect.gen(function* () {
              const count = countsByGroup.get(stateId ?? stateType) ?? 0;
              if (
                count === 0 ||
                limitPerGroup === 0 ||
                ["completed", "canceled", "duplicate"].includes(stateType)
              ) {
                return { stateId, stateType, count, items: [], nextCursor: null };
              }
              const continuation = stateId
                ? { ...filters, state: undefined, stateId }
                : { ...filters, stateType: [stateType] };
              const result = yield* issuePage(
                joinedIssues(tx, org),
                and(where, stateId ? eq(states.id, stateId) : eq(states.type, stateType)),
                scopeCursor(member, continuation),
                filters.order,
                limitPerGroup ?? 25,
              );
              return { stateId, stateType, count, ...result };
            }),
          );
          const snapshot = yield* issueSnapshot(
            tx,
            member,
            pages.flatMap(({ items }) => items),
          );
          return {
            groupBy,
            groups: pages.map((entry) => ({
              ...entry,
              items: entry.items.map((row) => issueDto(row, snapshot)),
            })),
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });

function activePeople(
  db: Db,
  member: Member,
  fields: { ownerId?: string | null; assigneeId?: string | null },
) {
  return Effect.gen(function* () {
    const ids = [...new Set([fields.ownerId, fields.assigneeId].filter((id) => id != null))];
    if (!ids.length) return;
    const valid = yield* db
      .select({ id: users.id, agentRank: users.agentRank })
      .from(users)
      .where(
        and(
          eq(users.organisationId, member.organisationId),
          eq(users.active, true),
          inArray(users.id, ids),
        ),
      );
    if (
      valid.length !== ids.length ||
      valid.some((user) => user.id === fields.ownerId && user.agentRank)
    )
      return yield* Effect.fail(
        problem(
          400,
          "Owners must be active humans and assignees must be active organisation members.",
        ),
      );
  });
}

const terminalStateTypes: ReadonlySet<string> = new Set(["completed", "canceled", "duplicate"]);
export const isTerminalState = (type: string) => terminalStateTypes.has(type);

/**
 * Works out the ready-for-agents flag a write leaves on an issue. Only an unassigned, open issue
 * that is not a tracker may be marked ready. A write that leaves an assignee, a terminal state or
 * a tracker clears the flag in the same write.
 */
export function readyForAgentsAfter(
  current: Pick<IssueRow, "assigneeId" | "readyForAgents"> | null,
  input: { assigneeId?: string | null; readyForAgents?: boolean },
  result: { tracker: boolean; terminal: boolean },
) {
  const assigneeId =
    input.assigneeId === undefined ? (current?.assigneeId ?? null) : input.assigneeId;
  if (input.readyForAgents === true) {
    if (assigneeId !== null)
      return Effect.fail(
        problem(400, "Remove the assignee before marking the issue ready for agents."),
      );
    if (result.tracker)
      return Effect.fail(problem(400, "Trackers cannot be marked ready for agents."));
    if (result.terminal)
      return Effect.fail(problem(400, "Reopen the issue before marking it ready for agents."));
  }
  const readyForAgents =
    assigneeId === null && !result.tracker && !result.terminal
      ? (input.readyForAgents ?? current?.readyForAgents ?? false)
      : false;
  return Effect.succeed({
    readyForAgents,
    changed: current !== null && current.readyForAgents !== readyForAgents,
  });
}

/** Clears the flag in a write that leaves the issue terminal or archived, and names the change. */
export function clearReadyForAgents(issue: Pick<IssueRow, "readyForAgents">, clears: boolean) {
  const cleared = clears && issue.readyForAgents;
  return {
    set: cleared ? { readyForAgents: false } : {},
    fields: cleared ? ["readyForAgents"] : [],
  };
}

function planningRefs(
  db: Db,
  member: Member,
  projectId: string,
  fields: { milestoneId?: string | null; cycleId?: string | null },
) {
  return Effect.gen(function* () {
    if (fields.milestoneId) {
      const [row] = yield* db
        .select({ id: milestones.id })
        .from(milestones)
        .where(
          and(
            eq(milestones.organisationId, member.organisationId),
            eq(milestones.projectId, projectId),
            eq(milestones.id, fields.milestoneId),
          ),
        );
      if (!row)
        return yield* Effect.fail(problem(400, "The milestone must belong to this project."));
    }
    if (fields.cycleId) {
      const [row] = yield* db
        .select({ state: cycles.state })
        .from(cycles)
        .where(
          and(
            eq(cycles.organisationId, member.organisationId),
            eq(cycles.projectId, projectId),
            eq(cycles.id, fields.cycleId),
          ),
        );
      if (!row || row.state === "closed")
        return yield* Effect.fail(
          problem(400, "Select a current or upcoming cycle from this project."),
        );
    }
  });
}

function validateState(db: Db, member: Member, projectId: string, stateId: string) {
  return Effect.gen(function* () {
    const [state] = yield* db
      .select()
      .from(states)
      .where(
        and(
          eq(states.organisationId, member.organisationId),
          eq(states.projectId, projectId),
          eq(states.id, stateId),
        ),
      );
    if (!state || state.type === "duplicate")
      return yield* Effect.fail(problem(400, "Select a writable state from this project."));
    return state.type;
  });
}

function linkParent(
  db: Db,
  member: Member,
  reference: string | null | undefined,
  movingId?: string,
) {
  return Effect.gen(function* () {
    if (!reference) return reference;
    const parent = yield* readIssue(db, member, reference);
    let current: IssueRow | undefined = parent;
    while (current) {
      if (current.id === movingId || current.archivedAt) {
        return yield* Effect.fail(problem(409, "The parent is archived or would create a cycle."));
      }
      current = current.parentId ? yield* readIssue(db, member, current.parentId) : undefined;
    }
    return parent.id;
  });
}

function writeLabels(db: Db, member: Member, issueId: string, values?: readonly string[]) {
  return Effect.gen(function* () {
    if (values === undefined) return;
    const ids = [...new Set(values)];
    if (ids.length) {
      const present = yield* db
        .select({ id: labels.id })
        .from(labels)
        .where(
          and(
            eq(labels.organisationId, member.organisationId),
            eq(labels.state, "active"),
            inArray(labels.id, ids),
          ),
        );
      if (present.length !== ids.length)
        return yield* Effect.fail(
          problem(400, "Labels must be active and belong to the organisation."),
        );
    }
    yield* db
      .delete(issueLabels)
      .where(
        and(
          eq(issueLabels.organisationId, member.organisationId),
          eq(issueLabels.issueId, issueId),
        ),
      );
    if (ids.length)
      yield* db
        .insert(issueLabels)
        .values(
          ids.map((labelId) => ({ organisationId: member.organisationId, issueId, labelId })),
        );
  });
}

export const addActivity = (
  db: Db,
  member: Pick<Member, "organisationId" | "source" | "relayedCrew"> & { id: string | null },
  issueId: string,
  kind: string,
  data: Record<string, unknown> = {},
) =>
  db.insert(issueActivity).values({
    organisationId: member.organisationId,
    issueId,
    actorId: member.id,
    source: member.id ? (member.source ?? "rest") : "github",
    kind,
    data: member.relayedCrew ? { ...data, relayedCrew: member.relayedCrew } : data,
  });

export function datesForState(state: StateRow["type"], existing?: IssueRow) {
  const now = new Date();
  return {
    startedAt: existing?.startedAt ?? (state === "started" || state === "completed" ? now : null),
    completedAt: state === "completed" ? (existing?.completedAt ?? now) : null,
    canceledAt: state === "canceled" ? (existing?.canceledAt ?? now) : null,
  };
}

export const createIssue = (member: Member, payload: typeof NewIssue.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { projectId, labelIds, parentId, relations, ...editable } = payload;
        const actor = yield* withCrew(member, payload.relayedCrew);
        const { project } = yield* readProject(tx, member, projectId);
        if (project.archivedAt)
          return yield* Effect.fail(problem(409, "Unarchive the project before adding issues."));
        yield* requireFleetOwnership(tx, member, project.id, editable);
        const stateId = editable.stateId ?? project.defaultStateId;
        const state = yield* validateState(tx, member, project.id, stateId);
        if (
          payload.type === "tracker" &&
          (payload.stateId || payload.assigneeId || payload.estimate != null)
        ) {
          return yield* Effect.fail(
            problem(400, "Trackers derive their state and cannot have an assignee or estimate."),
          );
        }
        if (
          editable.estimate != null &&
          !estimatePoints[project.estimateScale].includes(editable.estimate)
        )
          return yield* Effect.fail(problem(400, "Choose an estimate on the project scale."));
        const ready = yield* readyForAgentsAfter(null, editable, {
          tracker: payload.type === "tracker",
          terminal: isTerminalState(state) || Boolean(relations?.duplicateOf),
        });
        yield* activePeople(tx, member, editable);
        yield* planningRefs(tx, member, project.id, editable);
        const parent = yield* linkParent(tx, member, parentId);
        const [counter] = yield* takeIssueNumber(tx, member, project.id);
        if (!counter)
          return yield* Effect.fail(problem(409, "The project changed during issue creation."));
        const number = counter.nextIssueNumber - 1;
        const [issue] = yield* tx
          .insert(issues)
          .values({
            ...editable,
            ...datesForState(state),
            readyForAgents: ready.readyForAgents,
            organisationId: member.organisationId,
            projectId: project.id,
            identifier: `${project.key}-${number}`,
            number,
            stateId,
            parentId: parent,
            cycleJoinedAt: editable.cycleId ? new Date() : null,
            createdById: member.id,
          })
          .returning();
        if (!issue) return yield* Effect.fail(problem(500, "The issue could not be created."));
        yield* localMedia(tx, member, issue.id, editable.description, true);
        yield* writeLabels(tx, member, issue.id, labelIds);
        if (relations)
          yield* replaceRelations(tx, member, issue, relations, null, relations.duplicateOf);
        yield* addActivity(tx, actor, issue.id, "issue.created", {
          mentionUserIds: yield* validatedMentions(tx, member, editable.description),
        });
        yield* refreshTrackers(tx, actor, [issue.id, issue.parentId]);
        return yield* detailFrom(tx, member, issue.id);
      }),
    );
  });

/** Whether an edit leaves the issue in a terminal state, including through a duplicate link. */
function terminalAfterEdit(
  db: Db,
  member: Member,
  issue: IssueRow,
  next: StateRow["type"] | undefined,
  duplicateOf: string | null | undefined,
) {
  return Effect.gen(function* () {
    if (duplicateOf) return true;
    const type =
      next ??
      (yield* db
        .select({ type: states.type })
        .from(states)
        .where(
          and(eq(states.organisationId, member.organisationId), eq(states.id, issue.stateId)),
        ))[0]?.type;
    // Removing a duplicate link reopens a duplicate issue in the project's open default state.
    if (type === "duplicate" && duplicateOf === null && !next) return false;
    return type !== undefined && isTerminalState(type);
  });
}

export const editIssue = (member: Member, reference: string, payload: typeof EditIssue.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { version, labelIds, parentId, relations, ...editable } = payload;
        const actor = yield* withCrew(member, payload.relayedCrew);
        const issue = yield* readIssue(tx, member, reference);
        const { project } = yield* readProject(tx, member, issue.projectId);
        if (issue.archivedAt || project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the issue and project before editing."),
          );
        if (version !== issue.version)
          return yield* Effect.fail(problem(409, "The issue version is stale.", issue.version));
        if (Object.keys(payload).length === 1)
          return yield* Effect.fail(problem(400, "Supply an issue field to update."));
        yield* requireFleetOwnership(tx, member, issue.projectId, editable);
        if (
          (editable.type ?? issue.type) === "tracker" &&
          (editable.stateId ||
            (editable.assigneeId === undefined ? issue.assigneeId : editable.assigneeId) ||
            (editable.estimate === undefined
              ? issue.estimate !== null
              : editable.estimate !== null))
        ) {
          return yield* Effect.fail(
            problem(400, "Trackers derive their state and cannot have an assignee or estimate."),
          );
        }
        const state = editable.stateId
          ? yield* validateState(tx, member, issue.projectId, editable.stateId)
          : undefined;
        if (
          editable.estimate != null &&
          editable.estimate !== issue.estimate &&
          !estimatePoints[project.estimateScale].includes(editable.estimate)
        )
          return yield* Effect.fail(problem(400, "Choose an estimate on the project scale."));
        const ready = yield* readyForAgentsAfter(issue, editable, {
          tracker: (editable.type ?? issue.type) === "tracker",
          terminal: yield* terminalAfterEdit(tx, member, issue, state, relations?.duplicateOf),
        });
        yield* activePeople(tx, member, editable);
        yield* planningRefs(tx, member, issue.projectId, editable);
        const parent = yield* linkParent(tx, member, parentId, issue.id);
        const [updated] = yield* tx
          .update(issues)
          .set({
            ...editable,
            ...(editable.cycleId !== undefined && editable.cycleId !== issue.cycleId
              ? { cycleJoinedAt: editable.cycleId ? new Date() : null }
              : {}),
            ...(state ? datesForState(state, issue) : {}),
            readyForAgents: ready.readyForAgents,
            parentId: parent,
            updatedAt: sql`now()`,
            lastActivityAt: sql`now()`,
            version: version + 1,
          })
          .where(and(inOrg(member), eq(issues.id, issue.id), eq(issues.version, version)))
          .returning();
        if (!updated) {
          const current = yield* readIssue(tx, member, issue.id);
          return yield* Effect.fail(problem(409, "The issue version is stale.", current.version));
        }
        yield* localMedia(tx, member, issue.id, editable.description, true);
        yield* writeLabels(tx, member, issue.id, labelIds);
        if (relations)
          yield* replaceRelations(
            tx,
            member,
            updated,
            relations.add,
            relations.remove,
            relations.duplicateOf,
          );
        const fields = Object.keys(payload).filter((field) => field !== "version");
        // An assignment that clears the flag records it as changed although the caller did not send it.
        if (ready.changed && !fields.includes("readyForAgents")) fields.push("readyForAgents");
        yield* addActivity(tx, actor, issue.id, "issue.changed", {
          fields,
          mentionUserIds: yield* validatedMentions(
            tx,
            member,
            editable.description,
            issue.description,
          ),
          ...(issue.stateId !== updated.stateId
            ? { previousStateId: issue.stateId, stateId: updated.stateId }
            : {}),
        });
        yield* refreshTrackers(tx, actor, [updated.id, updated.parentId, issue.parentId]);
        return yield* detailFrom(tx, member, issue.id);
      }),
    );
  });

const nullableId = (column: typeof comments.parentCommentId, id?: string) =>
  id ? eq(column, id) : isNull(column);

export function commentDto(
  db: Db,
  member: Member,
  row: typeof comments.$inferSelect,
  author: Member,
  replies: number,
) {
  return Effect.gen(function* () {
    const mentions = row.deletedAt ? [] : markdownMentions(row.body);
    const ids = [
      ...new Set([
        ...mentions.map(({ userId }) => userId),
        ...(row.resolvedById ? [row.resolvedById] : []),
      ]),
    ];
    const people = new Map((yield* mentionPeople(db, member, ids)).map((row) => [row.id, row]));
    const media = row.deletedAt ? [] : yield* localMedia(db, member, row.issueId, row.body);
    return {
      id: row.id,
      issueId: row.issueId,
      parentCommentId: row.parentCommentId,
      body: row.deletedAt ? "" : row.body,
      version: row.version,
      actualAuthor: person(author)!,
      createdAt: dateTime(row.createdAt),
      displayCreatedAt: dateTime(row.createdAt),
      editedAt: dateTimeOrNull(row.editedAt),
      deletedAt: dateTimeOrNull(row.deletedAt),
      resolved: Boolean(row.resolvedAt),
      resolvedBy: row.resolvedById ? person(people.get(row.resolvedById)) : null,
      resolvedAt: dateTimeOrNull(row.resolvedAt),
      replyCount: replies,
      hasReplies: replies > 0,
      provenance: null,
      relayedCrew: row.relayedCrew,
      media,
      mentions: row.deletedAt ? [] : mentionStatuses(row.body, people),
    };
  });
}

export const commentKey = ({ comment }: { comment: typeof comments.$inferSelect }) =>
  `${dateTime(comment.createdAt)}:${comment.id}`;

export const issueComments = (
  member: Member,
  reference: string,
  query: { limit?: number; cursor?: string },
  root?: string,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const issue = yield* readIssue(tx, member, reference);
          const scope = and(
            eq(comments.organisationId, member.organisationId),
            eq(comments.issueId, issue.id),
          );
          const rows = yield* tx
            .select({ comment: comments, author: users })
            .from(comments)
            .innerJoin(
              users,
              and(eq(users.organisationId, member.organisationId), eq(users.id, comments.authorId)),
            )
            .where(scope);
          if (
            root &&
            !rows.some(({ comment }) => comment.id === root && comment.parentCommentId === null)
          ) {
            return yield* Effect.fail(problem(404, "Comment thread not found."));
          }
          const descendants = Map.groupBy(rows, ({ comment }) => comment.parentCommentId);
          const items = descendants.get(root ?? null) ?? [];
          items.sort(byKey(commentKey));
          const before = root && query.cursor?.startsWith("before:") ? query.cursor.slice(7) : null;
          const limit = query.limit ?? 50;
          let result;
          if (before !== null) {
            const index = items.findIndex(({ comment }) => comment.id === before);
            if (index < 0)
              return yield* Effect.fail(
                problem(400, "The reply cursor is invalid for this thread."),
              );
            const start = Math.max(0, index - limit);
            result = {
              items: items.slice(start, index),
              nextCursor: start ? `before:${items[start]!.comment.id}` : null,
            };
          } else {
            result = yield* page(
              items,
              { org: member.organisationId, issue: issue.id, root },
              commentKey,
              limit,
              query.cursor,
            );
          }
          return {
            ...result,
            items: yield* Effect.forEach(result.items, ({ comment, author }) =>
              commentDto(tx, member, comment, author, descendants.get(comment.id)?.length ?? 0),
            ),
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });

export const createComment = (
  member: Member,
  reference: string,
  payload: typeof CommentInput.Type,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const actor = yield* withCrew(member, payload.relayedCrew);
        const issue = yield* readIssue(tx, member, reference);
        const { project } = yield* readProject(tx, member, issue.projectId);
        if (issue.archivedAt || project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the issue and project before commenting."),
          );
        if (payload.parentCommentId) {
          const [parent] = yield* tx
            .select()
            .from(comments)
            .where(
              and(
                eq(comments.organisationId, member.organisationId),
                eq(comments.issueId, issue.id),
                eq(comments.id, payload.parentCommentId),
                nullableId(comments.parentCommentId),
                isNull(comments.deletedAt),
              ),
            );
          if (!parent) return yield* Effect.fail(problem(404, "Comment thread not found."));
        }
        const [row] = yield* tx
          .insert(comments)
          .values({
            ...payload,
            body: payload.body.trim(),
            organisationId: member.organisationId,
            issueId: issue.id,
            authorId: member.id,
          })
          .returning();
        if (!row) return yield* Effect.fail(problem(500, "The comment could not be created."));
        yield* tx
          .update(issues)
          .set({ lastActivityAt: sql`greatest(${issues.lastActivityAt}, now())` })
          .where(and(inOrg(member), eq(issues.id, issue.id)));
        yield* addActivity(tx, actor, issue.id, "comment.created", {
          commentId: row.id,
          mentionUserIds: yield* validatedMentions(tx, member, row.body),
        });
        yield* localMedia(tx, member, issue.id, row.body, true);
        return yield* commentDto(tx, member, row, member, 0);
      }),
    );
  });

export const issueTimeline = (
  member: Member,
  reference: string,
  query: { limit?: number; cursor?: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    const rows = yield* db
      .select({ event: issueActivity, actor: users })
      .from(issueActivity)
      .leftJoin(
        users,
        and(eq(users.organisationId, member.organisationId), eq(users.id, issueActivity.actorId)),
      )
      .where(
        and(
          eq(issueActivity.organisationId, member.organisationId),
          eq(issueActivity.issueId, issue.id),
        ),
      );
    const key = ({ event }: (typeof rows)[number]) =>
      `${String(Number.MAX_SAFE_INTEGER - event.createdAt.valueOf()).padStart(16, "0")}:${event.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, timeline: issue.id },
      key,
      query.limit,
      query.cursor,
    );
    const ids = [
      ...new Set(
        result.items.flatMap(({ event }) =>
          typeof event.data.decisionId === "string" && isUuid(event.data.decisionId)
            ? [event.data.decisionId]
            : [],
        ),
      ),
    ];
    const decisionRefs = ids.length
      ? yield* db
          .select({
            id: decisions.id,
            title: decisions.title,
            status: decisions.status,
          })
          .from(decisions)
          .where(
            and(eq(decisions.organisationId, member.organisationId), inArray(decisions.id, ids)),
          )
      : [];
    const items = yield* Effect.forEach(result.items, ({ event, actor }) =>
      Effect.gen(function* () {
        const id = event.data.commentId;
        const [row] =
          typeof id === "string" && isUuid(id)
            ? yield* db
                .select({ comment: comments, author: users })
                .from(comments)
                .innerJoin(
                  users,
                  and(
                    eq(users.organisationId, member.organisationId),
                    eq(users.id, comments.authorId),
                  ),
                )
                .where(
                  and(
                    eq(comments.organisationId, member.organisationId),
                    eq(comments.issueId, issue.id),
                    eq(comments.id, id),
                  ),
                )
            : [];
        const comment = row ? yield* commentDto(db, member, row.comment, row.author, 0) : null;
        return { ...activityDto(event, actor), displayAt: dateTime(event.createdAt), comment };
      }),
    );
    return { ...result, items, references: { decisions: decisionRefs } };
  });

export const lockIssues = (db: Db, member: Pick<Member, "organisationId">) =>
  db
    .select({ id: organisations.id })
    .from(organisations)
    .where(eq(organisations.id, member.organisationId))
    .for("update");

const relationNames = ["blocks", "blockedBy", "relatedTo"] as const;
type Relation = typeof issueRelations.$inferInsert;

function replaceRelations(
  db: Db,
  member: Member,
  issue: IssueRow,
  add?: typeof RelationList.Type | null,
  remove?: typeof RelationList.Type | null,
  duplicateOf?: string | null,
) {
  return Effect.gen(function* () {
    const org = member.organisationId;
    function writeEdges(input: typeof RelationList.Type | undefined | null, creating: boolean) {
      return Effect.gen(function* () {
        for (const kind of relationNames)
          for (const reference of new Set(input?.[kind])) {
            const other = yield* readIssue(db, member, reference);
            if (other.id === issue.id || (creating && other.archivedAt)) {
              return yield* Effect.fail(
                problem(400, "Relate this issue to a different active issue."),
              );
            }
            const reverse = kind === "blockedBy" || (kind === "relatedTo" && other.id < issue.id);
            const edge: Relation = {
              organisationId: org,
              kind: kind === "blockedBy" ? "blocks" : kind,
              sourceId: reverse ? other.id : issue.id,
              targetId: reverse ? issue.id : other.id,
            };
            if (creating) yield* db.insert(issueRelations).values(edge).onConflictDoNothing();
            else
              yield* db
                .delete(issueRelations)
                .where(
                  and(
                    eq(issueRelations.organisationId, org),
                    eq(issueRelations.kind, edge.kind),
                    eq(issueRelations.sourceId, edge.sourceId),
                    eq(issueRelations.targetId, edge.targetId),
                  ),
                );
          }
      });
    }
    yield* writeEdges(remove, false);
    yield* writeEdges(add, true);
    if (duplicateOf === undefined) return;
    yield* db
      .delete(issueRelations)
      .where(
        and(
          eq(issueRelations.organisationId, org),
          eq(issueRelations.kind, "duplicateOf"),
          eq(issueRelations.sourceId, issue.id),
        ),
      );
    const project = (yield* readProject(db, member, issue.projectId)).project;
    if (duplicateOf === null) {
      const [state] = yield* db
        .select()
        .from(states)
        .where(
          and(
            eq(states.organisationId, org),
            eq(states.projectId, project.id),
            eq(states.id, issue.stateId),
          ),
        );
      if (state?.type === "duplicate")
        yield* db
          .update(issues)
          .set({ stateId: project.defaultStateId })
          .where(and(inOrg(member), eq(issues.id, issue.id)));
      return;
    }
    const target = yield* readIssue(db, member, duplicateOf);
    if (target.id === issue.id || target.archivedAt || issue.type === "tracker") {
      return yield* Effect.fail(
        problem(400, "Select a different active issue as the duplicate target."),
      );
    }
    const related = yield* db
      .select()
      .from(issueRelations)
      .where(and(eq(issueRelations.organisationId, org), eq(issueRelations.kind, "duplicateOf")));
    const bySource = new Map(related.map((row) => [row.sourceId, row.targetId]));
    let next: string | undefined = target.id;
    while (next) {
      if (next === issue.id)
        return yield* Effect.fail(problem(409, "The duplicate link would create a cycle."));
      next = bySource.get(next);
    }
    const [duplicateState] = yield* db
      .select()
      .from(states)
      .where(
        and(
          eq(states.organisationId, org),
          eq(states.projectId, project.id),
          eq(states.type, "duplicate"),
        ),
      )
      .orderBy(asc(states.position), asc(states.id))
      .limit(1);
    if (!duplicateState)
      return yield* Effect.fail(problem(409, "The project needs a duplicate state."));
    yield* db.insert(issueRelations).values({
      organisationId: org,
      sourceId: issue.id,
      targetId: target.id,
      kind: "duplicateOf",
    });
    yield* db
      .update(issues)
      .set({ stateId: duplicateState.id })
      .where(and(inOrg(member), eq(issues.id, issue.id)));
  });
}

export const listRelations = (
  member: Member,
  reference: string,
  query: { limit?: number; cursor?: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction(
      (tx) =>
        Effect.gen(function* () {
          const issue = yield* readIssue(tx, member, reference);
          const org = member.organisationId;
          const scope = { org, relations: issue.id };
          const anchor = yield* cursorAnchor(scope, query.cursor);
          const outgoing = eq(issueRelations.sourceId, issue.id);
          const kind = sql<typeof issueRelations.$inferSelect.kind | "blockedBy" | "duplicatedBy">`
            case when ${outgoing} then ${issueRelations.kind}
              when ${issueRelations.kind} = 'blocks' then 'blockedBy'
              when ${issueRelations.kind} = 'duplicateOf' then 'duplicatedBy'
              else 'relatedTo' end
          `;
          const otherId = sql`case when ${outgoing} then ${issueRelations.targetId} else ${issueRelations.sourceId} end`;
          const key = sql<string>`(${kind} || ':' || ${issues.identifier} || ':' || ${issues.id}) collate "C"`;
          const limit = query.limit ?? 50;
          const linked = yield* tx
            .select({ id: issues.id, kind, key })
            .from(issueRelations)
            .innerJoin(issues, and(eq(issues.organisationId, org), eq(issues.id, otherId)))
            .where(
              and(
                eq(issueRelations.organisationId, org),
                or(outgoing, eq(issueRelations.targetId, issue.id)),
                anchor === undefined ? undefined : sql`${key} > ${anchor}`,
              ),
            )
            .orderBy(key)
            .limit(limit + 1);
          const selected = linked.slice(0, limit);
          const items = yield* issueRowsById(
            tx,
            member,
            selected.map(({ id }) => id),
          );
          return {
            items: items.map((item, index) => ({ ...item, kind: selected[index]!.kind })),
            nextCursor: linked.length > limit ? cursorAfter(scope, selected[limit - 1]!.key) : null,
          };
        }),
      { isolationLevel: "repeatable read", accessMode: "read only" },
    );
  });

export function refreshTrackers(
  db: Db,
  member: Pick<Member, "organisationId" | "source" | "relayedCrew"> & { id: string | null },
  candidates: readonly (string | null)[],
) {
  return Effect.gen(function* () {
    const org = member.organisationId;
    for (const initial of new Set(candidates)) {
      let id = initial;
      while (id) {
        const issue = yield* readIssue(db, member, id);
        id = issue.parentId;
        if (issue.type !== "tracker" || issue.archivedAt) continue;
        const direct = yield* db
          .select({ type: states.type })
          .from(issues)
          .innerJoin(
            states,
            and(
              eq(states.organisationId, org),
              eq(states.id, issues.stateId),
              eq(states.projectId, issues.projectId),
            ),
          )
          .where(and(inOrg(member), eq(issues.parentId, issue.id), isNull(issues.archivedAt)));
        const type = trackerState(direct.map((child) => child.type));
        const [state] = yield* db
          .select()
          .from(states)
          .where(
            and(
              eq(states.organisationId, org),
              eq(states.projectId, issue.projectId),
              eq(states.type, type),
            ),
          )
          .orderBy(asc(states.position), asc(states.id))
          .limit(1);
        if (!state) return yield* Effect.fail(problem(409, "A tracker workflow state is missing."));
        if (state.id === issue.stateId) continue;
        yield* db
          .update(issues)
          .set({
            stateId: state.id,
            ...datesForState(type, issue),
            version: issue.version + 1,
            updatedAt: sql`now()`,
            lastActivityAt: sql`now()`,
          })
          .where(and(inOrg(member), eq(issues.id, issue.id)));
        yield* addActivity(db, member, issue.id, "issue.state-derived", { stateId: state.id });
      }
    }
  });
}

export const setIssueArchived = (
  member: Member,
  reference: string,
  archive: boolean,
  version?: number,
) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const issue = yield* readIssue(tx, member, reference);
        if (version !== undefined && issue.version !== version)
          return yield* Effect.fail(problem(409, "The issue version is stale.", issue.version));
        if (archive !== Boolean(issue.archivedAt)) {
          const cleared = clearReadyForAgents(issue, archive);
          yield* tx
            .update(issues)
            .set({
              archivedAt: archive ? new Date() : null,
              archivedById: archive ? member.id : null,
              ...cleared.set,
              version: issue.version + 1,
              updatedAt: sql`now()`,
              lastActivityAt: sql`now()`,
            })
            .where(and(inOrg(member), eq(issues.id, issue.id)));
          yield* addActivity(
            tx,
            member,
            issue.id,
            archive ? "issue.archived" : "issue.unarchived",
            cleared.fields.length ? { fields: cleared.fields } : {},
          );
          yield* refreshTrackers(tx, member, [issue.id, issue.parentId]);
        }
        return yield* detailFrom(tx, member, issue.id);
      }),
    ),
  );
