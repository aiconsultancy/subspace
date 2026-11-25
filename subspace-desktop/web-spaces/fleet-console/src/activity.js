// Officer activity: what the rear admiral's or a captain's agent is doing in
// its turns, from `GET /api/v1/agents/{id}/activity` and the `agent-activity`
// events on the page's one event stream. Nothing is redacted or cut: paged
// items are read in full on demand. Content lives in memory only, for as long
// as the view is open; it is never stored or logged.
//
// Joining the snapshot and the stream (STAR-6):
//   1. the stream is (re)opened with `?agent=<id>` and its events buffered;
//   2. once the stream has answered (the gateway registers the watch when the
//      events response starts), the snapshot is read; never before, or
//      changes between the two could be lost with no sign in `seq`;
//   3. buffered and later events with `seq` above the snapshot's `cursor.seq`
//      are applied, upserting turns and items by ID; the rest are dropped;
//   4. the snapshot is read again (back to 3) when the stream re-opens (after
//      it answers, as in 2), when
//      an event's `generation` differs from the cursor's, or when `seq` fails
//      to rise past the last applied event. `seq` may skip values: coalesced
//      changes and other turns use them, so a skip is not a gap.
// `seq` is per agent: one watcher follows one agent and ignores the others'
// events, so its cursor and last applied seq are that agent's.

import { readPagedItem } from "./api.js";
import { coalesce } from "./coalesce.js";

/** The gateway keeps this many turns per officer; the view keeps no more. */
export const MAX_TURNS = 16;
/** Events held while a snapshot is read; past this the snapshot is read again instead. */
export const MAX_BUFFER = 4096;

const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const isNum = (v) => typeof v === "number" && Number.isFinite(v);
const isStr = (v) => typeof v === "string";

export const TURN_STATUSES = ["running", "completed", "failed", "cancelled"];
export const ITEM_KINDS = ["step", "text", "reasoning", "tool"];

export function isActivityItem(item) {
  return isObj(item) && isStr(item.id) && isNum(item.seq) && isStr(item.kind) && (item.paged === undefined || (isObj(item.paged) && isNum(item.paged.pages) && isNum(item.paged.bytes)));
}

/** A turn's own fields; `items` is checked separately so a bad item drops alone. */
export function isActivityTurn(turn) {
  return isObj(turn) && isStr(turn.id) && isNum(turn.seq) && isStr(turn.status) && isNum(turn.startedAt);
}

function decodeTurn(turn) {
  if (!isActivityTurn(turn)) return null;
  const items = Array.isArray(turn.items) ? turn.items.filter(isActivityItem) : [];
  return { ...turn, reconnects: isNum(turn.reconnects) ? turn.reconnects : 0, items };
}

/**
 * Decode a `V1AgentActivity` snapshot. Throws when its frame is wrong; drops
 * single turns or items that fail their checks, so a server bug shows as
 * missing data rather than a broken view.
 */
export function decodeActivity(json) {
  if (!isObj(json) || !isObj(json.agent) || !isStr(json.agent.id) || !isObj(json.cursor) || !isStr(json.cursor.generation) || !isNum(json.cursor.seq) || !Array.isArray(json.turns)) {
    throw new Error("Stargate sent an activity snapshot this console cannot read.");
  }
  const agent = { id: json.agent.id, role: json.agent.role ?? null, backend: json.agent.backend ?? null, collected: json.agent.collected !== false };
  const turns = json.turns.map(decodeTurn).filter(Boolean);
  return { agent, turns, cursor: { generation: json.cursor.generation, seq: json.cursor.seq } };
}

/** Decode an `agent-activity` event's data, or null. */
export function decodeActivityEvent(data) {
  if (!isObj(data) || !isStr(data.agentId) || !isStr(data.generation) || !isNum(data.seq) || !isObj(data.change) || !isStr(data.change.type)) return null;
  return { agentId: data.agentId, turnId: isStr(data.turnId) ? data.turnId : null, generation: data.generation, seq: data.seq, at: isNum(data.at) ? data.at : null, change: data.change };
}

const TURN_CHANGES = new Set(["turn-started", "turn-updated", "turn-ended"]);

/**
 * Apply one event's change to the turns (oldest first). Returns the new turns,
 * or null when the change cannot be placed (an item for a turn the view has
 * never seen) and the snapshot must be read again. Unknown change types are
 * ignored: the contract is additive.
 */
