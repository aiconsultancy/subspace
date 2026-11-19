import { and, eq, inArray, or, sql } from "drizzle-orm";
import { DateTime, Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { IssueFilter, IssueQuery } from "./issue-contract";
import { listIssues, lockIssues } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { StateType } from "./projects";
import { cycles, issueViews, labels, milestones, projects, states, users } from "./schema";
import {
  dateTime,
  isUuid,
  Line,
  Name,
  PageQuery,
  pageOf,
  Priority,
  Timestamp,
  Uuid,
} from "./validation";

const Visibility = Schema.Literals(["personal", "shared"]);
const listOf = <S extends Schema.Constraint>(value: S) =>
  Schema.optional(Schema.Array(value).check(Schema.isMinLength(1), Schema.isMaxLength(50)));
const selectors = <S extends string>(names: readonly S[]) =>
  Schema.Union([Uuid, Schema.Literals(names)]);
const Filters = Schema.Struct({
  creators: listOf(selectors(["me"])),
  parent: IssueFilter.parent,
  activitySince: Schema.optional(Timestamp),
  updatedSince: Schema.optional(Timestamp),
  completedSince: Schema.optional(Timestamp),
  order: IssueFilter.order,
  stateIds: listOf(Uuid),
  stateTypes: listOf(StateType),
  assignees: listOf(selectors(["me", "none"])),
  priorities: listOf(Priority),
  labelIds: listOf(Uuid),
  projects: listOf(selectors(["none"])),
  milestones: listOf(selectors(["none"])),
  cycles: listOf(selectors(["current", "next", "previous", "none", "unplanned"])),
  type: IssueFilter.type,
  due: IssueFilter.due,
  updatedWithin: Schema.optional(Schema.Literals(["P1D", "P7D", "P30D"])),
  includeArchived: Schema.optional(Schema.Literal(true)),
});
const Definition = Schema.Struct({
  version: Schema.Literals([1, 2]),
  tab: Schema.Literals(["my", "active", "backlog", "all"]),
  mine: Schema.optional(Schema.Literals(["created", "subscribed"])),
  projectId: Schema.optional(Uuid),
  q: Schema.optional(Line(200)),
  filters: Schema.optional(Filters),
  layout: Schema.optional(Schema.Literal("board")),
  subIssues: Schema.optional(Schema.Literal("nested")),
});
export const NewView = Schema.Struct({
  name: Name,
  visibility: Visibility,
  definition: Definition,
});
export const EditView = Schema.Struct({
  version: Version,
  name: Schema.optional(Name),
  visibility: Schema.optional(Visibility),
  definition: Schema.optional(Definition),
});
const View = Schema.Struct({
  id: Uuid,
  name: Name,
  visibility: Visibility,
  isOwn: Schema.Boolean,
  canEdit: Schema.Boolean,
  canDelete: Schema.Boolean,
  createdBy: Schema.Struct({ userId: Uuid, name: Name }),
  definition: Definition,
  effectiveDefinition: Definition,
  resolution: Schema.Struct({
    unavailable: Schema.Array(
      Schema.Struct({
        filter: Schema.String,
        values: Schema.Array(Schema.String),
        wholeFilter: Schema.Boolean,
      }),
    ),
    labels: Schema.Array(
      Schema.Struct({
        filter: Schema.String,
        value: Schema.String,
        label: Schema.String,
        hidden: Schema.Boolean,
        formerMember: Schema.Boolean,
      }),
    ),
  }),
  version: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
});
const ViewListItem = Schema.Struct({
  ...View.fields,
  issueCount: Schema.Struct({ value: Schema.Int, atLeast: Schema.Boolean }),
});
type Db = (typeof Database)["Service"];
type Row = typeof issueViews.$inferSelect;
type ViewDefinition = typeof Definition.Type;
type FilterName =
  | "creators"
  | "stateIds"
  | "assignees"
  | "labelIds"
  | "projects"
  | "milestones"
  | "cycles";
const referenceFilters: FilterName[] = [
  "creators",
  "stateIds",
  "assignees",
  "labelIds",
  "projects",
  "milestones",
  "cycles",
];

function viewRows(db: Db, member: Member, id?: string) {
  return db
    .select({ row: issueViews, owner: users })
    .from(issueViews)
    .innerJoin(
      users,
      and(eq(users.organisationId, member.organisationId), eq(users.id, issueViews.ownerId)),
    )
    .where(
      and(
        eq(issueViews.organisationId, member.organisationId),
        or(eq(issueViews.ownerId, member.id), eq(issueViews.visibility, "shared")),
        id ? eq(issueViews.id, id) : undefined,
      ),
    );
}

function referenceOptions(
  db: Db,
  member: Member,
  filter: FilterName,
  ids: string[],
  projectId?: string,
) {
  const org = member.organisationId;
  if (!ids.length)
    return Effect.succeed(
      [] as { id: string; label: string; hidden?: boolean; formerMember?: boolean }[],
    );
  switch (filter) {
    case "stateIds":
      return db
        .select({ id: states.id, label: states.name })
        .from(states)
        .where(
          and(
            eq(states.organisationId, org),
            inArray(states.id, ids),
            projectId ? eq(states.projectId, projectId) : undefined,
          ),
        );
    case "creators":
    case "assignees":
      return db
        .select({
          id: users.id,
          label: users.displayName,
          formerMember: sql<boolean>`not ${users.active}`,
        })
        .from(users)
        .where(and(eq(users.organisationId, org), inArray(users.id, ids)));
    case "labelIds":
      return db
        .select({
          id: labels.id,
          label: labels.name,
          hidden: sql<boolean>`${labels.state} = 'archived'`,
        })
        .from(labels)
        .where(and(eq(labels.organisationId, org), inArray(labels.id, ids)));
    case "projects":
      return db
        .select({
          id: projects.id,
          label: projects.name,
          hidden: sql<boolean>`${projects.archivedAt} is not null`,
        })
        .from(projects)
        .where(and(eq(projects.organisationId, org), inArray(projects.id, ids)));
    case "milestones":
      return db
        .select({ id: milestones.id, label: milestones.name })
        .from(milestones)
        .where(and(eq(milestones.organisationId, org), inArray(milestones.id, ids)));
    case "cycles":
      return db
        .select({ id: cycles.id, label: sql<string>`'Cycle ' || ${cycles.number}` })
        .from(cycles)
        .where(and(eq(cycles.organisationId, org), inArray(cycles.id, ids)));
  }
}

function resolveView(db: Db, member: Member, definition: ViewDefinition) {
  return Effect.gen(function* () {
    const unavailable: { filter: string; values: string[]; wholeFilter: boolean }[] = [];
    const labels: {
      filter: string;
      value: string;
      label: string;
      hidden: boolean;
      formerMember: boolean;
    }[] = [];
    const filters = { ...definition.filters };
    for (const filter of referenceFilters) {
      const values = filters[filter];
      if (!values) continue;
      const rows = yield* referenceOptions(
        db,
        member,
        filter,
        values.filter(isUuid),
        definition.projectId,
      );
      const byId = new Map<string, { label: string; hidden?: boolean; formerMember?: boolean }>(
        rows.map((row) => [row.id, row]),
      );
      const found: string[] = [];
      const missing: string[] = [];
      for (const value of values) {
        const option = byId.get(value);
        if (!option && isUuid(value)) {
          missing.push(value);
          continue;
        }
        found.push(value);
        labels.push({
          filter,
          value,
          label: option?.label ?? value,
          hidden: option?.hidden ?? false,
          formerMember: option?.formerMember ?? false,
        });
      }
      if (missing.length) unavailable.push({ filter, values: missing, wholeFilter: !found.length });
      if (found.length) filters[filter] = found;
      else delete filters[filter];
    }
    let projectId = definition.projectId;
    if (projectId && !(yield* referenceOptions(db, member, "projects", [projectId])).length) {
      unavailable.push({ filter: "projectId", values: [projectId], wholeFilter: true });
      projectId = undefined;
    }
    return {
      effectiveDefinition: {
        ...definition,
        projectId,
        filters: Object.keys(filters).length ? filters : undefined,
      },
      resolution: { unavailable, labels },
    };
  });
}

function parsedInstant(value?: string) {
  return value ? DateTime.makeUnsafe(value) : undefined;
}

function issueFilter(member: Member, definition: ViewDefinition) {
  const { filters, tab, mine } = definition;
  const baseline: readonly (typeof StateType.Type)[] | undefined =
    tab === "active"
      ? ["unstarted", "started"]
      : tab === "backlog"
        ? ["backlog", "unstarted"]
        : undefined;
  const stateType = baseline
    ? baseline.filter((state) => !filters?.stateTypes || filters.stateTypes.includes(state))
    : filters?.stateTypes;
  let updatedSince = parsedInstant(filters?.updatedSince);
  if (filters?.updatedWithin) {
    const days = Number(filters.updatedWithin.slice(1, -1));
    const relative = Date.now() - days * 86400_000;
    if (!updatedSince || updatedSince.epochMilliseconds < relative)
      updatedSince = DateTime.makeUnsafe(relative);
  }
  const ownAssigned = tab === "my" && !mine;
  const ownCreated = tab === "my" && mine === "created";
  const includesMe = (values?: readonly string[]) =>
    !values || values.includes("me") || values.includes(member.id);
  const matchesNone =
    stateType?.length === 0 ||
    (ownAssigned && !includesMe(filters?.assignees)) ||
    (ownCreated && !includesMe(filters?.creators));
  const query: typeof IssueQuery.Type = {
    project: definition.projectId,
    projectIds: filters?.projects?.filter(isUuid),
    q: definition.q,
    state: filters?.stateIds,
    stateType,
    creator: ownCreated ? ["me"] : filters?.creators,
    assignee: ownAssigned ? ["me"] : filters?.assignees,
    subscribed: tab === "my" && mine === "subscribed" ? true : undefined,
    priority: filters?.priorities,
    label: filters?.labelIds,
    type: filters?.type,
    due: filters?.due,
    milestone: filters?.milestones,
    cycle: filters?.cycles,
    includeArchived: filters?.includeArchived,
    parent:
      filters?.parent ?? (definition.version === 2 && !definition.subIssues ? "none" : undefined),
    order:
      filters?.order ??
      (definition.projectId && (definition.layout || tab === "backlog") ? "manual" : "priority"),
    updatedSince,
    activitySince: parsedInstant(filters?.activitySince),
    completedSince: parsedInstant(filters?.completedSince),
    limit: 200,
  };
  return { matchesNone, query };
}

function viewDto(db: Db, member: Member, row: Row, owner: Member) {
  return Effect.gen(function* () {
    const definition = yield* Schema.decodeUnknownEffect(Definition)(row.definition);
    const resolved = yield* resolveView(db, member, definition);
    const isOwn = row.ownerId === member.id;
    return {
      id: row.id,
      name: row.name,
      visibility: row.visibility,
      isOwn,
      canEdit: isOwn,
      canDelete: isOwn || (row.visibility === "shared" && !member.agentRank),
      createdBy: { userId: owner.id, name: owner.displayName },
      version: row.version,
      createdAt: dateTime(row.createdAt),
      updatedAt: dateTime(row.updatedAt),
      definition,
      ...resolved,
    };
  });
}

export function listViews(member: Member, query: { limit?: number; cursor?: string }) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* viewRows(db, member);
    const key = ({ row }: (typeof rows)[number]) => `${row.name.toLowerCase()}:${row.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, viewer: member.id },
      key,
      query.limit,
      query.cursor,
    );
    const items = yield* Effect.forEach(result.items, ({ row, owner }) =>
      Effect.gen(function* () {
        const view = yield* viewDto(db, member, row, owner);
        const { matchesNone, query: filters } = issueFilter(member, view.effectiveDefinition);
        const empty = matchesNone || view.resolution.unavailable.some((ref) => ref.wholeFilter);
        const count = empty ? { items: [], nextCursor: null } : yield* listIssues(member, filters);
        return {
          ...view,
          issueCount: { value: count.items.length, atLeast: count.nextCursor !== null },
        };
      }),
    );
    return { ...result, items };
  });
}

export function getView(member: Member, id: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const [entry] = yield* viewRows(db, member, id);
    if (!entry) return yield* Effect.fail(problem(404, "Issue view not found."));
    return yield* viewDto(db, member, entry.row, entry.owner);
  });
}

function validateDefinition(db: Db, member: Member, definition: ViewDefinition) {
  return Effect.gen(function* () {
    const { filters } = definition;
    const times = [filters?.updatedSince, filters?.activitySince, filters?.completedSince];
    if (
      (definition.subIssues && definition.version !== 2) ||
      (definition.mine && definition.tab !== "my") ||
      (filters?.stateIds && !definition.projectId) ||
      times.some(
        (time) =>
          time !== undefined &&
          !(Number.isFinite(Date.parse(time)) && /(?:Z|[+-]\d{2}:\d{2})$/iu.test(time)),
      ) ||
      Buffer.byteLength(JSON.stringify(definition)) > 16_384
    ) {
      return yield* Effect.fail(problem(400, "The saved view definition contains invalid fields."));
    }
    const { resolution } = yield* resolveView(db, member, definition);
    if (resolution.unavailable.length)
      return yield* Effect.fail(problem(400, "Choose available filters in this organisation."));
  });
}

type Change =
  | { create: typeof NewView.Type }
  | { id: string; edit: typeof EditView.Type }
  | { id: string; remove: number };
export function changeView(member: Member, change: Change) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const [old] = "id" in change ? yield* viewRows(tx, member, change.id) : [];
        if ("id" in change) {
          if (!old) return yield* Effect.fail(problem(404, "Issue view not found."));
          if (old.row.ownerId !== member.id && (!("remove" in change) || member.agentRank))
            return yield* Effect.fail(problem(403, "You cannot change this view."));
          if (old.row.version !== ("remove" in change ? change.remove : change.edit.version))
            return yield* Effect.fail(
              problem(409, "The issue view version is stale.", old.row.version),
            );
        }
        const scope = and(
          eq(issueViews.organisationId, member.organisationId),
          "id" in change ? eq(issueViews.id, change.id) : undefined,
        );
        if ("remove" in change) {
          yield* tx.delete(issueViews).where(scope);
          yield* viewAudit(tx, member, old!.row, "issueViewDeleted");
          return yield* viewDto(tx, member, old!.row, old!.owner);
        }
        const fields = "create" in change ? change.create : change.edit;
        if (fields.definition) yield* validateDefinition(tx, member, fields.definition);
        if (
          "create" in change &&
          (yield* tx
            .select({ id: issueViews.id })
            .from(issueViews)
            .where(
              and(
                eq(issueViews.organisationId, member.organisationId),
                eq(issueViews.ownerId, member.id),
              ),
            )
            .limit(50)).length === 50
        )
          return yield* Effect.fail(problem(400, "A member can save at most 50 issue views."));
        if (fields.name) {
          const [conflict] = yield* tx
            .select({ id: issueViews.id })
            .from(issueViews)
            .where(
              and(
                eq(issueViews.organisationId, member.organisationId),
                eq(issueViews.ownerId, member.id),
                sql`lower(${issueViews.name}) = lower(${fields.name})`,
                "id" in change ? sql`${issueViews.id} <> ${change.id}` : undefined,
              ),
            );
          if (conflict)
            return yield* Effect.fail(problem(409, "You already have a view with that name."));
        }
        const [row] =
          "create" in change
            ? yield* tx
                .insert(issueViews)
                .values({
                  ...change.create,
                  organisationId: member.organisationId,
                  ownerId: member.id,
                })
                .returning()
            : yield* tx
                .update(issueViews)
                .set({ ...change.edit, version: old!.row.version + 1, updatedAt: sql`now()` })
                .where(scope)
                .returning();
        if (!row) return yield* Effect.fail(problem(500, "The issue view could not be saved."));
        yield* viewAudit(tx, member, row, old ? "issueViewUpdated" : "issueViewCreated");
        return yield* viewDto(tx, member, row, member);
      }),
    ),
  );
}

function viewAudit(db: Db, member: Member, row: Row, action: string) {
  return recordAudit(db, member, {
    action,
    entityType: "IssueView",
    entityId: row.id,
    summary: "Changed a saved issue view.",
    changes: { visibility: row.visibility },
  });
}

const params = { id: Uuid };
export const viewGroup = HttpApiGroup.make("views")
  .add(
    HttpApiEndpoint.get("views", "/issue-views", {
      query: PageQuery,
      success: pageOf(ViewListItem),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("view", "/issue-views/:id", {
      params,
      success: View,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("create", "/issue-views", {
      payload: NewView,
      success: View.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("edit", "/issue-views/:id", {
      params,
      payload: EditView,
      success: View,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("remove", "/issue-views/:id", {
      params,
      payload: Schema.Struct({ version: Version }),
      success: HttpApiSchema.NoContent,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const viewHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof viewGroup>,
) =>
  handlers
    .handle("views", ({ query }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => listViews(member, query))),
    )
    .handle("view", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => getView(member, params.id))),
    )
    .handle("create", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => changeView(member, { create: payload }))),
    )
    .handle("edit", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeView(member, { id: params.id, edit: payload }),
        ),
      ),
    )
    .handle("remove", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          Effect.asVoid(changeView(member, { id: params.id, remove: payload.version })),
        ),
      ),
    );
