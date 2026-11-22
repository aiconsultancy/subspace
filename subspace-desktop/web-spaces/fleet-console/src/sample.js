// Sample fleet: invented data in the exact Fleet API v1 shapes, and a small
// simulation that keeps it changing by emitting the same events the live
// stream sends. Every name, repository and host here is fictional.

const M = 60_000;
const H = 60 * M;
const GiB = 1024 ** 3;

export const SAMPLE_SCOPE = Object.freeze({ profileName: "Sample", appOrigin: "", teamKeys: ["FLT", "OPS"], linearWorkspace: "example" });

/** Sample answers for the issue tiles, so sample mode shows them without a board. */
export const SAMPLE_ISSUES = {
  "FLT-212": ["Tier warp between fleet, ship and crew", "In Progress", "started", "Starbuck"],
  "FLT-198": ["Fleet API v1 routes behind a private listener", "In Review", "started", "Kendra"],
  "FLT-205": ["Desktop end-to-end tests against a local Stargate", "In Progress", "started", "Helo"],
  "OPS-41": ["Write the Fleet API v1 contract", "Todo", "unstarted", "Gaeta"],
  "FLT-219": ["Record idle-since for crew", "In Progress", "started", "Fisk"],
  "FLT-230": ["Initiatives schema and migrations", "Backlog", "backlog", null],
  "FLT-221": ["Release notes for framework 0.2.1", "Triage", "triage", null],
  "FLT-233": ["Seed data for the local stack", "Todo", "unstarted", "Juno"],
  "FLT-188": ["Theme tones for the fleet board", "Done", "completed", "Starbuck"],
  "FLT-240": ["Fleets landscape tier", "In Progress", "started", "Ada"],
};

export function sampleResolve(identifiers) {
  return identifiers.map((identifier) => {
    const entry = SAMPLE_ISSUES[identifier];
    if (!entry) return { identifier, state: "unavailable" };
    const [title, name, kind, assignee] = entry;
    return {
      identifier,
      state: "available",
      summary: { identifier, title, url: `https://linear.app/example/issue/${identifier}`, state: { name, kind }, assignee, archived: false },
    };
  });
}

let taskSerial = 1000;
const taskId = () => `task-${(taskSerial += 1)}`;

function task(status, summary, { phase = "", progress = "", created, started = null, finished = null } = {}) {
  return {
    id: taskId(),
    status,
    kind: "assignment",
    summary,
    phase,
    progress,
    createdAt: created ?? started ?? finished ?? null,
    startedAt: started,
    updatedAt: finished ?? started ?? created ?? null,
    finishedAt: finished,
  };
}

function done(summary, minutesAgo, tookMinutes, now, status = "completed") {
  const finished = now - minutesAgo * M;
  return task(status, summary, { phase: status === "completed" ? "Done" : "No confirmed outcome", created: finished - tookMinutes * M - 2 * M, started: finished - tookMinutes * M, finished });
}

function gitState(branch, head, ahead, behind, changed, repository, now) {
  return { branch, head, base: "origin/main", ahead, behind, changedFiles: changed, repository, checkedAt: now };
}

function contextActivity(context) {
  const current = context.current;
  if (current && ["running", "dispatching", "cancelling"].includes(current.status)) return "running";
  if (current?.status === "uncertain" || (!current && context.lastOutcome?.status === "uncertain")) return "uncertain";
  if (!current && context.queuedCount > 0) return "queued";
  return context.provisioned === "ready" ? "idle" : "unprovisioned";
}

const RANK = { running: 0, uncertain: 1, queued: 2, idle: 3, unprovisioned: 4 };

/** Recompute a member's derived fields from its contexts, as the projection does. */
export function deriveMember(member) {
  for (const context of member.contexts) {
    context.activity = contextActivity(context);
    context.queuedCount = Math.max(context.queuedCount, context.queued.length);
    context.recentCount = Math.max(context.recentCount, context.recent.length);
  }
  const primary = [...member.contexts].sort((a, b) => RANK[a.activity] - RANK[b.activity])[0];
  member.activity = primary?.activity ?? "unprovisioned";
  if (member.activity === "running") member.since = primary.current.startedAt ?? primary.current.createdAt;
  else if (member.activity === "uncertain") member.since = primary.current?.updatedAt ?? primary.lastOutcome?.finishedAt ?? member.since;
  else if (member.activity === "queued") {
    const times = primary.queued.map((entry) => entry.createdAt).filter(Number.isFinite);
    if (times.length) member.since = Math.min(...times);
  }
  else if (member.activity === "idle") member.since = primary.lastOutcome?.finishedAt ?? member.since;
  member.idleSince = member.activity === "idle" ? member.since : null;
  return member;
}

