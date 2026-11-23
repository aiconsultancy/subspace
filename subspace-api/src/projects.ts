import { and, asc, eq, inArray, isNotNull, isNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { recordAudit } from "./audit";
import { Database } from "./db";
export { stateTypes } from "./schema";
import { EstimateScale } from "./estimates";
import { AgentRank, memberKind, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { byKey, page } from "./paging";
import { problem } from "./problems";
import {
  cycleStartDays,
  cycles,
  initiatives,
  issues,
  labels,
  milestones,
  organisations,
  projects,
  projectStatuses,
  states,
  stateTypes,
  users,
} from "./schema";
import {
  DateOnly,
  dateTime,
  dateTimeOrNull,
  isUuid,
  Name,
  NullableString,
  PageQuery,
  Paragraph,
  Priority,
  Timestamp,
  today,
  Uuid,
} from "./validation";

type Db = (typeof Database)["Service"];

export const ProjectHealth = Schema.Literals(["onTrack", "atRisk", "offTrack"]);
export const ProjectStatus = Schema.Literals(projectStatuses);
export const StateType = Schema.Literals(stateTypes);
export const ProjectKey = Schema.String.check(Schema.isPattern(/^[A-Z][A-Z0-9]{1,9}$/u));

const projectInput = {
  name: Name,
  leadId: Schema.optional(Uuid),
  description: Schema.optional(Schema.NullOr(Paragraph(50_000))),
  status: Schema.optional(ProjectStatus),
  priority: Schema.optional(Priority),
  startDate: Schema.optional(Schema.NullOr(DateOnly)),
  targetDate: Schema.optional(Schema.NullOr(DateOnly)),
};

export const ProjectAssignment = Schema.Struct({
  initiativeId: Schema.NullOr(Uuid),
  version: Version,
});
export const ProjectUpdate = Schema.Struct({
  ...projectInput,
  key: Schema.optional(ProjectKey),
  version: Schema.optional(Version),
});
export const ProjectInput = Schema.Struct({ ...projectInput, key: ProjectKey });
export const ProjectQuery = Schema.Struct({
  ...PageQuery,
  status: Schema.optional(ProjectStatus),
  includeArchived: Schema.optional(Schema.Boolean),
  q: Schema.optional(Paragraph(200)),
});
export const CycleSettings = Schema.Struct({
  enabled: Schema.Boolean,
  lengthWeeks: Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 8 })),
  startDay: Schema.Literals(cycleStartDays),
  cyclesAhead: Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 4 })),
  carryOver: Schema.Boolean,
});
export const Project = Schema.Struct({
  id: Uuid,
  key: ProjectKey,
  name: Name,
  leadId: Uuid,
  leadName: NullableString,
  leadAvailable: Schema.Boolean,
  status: ProjectStatus,
  priority: Priority,
  startDate: Schema.NullOr(DateOnly),
  targetDate: Schema.NullOr(DateOnly),
  description: NullableString,
  version: Version,
  createdByUserId: Uuid,
  archived: Schema.Boolean,
  archivedByUserId: Schema.NullOr(Uuid),
  createdAt: Timestamp,
  updatedAt: Timestamp,
  archivedAt: Schema.NullOr(Timestamp),
  defaultStateId: Uuid,
  initiativeId: Schema.NullOr(Uuid),
  health: Schema.NullOr(ProjectHealth),
  estimateScale: EstimateScale,
  cycleSettings: CycleSettings,
}).annotate({ identifier: "Project" });
export const ProjectState = Schema.Struct({
  id: Uuid,
  projectId: Uuid,
  name: Name,
  type: StateType,
  position: Schema.Int,
  isDefault: Schema.Boolean,
}).annotate({ identifier: "ProjectState" });
export const Person = Schema.Struct({
  userId: Uuid,
  displayName: Name,
  /** The person's handle and IRC account name; an agent's IRC nick, or null without one. */
  username: NullableString,
  isFormerMember: Schema.Boolean,
  kind: Schema.Literals(["human", "agent"]),
  agent: Schema.NullOr(
    Schema.Struct({
      rank: AgentRank,
      fleet: Schema.String,
      ship: NullableString,
      fleetId: Schema.NullOr(Uuid),
      shipId: Schema.NullOr(Uuid),
    }),
  ),
}).annotate({ identifier: "Person" });
export const UserChoice = Schema.Struct({
  id: Uuid,
  displayName: Name,
  username: NullableString,
  kind: Schema.Literals(["human", "agent"]),
  email: NullableString,
}).annotate({ identifier: "UserChoice" });
export const Label = Schema.Struct({
  id: Uuid,
  name: Name,
  color: Schema.String,
  state: Schema.Literals(["active", "archived"]),
}).annotate({ identifier: "Label" });
export const LabelInput = Schema.Struct({
  name: Name,
  color: Schema.String.check(Schema.isPattern(/^#[a-fA-F0-9]{6}$/)),
  state: Schema.optional(Label.fields.state),
});
export const Milestone = Schema.Struct({
  id: Uuid,
  name: Name,
  projectId: Uuid,
  description: NullableString,
  targetDate: Schema.NullOr(DateOnly),
  createdAt: Timestamp,
  updatedAt: Timestamp,
  version: Version,
  sortOrder: Schema.Int,
  status: Schema.Literals(["unstarted", "next", "overdue", "done"]),
  completionSource: Schema.NullOr(Schema.Literals(["manual", "issues", "manualAndIssues"])),
  issueProgress: Schema.optional(Schema.Struct({ total: Schema.Int, done: Schema.Int })),
  completedAt: Schema.NullOr(Timestamp),
  completedBy: Schema.NullOr(Person),
  contentIncluded: Schema.Boolean,
}).annotate({ identifier: "Milestone" });
export const MilestoneInput = Schema.Struct({
  name: Name,
  description: Schema.optional(Schema.NullOr(Paragraph(50_000))),
  targetDate: Schema.optional(Schema.NullOr(DateOnly)),
  position: Schema.optional(Version),
});
export const Cycle = Schema.Struct({
  id: Uuid,
  projectId: Uuid,
  number: Schema.Int,
  state: Schema.Literals(["current", "upcoming", "closed"]),
  startDate: DateOnly,
  endDate: DateOnly,
}).annotate({ identifier: "Cycle" });

export function person(row: Member | null | undefined) {
  return row
    ? {
        userId: row.id,
        displayName: row.displayName,
        username: row.username,
        isFormerMember: !row.active,
        kind: memberKind(row),
        agent: row.agentRank
          ? {
              rank: row.agentRank,
              fleet: row.agentFleet!,
              ship: row.agentShip,
              fleetId: row.agentFleetId,
              shipId: row.agentShipId,
            }
          : null,
      }
    : null;
}

type ProjectRow = typeof projects.$inferSelect;

export function projectDto(project: ProjectRow, lead: Member) {
  const {
    organisationId: _,
    nextIssueNumber: _counter,
    lastCycleNumber: _cycles,
    ...fields
  } = project;
  return {
    ...fields,
    leadName: lead.displayName,
    leadAvailable: lead.active,
    archived: project.archivedAt !== null,
    archivedAt: dateTimeOrNull(project.archivedAt),
    createdAt: dateTime(project.createdAt),
    updatedAt: dateTime(project.updatedAt),
  };
}

const projectScope = (member: Member, reference: string) =>
  and(
    eq(projects.organisationId, member.organisationId),
    isUuid(reference) ? eq(projects.id, reference) : eq(projects.key, reference.toUpperCase()),
  );

export function readProject(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    const [row] = yield* db
      .select({ project: projects, lead: users })
      .from(projects)
      .innerJoin(
        users,
        and(eq(projects.leadId, users.id), eq(projects.organisationId, users.organisationId)),
      )
      .where(projectScope(member, reference));
    if (!row) return yield* Effect.fail(problem(404, "Project not found."));
    return row;
  });
}

