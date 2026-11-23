import { createHash } from "node:crypto";
import { and, asc, eq, inArray, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { estimatePoints } from "./estimates";
import { requireMoveOwnership } from "./fleet-ownership";
import { Database } from "./db";
import { type Member } from "./identity";
import { Version } from "./initiative-contract";
import { IssueDetail } from "./issue-contract";
import { addActivity, detailFrom, lockIssues, readIssue, refreshTrackers } from "./issues";
import { problem } from "./problems";
import { readProject } from "./projects";
import { issueAliases, issues, projects, states } from "./schema";
import { Name, Ref, Uuid } from "./validation";

const maximum = 500;
const MoveToken = Schema.String.check(Schema.isPattern(/^[0-9a-f]{64}$/u));
const MoveProject = Schema.Struct({ id: Uuid, key: Ref, name: Name });
const MoveItem = Schema.Struct({
  id: Uuid,
  version: Version,
  oldIdentifier: Ref,
  newIdentifier: Ref,
  stateId: Uuid,
  stateName: Name,
  cycleCleared: Schema.Boolean,
  milestoneCleared: Schema.Boolean,
  estimateCleared: Schema.Boolean,
  parentDetached: Schema.Boolean,
  archived: Schema.Boolean,
}).annotate({ identifier: "IssueMoveItem" });

export const MoveQuery = Schema.Struct({ destinationProjectId: Uuid, version: Version });
export const MoveRequest = Schema.Struct({ ...MoveQuery.fields, token: MoveToken });
export const MovePreview = Schema.Struct({
  token: MoveToken,
  source: MoveProject,
  destination: MoveProject,
  issues: Schema.Array(MoveItem),
});
export const MoveResult = Schema.Struct({ issue: IssueDetail, moved: Schema.Array(MoveItem) });

type Db = (typeof Database)["Service"];

function planning(
  db: Db,
  member: Member,
  reference: string,
  input: typeof MoveQuery.Type,
  lock: boolean,
) {
  return Effect.gen(function* () {
    const org = member.organisationId;
    const initial = yield* readIssue(db, member, reference);
    const projectIds = [initial.projectId, input.destinationProjectId];
    if (lock)
      yield* db
        .select({ id: projects.id })
        .from(projects)
        .where(and(eq(projects.organisationId, org), inArray(projects.id, projectIds)))
        .orderBy(asc(projects.id))
        .for("update");
    const { project: source } = yield* readProject(db, member, initial.projectId);
    const { project: destination } = yield* readProject(db, member, input.destinationProjectId);
    if (source.id === destination.id || destination.archivedAt || source.archivedAt) {
      return yield* Effect.fail(problem(409, "Select a different active destination project."));
    }
    const query = db
      .select()
      .from(issues)
      .where(eq(issues.organisationId, org))
      .orderBy(asc(issues.identifier), asc(issues.id));
    const rows = lock ? yield* query.for("update") : yield* query;
    const root = rows.find((row) => row.id === initial.id)!;
    if (root.version !== input.version)
      return yield* Effect.fail(problem(409, "The issue version is stale.", root.version));
    const children = Map.groupBy(rows, ({ parentId }) => parentId);
    const subtree = [root];
    for (const current of subtree) {
      subtree.push(...(children.get(current.id) ?? []));
      if (subtree.length > maximum)
        return yield* Effect.fail(problem(409, "At most 500 issues can move in one request."));
    }
    if (subtree.some((row) => row.projectId !== source.id)) {
      return yield* Effect.fail(
        problem(409, "Resolve cross-project descendants before moving this issue."),
      );
    }
    const catalogue = yield* db
      .select()
      .from(states)
      .where(and(eq(states.organisationId, org), inArray(states.projectId, projectIds)))
      .orderBy(asc(states.position), asc(states.id));
    const sourceStates = new Map(
      catalogue.filter(({ projectId }) => projectId === source.id).map((row) => [row.id, row]),
    );
    const choices = Map.groupBy(
      catalogue.filter(({ projectId }) => projectId === destination.id),
      ({ type }) => type,
    );
    if (destination.nextIssueNumber > 2_147_483_647 - subtree.length) {
      return yield* Effect.fail(problem(409, "The destination issue number limit was reached."));
    }
    const items: (typeof MoveItem.Type)[] = [];
    for (const [index, row] of subtree.entries()) {
      const from = sourceStates.get(row.stateId);
      const to = from ? choices.get(from.type)?.[0] : undefined;
      if (!to)
        return yield* Effect.fail(
          problem(409, "The destination has no equivalent workflow state."),
        );
      items.push({
        id: row.id,
        version: row.version,
        oldIdentifier: row.identifier,
        newIdentifier: `${destination.key}-${destination.nextIssueNumber + index}`,
        stateId: to.id,
        stateName: to.name,
        cycleCleared: row.cycleId !== null,
        milestoneCleared: row.milestoneId !== null,
        estimateCleared:
          row.estimate !== null &&
          !estimatePoints[destination.estimateScale].includes(row.estimate),
        parentDetached: index === 0 && row.parentId !== null,
        archived: row.archivedAt !== null,
      });
    }
    const token = createHash("sha256")
      .update(
        JSON.stringify([
          org,
          member.id,
          source.id,
          source.version,
          destination.id,
          destination.version,
          items,
        ]),
      )
      .digest("hex");
    const view = (row: typeof projects.$inferSelect) => ({
      id: row.id,
      key: row.key,
      name: row.name,
    });
    return {
      token,
      issues: items,
      source: view(source),
      destination: view(destination),
      firstNumber: destination.nextIssueNumber,
    };
  });
}

export const previewMove = (member: Member, reference: string, input: typeof MoveQuery.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) => planning(tx, member, reference, input, false), {
      isolationLevel: "repeatable read",
      accessMode: "read only",
    }),
  );