function context(project, { current = null, queued = [], recent = [], git = null, pr = null, observed = false, provisioned = "ready" } = {}) {
  return {
    project,
    activity: "idle",
    provisioned,
    observed,
    current,
    queuedCount: queued.length,
    queued,
    lastOutcome: recent[0] ?? null,
    recent,
    recentCount: recent.length,
    git,
    pullRequest: pr,
  };
}

function member(id, name, role, contexts, { backend = "opencode", model = "gpt-6.2-sol", subagents = [] } = {}) {
  return deriveMember({ id, name, role, model, backend, activity: "idle", since: null, idleSince: null, contexts, subagents });
}

function idleMember(id, name, role, minutes, project, last, repo, now, backend) {
  return member(id, name, role, [context(project, { recent: [done(last, minutes, 14, now)], git: gitState("main", "a2fa002", 0, 0, 0, repo, now) })], { backend });
}

const queue = (queued, running, done, failed = 0) => ({ queued, running, responding: 0, done, failed });

function captain(id, name, transport, backend, activity, since, q) {
  return { id, name, role: "captain", transport, backend, model: "claude-opus-5-5", queue: q, activity, since };
}

const feature = (key, label, enabled, defaultEnabled, value) => ({ key, label, enabled, defaultEnabled, ...(value ? { value } : {}) });

function shipFeatures(transport, peer) {
  const list = [feature("crewActivity", "Crew activity", true, true, "summary"), feature("peerCaptains", "Peer captains", peer, false)];
  if (transport === "irc") {
    list.push(
      feature("irc.typing", "Typing indicator", true, true),
      feature("irc.steerThreadFollowUps", "Steer thread follow-ups", true, false),
      feature("irc.questions", "Questions", false, false),
      feature("irc.reactions", "Reactions", true, true),
    );
  } else {
    list.push(feature(`${transport}.threads`, "Reply in threads", true, true));
  }
  return list;
}

function ship(id, name, cap, repository, crewMembers, { available = true, error, peer = false } = {}) {
  return {
    id,
    name,
    captain: cap,
    repository,
    available,
    ...(error ? { error } : {}),
    crew: available ? { people: 0, busy: 0, queued: 0 } : null,
    features: shipFeatures(cap.transport, peer),
    crewMembers,
  };
}

/** Recompute a ship's crew summary and captain activity from its members. */
export function deriveShip(detail, now) {
  if (!detail.available) return detail;
  const members = detail.crewMembers;
  const busy = members.filter((entry) => entry.activity === "running").length;
  const queued = members.reduce((sum, entry) => sum + entry.contexts.reduce((n, c) => n + c.queuedCount, 0), 0);
  detail.crew = { people: members.length, busy, queued };
  const activity = busy > 0 ? "running" : queued > 0 ? "queued" : "idle";
  if (detail.captain.activity !== activity) {
    detail.captain.activity = activity;
    detail.captain.since = now;
  }
  detail.captain.queue = { ...detail.captain.queue, running: busy, queued };
  return detail;
}

