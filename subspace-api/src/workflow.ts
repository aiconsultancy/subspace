import { and, eq, isNotNull, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup } from "effect/http-api";
import { recordAudit } from "./audit";
import { Database } from "./db";
import { EstimateScale, nearestEstimate } from "./estimates";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { addActivity, datesForState, lockIssues, refreshTrackers } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors, problem } from "./problems";
import { Project, projectDto, readProject, StateType, stateTypes } from "./projects";
import { issues, projects, states } from "./schema";
import { Name, Ref, Uuid } from "./validation";

export const ReplaceWorkflow = Schema.Struct({
  version: Version,
  states: Schema.Array(
    Schema.Struct({
      id: Schema.optional(Schema.NullOr(Uuid)),
      name: Name,
      type: StateType,
      isDefault: Schema.Boolean,
    }),
  ).check(Schema.isMinLength(7), Schema.isMaxLength(50)),
});
const DeleteState = Schema.Struct({ version: Version, replacementStateId: Schema.optional(Uuid) });
export const EstimateInput = Schema.Struct({ version: Version, scale: EstimateScale });
const EstimatePreview = Schema.Struct({
  scale: EstimateScale,
  issuesWithEstimate: Schema.Int,
  mappings: Schema.Array(
    Schema.Struct({ from: Schema.Number, to: Schema.Number, issues: Schema.Int }),
  ),
});
const EstimateResult = Schema.Struct({
  project: Project,
  changes: Schema.Struct({ issuesMapped: Schema.Int }),
});
type Db = (typeof Database)["Service"];

function projectForWrite(db: Db, member: Member, reference: string, version: number) {
  return Effect.gen(function* () {
    yield* lockIssues(db, member);
    const joined = yield* readProject(db, member, reference);
    const { project } = joined;
    if (member.agentRank || (member.role !== "admin" && member.id !== project.leadId))
      return yield* Effect.fail(
        problem(403, "Workflow and estimates require the human project lead or an administrator."),
      );
    if (project.archivedAt || project.version !== version)
      return yield* Effect.fail(
        problem(409, "The project is archived or its version is stale.", project.version),
      );
    return joined;
  });
}

const stateScope = (member: Member, projectId: string) =>
  and(eq(states.organisationId, member.organisationId), eq(states.projectId, projectId));
const issueScope = (member: Member, projectId: string) =>
  and(eq(issues.organisationId, member.organisationId), eq(issues.projectId, projectId));
const projectScope = (member: Member, projectId: string) =>
  and(eq(projects.organisationId, member.organisationId), eq(projects.id, projectId));

function workflowAudit(
  db: Db,
  member: Member,
  projectId: string,
  action: string,
  changes: Record<string, unknown>,
) {
  return recordAudit(db, member, {
    action,
    entityType: "IssueWorkflow",
    entityId: projectId,
    projectId,
    summary: "Changed project workflow settings.",
    changes,
  });
}

function refreshProjectTrackers(db: Db, member: Member, projectId: string) {
  return Effect.gen(function* () {
    const rows = yield* db
      .select({ id: issues.id })
      .from(issues)
      .where(and(issueScope(member, projectId), eq(issues.type, "tracker")));
    yield* refreshTrackers(
      db,
      member,
      rows.map(({ id }) => id),
    );
  });
}