export const listProjects = (member: Member, query: typeof ProjectQuery.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const { limit, cursor, ...filters } = query;
    const rows = yield* db
      .select({ project: projects, lead: users })
      .from(projects)
      .innerJoin(
        users,
        and(eq(projects.leadId, users.id), eq(projects.organisationId, users.organisationId)),
      )
      .where(eq(projects.organisationId, member.organisationId));
    const q = filters.q?.trim().toLowerCase();
    const items = rows.filter(
      ({ project }) =>
        (filters.includeArchived || !project.archivedAt) &&
        (!filters.status || project.status === filters.status) &&
        (!q || `${project.key} ${project.name}`.toLowerCase().includes(q)),
    );
    const key = ({ project }: (typeof items)[number]) =>
      `${project.name.toLowerCase()}:${project.id}`;
    const result = yield* page(
      items.sort(byKey(key)),
      { org: member.organisationId, ...filters },
      key,
      limit,
      cursor,
    );
    return { ...result, items: result.items.map(({ project, lead }) => projectDto(project, lead)) };
  });

export const getProject = (member: Member, reference: string) =>
  Effect.flatMap(Database, (db) =>
    Effect.map(readProject(db, member, reference), ({ project, lead }) =>
      projectDto(project, lead),
    ),
  );

export const createProject = (member: Member, payload: typeof ProjectInput.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        const leadId = payload.leadId ?? member.id;
        const [lead] = yield* tx
          .select()
          .from(users)
          .where(
            and(
              eq(users.id, leadId),
              isNull(users.agentRank),
              eq(users.organisationId, member.organisationId),
              eq(users.active, true),
            ),
          );
        if (!lead)
          return yield* Effect.fail(problem(400, "Project lead must be an active member."));
        if (payload.startDate && payload.targetDate && payload.startDate > payload.targetDate) {
          return yield* Effect.fail(
            problem(400, "The project target date must be on or after its start date."),
          );
        }
        const defaultStateId = crypto.randomUUID();
        const [project] = yield* tx
          .insert(projects)
          .values({
            ...payload,
            leadId,
            organisationId: member.organisationId,
            createdByUserId: member.id,
            defaultStateId,
          })
          .onConflictDoNothing()
          .returning();
        if (!project) return yield* Effect.fail(problem(409, "A project already uses that key."));
        const stateNames = {
          triage: "Triage",
          backlog: "Backlog",
          unstarted: "Todo",
          started: "In progress",
          completed: "Done",
          canceled: "Cancelled",
          duplicate: "Duplicate",
        };
        yield* tx.insert(states).values(
          stateTypes.map((type) => ({
            ...(type === "backlog" ? { id: defaultStateId } : {}),
            organisationId: member.organisationId,
            projectId: project.id,
            name: stateNames[type],
            type,
            position: 0,
            isDefault: type === "backlog",
          })),
        );
        return projectDto(project, lead);
      }),
    );
  });

