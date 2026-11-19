import { and, eq, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { Mention } from "./issue-contract";
import { addActivity, lockIssues } from "./issues";
import { markdownMentions, mentionPeople, mentionStatuses, validatedMentions } from "./mentions";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import {
  Label,
  LabelInput,
  Milestone,
  milestoneDto,
  milestoneProgress,
  Person,
  person,
  ProjectHealth,
  readProject,
} from "./projects";
import { issues, labels, milestones, projects, projectUpdates, users } from "./schema";
import {
  dateTime,
  dateTimeOrNull,
  PageQuery,
  pageOf,
  Paragraph,
  Ref,
  Timestamp,
  Uuid,
} from "./validation";

type Db = (typeof Database)["Service"];
type Project = typeof projects.$inferSelect;
const VersionInput = Schema.Struct({ version: Version });

function writableProject(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    yield* lockIssues(db, member);
    const { project } = yield* readProject(db, member, reference);
    if (project.archivedAt)
      return yield* Effect.fail(
        problem(409, "Unarchive the project before changing its planning records."),
      );
    return project;
  });
}

export function changeLabel(member: Member, id: string, input: typeof LabelInput.Type) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const [before] = yield* tx
          .select()
          .from(labels)
          .where(and(eq(labels.organisationId, member.organisationId), eq(labels.id, id)));
        if (!before) return yield* Effect.fail(problem(404, "Label not found."));
        const [conflict] = yield* tx
          .select({ id: labels.id })
          .from(labels)
          .where(
            and(
              eq(labels.organisationId, member.organisationId),
              eq(labels.name, input.name),
              sql`${labels.id} <> ${id}`,
            ),
          );
        if (conflict) return yield* Effect.fail(problem(409, "A label already uses that name."));
        const [label] = yield* tx
          .update(labels)
          .set(input)
          .where(and(eq(labels.organisationId, member.organisationId), eq(labels.id, id)))
          .returning();
        yield* recordAudit(tx, member, {
          action: "labelUpdated",
          entityType: "Label",
          entityId: id,
          summary: "Changed an issue label.",
          changes: { state: label?.state },
        });
        return label!;
      }),
    ),
  );
}

export const MilestoneChange = Schema.Struct({
  version: Version,
  name: Schema.optional(Milestone.fields.name),
  description: Schema.optional(Schema.NullOr(Paragraph(50_000))),
  targetDate: Schema.optional(Milestone.fields.targetDate),
  completed: Schema.optional(Schema.Boolean),
  move: Schema.optional(Schema.Literals(["up", "down"])),
});