export function replaceWorkflow(
  member: Member,
  reference: string,
  input: typeof ReplaceWorkflow.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const { project, lead } = yield* projectForWrite(tx, member, reference, input.version);
        const scope = stateScope(member, project.id);
        const old = yield* tx.select().from(states).where(scope);
        const known = new Map(old.map((state) => [state.id, state]));
        const ids = input.states.flatMap(({ id }) => (id ? [id] : []));
        const names = new Set(input.states.map(({ name }) => name.toLowerCase()));
        const defaults = input.states.filter(({ isDefault }) => isDefault);
        if (
          new Set(ids).size !== old.length ||
          ids.length !== old.length ||
          names.size !== input.states.length ||
          input.states.some((draft) => draft.id && known.get(draft.id)?.type !== draft.type) ||
          defaults.length !== 1 ||
          !["triage", "backlog", "unstarted"].includes(defaults[0]!.type) ||
          stateTypes.some((type) => !input.states.some((draft) => draft.type === type))
        ) {
          return yield* Effect.fail(
            problem(
              400,
              "Retain every existing state and its type, use unique names and choose one open default.",
            ),
          );
        }
        const positions = new Map<string, number>();
        const nextStates = input.states.map((draft) => {
          const position = positions.get(draft.type) ?? 0;
          positions.set(draft.type, position + 1);
          return { ...draft, id: draft.id ?? crypto.randomUUID(), position };
        });
        if (
          nextStates.every((row) => {
            const before = known.get(row.id);
            return (
              before &&
              before.position === row.position &&
              before.name === row.name &&
              before.isDefault === row.isDefault
            );
          })
        )
          return projectDto(project, lead);
        yield* tx
          .update(states)
          .set({ position: sql`-${states.position} - 1` })
          .where(scope);
        for (const state of nextStates) {
          const { id, ...fields } = state;
          if (known.has(id))
            yield* tx
              .update(states)
              .set(fields)
              .where(and(scope, eq(states.id, id)));
          else
            yield* tx.insert(states).values({
              ...fields,
              id,
              projectId: project.id,
              organisationId: member.organisationId,
            });
        }
        const [saved] = yield* tx
          .update(projects)
          .set({
            defaultStateId: nextStates.find(({ isDefault }) => isDefault)!.id,
            version: project.version + 1,
            updatedAt: sql`now()`,
          })
          .where(projectScope(member, project.id))
          .returning();
        yield* refreshProjectTrackers(tx, member, project.id);
        yield* workflowAudit(tx, member, project.id, "issueWorkflowReplaced", {
          states: nextStates.length,
        });
        return projectDto(saved!, lead);
      }),
    ),
  );
}

export function deleteState(
  member: Member,
  reference: string,
  stateId: string,
  input: typeof DeleteState.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const { project, lead } = yield* projectForWrite(tx, member, reference, input.version);
        const scope = stateScope(member, project.id);
        const rows = yield* tx.select().from(states).where(scope).orderBy(states.position);
        const state = rows.find(({ id }) => id === stateId);
        if (!state) return yield* Effect.fail(problem(404, "Workflow state not found."));
        const siblings = rows.filter(({ type, id }) => type === state.type && id !== stateId);
        if (project.defaultStateId === stateId || !siblings.length)
          return yield* Effect.fail(
            problem(400, "Keep the default state and at least one state of each type."),
          );
        const replacement = rows.find(
          ({ id }) => id === input.replacementStateId && id !== stateId,
        );
        if (input.replacementStateId && !replacement)
          return yield* Effect.fail(problem(400, "Choose a replacement state from this project."));
        const affected = yield* tx
          .select()
          .from(issues)
          .where(and(issueScope(member, project.id), eq(issues.stateId, stateId)));
        if (affected.length && !replacement)
          return yield* Effect.fail(
            problem(409, "Choose a replacement for the issues using this state."),
          );
        for (const issue of affected) {
          const next =
            issue.type === "tracker" && replacement!.type !== state.type
              ? siblings[0]!
              : replacement!;
          yield* tx
            .update(issues)
            .set({
              ...datesForState(next.type, issue),
              stateId: next.id,
              version: issue.version + 1,
              updatedAt: sql`now()`,
              lastActivityAt: sql`now()`,
            })
            .where(and(issueScope(member, project.id), eq(issues.id, issue.id)));
          yield* addActivity(tx, member, issue.id, "issue.changed", {
            previousStateId: stateId,
            stateId: next.id,
          });
        }
        yield* tx.delete(states).where(and(scope, eq(states.id, state.id)));
        for (const [position, sibling] of siblings.entries()) {
          if (position !== sibling.position)
            yield* tx
              .update(states)
              .set({ position })
              .where(and(scope, eq(states.id, sibling.id)));
        }
        const [saved] = yield* tx
          .update(projects)
          .set({ version: project.version + 1, updatedAt: sql`now()` })
          .where(projectScope(member, project.id))
          .returning();
        yield* refreshProjectTrackers(tx, member, project.id);
        yield* workflowAudit(tx, member, project.id, "issueWorkflowStateDeleted", {
          stateId,
          replacementStateId: replacement?.id,
        });
        return projectDto(saved!, lead);
      }),
    ),
  );
}