export function applyChange(turns, event) {
  const { change } = event;
  if (TURN_CHANGES.has(change.type)) {
    const incoming = isObj(change.turn) ? { ...change.turn, id: change.turn.id ?? event.turnId } : null;
    if (!incoming || !isActivityTurn(incoming)) return null;
    const index = turns.findIndex((turn) => turn.id === incoming.id);
    const existing = index >= 0 ? turns[index] : null;
    if (existing && existing.seq > incoming.seq) return turns;
    // Turn events carry the turn without its items: keep the ones already held.
    const merged = { ...decodeTurn({ ...incoming, items: [] }), items: existing ? existing.items : (decodeTurn(incoming)?.items ?? []) };
    if (existing) return turns.map((turn, i) => (i === index ? merged : turn));
    return [...turns, merged].slice(-MAX_TURNS);
  }
  if (change.type === "item") {
    const item = change.item;
    if (!isActivityItem(item)) return turns;
    const index = turns.findIndex((turn) => turn.id === event.turnId);
    if (index < 0) return null;
    const turn = turns[index];
    const at = turn.items.findIndex((entry) => entry.id === item.id);
    // Keep the version with the higher seq.
    if (at >= 0 && turn.items[at].seq > item.seq) return turns;
    const items = at >= 0 ? turn.items.map((entry, i) => (i === at ? item : entry)) : [...turn.items, item];
    return turns.map((entry, i) => (i === index ? { ...turn, items } : entry));
  }
  return turns;
}

export const ACTIVITY_MESSAGES = Object.freeze({
  reconnected: "Some changes may be missing (the stream reconnected)",
  notCollected: "This officer's backend isn't collected yet.",
  unavailable: "Activity isn't available on this gateway.",
});

const fullKey = (turn, item) => `${turn}\u0000${item}`;

/**
 * One officer's activity, joined from its snapshot and events.
 *
 * @param {object} options
 * @param {{activity: Function, activityItemPage: Function}} options.client
 * @param {string} options.agent  the officer's registration ID
 * @param {(view: object) => void} options.onChange
 * @param {boolean} [options.awaitStream]  read the first snapshot only on `reopened()`, once the stream answered with this agent
 */