type MilestoneWrite = typeof MilestoneChange.Type & { remove?: boolean };
export function changeMilestone(
  member: Member,
  reference: string,
  id: string,
  input: MilestoneWrite,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const project = yield* writableProject(tx, member, reference);
        const scope = and(
          eq(milestones.organisationId, member.organisationId),
          eq(milestones.projectId, project.id),
        );
        const rows = yield* tx
          .select()
          .from(milestones)
          .where(scope)
          .orderBy(milestones.sortOrder, milestones.id);
        const index = rows.findIndex((row) => row.id === id);
        const before = rows[index];
        if (!before) return yield* Effect.fail(problem(404, "Milestone not found."));
        if (before.version !== input.version)
          return yield* Effect.fail(
            problem(409, "The milestone version is stale.", before.version),
          );
        const target = and(scope, eq(milestones.id, id));
        if (input.remove) {
          const updated = yield* tx
            .update(issues)
            .set({
              milestoneId: null,
              version: sql`${issues.version} + 1`,
              updatedAt: sql`now()`,
              lastActivityAt: sql`now()`,
            })
            .where(
              and(
                eq(issues.organisationId, member.organisationId),
                eq(issues.projectId, project.id),
                eq(issues.milestoneId, id),
              ),
            )
            .returning({ id: issues.id });
          for (const issue of updated)
            yield* addActivity(tx, member, issue.id, "issue.changed", {
              fields: ["milestoneId"],
              previousMilestoneId: id,
            });
          yield* tx.delete(milestones).where(target);
          yield* planningAudit(tx, member, project.id, "milestoneDeleted", "ProjectMilestone", id);
          return milestoneDto(before);
        }
        const { version: _version, completed, move, remove: _remove, ...fields } = input;
        if (!Object.keys(fields).length && completed === undefined && !move)
          return yield* Effect.fail(problem(400, "Supply a milestone field to update."));
        const progress = (yield* milestoneProgress(tx, member, project.id, [id])).get(id);
        if (
          completed === false &&
          progress &&
          progress.done > 0 &&
          progress.done === progress.total
        )
          return yield* Effect.fail(
            problem(400, "Reopen a completed issue before reopening this milestone."),
          );
        const neighbor = move ? rows[index + (move === "up" ? -1 : 1)] : undefined;
        const [saved] = yield* tx
          .update(milestones)
          .set({
            ...fields,
            version: before.version + 1,
            updatedAt: sql`now()`,
            sortOrder: neighbor?.sortOrder,
            completedAt:
              completed === undefined
                ? undefined
                : completed
                  ? (before.completedAt ?? new Date())
                  : null,
            completedById:
              completed === undefined
                ? undefined
                : completed
                  ? (before.completedById ?? member.id)
                  : null,
          })
          .where(target)
          .returning();
        if (neighbor)
          yield* tx
            .update(milestones)
            .set({
              sortOrder: before.sortOrder,
              version: neighbor.version + 1,
              updatedAt: sql`now()`,
            })
            .where(and(scope, eq(milestones.id, neighbor.id)));
        yield* planningAudit(tx, member, project.id, "milestoneUpdated", "ProjectMilestone", id);
        const [author] = saved?.completedById
          ? yield* mentionPeople(tx, member, [saved.completedById])
          : [];
        return milestoneDto(saved!, author, true, progress);
      }),
    ),
  );
}

function planningAudit(
  db: Db,
  member: Member,
  projectId: string,
  action: string,
  entityType: string,
  entityId: string,
) {
  return recordAudit(db, member, {
    action,
    entityType,
    entityId,
    projectId,
    summary: "Changed a project planning record.",
    changes: {},
  });
}

const Update = Schema.Struct({
  id: Uuid,
  projectId: Uuid,
  body: Schema.NullOr(Schema.String),
  contentIncluded: Schema.Boolean,
  health: ProjectHealth,
  previousHealth: Schema.NullOr(ProjectHealth),
  author: Person,
  version: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
  editedAt: Schema.NullOr(Timestamp),
  mentions: Schema.Array(Mention),
});
const Save = Schema.Struct({
  update: Update,
  outcome: Schema.Literals(["created", "updated", "unchanged"]),
  mentionOutcome: Schema.Null,
});
const body = Paragraph(50_000).check(Schema.isPattern(/\S/u));
export const ProjectUpdateInput = Schema.Struct({ body, health: ProjectHealth });
export const ProjectUpdateEdit = Schema.Struct({
  version: Version,
  body: Schema.optional(body),
  health: Schema.optional(ProjectHealth),
});
export const ProjectUpdateQuery = Schema.Struct({
  ...PageQuery,
  includeContent: Schema.optional(Schema.Boolean),
  updateId: Schema.optional(Uuid),
});
type UpdateRow = typeof projectUpdates.$inferSelect;
const updateScope = (member: Member, projectId: string, id?: string) =>
  and(
    eq(projectUpdates.organisationId, member.organisationId),
    eq(projectUpdates.projectId, projectId),
    id ? eq(projectUpdates.id, id) : undefined,
  );
const updateKey = (row: UpdateRow) =>
  `${String(Number.MAX_SAFE_INTEGER - row.createdAt.valueOf()).padStart(16, "0")}:${row.id}`;