function estimatedIssues(db: Db, member: Member, projectId: string) {
  return db
    .select({ id: issues.id, estimate: issues.estimate })
    .from(issues)
    .where(and(issueScope(member, projectId), isNotNull(issues.estimate)));
}

export function previewEstimates(
  member: Member,
  reference: string,
  scale: typeof EstimateScale.Type,
) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const { project } = yield* readProject(db, member, reference);
    const rows = yield* estimatedIssues(db, member, project.id);
    const counts = new Map<number, number>();
    for (const { estimate } of rows) counts.set(estimate!, (counts.get(estimate!) ?? 0) + 1);
    const mappings = [...counts]
      .sort(([a], [b]) => a - b)
      .flatMap(([from, issues]) => {
        const to = nearestEstimate(from, scale);
        return to === undefined || to === from ? [] : [{ from, to, issues }];
      });
    return { scale, issuesWithEstimate: rows.length, mappings };
  });
}

export function changeEstimateScale(
  member: Member,
  reference: string,
  input: typeof EstimateInput.Type,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const { project, lead } = yield* projectForWrite(tx, member, reference, input.version);
        let issuesMapped = 0;
        if (project.estimateScale === input.scale)
          return { project: projectDto(project, lead), changes: { issuesMapped } };
        if (input.scale !== "notUsed") {
          const rows = yield* estimatedIssues(tx, member, project.id);
          for (const issue of rows) {
            const estimate = nearestEstimate(issue.estimate!, input.scale);
            if (estimate === issue.estimate) continue;
            yield* tx
              .update(issues)
              .set({
                estimate,
                version: sql`${issues.version} + 1`,
                updatedAt: sql`now()`,
                lastActivityAt: sql`now()`,
              })
              .where(and(issueScope(member, project.id), eq(issues.id, issue.id)));
            yield* addActivity(tx, member, issue.id, "issue.changed", {
              fields: ["estimate"],
              previous: issue.estimate,
              estimate,
            });
            issuesMapped++;
          }
        }
        const [saved] = yield* tx
          .update(projects)
          .set({
            estimateScale: input.scale,
            version: project.version + 1,
            updatedAt: sql`now()`,
          })
          .where(projectScope(member, project.id))
          .returning();
        yield* workflowAudit(tx, member, project.id, "issueEstimateScaleUpdated", {
          scale: input.scale,
          issuesMapped,
        });
        return { project: projectDto(saved!, lead), changes: { issuesMapped } };
      }),
    ),
  );
}

const p = { reference: Ref };
export const workflowGroup = HttpApiGroup.make("workflow")
  .add(
    HttpApiEndpoint.put("replaceWorkflow", "/projects/:reference/workflow-states", {
      params: p,
      payload: ReplaceWorkflow,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("deleteState", "/projects/:reference/workflow-states/:stateId", {
      params: { ...p, stateId: Uuid },
      query: DeleteState,
      success: Project,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("previewEstimates", "/projects/:reference/estimate-scale/preview", {
      params: p,
      query: { scale: EstimateScale },
      success: EstimatePreview,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("updateEstimateScale", "/projects/:reference/estimate-scale", {
      params: p,
      payload: EstimateInput,
      success: EstimateResult,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const workflowHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof workflowGroup>,
) =>
  handlers
    .handle("replaceWorkflow", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          replaceWorkflow(member, params.reference, payload),
        ),
      ),
    )
    .handle("deleteState", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          deleteState(member, params.reference, params.stateId, query),
        ),
      ),
    )
    .handle("previewEstimates", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          previewEstimates(member, params.reference, query.scale),
        ),
      ),
    )
    .handle("updateEstimateScale", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          changeEstimateScale(member, params.reference, payload),
        ),
      ),
    );
