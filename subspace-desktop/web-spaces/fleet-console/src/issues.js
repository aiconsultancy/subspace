// Issue references: find identifiers such as `FLT-212` in crew text, branch
// names and Linear links for the board's team keys, and resolve them through
// the bridge's `issues` contract in debounced batches of at most 50.

export const RESOLVE_BATCH = 50;
export const REFRESH_MS = 60_000;

const escapeRegex = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function keyPattern(teamKeys) {
  const keys = (teamKeys ?? []).filter((key) => /^[A-Za-z][A-Za-z0-9]{0,15}$/.test(key));
  if (keys.length === 0) return null;
  return keys.map(escapeRegex).sort((a, b) => b.length - a.length).join("|");
}

function canonical(teamKeys, key, number) {
  const match = teamKeys.find((entry) => entry.toLowerCase() === key.toLowerCase());
  if (!match || number.startsWith("0")) return null;
  return `${match.toUpperCase()}-${number}`;
}

/**
 * Distinct identifiers, in the order found.
 * @param {{kind: "text"|"branch", text: string}[]} sources
 * @param {{teamKeys: string[], linearWorkspace?: string|null}} scope
 */
export function extractIdentifiers(sources, scope) {
  const pattern = keyPattern(scope?.teamKeys);
  if (!pattern) return [];
  const teamKeys = scope.teamKeys;
  // Prose: the key as written, bounded by non-word characters.
  const prose = new RegExp(`(?<![A-Za-z0-9])(${pattern})-(\\d{1,7})(?![A-Za-z0-9])`, "g");
  // Branches: any case, bounded by separators, such as feat/abc-123-tier-warp.
  const branch = new RegExp(`(?<![A-Za-z0-9])(${pattern})-(\\d{1,7})(?![0-9])`, "gi");
  const workspace = scope.linearWorkspace ? escapeRegex(scope.linearWorkspace) : null;
  const linear = workspace
    ? new RegExp(`https://linear\\.app/${workspace}/issue/(${pattern})-(\\d{1,7})(?![0-9])`, "gi")
    : null;

  const found = [];
  const add = (key, number) => {
    const identifier = canonical(teamKeys, key, number);
    if (identifier && !found.includes(identifier)) found.push(identifier);
  };
  for (const source of sources ?? []) {
    const text = String(source?.text ?? "");
    if (!text) continue;
    if (linear) for (const match of text.matchAll(linear)) add(match[1], match[2]);
    if (source.kind === "branch") {
      for (const match of text.matchAll(branch)) add(match[1], match[2]);
      continue;
    }
    // Other links are links, as in IRC: an identifier inside one does not count.
    const words = text.replace(/https?:\/\/\S+/gi, " ");
    for (const match of words.matchAll(prose)) add(match[1], match[2]);
  }
  return found;
}

/** The most identifiers kept; the least recently wanted are forgotten first. */
export const MAX_WANTED = 500;

/**
 * Resolves identifiers through `resolve(ids)` (the bridge), at most 50 per
 * call, coalescing requests made within `debounceMs`. It keeps answers for
 * the MAX_WANTED most recently wanted identifiers, so a long session does
 * not grow without bound.
 */
export function createIssueResolver({ resolve, onChange, debounceMs = 250, setTimer = setTimeout, clearTimer = clearTimeout }) {
  const results = new Map();
  const wanted = new Set();
  let queued = new Set();
  let timer = null;
  let running = false;

  async function flush() {
    timer = null;
    if (running) {
      schedule();
      return;
    }
    const ids = [...queued];
    queued = new Set();
    if (ids.length === 0) return;
    running = true;
    let changed = false;
    try {
      for (let i = 0; i < ids.length; i += RESOLVE_BATCH) {
        const batch = ids.slice(i, i + RESOLVE_BATCH);
        let answers;
        try {
          answers = await resolve(batch);
        } catch {
          answers = batch.map((identifier) => ({ identifier, state: "failed" }));
        }
        for (const answer of Array.isArray(answers) ? answers : []) {
          if (!answer || typeof answer.identifier !== "string") continue;
          const previous = results.get(answer.identifier);
          if (JSON.stringify(previous) !== JSON.stringify(answer)) changed = true;
          results.set(answer.identifier, answer);
        }
      }
    } finally {
      running = false;
    }
    if (changed) onChange?.();
  }

  function schedule() {
    if (timer === null) timer = setTimer(flush, debounceMs);
  }

  return {
    /** Ask for identifiers; only ones not yet answered are resolved. */
    want(ids) {
      for (const id of ids) {
        wanted.delete(id);
        wanted.add(id);
        if (!results.has(id)) queued.add(id);
      }
      for (const id of wanted) {
        if (wanted.size <= MAX_WANTED) break;
        wanted.delete(id);
        results.delete(id);
        queued.delete(id);
      }
      if (queued.size) schedule();
    },
    /** Re-resolve everything asked for so far, as on `subspace:issues-changed`. */
    refresh() {
      for (const id of wanted) queued.add(id);
      if (queued.size) schedule();
    },
    get(id) {
      return results.get(id) ?? null;
    },
    clear() {
      results.clear();
      wanted.clear();
      queued = new Set();
      if (timer !== null) clearTimer(timer);
      timer = null;
    },
    /** For tests: run a pending flush now. */
    flushNow: () => flush(),
  };
}

/** The state glyph IRC uses for each issue state kind, and its tone. */
export const ISSUE_STATE = {
  triage: { glyph: "circle-dot-dashed", tone: "orange" },
  backlog: { glyph: "circle-dashed", tone: "muted" },
  unstarted: { glyph: "circle", tone: "fg" },
  started: { glyph: "contrast", tone: "yellow" },
  completed: { glyph: "circle-check", tone: "accent" },
  canceled: { glyph: "circle-x", tone: "muted" },
  duplicate: { glyph: "circle-slash", tone: "muted" },
};

export const issueStateOf = (kind) => ISSUE_STATE[kind] ?? { glyph: "circle", tone: "muted" };