function updates(db: Db, member: Member, projectId: string) {
  return Effect.map(
    db
      .select({ row: projectUpdates, author: users })
      .from(projectUpdates)
      .innerJoin(
        users,
        and(eq(users.organisationId, member.organisationId), eq(users.id, projectUpdates.authorId)),
      )
      .where(updateScope(member, projectId)),
    (rows) => rows.sort(byKey(({ row }) => updateKey(row))),
  );
}

function updateDto(
  db: Db,
  member: Member,
  row: UpdateRow,
  author: Member,
  previousHealth: UpdateRow["health"] | null,
  includeContent: boolean,
) {
  return Effect.gen(function* () {
    const ids = includeContent ? markdownMentions(row.body).map(({ userId }) => userId) : [];
    const people = new Map(
      (yield* mentionPeople(db, member, ids)).map((entry) => [entry.id, entry]),
    );
    return {
      ...row,
      author: person(author)!,
      previousHealth,
      body: includeContent ? row.body : null,
      contentIncluded: includeContent,
      createdAt: dateTime(row.createdAt),
      updatedAt: dateTime(row.updatedAt),
      editedAt: dateTimeOrNull(row.editedAt),
      mentions: includeContent ? mentionStatuses(row.body, people) : [],
    };
  });
}

export function listProjectUpdates(
  member: Member,
  reference: string,
  query: typeof ProjectUpdateQuery.Type,
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, reference);
    const rows = yield* updates(db, member, project.id);
    const ordered = query.updateId ? rows.filter(({ row }) => row.id === query.updateId) : rows;
    const prev = new Map(
      rows.map(({ row }, index) => [row.id, rows[index + 1]?.row.health ?? null]),
    );
    const result = yield* page(
      ordered,
      { org: member.organisationId, projectUpdates: project.id, updateId: query.updateId },
      ({ row }) => updateKey(row),
      query.limit ?? 20,
      query.cursor,
    );
    return {
      ...result,
      items: yield* Effect.forEach(result.items, ({ row, author }) =>
        updateDto(
          db,
          member,
          row,
          author,
          prev.get(row.id) ?? null,
          query.includeContent !== false,
        ),
      ),
    };
  });
}

type Write =
  | { create: typeof ProjectUpdateInput.Type }
  | { id: string; edit: typeof ProjectUpdateEdit.Type }
  | { id: string; remove: number };
function updateProjectHealth(db: Db, member: Member, project: Project, health: Project["health"]) {
  return db
    .update(projects)
    .set({ health })
    .where(and(eq(projects.organisationId, member.organisationId), eq(projects.id, project.id)));
}