export const moveIssue = (member: Member, reference: string, input: typeof MoveRequest.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        yield* lockIssues(tx, member);
        const before = yield* readIssue(tx, member, reference);
        const plan = yield* planning(tx, member, reference, input, true);
        yield* requireMoveOwnership(tx, member, plan.source.id, plan.destination.id);
        if (input.token !== plan.token)
          return yield* Effect.fail(problem(409, "The move changed. Request a fresh preview."));
        yield* tx.insert(issueAliases).values(
          plan.issues.map((row) => ({
            organisationId: member.organisationId,
            identifier: row.oldIdentifier,
            issueId: row.id,
          })),
        );
        for (const [index, row] of plan.issues.entries()) {
          yield* tx
            .update(issues)
            .set({
              projectId: plan.destination.id,
              identifier: row.newIdentifier,
              number: plan.firstNumber + index,
              stateId: row.stateId,
              cycleId: null,
              cycleJoinedAt: null,
              milestoneId: null,
              estimate: row.estimateCleared ? null : undefined,
              ...(row.parentDetached ? { parentId: null } : {}),
              version: row.version + 1,
              updatedAt: sql`now()`,
              lastActivityAt: sql`now()`,
            })
            .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, row.id)));
          yield* addActivity(tx, member, row.id, "issue.moved", {
            oldIdentifier: row.oldIdentifier,
            newIdentifier: row.newIdentifier,
            sourceProjectId: plan.source.id,
          });
        }
        yield* tx
          .update(projects)
          .set({ nextIssueNumber: plan.firstNumber + plan.issues.length })
          .where(
            and(
              eq(projects.organisationId, member.organisationId),
              eq(projects.id, plan.destination.id),
            ),
          );
        if (before.parentId) yield* refreshTrackers(tx, member, [before.parentId]);
        return { issue: yield* detailFrom(tx, member, plan.issues[0]!.id), moved: plan.issues };
      }),
    ),
  );
