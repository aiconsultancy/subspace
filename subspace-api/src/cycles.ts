import { and, asc, eq, inArray, isNull, or, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { IssueFields } from "./issue-contract";
import { addActivity, issueRowsById, lockIssues, readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { byKey, page } from "./paging";
import { endpointErrors, problem } from "./problems";
import { Cycle, CycleSettings, Project, projectDto, readProject, StateType } from "./projects";
import { cycleResults, cycles, cycleStartDays, issues, projects, states } from "./schema";
import { dateTimeOrNull, PageQuery, pageOf, Ref, Timestamp, today, Uuid } from "./validation";

type Db = (typeof Database)["Service"];
type CycleRow = typeof cycles.$inferSelect;
type ProjectRow = typeof projects.$inferSelect;
type IssueRow = typeof issues.$inferSelect;
type Settings = typeof CycleSettings.Type;
type Changes = typeof ChangesSchema.Type;

export const SetCycleSettings = Schema.Struct({ version: Version, ...CycleSettings.fields });
export const AddCycleIssues = Schema.Struct({
  issues: Schema.Array(Ref).check(Schema.isMinLength(1), Schema.isMaxLength(500)),
});
const ChangesSchema = Schema.Struct({
  cyclesCreated: Schema.Int,
  cyclesRemoved: Schema.Int,
  cyclesRedated: Schema.Int,
  issuesReleased: Schema.Int,
});
const ScheduleResult = Schema.Struct({ project: Project, changes: ChangesSchema });
const Outcome = Schema.Literals(["completed", "canceled", "carriedOver", "notCompleted"]);
const CycleDetail = Schema.Struct({
  ...Cycle.fields,
  projectKey: Ref,
  startsOn: Cycle.fields.startDate,
  endsOn: Cycle.fields.endDate,
  finishedAt: Schema.NullOr(Timestamp),
  finishedEarly: Schema.Boolean,
  carriedToNumber: Schema.NullOr(Schema.Int),
  progress: Schema.Struct({
    counted: Schema.Int,
    completed: Schema.Int,
    started: Schema.Int,
    toDo: Schema.Int,
    canceled: Schema.Int,
    carriedOver: Schema.Int,
    notCompleted: Schema.Int,
    points: Schema.NullOr(Schema.Struct({ total: Schema.Number, completed: Schema.Number })),
  }),
});
const Added = Schema.Struct({
  cycle: CycleDetail,
  added: Schema.Int,
  skippedCount: Schema.Int,
  skipped: Schema.Array(
    Schema.Struct({
      id: Uuid,
      identifier: Ref,
      reason: Schema.Literals(["archived", "otherProject", "closed", "otherCycle"]),
      fromTrackerId: Schema.NullOr(Uuid),
    }),
  ),
});
const Result = Schema.Struct({
  ...IssueFields.fields,
  stateType: StateType,
  stateName: Schema.String,
  points: Schema.NullOr(Schema.Number),
  outcome: Outcome,
  joinedAfterStart: Schema.Boolean,
});
const Tracker = Schema.Struct({
  ...IssueFields.fields,
  inCycle: Schema.Int,
  completedInCycle: Schema.Int,
});
const terminal = new Set(["completed", "canceled", "duplicate"]);

const cycleScope = (member: Member, projectId?: string) =>
  and(
    eq(cycles.organisationId, member.organisationId),
    projectId ? eq(cycles.projectId, projectId) : undefined,
  );
const issueScope = (member: Member, id: string) =>
  and(eq(issues.organisationId, member.organisationId), eq(issues.id, id));
const shift = (date: string, days: number) =>
  new Date(Date.parse(date) + days * 86400_000).toISOString().slice(0, 10);
function nextStart(date: string, day: Settings["startDay"]) {
  const weekday = new Date(date).getUTCDay();
  return shift(date, (cycleStartDays.indexOf(day) + 1 - weekday + 7) % 7);
}

function readCycle(db: Db, member: Member, id: string) {
  return Effect.gen(function* () {
    const [cycle] = yield* db
      .select()
      .from(cycles)
      .where(and(cycleScope(member), eq(cycles.id, id)));
    if (!cycle) return yield* Effect.fail(problem(404, "Cycle not found."));
    return cycle;
  });
}

function cycleIssues(db: Db, member: Member, cycleId: string) {
  return db
    .select({ issue: issues, state: states })
    .from(issues)
    .innerJoin(
      states,
      and(
        eq(states.organisationId, member.organisationId),
        eq(states.id, issues.stateId),
        eq(states.projectId, issues.projectId),
      ),
    )
    .where(
      and(
        eq(issues.organisationId, member.organisationId),
        eq(issues.cycleId, cycleId),
        isNull(issues.archivedAt),
      ),
    );
}

const results = (db: Db, member: Member, cycleId: string) =>
  db
    .select()
    .from(cycleResults)
    .where(
      and(
        eq(cycleResults.organisationId, member.organisationId),
        eq(cycleResults.cycleId, cycleId),
      ),
    );

function cycleDto(db: Db, member: Member, cycle: CycleRow, project: ProjectRow) {
  return Effect.gen(function* () {
    const closed = cycle.state === "closed";
    const rows = closed
      ? (yield* results(db, member, cycle.id)).map((row) => ({
          state: row.stateType,
          estimate: row.points,
          outcome: row.outcome,
        }))
      : (yield* cycleIssues(db, member, cycle.id)).map(({ issue, state }) => ({
          state: state.type,
          estimate: issue.estimate,
          outcome: undefined,
        }));
    const counted = rows.filter((row) => row.state !== "canceled" && row.state !== "duplicate");
    const completed = counted.filter((row) => row.state === "completed");
    const started = counted.filter((row) => row.state === "started").length;
    const sum = (list: readonly { estimate: number | null }[]) =>
      list.reduce((total, row) => total + (row.estimate ?? 0), 0);
    return {
      id: cycle.id,
      projectId: project.id,
      projectKey: project.key,
      number: cycle.number,
      state: cycle.state,
      startDate: cycle.startDate,
      endDate: cycle.endDate,
      startsOn: cycle.startDate,
      endsOn: cycle.endDate,
      finishedAt: dateTimeOrNull(cycle.finishedAt),
      finishedEarly: cycle.finishedEarly,
      carriedToNumber: cycle.carriedToNumber,
      progress: {
        counted: counted.length,
        completed: completed.length,
        started,
        toDo: counted.length - completed.length - started,
        canceled: rows.length - counted.length,
        carriedOver: rows.filter((row) => row.outcome === "carriedOver").length,
        notCompleted: rows.filter((row) => row.outcome === "notCompleted").length,
        points:
          project.estimateScale === "notUsed" && !closed
            ? null
            : { total: sum(counted), completed: sum(completed) },
      },
    };
  });
}

function cycleAudit(
  db: Db,
  member: Member,
  cycle: CycleRow,
  action: string,
  changes: Record<string, unknown> = {},
) {
  return recordAudit(db, member, {
    action,
    projectId: cycle.projectId,
    entityType: "IssueCycle",
    entityId: cycle.id,
    summary: "Changed issue cycle planning.",
    changes,
  });
}

function assignCycle(
  db: Db,
  member: Member,
  issue: IssueRow,
  next: CycleRow | null,
  joinedAt?: Date,
) {
  return Effect.gen(function* () {
    yield* db
      .update(issues)
      .set({
        cycleId: next?.id ?? null,
        cycleJoinedAt: next ? (joinedAt ?? new Date()) : null,
        version: issue.version + 1,
        updatedAt: sql`now()`,
        lastActivityAt: sql`now()`,
      })
      .where(issueScope(member, issue.id));
    yield* addActivity(db, member, issue.id, "issue.changed", {
      fields: ["cycleId"],
      previousCycleId: issue.cycleId,
      cycleId: next?.id ?? null,
    });
  });
}

function closeCycle(
  db: Db,
  member: Member,
  project: ProjectRow,
  cycle: CycleRow,
  next?: CycleRow,
  early = false,
) {
  return Effect.gen(function* () {
    const carry = !early && project.cycleSettings.carryOver && next;
    let released = 0;
    for (const { issue, state } of yield* cycleIssues(db, member, cycle.id)) {
      const closed = terminal.has(state.type);
      const outcome =
        state.type === "completed"
          ? ("completed" as const)
          : closed
            ? ("canceled" as const)
            : carry
              ? ("carriedOver" as const)
              : ("notCompleted" as const);
      yield* db
        .insert(cycleResults)
        .values({
          organisationId: member.organisationId,
          projectId: project.id,
          cycleId: cycle.id,
          issueId: issue.id,
          stateType: state.type,
          stateName: state.name,
          points: project.estimateScale === "notUsed" ? null : issue.estimate,
          outcome,
          joinedAfterStart: Boolean(
            issue.cycleJoinedAt &&
            issue.cycleJoinedAt.valueOf() >= Date.parse(shift(cycle.startDate, 1)),
          ),
        })
        .onConflictDoNothing();
      if (!closed) {
        yield* assignCycle(
          db,
          member,
          issue,
          carry || null,
          carry ? new Date(carry.startDate) : undefined,
        );
        if (!carry) released++;
      }
    }
    yield* db
      .update(cycles)
      .set({
        state: "closed",
        finishedAt: new Date(),
        finishedEarly: early,
        carriedToNumber: carry ? carry.number : null,
      })
      .where(and(cycleScope(member, project.id), eq(cycles.id, cycle.id)));
    yield* cycleAudit(db, member, cycle, "cycleFinished", {
      early,
      carriedToNumber: carry ? carry.number : null,
    });
    return released;
  });
}

function removePending(db: Db, member: Member, cycle: CycleRow) {
  return Effect.gen(function* () {
    const enrolled = yield* db
      .select()
      .from(issues)
      .where(and(eq(issues.organisationId, member.organisationId), eq(issues.cycleId, cycle.id)));
    for (const issue of enrolled) yield* assignCycle(db, member, issue, null);
    yield* db
      .delete(cycles)
      .where(and(cycleScope(member, cycle.projectId), eq(cycles.id, cycle.id)));
    yield* cycleAudit(db, member, cycle, "cycleDeleted");
    return enrolled.length;
  });
}

function reconcile(db: Db, member: Member, project: ProjectRow, updated: boolean) {
  return Effect.gen(function* () {
    const changes = { cyclesCreated: 0, cyclesRemoved: 0, cyclesRedated: 0, issuesReleased: 0 };
    const scope = cycleScope(member, project.id);
    const rows = yield* db.select().from(cycles).where(scope).orderBy(asc(cycles.number));
    const config = project.cycleSettings;
    let current = rows.find((row) => row.state === "current");
    const pending = rows.filter(({ state }) => state === "upcoming");
    if (!config.enabled) {
      if (current)
        changes.issuesReleased += yield* closeCycle(db, member, project, current, undefined, true);
      for (const cycle of pending)
        changes.issuesReleased += yield* removePending(db, member, cycle);
      changes.cyclesRemoved = pending.length;
      return changes;
    }
    let number = Math.max(project.lastCycleNumber, rows.at(-1)?.number ?? 0);
    const date = today();
    let lastEnd = rows.findLast(({ state }) => state === "closed")?.endDate;
    for (const cycle of pending.splice(config.cyclesAhead)) {
      changes.issuesReleased += yield* removePending(db, member, cycle);
      changes.cyclesRemoved++;
    }
    if (updated && pending.length) {
      let end = current?.endDate ?? lastEnd;
      for (const cycle of pending) {
        const startDate = nextStart(end && end >= date ? shift(end, 1) : date, config.startDay);
        const endDate = shift(startDate, config.lengthWeeks * 7 - 1);
        end = endDate;
        if (cycle.startDate === startDate && cycle.endDate === endDate) continue;
        yield* db
          .update(cycles)
          .set({ startDate, endDate })
          .where(and(scope, eq(cycles.id, cycle.id)));
        cycle.startDate = startDate;
        cycle.endDate = endDate;
        changes.cyclesRedated++;
        yield* cycleAudit(db, member, cycle, "cycleRedated", { startDate, endDate });
      }
    }
    for (let boundary = 0; boundary < 60; boundary++) {
      if (current && current.endDate < date && pending.length) {
        changes.issuesReleased += yield* closeCycle(db, member, project, current, pending[0]);
        lastEnd = current.endDate;
        current = undefined;
        continue;
      }
      if (!current && pending[0] && pending[0].startDate <= date) {
        current = pending.shift()!;
        current.state = "current";
        yield* db
          .update(cycles)
          .set({ state: "current" })
          .where(and(scope, eq(cycles.id, current.id)));
        yield* cycleAudit(db, member, current, "cycleStarted");
        continue;
      }
      if (pending.length >= config.cyclesAhead) break;
      const previous = pending.at(-1)?.endDate ?? current?.endDate ?? lastEnd;
      const startDate = nextStart(
        previous && (current || previous >= date) ? shift(previous, 1) : date,
        config.startDay,
      );
      const [cycle] = yield* db
        .insert(cycles)
        .values({
          organisationId: member.organisationId,
          projectId: project.id,
          number: ++number,
          state: "upcoming",
          startDate,
          endDate: shift(startDate, config.lengthWeeks * 7 - 1),
        })
        .returning();
      pending.push(cycle!);
      yield* cycleAudit(db, member, cycle!, "cycleCreated");
      changes.cyclesCreated++;
    }
    if (number !== project.lastCycleNumber) {
      yield* db
        .update(projects)
        .set({ lastCycleNumber: number })
        .where(
          and(eq(projects.organisationId, member.organisationId), eq(projects.id, project.id)),
        );
    }
    return changes;
  });
}

export function reconcileCycles(
  member: Member,
  reference: string,
  input?: typeof SetCycleSettings.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const { project, lead } = yield* readProject(tx, member, reference);
        if (project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Unarchive the project before scheduling cycles."),
          );
        let active = project;
        if (input) {
          const { version, ...cycleSettings } = input;
          if (member.agentRank || (member.role !== "admin" && member.id !== project.leadId))
            return yield* Effect.fail(
              problem(403, "Cycle settings require the human project lead or an administrator."),
            );
          if (version !== project.version)
            return yield* Effect.fail(
              problem(409, "The project version is stale.", project.version),
            );
          if (
            Object.entries(cycleSettings).some(
              ([key, value]) => project.cycleSettings[key as keyof Settings] !== value,
            )
          ) {
            const [saved] = yield* tx
              .update(projects)
              .set({ cycleSettings, version: version + 1, updatedAt: sql`now()` })
              .where(
                and(
                  eq(projects.organisationId, member.organisationId),
                  eq(projects.id, project.id),
                ),
              )
              .returning();
            active = saved!;
            yield* recordAudit(tx, member, {
              action: "cycleSettingsUpdated",
              entityType: "Project",
              entityId: project.id,
              projectId: project.id,
              summary: "Changed the project cycle schedule.",
              changes: cycleSettings,
            });
          }
        }
        const redated =
          active !== project &&
          (active.cycleSettings.lengthWeeks !== project.cycleSettings.lengthWeeks ||
            active.cycleSettings.startDay !== project.cycleSettings.startDay);
        const changes: Changes = yield* reconcile(tx, member, active, redated);
        return { project: projectDto(active, lead), changes };
      }),
    ),
  );
}

