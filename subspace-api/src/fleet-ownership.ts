import { and, eq } from "drizzle-orm";
import { Effect } from "effect";
import { type Database } from "./db";
import { type Member } from "./identity";
import { problem } from "./problems";
import { initiatives, projects, users } from "./schema";

type Db = (typeof Database)["Service"];
type Actor = Pick<Member, "id" | "organisationId" | "agentRank">;

export const foreignFleetDetail =
  "The project belongs to another fleet. Fleet agents may assign issues or mark them ready for agents only in their own fleet's projects.";
export const foreignFleetMoveDetail =
  "The project belongs to another fleet. Fleet agents may move issues only between their own fleet's projects and projects without a fleet.";
export const foreignFleetReparentDetail =
  "The initiative belongs to another fleet. Fleet agents may reparent initiatives only between their own fleet's initiatives and initiatives without a fleet.";

/**
 * Whether an issue write asks to dispatch the issue: it names a non-null assignee, or it carries
 * `readyForAgents` with either value. Clearing the assignee is not a dispatch.
 */
export const dispatchesIssue = (input: { assigneeId?: string | null; readyForAgents?: boolean }) =>
  input.assigneeId != null || input.readyForAgents !== undefined;

/**
 * Reads the fleet that owns an initiative: its own fleet, or else the fleet of its nearest
 * ancestor with one. Returns null when nothing on the path has a fleet, including when
 * `initiativeId` is null.
 *
 * Each initiative row read on the path is locked for share until the transaction ends. A
 * concurrent change to the parent or fleet of an initiative on the path therefore either commits
 * first and is read here, or waits until this transaction commits. Ancestors above the first fleet
 * are not read, because they cannot change the answer.
 */
export function lockInitiativeFleet(db: Db, organisationId: string, initiativeId: string | null) {
  return Effect.gen(function* () {
    const visited = new Set<string>();
    let id = initiativeId;
    while (id && !visited.has(id)) {
      visited.add(id);
      const [initiative] = yield* db
        .select({ parentId: initiatives.parentId, fleetId: initiatives.fleetId })
        .from(initiatives)
        .where(and(eq(initiatives.organisationId, organisationId), eq(initiatives.id, id)))
        .for("share");
      if (!initiative) break;
      if (initiative.fleetId) return initiative.fleetId;
      id = initiative.parentId;
    }
    return null;
  });
}

/**
 * Reads the fleet that owns a project: its initiative's fleet, or else the fleet of that
 * initiative's nearest ancestor with one. Returns null when nothing on the path has a fleet.
 *
 * The project row is locked for share until the transaction ends, and so is each initiative row
 * read on the path, as {@link lockInitiativeFleet} describes. A concurrent change to the
 * project's initiative therefore either commits first and is read here, or waits until this
 * transaction commits.
 */
export function lockProjectFleet(db: Db, organisationId: string, projectId: string) {
  return Effect.gen(function* () {
    const [project] = yield* db
      .select({ initiativeId: projects.initiativeId })
      .from(projects)
      .where(and(eq(projects.organisationId, organisationId), eq(projects.id, projectId)))
      .for("share");
    if (!project) return yield* Effect.fail(problem(404, "Project not found."));
    return yield* lockInitiativeFleet(db, organisationId, project.initiativeId);
  });
}

/**
 * Reads the fleet of a fleet agent under a share lock, so a concurrent re-registration is
 * honoured in the same way as a fleet move. Returns null for humans and agents without a fleet,
 * reading nothing for humans.
 */
function lockAgentFleet(db: Db, member: Actor) {
  return Effect.gen(function* () {
    if (!member.agentRank) return null;
    const [agent] = yield* db
      .select({ agentRank: users.agentRank, fleetId: users.agentFleetId })
      .from(users)
      .where(and(eq(users.organisationId, member.organisationId), eq(users.id, member.id)))
      .for("share");
    return agent?.agentRank ? agent.fleetId : null;
  });
}

/**
 * Refuses an agent's write with `detail` when any of the projects belongs to a fleet other than
 * the agent's own. Humans and agents without a fleet are unaffected, and so are projects without
 * a fleet. The agent's fleet is read again under a share lock. Ownership paths are locked in
 * project ID order, the order in which a move already locks its two project rows.
 */
function refuseForeignProjects(
  db: Db,
  member: Actor,
  projectIds: readonly string[],
  detail: string,
) {
  return Effect.gen(function* () {
    const fleetId = yield* lockAgentFleet(db, member);
    if (!fleetId) return;
    for (const projectId of [...new Set(projectIds)].sort()) {
      const owner = yield* lockProjectFleet(db, member.organisationId, projectId);
      if (owner !== null && owner !== fleetId) return yield* Effect.fail(problem(403, detail));
    }
  });
}

/**
 * Refuses an agent's dispatch write when the issue's project belongs to a fleet other than the
 * agent's own. Call it inside the write transaction, before the write.
 */
export function requireFleetOwnership(
  db: Db,
  member: Actor,
  projectId: string,
  input: { assigneeId?: string | null; readyForAgents?: boolean },
) {
  return dispatchesIssue(input)
    ? refuseForeignProjects(db, member, [projectId], foreignFleetDetail)
    : Effect.void;
}

/**
 * Refuses an agent's move when the source or destination project belongs to a fleet other than
 * the agent's own. Call it inside the move transaction, before the write.
 */
export function requireMoveOwnership(
  db: Db,
  member: Actor,
  sourceProjectId: string,
  destinationProjectId: string,
) {
  return refuseForeignProjects(
    db,
    member,
    [sourceProjectId, destinationProjectId],
    foreignFleetMoveDetail,
  );
}

/**
 * Refuses an agent's reparenting of an initiative when the initiative's current fleet, or the new
 * parent's fleet, is a fleet other than the agent's own. Each is the initiative's own fleet, or
 * else its nearest ancestor's, so reparenting can neither take another fleet's branch out of that
 * fleet nor place a branch under another fleet. A null `parentId` makes the initiative a root,
 * which belongs to no fleet unless it has its own. Humans and agents without a fleet are
 * unaffected.
 *
 * Call it inside the update transaction after `lockInitiativeWrites`, before the write. Both
 * ownership paths are locked for share in initiative ID order.
 */
export function requireReparentOwnership(
  db: Db,
  member: Actor,
  initiativeId: string,
  parentId: string | null,
) {
  return Effect.gen(function* () {
    const fleetId = yield* lockAgentFleet(db, member);
    if (!fleetId) return;
    const ids = [...new Set([initiativeId, ...(parentId ? [parentId] : [])])].sort();
    for (const id of ids) {
      const owner = yield* lockInitiativeFleet(db, member.organisationId, id);
      if (owner !== null && owner !== fleetId)
        return yield* Effect.fail(problem(403, foreignFleetReparentDetail));
    }
  });
}
