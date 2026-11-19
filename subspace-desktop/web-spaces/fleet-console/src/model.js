// Derived views over the Fleet API v1 data: what each node shows, its tone and
// label, and the counts the toolbar and badge use. Pure.

/** Theme tone for each state a node or task can be in. */
export const TONE = {
  running: "accent",
  dispatching: "accent",
  cancelling: "yellow",
  queued: "yellow",
  idle: "muted",
  uncertain: "orange",
  unprovisioned: "muted",
  completed: "green",
  cancelled: "muted",
  abandoned: "red",
  failed: "red",
  unreachable: "red",
};

export const LABEL = {
  running: "Running",
  dispatching: "Dispatching",
  cancelling: "Cancelling",
  queued: "Queued",
  idle: "Idle",
  uncertain: "Uncertain",
  unprovisioned: "Not provisioned",
  completed: "Completed",
  cancelled: "Cancelled",
  abandoned: "Abandoned",
  failed: "Failed",
  unreachable: "Unavailable",
};

/** The order V1Activity's doc gives for "most active". */
const ACTIVITY_RANK = { running: 0, uncertain: 1, queued: 2, idle: 3, unprovisioned: 4 };

export const toneOf = (state) => TONE[state] ?? "muted";
export const labelOf = (state) => LABEL[state] ?? String(state ?? "");

/** The context that decides what a crew member shows: the most active one, first on ties. */
export function primaryContext(member) {
  const contexts = member?.contexts ?? [];
  let best = null;
  for (const context of contexts) {
    if (!best || (ACTIVITY_RANK[context.activity] ?? 9) < (ACTIVITY_RANK[best.activity] ?? 9)) best = context;
  }
  return best;
}

const taskEnd = (task) => task.finishedAt ?? task.updatedAt ?? task.createdAt ?? 0;

/** How long a finished task took, in milliseconds, or null when unknown. */
export function taskTook(task) {
  const start = task?.startedAt ?? task?.createdAt;
  const end = task?.finishedAt;
  return Number.isFinite(start) && Number.isFinite(end) ? Math.max(0, end - start) : null;
}

const views = new WeakMap();

/**
 * A crew member flattened for drawing and the inspector: state, since, the
 * primary context's task, git and pull request, all queued work and the
 * newest outcomes across contexts. Members are immutable (the reducer
 * replaces them), so the view is memoized per member object.
 */
export function crewView(member) {
  let view = views.get(member);
  if (!view) {
    view = buildCrewView(member);
    views.set(member, view);
  }
  return view;
}

function buildCrewView(member) {
  const primary = primaryContext(member);
  const contexts = member.contexts ?? [];
  const queued = contexts.flatMap((context) => context.queued ?? []);
  const queuedCount = contexts.reduce((sum, context) => sum + (context.queuedCount ?? 0), 0);
  const recent = contexts
    .flatMap((context) => context.recent ?? [])
    .sort((a, b) => taskEnd(b) - taskEnd(a))
    .slice(0, 8);
  return {
    id: member.id,
    name: member.name,
    role: member.role,
    model: member.model,
    backend: member.backend,
    state: member.activity,
    since: member.since ?? member.idleSince ?? null,
    idleSince: member.idleSince,
    task: primary?.current ?? null,
    observed: Boolean(primary?.observed),
    git: primary?.git ?? null,
    branch: primary?.git?.branch ?? null,
    pr: primary?.pullRequest ?? null,
    project: primary?.project ?? null,
    contexts,
    queued,
    queuedCount: Math.max(queuedCount, queued.length),
    recent,
    lastOutcome: primary?.lastOutcome ?? recent[0] ?? null,
    subagents: member.subagents ?? [],
  };
}

/** A ship's overall state: unreachable when unavailable, else its most active crew, else its captain. */
export function shipState(ship, detail) {
  if (!ship || ship.available === false) return "unreachable";
  const members = detail?.crewMembers;
  if (members && members.length) {
    const states = members.map((member) => member.activity);
    for (const state of ["running", "uncertain", "queued"]) if (states.includes(state)) return state;
    return "idle";
  }
  if (ship.crew && ship.crew.busy > 0) return "running";
  if (ship.crew && ship.crew.queued > 0) return "queued";
  return ship.captain?.activity ?? "idle";
}