function buildShips(now) {
  const ago = (minutes) => now - minutes * M;
  const galacticaRepo = "example-org/subspace";
  const pegasusRepo = "example-org/stargate";
  const defiantRepo = "example-org/subspace-irc";
  const endeavourRepo = "example-org/subspace-api";
  const running = (summary, phase, progress, minutes) => task("running", summary, { phase, progress, created: ago(minutes + 1), started: ago(minutes) });
  const waiting = (summary, minutes) => task("queued", summary, { phase: "Waiting for a worker slot", created: ago(minutes) });

  const galactica = ship("galactica", "Galactica", captain("galactica", "Adama", "irc", "opencode", "running", ago(40), queue(3, 2, 41)), galacticaRepo, [
    member("starbuck", "Starbuck", "Implementation and focused tests", [
      context("subspace", {
        observed: true,
        current: running("Fleet bridge canvas: tier warp and hit testing", "Implementing", "Orbit layout and hit testing pass. Wiring the warp between the ship and crew tiers for FLT-212.", 38),
        queued: [waiting("Snapshot tests for tier layout", 30)],
        recent: [done("Theme tones for the fleet board (FLT-188)", 190, 52, now), done("Stargate client crate scaffold", 320, 34, now), done("Space Lab capacity test retry", 470, 12, now, "uncertain"), done("Inspector keyboard focus", 860, 21, now)],
        git: gitState("feat/flt-212-tier-warp", "a3f91c2", 6, 0, 3, galacticaRepo, now),
        pr: { number: 81, url: "https://github.com/example-org/subspace/pull/81", title: "Fleet Console: tier warp (FLT-212)", state: "draft", checks: "running" },
      }),
      context("stargate", { queued: [waiting("Expose crew events over SSE", 22)], git: gitState("main", "1d0c3aa", 0, 0, 0, pegasusRepo, now) }),
    ], { subagents: [{ id: "s1", title: "explore · path builder", updatedAt: ago(3) }, { id: "s2", title: "test-runner · fleet-board", updatedAt: ago(1) }] }),
    member("gaeta", "Gaeta", "Documentation and research", [
      context("stargate", {
        observed: true,
        current: running("Draft the Stargate fleet API contract", "Researching", "Read the dashboard model and crew status. Drafting additive v1 fields for git state and idle time.", 17),
        recent: [done("Crew activity docs review", 240, 18, now)],
        git: gitState("docs/ops-41-fleet-api-v1", "5be20d7", 2, 1, 1, pegasusRepo, now),
      }),
    ], { subagents: [{ id: "s3", title: "explore · dashboard routes", updatedAt: ago(2) }] }),
    member("bosun", "Bosun", "Ship maintenance and inspections", [
      context("subspace", { recent: [done("Nightly crew inspection", 134, 26, now), done("Prune stale worktrees", 600, 4, now)], git: gitState("main", "42b07e3", 0, 0, 0, galacticaRepo, now) }),
    ]),
    member("apollo", "Apollo", "Release and CI", [context("subspace", { queued: [waiting("Release notes for framework 0.2.1 (FLT-221)", 6)], recent: [done("Release framework 0.2.0", 1300, 31, now)] })]),
    member("helo", "Helo", "Integration tests", [
      context("subspace", {
        current: task("uncertain", "End-to-end: desktop against a local Stargate", { phase: "No confirmed outcome", progress: "The worker stopped reporting during the run. Stored state does not prove the run is live.", created: ago(42), started: ago(41) }),
        recent: [done("IRC tag parsing regression test", 380, 15, now)],
        git: gitState("test/flt-205-desktop-e2e", "9d1e44a", 3, 2, 0, galacticaRepo, now),
        pr: { number: 79, url: "https://github.com/example-org/subspace/pull/79", title: "Desktop end-to-end suite", state: "open", checks: "failing" },
      }),
    ]),
  ], { peer: true });

  const pegasus = ship("pegasus", "Pegasus", captain("pegasus", "Garner", "slack", "opencode", "running", ago(25), queue(1, 2, 28)), pegasusRepo, [
    member("kendra", "Kendra", "Gateway routes", [
      context("stargate", {
        observed: true,
        current: running("Additive v1 routes behind a private listener", "Running tests", "bun test: 412 passed, 3 running. Route table and no-store headers covered.", 24),
        queued: [waiting("SSE heartbeat and resume", 20)],
        recent: [done("Schema for the fleet view", 150, 44, now)],
        git: gitState("feat/api-v1-routes", "c71a0be", 4, 0, 0, pegasusRepo, now),
        pr: { number: 66, url: "https://github.com/example-org/stargate/pull/66", title: "Fleet API v1 routes (FLT-198)", state: "open", checks: "passing" },
      }),
    ], { subagents: [{ id: "s4", title: "test-runner · bun", updatedAt: ago(1) }] }),
    idleMember("hoshi", "Hoshi", "Docs and examples", 12, "stargate", "Dashboard proxy example for Caddy", pegasusRepo, now),
    member("fisk", "Fisk", "Crew journal", [
      context("stargate", {
        observed: true,
        current: running("Record idle-since in the crew journal", "Implementing", "Adding lastActivityAt to task transitions. A migration for existing rows comes next.", 9),
        recent: [done("Journal compaction", 500, 8, now, "abandoned")],
        git: gitState("feat/flt-219-crew-idle-since", "e0b9f12", 1, 0, 5, pegasusRepo, now),
      }),
    ]),
    idleMember("mara", "Mara", "Security review", 192, "stargate", "Review dashboard socket permissions", pegasusRepo, now),
  ], { peer: true });

  const defiant = ship("defiant", "Defiant", captain("defiant", "Okafor", "irc", "pi-durable", "idle", ago(47), queue(0, 0, 17)), defiantRepo, [
    idleMember("ezri", "Ezri", "Ergo configuration", 47, "subspace-irc", "Channel modes for fleet rooms", defiantRepo, now, "pi-durable"),
    idleMember("nog", "Nog", "Container builds", 65, "subspace-irc", "Rebuild the Ergo image", defiantRepo, now, "pi-durable"),
    idleMember("rom", "Rom", "Ops runbooks", 47, "subspace-irc", "Runbook: rotating SASL accounts", defiantRepo, now, "pi-durable"),
  ]);

  const endeavour = ship("endeavour", "Endeavour", captain("endeavour", "Reyes", "teams", "opencode", "running", ago(52), queue(2, 1, 9)), endeavourRepo, [
    member("tali", "Tali", "API and database", [
      context("subspace-api", {
        observed: false,
        current: running("Migrations for initiatives", "Reviewing diff", "Migration and rollback written. Checking the fleet assignment foreign keys. Tracked in https://linear.app/example/issue/FLT-230/initiatives-schema", 51),
        recent: [done("Session middleware", 420, 63, now)],
        git: gitState("feat/initiatives-schema", "7c3d915", 5, 3, 2, endeavourRepo, now),
      }),
    ]),
    member("juno", "Juno", "Local stack", [context("subspace-api", { queued: [waiting("Seed data for local Compose (FLT-233)", 14)], recent: [done("Compose file for Postgres and IRC", 300, 22, now)] })]),
    idleMember("pell", "Pell", "Frontend checks", 26, "subspace", "Board snapshot baseline update", galacticaRepo, now),
    member("vos", "Vos", "Auth", [context("subspace-api", { queued: [waiting("Development sign-in fallback", 3)] })]),
  ]);

  const resolute = ship("resolute", "Resolute", captain("resolute", "Vance", "irc", "opencode", "idle", ago(4), queue(0, 0, 0)), galacticaRepo, [], {
    available: false,
    error: "The gateway socket stopped answering.",
  });

  return [galactica, pegasus, defiant, endeavour, resolute].map((entry) => deriveShip(entry, now));
}