export function changeProjectUpdate(member: Member, reference: string, input: Write) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const project = yield* writableProject(tx, member, reference);
        const rows = yield* updates(tx, member, project.id);
        const index = "id" in input ? rows.findIndex(({ row }) => row.id === input.id) : -1;
        const before = rows[index];
        if ("id" in input) {
          if (!before) return yield* Effect.fail(problem(404, "Project update not found."));
          if (
            before.author.id !== member.id &&
            (member.agentRank || (member.role !== "admin" && project.leadId !== member.id))
          )
            return yield* Effect.fail(
              problem(
                403,
                "Only the author, project lead or an administrator may change this update.",
              ),
            );
          const version = "remove" in input ? input.remove : input.edit.version;
          if (version !== before.row.version)
            return yield* Effect.fail(
              problem(409, "The project update version is stale.", before.row.version),
            );
        }
        if ("remove" in input) {
          yield* tx.delete(projectUpdates).where(updateScope(member, project.id, input.id));
          if (!index) yield* updateProjectHealth(tx, member, project, rows[1]?.row.health ?? null);
          yield* planningAudit(
            tx,
            member,
            project.id,
            "projectUpdateDeleted",
            "ProjectUpdate",
            input.id,
          );
          return {
            update: yield* updateDto(tx, member, before!.row, before!.author, null, true),
            outcome: "updated" as const,
            mentionOutcome: null,
          };
        }
        const fields = "create" in input ? input.create : input.edit;
        const text = fields.body?.trim() ?? before?.row.body;
        const health = fields.health ?? before?.row.health;
        const previousHealth = rows[index + 1]?.row.health ?? null;
        if (before && text === before.row.body && health === before.row.health) {
          return {
            update: yield* updateDto(tx, member, before.row, before.author, previousHealth, true),
            outcome: "unchanged" as const,
            mentionOutcome: null,
          };
        }
        yield* validatedMentions(tx, member, text, before?.row.body);
        const [row] =
          "create" in input
            ? yield* tx
                .insert(projectUpdates)
                .values({
                  organisationId: member.organisationId,
                  projectId: project.id,
                  authorId: member.id,
                  body: text!,
                  health: health!,
                })
                .returning()
            : yield* tx
                .update(projectUpdates)
                .set({
                  body: text,
                  health,
                  version: before!.row.version + 1,
                  updatedAt: sql`now()`,
                  editedAt: sql`now()`,
                })
                .where(updateScope(member, project.id, input.id))
                .returning();
        if (!row) return yield* Effect.fail(problem(500, "The update could not be saved."));
        if (index <= 0) yield* updateProjectHealth(tx, member, project, row.health);
        yield* planningAudit(
          tx,
          member,
          project.id,
          before ? "projectUpdateUpdated" : "projectUpdateCreated",
          "ProjectUpdate",
          row.id,
        );
        const update = yield* updateDto(
          tx,
          member,
          row,
          before?.author ?? member,
          previousHealth,
          true,
        );
        return {
          update,
          outcome: before ? ("updated" as const) : ("created" as const),
          mentionOutcome: null,
        };
      }),
    ),
  );
}

const p = { reference: Ref };
const item = { ...p, id: Uuid };
const milestone = "/projects/:reference/milestones/:id";
const projectUpdate = "/projects/:reference/updates";
const updateItem = `${projectUpdate}/:id`;
export const planningGroup = HttpApiGroup.make("planning")
  .add(
    HttpApiEndpoint.put("changeLabel", "/labels/:id", {
      params: { id: Uuid },
      payload: LabelInput,
      success: Label,
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("editMilestone", milestone, {
      params: item,
      payload: MilestoneChange,
      success: Milestone,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("removeMilestone", milestone, {
      params: item,
      payload: VersionInput,
      success: HttpApiSchema.NoContent,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("updates", projectUpdate, {
      params: p,
      query: ProjectUpdateQuery,
      success: pageOf(Update),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("createUpdate", projectUpdate, {
      params: p,
      payload: ProjectUpdateInput,
      success: Save.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("editUpdate", updateItem, {
      params: item,
      payload: ProjectUpdateEdit,
      success: Save,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("deleteUpdate", updateItem, {
      params: item,
      payload: VersionInput,
      success: HttpApiSchema.NoContent,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const planningHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof planningGroup>,
) =>
  handlers
    .handle("changeLabel", ({ params, payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => changeLabel(member, params.id, payload))),
    )
    .handle("editMilestone", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeMilestone(member, params.reference, params.id, payload),
        ),
      ),
    )
    .handle("removeMilestone", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          Effect.asVoid(
            changeMilestone(member, params.reference, params.id, { ...payload, remove: true }),
          ),
        ),
      ),
    )
    .handle("updates", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          listProjectUpdates(member, params.reference, query),
        ),
      ),
    )
    .handle("createUpdate", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeProjectUpdate(member, params.reference, { create: payload }),
        ),
      ),
    )
    .handle("editUpdate", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeProjectUpdate(member, params.reference, { id: params.id, edit: payload }),
        ),
      ),
    )
    .handle("deleteUpdate", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          Effect.asVoid(
            changeProjectUpdate(member, params.reference, {
              id: params.id,
              remove: payload.version,
            }),
          ),
        ),
      ),
    );