export function cycleDetail(member: Member, id: string) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const cycle = yield* readCycle(db, member, id);
    const { project } = yield* readProject(db, member, cycle.projectId);
    return yield* cycleDto(db, member, cycle, project);
  });
}

export function addCycleIssues(member: Member, id: string, references: readonly string[]) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const cycle = yield* readCycle(tx, member, id);
        const { project } = yield* readProject(tx, member, cycle.projectId);
        if (cycle.state === "closed" || project.archivedAt)
          return yield* Effect.fail(
            problem(409, "Choose a current or upcoming cycle in an active project."),
          );
        const selected = yield* Effect.forEach(references, (ref) => readIssue(tx, member, ref));
        const ids = selected.map(({ id }) => id);
        if (new Set(ids).size !== ids.length)
          return yield* Effect.fail(problem(400, "Choose each issue once."));
        const tracks = new Set(
          selected
            .filter(
              (issue) =>
                issue.type === "tracker" && !issue.archivedAt && issue.projectId === project.id,
            )
            .map(({ id }) => id),
        );
        const rows = yield* tx
          .select({ issue: issues, state: states, previous: cycles })
          .from(issues)
          .innerJoin(
            states,
            and(
              eq(states.organisationId, member.organisationId),
              eq(states.projectId, issues.projectId),
              eq(states.id, issues.stateId),
            ),
          )
          .leftJoin(
            cycles,
            and(
              eq(cycles.organisationId, member.organisationId),
              eq(cycles.projectId, issues.projectId),
              eq(cycles.id, issues.cycleId),
            ),
          )
          .where(
            and(
              eq(issues.organisationId, member.organisationId),
              or(
                inArray(issues.id, ids),
                tracks.size ? inArray(issues.parentId, [...tracks]) : undefined,
              ),
            ),
          );
        const skipped: (typeof Added.Type.skipped)[number][] = [];
        const eligible: IssueRow[] = [];
        for (const { issue, state, previous } of rows) {
          if (tracks.has(issue.id) || issue.cycleId === cycle.id) continue;
          const reason = issue.archivedAt
            ? "archived"
            : issue.projectId !== project.id
              ? "otherProject"
              : terminal.has(state.type)
                ? "closed"
                : previous && previous.state !== "closed"
                  ? "otherCycle"
                  : null;
          if (reason)
            skipped.push({
              id: issue.id,
              identifier: issue.identifier,
              reason,
              fromTrackerId: tracks.has(issue.parentId ?? "") ? issue.parentId : null,
            });
          else if (issue.type === "issue") eligible.push(issue);
        }
        if (eligible.length > 500)
          return yield* Effect.fail(problem(400, "Add at most 500 issues at a time."));
        for (const issue of eligible) yield* assignCycle(tx, member, issue, cycle);
        if (eligible.length)
          yield* cycleAudit(tx, member, cycle, "cycleIssuesAdded", {
            added: eligible.length,
            skipped: skipped.length,
          });
        return {
          cycle: yield* cycleDto(tx, member, cycle, project),
          added: eligible.length,
          skipped: skipped.slice(0, 100),
          skippedCount: skipped.length,
        };
      }),
    ),
  );
}