/** A second, smaller sample fleet for the Fleets tier. */
function buildFrontierShips(now) {
  const ago = (minutes) => now - minutes * M;
  const consoleRepo = "example-org/console";
  const infraRepo = "example-org/infra";
  const docsRepo = "example-org/docs";
  const running = (summary, phase, progress, minutes) => task("running", summary, { phase, progress, created: ago(minutes + 1), started: ago(minutes) });
  const kestrel = ship("kestrel", "Kestrel", captain("kestrel", "Iris", "slack", "opencode", "running", ago(18), queue(1, 1, 22)), consoleRepo, [
    member("ada", "Ada", "Frontend", [
      context("console", {
        observed: true,
        current: running("Fleets landscape tier (FLT-240)", "Implementing", "Layout and hit testing for fleet nodes; the warp into a fleet comes next.", 16),
        recent: [done("Fleet switcher keyboard map", 140, 33, now)],
        git: gitState("feat/flt-240-landscape", "4be1c09", 3, 0, 4, consoleRepo, now),
      }),
    ]),
    idleMember("brook", "Brook", "Backend", 37, "console", "Paginate the activity feed", consoleRepo, now),
    member("cass", "Cass", "Tests", [context("console", { queued: [task("queued", "End-to-end run with two fleets", { phase: "Waiting for a worker slot", created: ago(9) })] })]),
  ]);
  const osprey = ship("osprey", "Osprey", captain("osprey", "Rhea", "irc", "opencode", "running", ago(33), queue(0, 1, 15)), infraRepo, [
    member("dex", "Dex", "Infrastructure", [
      context("infra", {
        observed: true,
        current: running("Rotate the fleet VM certificates", "Running tests", "Staging certificates issued; checking the reload on each VM.", 28),
        recent: [done("Private listener for the Fleet API", 300, 41, now)],
        git: gitState("chore/certs", "91aa2de", 2, 0, 1, infraRepo, now),
      }),
    ]),
    member("eve", "Eve", "Docs", [
      context("infra", {
        current: task("uncertain", "Document the token rotation", { phase: "No confirmed outcome", progress: "The worker stopped reporting before the outcome.", created: ago(25), started: ago(24) }),
        git: gitState("docs/token-rotation", "c0ffee1", 1, 0, 2, infraRepo, now),
      }),
    ]),
    idleMember("finn", "Finn", "Operations", 74, "infra", "Disk usage alert thresholds", infraRepo, now),
  ]);
  const heron = ship("heron", "Heron", captain("heron", "Soren", "teams", "pi-durable", "idle", ago(51), queue(0, 0, 8)), docsRepo, [
    idleMember("gale", "Gale", "Design", 51, "docs", "Icon set for fleet states", docsRepo, now, "pi-durable"),
    idleMember("hal", "Hal", "Editing", 95, "docs", "Glossary for fleets and ships", docsRepo, now, "pi-durable"),
  ]);
  return [kestrel, osprey, heron].map((entry) => deriveShip(entry, now));
}