export function createActivityWatcher({ client, agent, onChange, awaitStream = true }) {
  // `stale`: a refetch failed while ready. The last snapshot stays on show,
  // but events are not applied on top of it until a snapshot is read again.
  let state = { status: "loading", agent: null, turns: [], cursor: null, snapshotSeq: 0, lastSeq: 0, error: null, refreshing: false, stale: false };
  /** Paged items read in full: key -> {seq, state: loading|ready|failed, item, error}. */
  const full = new Map();
  const reads = new Map();
  let buffer = [];
  let pending = true;
  let stopped = false;
  let controller = null;
  /** True until the stream first answers with this agent. */
  let waiting = awaitStream;

  const changed = () => {
    if (!stopped) onChange?.(view());
  };

  const load = coalesce(async () => {
    pending = true;
    controller = new AbortController();
    const own = controller;
    try {
      const snapshot = decodeActivity(await client.activity(agent, {}, own.signal));
      if (stopped) return;
      state = { ...state, status: "ready", agent: snapshot.agent, turns: snapshot.turns.slice(-MAX_TURNS), cursor: snapshot.cursor, snapshotSeq: snapshot.cursor.seq, lastSeq: snapshot.cursor.seq, error: null, refreshing: false, stale: false };
      pending = false;
      const queued = buffer;
      buffer = [];
      for (let i = 0; i < queued.length; i += 1) {
        if (!apply(queued[i])) {
          // A refetch was asked for: the rest wait for the next snapshot.
          buffer = [...queued.slice(i + 1), ...buffer];
          break;
        }
      }
      refreshFull();
    } catch (error) {
      if (stopped || own.signal.aborted) return;
      pending = false;
      buffer = [];
      state = error?.status === 404
        ? { ...state, status: "unavailable", error: null, refreshing: false }
        : state.status === "ready"
          ? // Out of step: the events that asked for the refetch are gone, so stop applying events until a snapshot is read.
            { ...state, stale: true, error: error?.message ?? String(error), refreshing: false }
          : { ...state, status: "failed", error: error?.message ?? String(error), refreshing: false };
    } finally {
      if (controller === own) controller = null;
    }
    changed();
  });

  function refetch() {
    if (stopped) return;
    pending = true;
    state = { ...state, refreshing: true };
    load(true);
  }

  /** Apply one decoded event; false when it asked for a new snapshot. */
  function apply(event) {
    if (event.agentId !== agent) return true;
    if (pending) {
      buffer.push(event);
      if (buffer.length > MAX_BUFFER) {
        buffer = [];
        load(true);
      }
      return true;
    }
    if (state.status !== "ready" || state.stale) return true;
    if (event.generation !== state.cursor.generation) return refetch(), false;
    if (event.seq <= state.snapshotSeq) return true;
    if (event.seq <= state.lastSeq) return refetch(), false;
    const turns = applyChange(state.turns, event);
    if (turns === null) return refetch(), false;
    state = { ...state, turns, lastSeq: event.seq, cursor: { ...state.cursor, seq: event.seq } };
    return true;
  }

  /** Keep items read in full current: read a newer version again, forget ones no longer paged. */
  function refreshFull() {
    for (const [key, entry] of full) {
      const [turnId, itemId] = key.split("\u0000");
      const item = findItem(turnId, itemId);
      if (!item?.paged) {
        reads.get(key)?.abort();
        reads.delete(key);
        full.delete(key);
      } else if (item.seq > entry.seq && entry.state !== "loading") {
        loadItem(turnId, itemId);
      }
    }
  }

  function findItem(turnId, itemId) {
    return state.turns.find((turn) => turn.id === turnId)?.items.find((item) => item.id === itemId) ?? null;
  }

  async function loadItem(turnId, itemId) {
    const item = findItem(turnId, itemId);
    if (!item?.paged || stopped) return;
    const key = fullKey(turnId, itemId);
    const previous = full.get(key);
    if (previous?.state === "loading") return;
    if (previous?.state === "ready" && previous.seq >= item.seq) return;
    const own = new AbortController();
    reads.set(key, own);
    full.set(key, { seq: previous?.seq ?? item.seq, state: "loading", item: previous?.item ?? null, error: null });
    changed();
    let next;
    try {
      const result = await readPagedItem((page) => client.activityItemPage(agent, turnId, itemId, page, own.signal));
      next = { seq: result.seq, state: "ready", item: result.item, error: null };
    } catch (error) {
      next = { seq: previous?.seq ?? item.seq, state: "failed", item: previous?.item ?? null, error: error?.status === 404 ? "This turn is no longer on the gateway." : (error?.message ?? String(error)) };
    }
    if (stopped || reads.get(key) !== own) return;
    reads.delete(key);
    full.set(key, next);
    if (next.state === "ready") refreshFull();
    changed();
  }

  function view() {
    return {
      agentId: agent,
      status: state.status,
      agent: state.agent,
      // Newest first, as the view shows them.
      turns: [...state.turns].reverse(),
      cursor: state.cursor,
      error: state.error,
      refreshing: state.refreshing,
      stale: state.stale,
      waiting,
      full: (turnId, itemId) => full.get(fullKey(turnId, itemId)) ?? null,
    };
  }

  return {
    /**
     * Begin. With `awaitStream`, the first snapshot waits for `reopened()`;
     * events that arrive meanwhile are buffered either way.
     */
    start() {
      if (!waiting) load(true);
    },
    /** An `agent-activity` event's data from the stream. */
    event(data) {
      const event = decodeActivityEvent(data);
      if (!event || stopped) return;
      apply(event);
      if (!pending) refreshFull();
      changed();
    },
    /** The stream (re)opened with this agent: activity events have no replay, so read the snapshot again. */
    reopened() {
      waiting = false;
      refetch();
      changed();
    },
    /** The gateway refused the stream subscription (400 or 404): no activity to follow here. */
    refused() {
      if (stopped) return;
      waiting = false;
      controller?.abort();
      buffer = [];
      pending = false;
      state = { ...state, status: "unavailable", refreshing: false };
      changed();
    },
    retry() {
      if (waiting) return;
      if (state.status !== "ready") state = { ...state, status: "loading", error: null };
      refetch();
      changed();
    },
    loadItem,
    get view() {
      return view();
    },
    stop() {
      stopped = true;
      controller?.abort();
      for (const read of reads.values()) read.abort();
      reads.clear();
      full.clear();
      buffer = [];
      state = { ...state, turns: [] };
    },
  };
}