export const projectStates = (
  member: Member,
  reference: string,
  query: { cursor?: string; limit?: number },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, reference);
    const rows = yield* db
      .select()
      .from(states)
      .where(
        and(eq(states.organisationId, member.organisationId), eq(states.projectId, project.id)),
      );
    const key = (row: typeof states.$inferSelect) =>
      `${stateTypes.indexOf(row.type)}:${String(row.position).padStart(8, "0")}:${row.id}`;
    return yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, states: project.id },
      key,
      query.limit,
      query.cursor,
    );
  });

export const listUsers = (member: Member, query: { cursor?: string; limit?: number }) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* db
      .select({
        id: users.id,
        displayName: users.displayName,
        username: users.username,
        email: users.email,
        agentRank: users.agentRank,
      })
      .from(users)
      .where(and(eq(users.organisationId, member.organisationId), eq(users.active, true)));
    const key = (row: (typeof rows)[number]) => `${row.displayName.toLowerCase()}:${row.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, resource: "users" },
      key,
      query.limit,
      query.cursor,
    );
    return {
      ...result,
      items: result.items.map(({ agentRank, ...user }) => ({
        ...user,
        kind: memberKind({ agentRank }),
      })),
    };
  });

type LabelQuery = { cursor?: string; limit?: number; includeArchived?: boolean };

export const listLabels = (member: Member, query: LabelQuery) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const rows = yield* db
      .select()
      .from(labels)
      .where(
        and(
          eq(labels.organisationId, member.organisationId),
          query.includeArchived ? undefined : eq(labels.state, "active"),
        ),
      );
    const key = (row: typeof labels.$inferSelect) => `${row.name.toLowerCase()}:${row.id}`;
    return yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, labels: Boolean(query.includeArchived) },
      key,
      query.limit,
      query.cursor,
    );
  });

export const createLabel = (member: Member, fields: typeof LabelInput.Type) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const [row] = yield* db
      .insert(labels)
      .values({ ...fields, organisationId: member.organisationId })
      .onConflictDoNothing()
      .returning();
    if (!row) return yield* Effect.fail(problem(409, "A label already uses that name."));
    return row;
  });

export function milestoneProgress(
  db: Db,
  member: Member,
  projectId: string,
  ids: readonly string[],
) {
  if (!ids.length) return Effect.succeed(new Map<string, { total: number; done: number }>());
  return Effect.map(
    db
      .select({
        milestoneId: issues.milestoneId,
        total: sql<number>`count(*) filter (where ${states.type} not in ('canceled', 'duplicate'))::int`,
        done: sql<number>`count(*) filter (where ${states.type} = 'completed')::int`,
      })
      .from(issues)
      .innerJoin(
        states,
        and(
          eq(issues.organisationId, states.organisationId),
          eq(issues.projectId, states.projectId),
          eq(issues.stateId, states.id),
        ),
      )
      .where(
        and(
          eq(issues.organisationId, member.organisationId),
          eq(issues.projectId, projectId),
          isNotNull(issues.milestoneId),
          inArray(issues.milestoneId, [...ids]),
          isNull(issues.archivedAt),
        ),
      )
      .groupBy(issues.milestoneId),
    (rows) => new Map(rows.map(({ milestoneId, total, done }) => [milestoneId!, { total, done }])),
  );
}

export function milestoneDto(
  row: typeof milestones.$inferSelect,
  author?: Member,
  includeContent = true,
  progress?: { total: number; done: number },
  next = false,
) {
  const complete = Boolean(progress && progress.done > 0 && progress.done === progress.total);
  return {
    ...row,
    createdAt: dateTime(row.createdAt),
    updatedAt: dateTime(row.updatedAt),
    completedAt: dateTimeOrNull(row.completedAt),
    completedBy: person(author),
    contentIncluded: includeContent,
    description: includeContent ? row.description : null,
    issueProgress: progress,
    status:
      row.completedAt || complete
        ? ("done" as const)
        : row.targetDate && row.targetDate < today()
          ? ("overdue" as const)
          : next
            ? ("next" as const)
            : ("unstarted" as const),
    completionSource: row.completedAt
      ? complete
        ? ("manualAndIssues" as const)
        : ("manual" as const)
      : complete
        ? ("issues" as const)
        : null,
  };
}

type MilestoneQuery = {
  cursor?: string;
  limit?: number;
  milestoneId?: string;
  includeContent?: boolean;
};

export const listMilestones = (member: Member, reference: string, query: MilestoneQuery) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, reference);
    const rows = yield* db
      .select()
      .from(milestones)
      .where(
        and(
          eq(milestones.organisationId, member.organisationId),
          eq(milestones.projectId, project.id),
          query.milestoneId ? eq(milestones.id, query.milestoneId) : undefined,
        ),
      );
    const key = (row: typeof milestones.$inferSelect) =>
      `${String(row.sortOrder).padStart(10, "0")}:${row.id}`;
    const result = yield* page(
      rows.sort(byKey(key)),
      { project: project.id, milestoneId: query.milestoneId },
      key,
      query.limit,
      query.cursor,
    );
    const people = new Map(
      (yield* db.select().from(users).where(eq(users.organisationId, member.organisationId))).map(
        (entry) => [entry.id, entry],
      ),
    );
    const progress = yield* milestoneProgress(
      db,
      member,
      project.id,
      rows.map((row) => row.id),
    );
    const next = rows.find(
      (row) =>
        !row.completedAt &&
        (!row.targetDate || row.targetDate >= today()) &&
        milestoneDto(row, undefined, false, progress.get(row.id)).status !== "done",
    );
    return {
      ...result,
      items: result.items.map((row) =>
        milestoneDto(
          row,
          row.completedById ? people.get(row.completedById) : undefined,
          query.includeContent !== false,
          progress.get(row.id),
          next?.id === row.id,
        ),
      ),
    };
  });

export const createMilestone = (
  member: Member,
  reference: string,
  fields: typeof MilestoneInput.Type,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        yield* tx
          .select({ id: organisations.id })
          .from(organisations)
          .where(eq(organisations.id, member.organisationId))
          .for("update");
        const { project } = yield* readProject(tx, member, reference);
        if (project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the project before adding milestones."),
          );
        const { position, ...metadata } = fields;
        const scope = and(
          eq(milestones.organisationId, member.organisationId),
          eq(milestones.projectId, project.id),
        );
        const rows = yield* tx
          .select({ id: milestones.id, sortOrder: milestones.sortOrder })
          .from(milestones)
          .where(scope)
          .orderBy(asc(milestones.sortOrder), asc(milestones.id));
        const index = (position ?? rows.length + 1) - 1;
        if (index > rows.length)
          return yield* Effect.fail(
            problem(400, "Choose a milestone position within the project."),
          );
        const [row] = yield* tx
          .insert(milestones)
          .values({
            ...metadata,
            organisationId: member.organisationId,
            projectId: project.id,
            sortOrder: index + 1,
          })
          .returning();
        if (!row) return yield* Effect.fail(problem(500, "The milestone could not be created."));
        rows.splice(index, 0, row);
        for (const [order, entry] of rows.entries()) {
          if (entry.id === row.id || entry.sortOrder === order + 1) continue;
          yield* tx
            .update(milestones)
            .set({
              sortOrder: order + 1,
              version: sql`${milestones.version} + 1`,
              updatedAt: sql`now()`,
            })
            .where(and(scope, eq(milestones.id, entry.id)));
        }
        yield* recordAudit(tx, member, {
          action: "milestoneCreated",
          entityType: "ProjectMilestone",
          entityId: row.id,
          projectId: project.id,
          summary: "Created a project milestone.",
          changes: {},
        });
        return milestoneDto(row);
      }),
    );
  });

export const listCycles = (
  member: Member,
  query: { project?: string; limit?: number; cursor?: string },
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const project = query.project
      ? (yield* readProject(db, member, query.project)).project
      : undefined;
    const rows = yield* db
      .select()
      .from(cycles)
      .where(
        and(
          eq(cycles.organisationId, member.organisationId),
          project ? eq(cycles.projectId, project.id) : undefined,
        ),
      );
    const key = (row: typeof cycles.$inferSelect) => `${row.startDate}:${row.id}`;
    return yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, cycles: project?.id },
      key,
      query.limit,
      query.cursor,
    );
  });

export const takeIssueNumber = (db: Db, member: Member, projectId: string) =>
  db
    .update(projects)
    .set({ nextIssueNumber: sql`${projects.nextIssueNumber} + 1` })
    .where(and(eq(projects.organisationId, member.organisationId), eq(projects.id, projectId)))
    .returning({ nextIssueNumber: projects.nextIssueNumber });

export const assignProject = (
  member: Member,
  reference: string,
  payload: typeof ProjectAssignment.Type,
) =>
  Effect.gen(function* () {
    const db = yield* Database;
    return yield* db.transaction((tx) =>
      Effect.gen(function* () {
        if (member.agentRank)
          return yield* Effect.fail(
            problem(403, "Project initiative changes require a human member."),
          );
        const { project, lead } = yield* readProject(tx, member, reference);
        if (member.role !== "admin" && member.id !== project.leadId) {
          return yield* Effect.fail(
            problem(403, "Project assignment requires the project lead or an organisation admin."),
          );
        }
        if (project.archivedAt)
          return yield* Effect.fail(problem(409, "Unarchive the project before editing."));
        if (project.version !== payload.version)
          return yield* Effect.fail(problem(409, "The project version is stale.", project.version));
        if (payload.initiativeId) {
          const [initiative] = yield* tx
            .select()
            .from(initiatives)
            .where(
              and(
                eq(initiatives.organisationId, member.organisationId),
                eq(initiatives.id, payload.initiativeId),
              ),
            );
          if (
            !initiative ||
            initiative.archived ||
            !["active", "planned"].includes(initiative.status)
          ) {
            return yield* Effect.fail(problem(400, "Select an active or planned initiative."));
          }
        }
        const [saved] = yield* tx
          .update(projects)
          .set({
            initiativeId: payload.initiativeId,
            updatedAt: sql`now()`,
            version: payload.version + 1,
          })
          .where(and(projectScope(member, project.id), eq(projects.version, payload.version)))
          .returning();
        if (!saved) return yield* Effect.fail(problem(409, "The project version is stale."));
        return projectDto(saved, lead);
      }),
    );
  });

export const editProject = (
  member: Member,
  reference: string,
  data: { version?: number; archived?: boolean; fields?: typeof ProjectUpdate.Type },
) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* tx
          .select({ id: organisations.id })
          .from(organisations)
          .where(eq(organisations.id, member.organisationId))
          .for("update");
        const { project } = yield* readProject(tx, member, reference);
        if (member.role !== "admin" && member.id !== project.leadId) {
          return yield* Effect.fail(
            problem(403, "Project changes require the project lead or an organisation admin."),
          );
        }
        const version = data.fields?.version ?? data.version;
        if (version !== undefined && version !== project.version) {
          return yield* Effect.fail(problem(409, "The project version is stale.", project.version));
        }
        let changes: Partial<typeof projects.$inferInsert>;
        if (data.archived !== undefined) {
          if (data.archived === Boolean(project.archivedAt))
            return yield* getProject(member, project.id).pipe(Effect.provideService(Database, tx));
          changes = {
            archivedAt: data.archived ? new Date() : null,
            archivedByUserId: data.archived ? member.id : null,
          };
        } else if (data.fields) {
          if (project.archivedAt)
            return yield* Effect.fail(problem(409, "Unarchive the project before editing."));
          const { version: _, key, ...fields } = data.fields;
          if (key && key !== project.key)
            return yield* Effect.fail(problem(409, "Project keys remain fixed after creation."));
          if (fields.leadId) {
            const [lead] = yield* tx
              .select({ id: users.id })
              .from(users)
              .where(
                and(
                  eq(users.organisationId, member.organisationId),
                  eq(users.id, fields.leadId),
                  isNull(users.agentRank),
                  eq(users.active, true),
                ),
              );
            if (!lead)
              return yield* Effect.fail(
                problem(400, "The project lead must be an active organisation member."),
              );
          }
          const start = fields.startDate === undefined ? project.startDate : fields.startDate;
          const target = fields.targetDate === undefined ? project.targetDate : fields.targetDate;
          if (start && target && start > target)
            return yield* Effect.fail(
              problem(400, "The project target date must be on or after its start date."),
            );
          changes = fields;
        } else {
          return yield* Effect.fail(problem(400, "Supply project changes."));
        }
        const [saved] = yield* tx
          .update(projects)
          .set({
            ...changes,
            updatedAt: sql`now()`,
            version: project.version + 1,
          })
          .where(and(projectScope(member, project.id), eq(projects.version, project.version)))
          .returning();
        if (!saved) return yield* Effect.fail(problem(409, "The project version is stale."));
        const current = yield* readProject(tx, member, project.id);
        return projectDto(current.project, current.lead);
      }),
    ),
  );