/** The sample fleets: two simulated, one that never answers. */
export const SAMPLE_FLEETS = Object.freeze([
  Object.freeze({ id: "sample-colonial", variant: "colonial", endpoint: "https://fleet-vm-01.example.com" }),
  Object.freeze({ id: "sample-frontier", variant: "frontier", endpoint: "https://fleet-vm-02.example.com" }),
  Object.freeze({ id: "sample-outer-rim", variant: null, name: "outer-rim-fleet", endpoint: "https://fleet-vm-03.example.com" }),
]);

const VARIANTS = {
  colonial: {
    name: "colonial-fleet",
    host: { hostname: "fleet-vm-01", cores: 8, usage: 0.37, load: [1.92, 1.71, 1.55], totalGiB: 32, availableGiB: 13.4 },
    gateway: { version: "0.18.2", opencodeVersion: "1.4.7" },
    admiral: { id: "kirrin", name: "Kirrin" },
    ships: (now) => buildShips(now),
  },
  frontier: {
    name: "frontier-fleet",
    host: { hostname: "fleet-vm-02", cores: 4, usage: 0.62, load: [2.6, 2.31, 2.02], totalGiB: 16, availableGiB: 4.1 },
    gateway: { version: "0.18.1", opencodeVersion: "1.4.6" },
    admiral: { id: "vela", name: "Vela" },
    ships: (now) => buildFrontierShips(now),
  },
};

function gatewayFeatures() {
  return [
    feature("dashboard", "Dashboard socket", true, false),
    feature("api", "Fleet API", true, false),
    feature("cron", "Scheduled jobs", true, true),
    feature("maintenance", "Maintenance drain", true, true),
    feature("quotaWatch", "Quota watch", false, true),
    feature("transcription", "Voice transcription", false, false),
    feature("updateNotices", "Update notices", true, true),
    feature("nightly", "Nightly inspection", true, false, "03:30"),
  ];
}

function host(now, spec = VARIANTS.colonial.host) {
  return {
    hostname: spec.hostname,
    platform: "linux",
    arch: "x64",
    uptimeSec: 9 * 86400 + 4 * 3600 + 12 * 60,
    cpu: { cores: spec.cores, usage: spec.usage, load: [...spec.load] },
    memory: { totalBytes: spec.totalGiB * GiB, availableBytes: Math.round(spec.availableGiB * GiB) },
    disks: [
      { path: "/", totalBytes: 200 * GiB, freeBytes: 88 * GiB },
      { path: "/var/lib/stargate", totalBytes: 500 * GiB, freeBytes: 341 * GiB },
    ],
    sampledAt: now,
  };
}

