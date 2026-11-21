import {
  and,
  eq,
  exists,
  gte,
  inArray,
  isNotNull,
  isNull,
  lt,
  lte,
  notExists,
  or,
  sql,
} from "drizzle-orm";
import { alias } from "drizzle-orm/pg-core";
import { Effect } from "effect";
import { type Database } from "./db";
import { type Member } from "./identity";
import { type IssueQuery } from "./issue-contract";
import { readProject } from "./projects";
import { cycles, issueLabels, issues, issueSubscriptions, projects, states, users } from "./schema";
import { today } from "./validation";

type Db = (typeof Database)["Service"];
export type IssueFilter = Omit<typeof IssueQuery.Type, "limit" | "cursor">;

function personFilter(
  column: typeof issues.assigneeId | typeof issues.ownerId | typeof issues.createdById,
  actor: string,
  values?: readonly string[],
) {
  if (!values?.length) return;
  const ids = values
    .filter((value) => value !== "none")
    .map((value) => (value === "me" ? actor : value));
  return or(
    ids.length ? inArray(sql`${column}::text`, ids) : undefined,
    values.includes("none") ? isNull(column) : undefined,
  );
}

function cycleFilter(db: Db, values: readonly string[], inProject: boolean) {
  const calendar = alias(cycles, "calendar");
  const adjacent = (state: "upcoming" | "closed") =>
    and(
      eq(cycles.state, state),
      notExists(
        db
          .select({ id: calendar.id })
          .from(calendar)
          .where(
            and(
              eq(calendar.organisationId, cycles.organisationId),
              eq(calendar.projectId, cycles.projectId),
              eq(calendar.state, state),
              state === "upcoming"
                ? lt(calendar.number, cycles.number)
                : lt(cycles.number, calendar.number),
            ),
          ),
      ),
    );
  return or(
    inArray(sql`${cycles.id}::text`, values),
    inProject ? inArray(sql`${cycles.number}::text`, values) : undefined,
    values.includes("none") || values.includes("unplanned") ? isNull(cycles.id) : undefined,
    values.includes("current") ? eq(cycles.state, "current") : undefined,
    values.includes("next") ? adjacent("upcoming") : undefined,
    values.includes("previous") ? adjacent("closed") : undefined,
  );
}

function dueFilter(filter: IssueFilter["due"]) {
  if (filter === "none") return isNull(issues.dueDate);
  if (filter === "overdue") return lt(issues.dueDate, today());
  if (filter === "thisWeek") {
    const monday = new Date(`${today()}T00:00:00.000Z`);
    monday.setUTCDate(monday.getUTCDate() - ((monday.getUTCDay() + 6) % 7));
    const sunday = new Date(monday);
    sunday.setUTCDate(sunday.getUTCDate() + 6);
    return and(
      gte(issues.dueDate, monday.toISOString().slice(0, 10)),
      lte(issues.dueDate, sunday.toISOString().slice(0, 10)),
    );
  }
  return undefined;
}

export function issueConditions(db: Db, member: Member, filters: IssueFilter, parent?: string) {
  return Effect.gen(function* () {
    const org = member.organisationId;
    const project = filters.project
      ? (yield* readProject(db, member, filters.project)).project
      : null;
    const q = filters.q?.trim().toLowerCase();
    const subscription = db
      .select({ id: issueSubscriptions.issueId })
      .from(issueSubscriptions)
      .where(
        and(
          eq(issueSubscriptions.organisationId, org),
          eq(issueSubscriptions.issueId, issues.id),
          eq(issueSubscriptions.userId, member.id),
          eq(issueSubscriptions.subscribed, true),
        ),
      );
    return {
      project,
      where: and(
        eq(issues.organisationId, org),
        filters.includeArchived
          ? undefined
          : and(isNull(issues.archivedAt), isNull(projects.archivedAt)),
        project ? eq(issues.projectId, project.id) : undefined,
        filters.projectIds ? inArray(issues.projectId, filters.projectIds) : undefined,
        filters.milestone?.length
          ? inArray(sql`coalesce(${issues.milestoneId}::text, 'none')`, filters.milestone)
          : undefined,
        filters.cycle?.length ? cycleFilter(db, filters.cycle, Boolean(project)) : undefined,
        filters.subscribed === undefined
          ? undefined
          : filters.subscribed
            ? exists(subscription)
            : notExists(subscription),
        filters.updatedSince
          ? gte(issues.updatedAt, new Date(filters.updatedSince.epochMilliseconds))
          : undefined,
        filters.activitySince
          ? gte(issues.lastActivityAt, new Date(filters.activitySince.epochMilliseconds))
          : undefined,
        filters.completedSince
          ? gte(issues.completedAt, new Date(filters.completedSince.epochMilliseconds))
          : undefined,
        parent === "none"
          ? isNull(issues.parentId)
          : parent
            ? eq(issues.parentId, parent)
            : undefined,
        filters.stateId ? eq(issues.stateId, filters.stateId) : undefined,
        filters.stateType?.length ? inArray(states.type, filters.stateType) : undefined,
        filters.state?.length
          ? or(
              inArray(sql`${states.id}::text`, filters.state),
              inArray(
                sql`lower(${states.name})`,
                filters.state.map((name) => name.toLowerCase()),
              ),
            )
          : undefined,
        filters.priority?.length ? inArray(issues.priority, filters.priority) : undefined,
        personFilter(issues.assigneeId, member.id, filters.assignee),
        filters.assigneeKind
          ? exists(
              db
                .select({ id: users.id })
                .from(users)
                .where(
                  and(
                    eq(users.organisationId, org),
                    eq(users.id, issues.assigneeId),
                    filters.assigneeKind === "agent"
                      ? isNotNull(users.agentRank)
                      : isNull(users.agentRank),
                  ),
                ),
            )
          : undefined,
        personFilter(issues.ownerId, member.id, filters.owner),
        personFilter(issues.createdById, member.id, filters.creator),
        filters.type ? eq(issues.type, filters.type) : undefined,
        filters.label?.length
          ? exists(
              db
                .select({ id: issueLabels.issueId })
                .from(issueLabels)
                .where(
                  and(
                    eq(issueLabels.organisationId, org),
                    eq(issueLabels.issueId, issues.id),
                    inArray(issueLabels.labelId, filters.label),
                  ),
                ),
            )
          : undefined,
        q
          ? or(
              ...[issues.identifier, issues.title, issues.summary, issues.description].map(
                (column) => sql`strpos(lower(${column}), ${q}) > 0`,
              ),
            )
          : undefined,
        dueFilter(filters.due),
      ),
    };
  });
}
