// Shape checks for Stargate Fleet API v1 payloads (src/api/v1/types.ts in
// Stargate). The reducer drops events that fail them, so a server bug shows as
// missing data rather than a broken page. Optional fields are allowed to be
// absent; unknown extra fields are ignored, as the contract is additive.

const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const isNum = (v) => typeof v === "number" && Number.isFinite(v);
const isNumOrNull = (v) => v === null || isNum(v);
const isStr = (v) => typeof v === "string";
const isStrOrNull = (v) => v === null || isStr(v);
const isBool = (v) => typeof v === "boolean";
const isArrOf = (v, check) => Array.isArray(v) && v.every(check);
const isOneOf = (values) => (v) => values.includes(v);

export const TASK_STATUSES = ["queued", "cancelled", "dispatching", "running", "completed", "cancelling", "uncertain", "abandoned"];
export const ACTIVITIES = ["running", "queued", "idle", "uncertain", "unprovisioned"];
export const TRANSPORTS = ["slack", "irc", "teams"];
export const BACKENDS = ["opencode", "pi-durable", "claude", "codex", "module"];
export const ACTIVITY_KINDS = ["dispatch", "phase", "outcome", "queued", "git", "availability"];

export function isTask(t) {
  return (
    isObj(t) &&
    isStr(t.id) &&
    isOneOf(TASK_STATUSES)(t.status) &&
    isOneOf(["assignment", "report", "unknown"])(t.kind) &&
    isStr(t.summary) &&
    isStr(t.phase) &&
    isStr(t.progress) &&
    isNumOrNull(t.createdAt) &&
    isNumOrNull(t.startedAt) &&
    isNumOrNull(t.updatedAt) &&
    isNumOrNull(t.finishedAt)
  );
}

export function isGit(g) {
  return (
    isObj(g) &&
    isStrOrNull(g.branch) &&
    isStrOrNull(g.head) &&
    isStrOrNull(g.base) &&
    isNumOrNull(g.ahead) &&
    isNumOrNull(g.behind) &&
    isNumOrNull(g.changedFiles) &&
    isStrOrNull(g.repository) &&
    isNum(g.checkedAt) &&
    (g.error === undefined || isStr(g.error))
  );
}

export function isPullRequest(p) {
  return (
    isObj(p) &&
    isNum(p.number) &&
    isStr(p.url) &&
    isStr(p.title) &&
    isOneOf(["draft", "open", "merged", "closed"])(p.state) &&
    isOneOf(["passing", "running", "failing", "none"])(p.checks)
  );
}

export function isCrewContext(c) {
  return (
    isObj(c) &&
    isStrOrNull(c.project) &&
    isOneOf(ACTIVITIES)(c.activity) &&
    isOneOf(["unprovisioned", "pending", "ready"])(c.provisioned) &&
    isBool(c.observed) &&
    (c.current === null || isTask(c.current)) &&
    isNum(c.queuedCount) &&
    isArrOf(c.queued, isTask) &&
    (c.lastOutcome === null || isTask(c.lastOutcome)) &&
    isArrOf(c.recent, isTask) &&
    isNum(c.recentCount) &&
    (c.git === null || isGit(c.git)) &&
    (c.pullRequest === null || isPullRequest(c.pullRequest))
  );
}

export function isSubagent(s) {
  return isObj(s) && isStr(s.id) && isStr(s.title) && isNumOrNull(s.updatedAt);
}

export function isCrewMember(m) {
  return (
    isObj(m) &&
    isStr(m.id) &&
    isStr(m.name) &&
    isStrOrNull(m.role) &&
    isStrOrNull(m.model) &&
    isOneOf(["opencode", "pi-durable"])(m.backend) &&
    isOneOf(ACTIVITIES)(m.activity) &&
    isNumOrNull(m.since) &&
    isNumOrNull(m.idleSince) &&
    isArrOf(m.contexts, isCrewContext) &&
    isArrOf(m.subagents, isSubagent)
  );
}