/** Busy, people and queued counts for a ship, from detail when known, else the fleet summary. */
export function shipCounts(ship, detail) {
  const members = detail?.crewMembers;
  if (members) {
    const views = members.map(crewView);
    return {
      people: views.length,
      busy: views.filter((view) => view.state === "running").length,
      queued: views.reduce((sum, view) => sum + view.queuedCount, 0),
    };
  }
  return { people: ship?.crew?.people ?? 0, busy: ship?.crew?.busy ?? 0, queued: ship?.crew?.queued ?? 0 };
}

/** One line for a captain or rear admiral; the contract has no task text for them. */
export function captainLine(captain, busy = 0) {
  if (!captain) return "No answer";
  const queue = captain.queue ?? {};
  const running = (queue.running ?? 0) + (queue.responding ?? 0);
  if (captain.activity === "running") {
    if (busy > 0) return `Delegating ${busy} task${busy === 1 ? "" : "s"}`;
    return running > 1 ? `Running ${running} jobs` : "Running a job";
  }
  if (captain.activity === "queued") {
    const queued = queue.queued ?? 0;
    return `${queued} job${queued === 1 ? "" : "s"} queued`;
  }
  return "Waiting for work";
}

/** Every crew member across available ships with ship detail, as {ship, member, view}. */
export function allCrew(fleet, ships) {
  if (!fleet) return [];
  return fleet.ships.flatMap((ship) => {
    const detail = ships[ship.id];
    if (!ship.available || !detail) return [];
    return detail.crewMembers.map((member) => ({ ship, member, view: crewView(member) }));
  });
}

/** Toolbar numbers. */
export function fleetStats(fleet, ships) {
  if (!fleet) return null;
  const crew = allCrew(fleet, ships);
  const counts = fleet.ships.map((ship) => shipCounts(ship, ships[ship.id]));
  return {
    shipsOnline: fleet.ships.filter((ship) => ship.available).length,
    shipsTotal: fleet.ships.length,
    busy: counts.reduce((sum, count) => sum + count.busy, 0),
    people: counts.reduce((sum, count) => sum + count.people, 0),
    queued: counts.reduce((sum, count) => sum + count.queued, 0),
    longestIdle: fleet.longestIdle,
    uncertain: crew.filter((entry) => entry.view.state === "uncertain").length,
  };
}

/** The tab badge: uncertain crew plus unavailable ships, or null when there are none. */
export function badgeText(fleet, ships) {
  if (!fleet) return null;
  const uncertain = allCrew(fleet, ships).filter((entry) => entry.view.state === "uncertain").length;
  const unavailable = fleet.ships.filter((ship) => !ship.available).length;
  const total = uncertain + unavailable;
  return total > 0 ? String(Math.min(total, 9999)) : null;
}

/** Idle crew, longest first. */
export function idleWatch(fleet, ships, limit = 5) {
  return allCrew(fleet, ships)
    .filter((entry) => entry.view.state === "idle")
    .sort((a, b) => (a.view.idleSince ?? a.view.since ?? 0) - (b.view.idleSince ?? b.view.since ?? 0))
    .slice(0, limit);
}

/** Feature flags, with whether each differs from Stargate's default. */
export function featureRows(features) {
  return (features ?? []).map((feature) => ({
    ...feature,
    changed: feature.enabled !== feature.defaultEnabled,
  }));
}

/** Text a crew member's issue references may hide in, labelled by kind for extraction. */
export function issueSources(member, { includeQueued = true } = {}) {
  const sources = [];
  for (const context of member.contexts ?? []) {
    const current = context.current;
    if (current) sources.push({ kind: "text", text: current.summary }, { kind: "text", text: current.phase }, { kind: "text", text: current.progress });
    if (includeQueued) for (const task of context.queued ?? []) sources.push({ kind: "text", text: task.summary });
    if (context.git?.branch) sources.push({ kind: "branch", text: context.git.branch });
    if (context.pullRequest) sources.push({ kind: "text", text: context.pullRequest.title });
  }
  return sources;
}

/** A selection's crew member and ship, or nulls. */
export function lookup(fleet, ships, selection) {
  const ship = selection?.ship ? fleet?.ships.find((entry) => entry.id === selection.ship) ?? null : null;
  const detail = ship ? ships[ship.id] ?? null : null;
  const member = selection?.crew && detail ? detail.crewMembers.find((entry) => entry.id === selection.crew) ?? null : null;
  return { ship, detail, member };
}