export function listCycleResults(
  member: Member,
  id: string,
  query: { limit?: number; cursor?: string },
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    yield* readCycle(db, member, id);
    const rows = yield* results(db, member, id);
    const key = (row: (typeof rows)[number]) => row.issueId;
    const result = yield* page(
      rows.sort(byKey(key)),
      { org: member.organisationId, cycleResults: id },
      key,
      query.limit,
      query.cursor,
    );
    const byId = new Map(result.items.map((row) => [row.issueId, row]));
    const items = yield* issueRowsById(
      db,
      member,
      result.items.map(({ issueId }) => issueId),
    );
    return {
      ...result,
      items: items.map((issue) => {
        const { stateType, stateName, points, outcome, joinedAfterStart } = byId.get(issue.id)!;
        return { ...issue, stateType, stateName, points, outcome, joinedAfterStart };
      }),
    };
  });
}

export function listCycleTrackers(
  member: Member,
  id: string,
  query: { limit?: number; cursor?: string },
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const cycle = yield* readCycle(db, member, id);
    const enrolled =
      cycle.state === "closed"
        ? yield* db
            .select({ issue: issues, state: cycleResults.stateType })
            .from(cycleResults)
            .innerJoin(
              issues,
              and(
                eq(issues.organisationId, member.organisationId),
                eq(issues.id, cycleResults.issueId),
              ),
            )
            .where(
              and(
                eq(cycleResults.organisationId, member.organisationId),
                eq(cycleResults.cycleId, id),
              ),
            )
        : (yield* cycleIssues(db, member, id)).map(({ issue, state }) => ({
            issue,
            state: state.type,
          }));
    const grouped = Map.groupBy(enrolled, ({ issue }) => issue.parentId);
    const trackers = yield* issueRowsById(
      db,
      member,
      [...grouped.keys()].filter((id) => id !== null),
    );
    const key = (row: (typeof trackers)[number]) => row.identifier;
    const result = yield* page(
      trackers.filter(({ type, archivedAt }) => type === "tracker" && !archivedAt).sort(byKey(key)),
      { org: member.organisationId, cycleTrackers: id },
      key,
      query.limit,
      query.cursor,
    );
    return {
      ...result,
      items: result.items.map((tracker) => {
        const items = grouped.get(tracker.id) ?? [];
        return {
          ...tracker,
          inCycle: items.length,
          completedInCycle: items.filter(({ state }) => state === "completed").length,
        };
      }),
    };
  });
}