const summaryOf = (detail) => {
  const { crewMembers: _members, ...rest } = detail;
  return rest;
};

function longestIdle(ships) {
  let best = null;
  for (const detail of ships) {
    for (const entry of detail.crewMembers) {
      if (entry.activity === "idle" && entry.idleSince && (!best || entry.idleSince < best.since)) best = { ship: detail.id, member: entry.id, since: entry.idleSince };
    }
  }
  return best;
}

function fleetOf(model, now) {
  return {
    apiVersion: 1,
    generatedAt: now,
    name: model.name,
    host: model.host,
    gateway: model.gateway,
    admiral: model.admiral,
    ships: model.ships.map(summaryOf),
    longestIdle: longestIdle(model.ships),
  };
}

/** A complete sample: a V1Fleet and a V1ShipDetail per ship. */
export function buildSampleFleet(now, variant = "colonial") {
  const model = createModel(now, variant);
  return { fleet: fleetOf(model, now), ships: Object.fromEntries(model.ships.map((detail) => [detail.id, structuredClone(detail)])) };
}

function createModel(now, variant = "colonial") {
  const spec = VARIANTS[variant] ?? VARIANTS.colonial;
  return {
    name: spec.name,
    host: host(now, spec.host),
    gateway: { version: spec.gateway.version, opencodeVersion: spec.gateway.opencodeVersion, bunVersion: "1.3.4", startedAt: now - (2 * 24 + 6) * H, features: gatewayFeatures() },
    admiral: { id: spec.admiral.id, name: spec.admiral.name, role: "rear_admiral", transport: "irc", backend: "claude", model: "claude-opus-5-5", queue: queue(0, 1, 12), activity: "running", since: now - 11 * M },
    ships: spec.ships(now),
  };
}

const POOL = [
  ["Hit-test cache for orbit nodes", "perf/orbit-hit-cache"],
  ["Idle watch ordering in the inspector", "feat/idle-watch"],
  ["Stream resume after reconnect", "feat/sse-resume"],
  ["Theme tones for warp streaks", "feat/warp-tones"],
  ["Fix the flaky capacity test", "fix/capacity-race"],
  ["Keyboard map for the fleet console", "docs/fleet-keys"],
];
const PHASES = ["Implementing", "Running tests", "Reviewing diff", "Opening PR"];
const PROGRESS = {
  Implementing: "Writing the change and its focused tests.",
  "Running tests": "Focused tests running. Lint is clean.",
  "Reviewing diff": "Reviewing the diff before opening a pull request.",
  "Opening PR": "Pushing the branch and opening a draft pull request.",
};
const slug = (text) => text.toLowerCase().replace(/\(.*?\)/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 28);

/**
 * The simulation. `initial()` returns the events a live stream sends on
 * connect; `tick()` makes one change and returns its events; `hostTick()`
 * returns a host sample.
 */