export function isFeature(f) {
  return (
    isObj(f) &&
    isStr(f.key) &&
    isStr(f.label) &&
    isBool(f.enabled) &&
    isBool(f.defaultEnabled) &&
    (f.value === undefined || isStr(f.value))
  );
}

export function isQueueCounts(q) {
  return isObj(q) && Object.values(q).every(isNum);
}

export function isCaptain(c) {
  return (
    isObj(c) &&
    isStr(c.id) &&
    isStr(c.name) &&
    isOneOf(["captain", "rear_admiral"])(c.role) &&
    isOneOf(TRANSPORTS)(c.transport) &&
    isOneOf(BACKENDS)(c.backend) &&
    isStrOrNull(c.model) &&
    isQueueCounts(c.queue) &&
    isOneOf(["running", "queued", "idle"])(c.activity) &&
    isNumOrNull(c.since)
  );
}

function isCrewSummary(c) {
  return (
    c === null ||
    (isObj(c) && isNum(c.people) && isNum(c.busy) && isNum(c.queued) && (c.partial === undefined || c.partial === true))
  );
}

export function isShip(s) {
  return (
    isObj(s) &&
    isStr(s.id) &&
    isStr(s.name) &&
    isCaptain(s.captain) &&
    isStrOrNull(s.repository) &&
    isBool(s.available) &&
    (s.error === undefined || isStr(s.error)) &&
    isCrewSummary(s.crew) &&
    isArrOf(s.features, isFeature)
  );
}

export function isShipDetail(s) {
  return isShip(s) && isArrOf(s.crewMembers, isCrewMember);
}

export function isCrewDetail(d) {
  return isObj(d) && isStr(d.ship) && isCrewMember(d.member);
}

export function isHost(h) {
  return (
    isObj(h) &&
    isStr(h.hostname) &&
    isStr(h.platform) &&
    isStr(h.arch) &&
    isNum(h.uptimeSec) &&
    isObj(h.cpu) &&
    isNum(h.cpu.cores) &&
    isNumOrNull(h.cpu.usage) &&
    isArrOf(h.cpu.load, isNum) &&
    h.cpu.load.length === 3 &&
    isObj(h.memory) &&
    isNum(h.memory.totalBytes) &&
    isNum(h.memory.availableBytes) &&
    isArrOf(h.disks, (d) => isObj(d) && isStr(d.path) && isNum(d.totalBytes) && isNum(d.freeBytes)) &&
    isNum(h.sampledAt)
  );
}

export function isGateway(g) {
  return (
    isObj(g) &&
    isStr(g.version) &&
    isStrOrNull(g.opencodeVersion) &&
    isStr(g.bunVersion) &&
    isNum(g.startedAt) &&
    isArrOf(g.features, isFeature)
  );
}

export function isFleet(f) {
  return (
    isObj(f) &&
    f.apiVersion === 1 &&
    isNum(f.generatedAt) &&
    isStr(f.name) &&
    isHost(f.host) &&
    isGateway(f.gateway) &&
    (f.admiral === null || isCaptain(f.admiral)) &&
    isArrOf(f.ships, isShip) &&
    (f.longestIdle === null ||
      (isObj(f.longestIdle) && isStr(f.longestIdle.ship) && isStr(f.longestIdle.member) && isNum(f.longestIdle.since)))
  );
}

export function isActivityEvent(a) {
  return (
    isObj(a) &&
    isStr(a.ship) &&
    isStrOrNull(a.member) &&
    isOneOf(ACTIVITY_KINDS)(a.kind) &&
    isStr(a.text) &&
    isNum(a.at) &&
    (a.branch === undefined || isStr(a.branch)) &&
    (a.status === undefined || isOneOf(TASK_STATUSES)(a.status))
  );
}

export function isHeartbeat(h) {
  return isObj(h) && isNum(h.at);
}

const EVENT_CHECKS = {
  fleet: isFleet,
  ship: isShipDetail,
  host: isHost,
  activity: isActivityEvent,
  heartbeat: isHeartbeat,
};

/** True when `data` is a valid payload for the SSE event `type`. */
export function isValidEvent(type, data) {
  const check = EVENT_CHECKS[type];
  return Boolean(check && check(data));
}