const p = { reference: Ref };
const c = { id: Uuid };
export const cycleGroup = HttpApiGroup.make("cycles")
  .add(
    HttpApiEndpoint.put("settings", "/projects/:reference/cycle-settings", {
      params: p,
      payload: SetCycleSettings,
      success: ScheduleResult,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("reconcile", "/projects/:reference/cycles/reconcile", {
      params: p,
      success: ScheduleResult,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("cycle", "/issue-cycles/:id", {
      params: c,
      success: CycleDetail,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("add", "/issue-cycles/:id/issues", {
      params: c,
      payload: AddCycleIssues,
      success: Added,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("results", "/issue-cycles/:id/results", {
      params: c,
      query: PageQuery,
      success: pageOf(Result),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("trackers", "/issue-cycles/:id/trackers", {
      params: c,
      query: PageQuery,
      success: pageOf(Tracker),
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const cycleHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof cycleGroup>,
) =>
  handlers
    .handle("settings", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          reconcileCycles(member, params.reference, payload),
        ),
      ),
    )
    .handle("reconcile", ({ params }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => reconcileCycles(member, params.reference)),
      ),
    )
    .handle("cycle", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => cycleDetail(member, params.id))),
    )
    .handle("add", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          addCycleIssues(member, params.id, payload.issues),
        ),
      ),
    )
    .handle("results", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listCycleResults(member, params.id, query)),
      ),
    )
    .handle("trackers", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listCycleTrackers(member, params.id, query)),
      ),
    );