export function createSampleSim({ now = Date.now, random = Math.random, variant = "colonial" } = {}) {
  const model = createModel(now(), variant);
  let prNumber = 83;
  const pick = (list) => list[Math.floor(random() * list.length)];
  const clone = (value) => structuredClone(value);
  const activity = (ship, who, kind, text, extra = {}) => ({ type: "activity", data: { ship: ship.id, member: who?.id ?? null, kind, text: text.slice(0, 200), at: now(), ...extra } });
  const shipEvents = (ship) => {
    deriveShip(ship, now());
    return [{ type: "ship", data: clone(ship) }, { type: "fleet", data: clone(fleetOf(model, now())) }];
  };
  const crewEntries = () => model.ships.filter((ship) => ship.available).flatMap((ship) => ship.crewMembers.map((who) => ({ ship, who })));

  function start(ship, who) {
    const t = now();
    const context = who.contexts.find((entry) => entry.queued.length) ?? who.contexts[0];
    const next = context.queued.shift();
    const [summary, branchName] = next ? [next.summary, `feat/${slug(next.summary)}`] : pick(POOL);
    context.current = task("running", summary, { phase: "Implementing", progress: PROGRESS.Implementing, created: next?.createdAt ?? t, started: t });
    context.queuedCount = context.queued.length;
    context.observed = true;
    context.git = gitState(branchName, random().toString(16).slice(2, 9), 0, 0, 1, ship.repository, t);
    context.pullRequest = null;
    deriveMember(who);
    return [activity(ship, who, "dispatch", `${who.name} started “${summary}”`, { branch: branchName }), ...shipEvents(ship)];
  }

  function complete(ship, who) {
    const t = now();
    const context = who.contexts.find((entry) => entry.current) ?? who.contexts[0];
    const finished = { ...context.current, status: "completed", phase: "Done", updatedAt: t, finishedAt: t };
    context.current = null;
    context.lastOutcome = finished;
    context.recent = [finished, ...context.recent].slice(0, 8);
    context.recentCount += 1;
    deriveMember(who);
    return [activity(ship, who, "outcome", `${who.name} completed “${finished.summary}”`, { status: "completed" }), ...shipEvents(ship)];
  }

  function advance(ship, who) {
    const context = who.contexts.find((entry) => entry.current?.status === "running");
    const index = PHASES.indexOf(context.current.phase);
    if (index === PHASES.length - 1) return complete(ship, who);
    const phase = PHASES[index + 1] ?? "Running tests";
    context.current = { ...context.current, phase, progress: PROGRESS[phase], updatedAt: now() };
    if (context.git) {
      context.git = { ...context.git, ahead: (context.git.ahead ?? 0) + 1, changedFiles: phase === "Opening PR" ? 0 : (context.git.changedFiles ?? 0) + 1, checkedAt: now() };
    }
    if (phase === "Opening PR" && !context.pullRequest && context.git?.branch) {
      prNumber += 1;
      context.pullRequest = { number: prNumber, url: `https://github.com/${ship.repository}/pull/${prNumber}`, title: context.current.summary, state: "draft", checks: "running" };
      return [activity(ship, who, "git", `${who.name} opened draft pull request #${prNumber}`, { branch: context.git.branch }), ...shipEvents(ship)];
    }
    return [activity(ship, who, "phase", `${who.name} · ${phase}`, context.git?.branch ? { branch: context.git.branch } : {}), ...shipEvents(ship)];
  }

  return {
    model,
    initial() {
      return [{ type: "fleet", data: clone(fleetOf(model, now())) }, ...model.ships.map((ship) => ({ type: "ship", data: clone(ship) }))];
    },
    tick() {
      const entries = crewEntries();
      const running = entries.filter(({ who }) => who.activity === "running" && who.contexts.some((c) => c.current?.status === "running"));
      const free = entries.filter(({ who }) => who.activity === "idle" || who.activity === "queued");
      const roll = random();
      if (roll < 0.45 && running.length) {
        const { ship, who } = pick(running);
        return advance(ship, who);
      }
      if (roll < 0.7 && running.length > 3) {
        const { ship, who } = pick(running);
        return complete(ship, who);
      }
      if (free.length) {
        const { ship, who } = pick(free);
        return start(ship, who);
      }
      return [];
    },
    hostTick() {
      const previous = model.host.cpu.usage ?? 0.3;
      const usage = Math.max(0.05, Math.min(0.95, previous + (random() - 0.5) * 0.12));
      const load = model.host.cpu.load.map((value) => Math.max(0.1, +(value + (random() - 0.5) * 0.3).toFixed(2)));
      const available = Math.max(4 * GiB, Math.min(20 * GiB, model.host.memory.availableBytes + (random() - 0.5) * 0.6 * GiB));
      model.host = { ...model.host, uptimeSec: model.host.uptimeSec + 10, cpu: { ...model.host.cpu, usage, load }, memory: { ...model.host.memory, availableBytes: Math.round(available) }, sampledAt: now() };
      return [{ type: "host", data: clone(model.host) }];
    },
    heartbeat() {
      return [{ type: "heartbeat", data: { at: now() } }];
    },
  };
}
