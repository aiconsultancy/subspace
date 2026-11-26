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
//
// Paged items are read on request. One that fits the view's budget is read
// whole; one larger than it is read and shown a part (a run of pages) at a
// time, pinned to one version, so every byte stays reachable and memory
// stays bounded. The budget is shared with the panel, which counts what it
// draws against it too. The snapshot's inline content is charged first, as
// the floor: it is never freed, and only it and the latest request (a Show,
// a Load or another part, which the budget tracks) may take the view past
// its budget (SUB decisions 8357ec55, 1c19f413).
//
// A failed read keeps the last snapshot on show, marked stale, and applies no
// events until a read succeeds; it is tried again with backoff (1, 2, 4 … 30 s,
// or the gateway's Retry-After up to 30 s) and on Try again, once the current
// stream has answered. A gateway that refuses `?agent=` is polled every five
// seconds instead. When the fleet's address or token changes, everything read
// from the old one is dropped and the view starts again on the new stream.
//
// Items read belong to the snapshot generation they were read in (SUB
// decision 1c19f413 §4): a seq means nothing in another generation. A
// snapshot of a new generation aborts every read in flight, settles their
// waiters and drops every item read, and a reply from the old generation is
// never shown.

import { ChangingError, DEFAULT_ROOM_BYTES, PAGE_DATA_BYTES, PART_PAGES, readItemPart, readPagedItem, TooLargeError } from "./api.js";
import { createBudget } from "./budget.js";
import { coalesce } from "./coalesce.js";
import { jsonBytes, shownBytes, WINDOW_BYTES } from "./document.js";
import { createPager } from "./pager.js";
import { utf8Length } from "./utf8.js";

/** The gateway keeps this many turns per officer; the view keeps no more. */
export const MAX_TURNS = 16;
/** Events held while a snapshot is read; past this many, or past MAX_BUFFER_BYTES, they are dropped and the snapshot read again (as Channels). */
export const MAX_BUFFER = 512;
export const MAX_BUFFER_BYTES = 8 * 1024 * 1024;
/** One view's budget: the inline snapshot (never freed), items read and what the panel draws; the least recently opened read goes first. */
export const MAX_LOADED_BYTES = DEFAULT_ROOM_BYTES;
/** How often a gateway that refuses the subscription is read instead. */
export const POLL_MS = 5_000;
/** Backoff for reading the snapshot again after a failure: 1, 2, 4 … 30 s. */
export const RETRY_BASE_MS = 1_000;
export const RETRY_MAX_MS = 30_000;

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

/**
 * Whether a known kind has the fields the view draws it from. A known kind
 * that fails is shown as the JSON it came as, never hidden (SUB-70).
 */
export function isWellFormedItem(item) {
  const optStr = (v) => v === undefined || v === null || isStr(v);
  if (item.paged) return isStr(item.status) || item.status === undefined;
  switch (item.kind) {
    case "step":
      return isStr(item.status) && isNum(item.startedAt);
    case "text":
    case "reasoning":
      return isStr(item.status) && isStr(item.text);
    case "tool":
      return isStr(item.status) && optStr(item.name) && optStr(item.callId) && optStr(item.rawInput);
    default:
      return true;
  }
}

/**
 * An item as the view holds it: as sent when it passes its checks, or else
 * kept whole as `{kind: "unreadable", raw}` so it still shows.
 */
function decodeItem(item, index, fallbackSeq) {
  if (isActivityItem(item)) return item;
  const id = isObj(item) && isStr(item.id) ? item.id : `#unreadable-${index}`;
  return { id, seq: isObj(item) && isNum(item.seq) ? item.seq : fallbackSeq, kind: "unreadable", raw: item };
}

/**
 * A turn that fails its checks, kept so it still shows: as an unreadable turn
 * whose one item is the whole turn as sent (SUB-70), as unreadable items are.
 */
function unreadableTurn(turn, index) {
  const id = isObj(turn) && isStr(turn.id) ? turn.id : `#unreadable-turn-${index}`;
  const seq = isObj(turn) && isNum(turn.seq) ? turn.seq : 0;
  const startedAt = isObj(turn) && isNum(turn.startedAt) ? turn.startedAt : null;
  return { id, seq, status: "unreadable", startedAt, reconnects: 0, unreadable: true, items: [{ id: "#turn", seq, kind: "unreadable", wholeTurn: true, raw: turn }] };
}

/**
 * A turn as the view holds it. One whose `items` is not a list is kept as
 * an unreadable turn too, its payload shown as sent, never as a turn that
 * observed nothing (SUB-70).
 */
function decodeTurn(turn, index = 0) {
  if (!isActivityTurn(turn) || !Array.isArray(turn.items)) return unreadableTurn(turn, index);
  const items = turn.items.map((item, index) => decodeItem(item, index, turn.seq));
  return { ...turn, reconnects: isNum(turn.reconnects) ? turn.reconnects : 0, items };
}

/**
 * Decode a `V1AgentActivity` snapshot. Throws when its frame is wrong. A turn
 * or an item that fails its checks is kept as unreadable and shown as its
 * JSON. Unknown optional fields
 * (such as a later `session` or `unavailable`) are kept and ignored.
 */
export function decodeActivity(json) {
  if (!isObj(json) || !isObj(json.agent) || !isStr(json.agent.id) || !isObj(json.cursor) || !isStr(json.cursor.generation) || !isNum(json.cursor.seq) || !Array.isArray(json.turns)) {
    throw new Error("Stargate sent an activity snapshot this console cannot read.");
  }
  const agent = { id: json.agent.id, role: json.agent.role ?? null, backend: json.agent.backend ?? null, collected: json.agent.collected !== false };
  const turns = json.turns.map((turn, index) => decodeTurn(turn, index));
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
    // An unreadable turn holds no items to keep: read the snapshot again.
    if (existing?.unreadable) return null;
    // Turn events carry the turn without its items: keep the ones already held.
    // A turn event that carries items that are not a list cannot be placed: read the snapshot again.
    if (incoming.items !== undefined && !Array.isArray(incoming.items)) return null;
    const merged = { ...decodeTurn({ ...incoming, items: [] }), items: existing ? existing.items : decodeTurn({ ...incoming, items: incoming.items ?? [] }).items };
    if (existing) return turns.map((turn, i) => (i === index ? merged : turn));
    // A turn the view does not hold goes where its start puts it, not on top:
    // a late `turn-ended` for a turn already evicted is older than those held,
    // and falls off again when the view holds MAX_TURNS.
    let at = turns.findIndex((turn) => turn.startedAt > merged.startedAt);
    if (at < 0) at = turns.length;
    return [...turns.slice(0, at), merged, ...turns.slice(at)].slice(-MAX_TURNS);
  }
  if (change.type === "item") {
    // An item without an ID cannot be placed: read the snapshot again, where it
    // shows as unreadable. One that fails its other checks is kept as unreadable.
    if (!isObj(change.item) || !isStr(change.item.id)) return null;
    const item = decodeItem(change.item, 0, event.seq);
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
  polling: "Live updates aren't available here, so turns refresh every few seconds.",
});

const fullKey = (turn, item) => `${turn}\u0000${item}`;

class AbortedError extends Error {
  constructor() {
    super("aborted");
    this.name = "AbortError";
  }
}

/** `promise`, or a rejection as soon as `signal` aborts, so a read that ignores its signal still lets go. */
function untilAborted(promise, signal) {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(new AbortedError());
      return;
    }
    const onAbort = () => reject(new AbortedError());
    signal.addEventListener("abort", onAbort, { once: true });
    Promise.resolve(promise).then(
      (value) => {
        signal.removeEventListener("abort", onAbort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener("abort", onAbort);
        reject(error);
      },
    );
  });
}

const errorText = (error) => error?.message ?? String(error);

/** A timer that never keeps a process alive on its own (a no-op in the page). */
function defaultTimer(callback, ms) {
  const timer = setTimeout(callback, ms);
  timer?.unref?.();
  return timer;
}

/** The most a part of a too-large item holds: PART_PAGES pages of 64 KiB. */
export const PART_MAX_BYTES = PART_PAGES * PAGE_DATA_BYTES;
/**
 * An item read whole is charged this many times its length: the item and
 * the formatted copy the panel shows it from. One that does not fit so is
 * read in parts.
 */
export const FORMAT_FACTOR = 2;
/** The inline snapshot's key in the budget: pinned, never freed. */
export const INLINE_KEY = "inline";

/** Each turn's compact JSON length, counted once per turn object (a change makes a new one). */
const turnBytes = new WeakMap();
function bytesOfTurn(turn) {
  let bytes = turnBytes.get(turn);
  if (bytes === undefined) {
    bytes = jsonBytes(turn);
    turnBytes.set(turn, bytes);
  }
  return bytes;
}

/**
 * One officer's activity, joined from its snapshot and events.
 *
 * @param {object} options
 * @param {{activity: Function, activityItemPage: Function}} options.client  read at each call, so it may resolve the fleet's current client
 * @param {string} options.agent  the officer's registration ID
 * @param {(view: object) => void} options.onChange
 * @param {boolean} [options.awaitStream]  read the snapshot only on `reopened()`, once the stream answered with this agent
 * @param {number} [options.maxLoadedBytes]  the view's budget (`view.budget`): items read, and what the panel formats and mounts
 * @param {ReturnType<import("./budget.js").createBudget>} [options.budget]  that budget, when the console shares one
 *   (a `scope` of it, SUB decision 75c3ed93 §3); `stop` gives up this view's keys only
 * @param {(callback: () => void, ms: number) => unknown} [options.setTimer]  schedules the backoff retries and polls (tests pass fake timers)
 * @param {(timer: unknown) => void} [options.clearTimer]  cancels a timer `setTimer` returned
 */

export function createActivityWatcher({ client, agent, onChange, awaitStream = true, maxLoadedBytes = MAX_LOADED_BYTES, budget = createBudget(maxLoadedBytes), setTimer = defaultTimer, clearTimer = clearTimeout }) {
  const initial = () => ({ status: "loading", agent: null, turns: [], cursor: null, snapshotSeq: 0, lastSeq: 0, error: null, refreshing: false, stale: false, polling: false });
  // `stale`: a read failed while ready. The last snapshot stays on show, but
  // events are only buffered, not applied, until a snapshot is read again.
  let state = initial();
  /**
   * Paged items read: key -> an entry. `mode: "whole"` is {turnId, itemId,
   * seq, state: queued|loading|ready|failed, error, item, bytes};
   * `mode: "parts"`, for an item over the budget, is {turnId, itemId, pager}.
   */
  const full = new Map();
  /** Where the view shows each item read whole: key -> {at, bytes}, `place` below. */
  const positions = new Map();
  /** Keys waiting to be read; one item is read at a time. */
  let queue = [];
  /** The item being read: {key, controller}, or null. */
  let reading = null;
  /** Callers of `loadItem` waiting for a key's read to settle. */
  const settlers = new Map();
  let buffer = [];
  /** The UTF-8 bytes of the events in `buffer`, kept as it changes rather than summed again. */
  let bufferBytes = 0;
  let pending = true;
  let stopped = false;
  let controller = null;
  /** Bumped when the fleet's source changes: results of earlier reads are dropped. */
  let epoch = 0;
  /** True until the current stream attempt answers with this agent. */
  let waiting = awaitStream;
  let retryTimer = null;
  let failures = 0;
  let pollTimer = null;

  /**
   * Charge the turns held, as their compact JSON: the inline content, which
   * is the floor and is never freed. Pinned, so the snapshot always shows;
   * loaded items are freed for it first.
   */
  function chargeInline() {
    let bytes = 0;
    for (const turn of state.turns) bytes += bytesOfTurn(turn);
    const grew = bytes > budget.bytes(INLINE_KEY);
    if (bytes !== budget.bytes(INLINE_KEY) || !budget.has(INLINE_KEY)) budget.reserve(INLINE_KEY, bytes, null, { pinned: true });
    if (grew) recheckWholes();
  }

  /**
   * The pinned charge grew: an item read whole that no longer fits beside
   * it (and the latest request, when that is another item) reads in parts
   * instead. Being the latest request never keeps a whole read past its
   * allowance (SUB decision 533c2b29).
   */
  function recheckWholes() {
    for (const key of [...full.keys()]) {
      // Each switch reserves a part, which may free another entry meanwhile.
      if (full.get(key)?.mode === "whole" && budget.has(key) && !budget.fits(key, budget.bytes(key))) wholeToParts(key);
    }
  }

  const changed = () => {
    if (stopped) return;
    chargeInline();
    onChange?.(view());
  };

  const load = coalesce(
    async () => {
      if (stopped) return;
      // Checked here, right before the read, not when the read was asked for:
      // a rerun queued behind a read in flight can outlive the stream attempt
      // that answered. Until the current attempt answers there is no read;
      // `reopened()` asks for it then.
      if (waiting) return;
      const at = epoch;
      pending = true;
      // Events held from before this read (while stale) are covered by it unless they are newer.
      for (const event of buffer) event.early = true;
      const own = new AbortController();
      controller = own;
      const gone = () => stopped || at !== epoch || own.signal.aborted;
      try {
        const json = await untilAborted(client.activity(agent, {}, own.signal), own.signal);
        if (gone()) return;
        const snapshot = decodeActivity(json);
        failures = 0;
        clearRetry();
        // A new generation: nothing read in the old one carries over, and its reads stop.
        if (state.cursor && state.cursor.generation !== snapshot.cursor.generation) dropAll();
        state = { ...state, status: "ready", agent: snapshot.agent, turns: snapshot.turns.slice(-MAX_TURNS), cursor: snapshot.cursor, snapshotSeq: snapshot.cursor.seq, lastSeq: snapshot.cursor.seq, error: null, refreshing: false, stale: false };
        pending = false;
        const queued = buffer;
        setBuffer([]);
        for (let i = 0; i < queued.length; i += 1) {
          // An event from before the read in another generation is older than the snapshot just read.
          if (queued[i].early && queued[i].generation !== state.cursor.generation) continue;
          if (!apply(queued[i])) {
            // A refetch was asked for: the rest wait for the next snapshot.
            setBuffer([...queued.slice(i + 1), ...buffer]);
            break;
          }
        }
        chargeInline();
        refreshFull();
      } catch (error) {
        if (gone()) return;
        pending = false;
        // A stale view keeps buffering (bounded): after the next good read,
        // the events above its seq are applied. Without a snapshot there is nothing to apply them to.
        if (state.status !== "ready") setBuffer([]);
        if (error?.status === 404) {
          setBuffer([]);
          clearRetry();
          stopPolling();
          state = { ...state, status: "unavailable", error: null, refreshing: false, polling: false };
        } else {
          state =
            state.status === "ready"
              ? // Out of step: the events that asked for the read are gone, so apply none until a snapshot is read.
                { ...state, stale: true, error: errorText(error), refreshing: false }
              : { ...state, status: "failed", error: errorText(error), refreshing: false };
          scheduleRetry(error);
        }
      } finally {
        if (controller === own) controller = null;
      }
      changed();
    },
    { cancelled: () => stopped },
  );

  function setBuffer(events) {
    buffer = events;
    bufferBytes = events.reduce((total, held) => total + held.bytes, 0);
  }

  function refetch() {
    if (stopped) return;
    pending = true;
    state = { ...state, refreshing: true };
    load(true);
  }

  function clearRetry() {
    if (retryTimer !== null) clearTimer(retryTimer);
    retryTimer = null;
  }

  /** Read again after a failure, single-flight, with backoff capped at 30 s; Retry-After counts up to the cap. */
  function scheduleRetry(error) {
    clearRetry();
    if (stopped) return;
    const backoff = Math.min(RETRY_MAX_MS, RETRY_BASE_MS * 2 ** Math.min(failures, 16));
    failures += 1;
    const after = Number.isFinite(error?.retryAfterMs) ? error.retryAfterMs : 0;
    retryTimer = setTimer(() => {
      retryTimer = null;
      // Without an answered stream the read waits for `reopened()`, which reads anyway.
      if (stopped || waiting) return;
      refetch();
      changed();
    }, Math.min(RETRY_MAX_MS, Math.max(backoff, after)));
  }

  function stopPolling() {
    if (pollTimer !== null) clearTimer(pollTimer);
    pollTimer = null;
  }

  function schedulePoll() {
    stopPolling();
    if (stopped || !state.polling) return;
    pollTimer = setTimer(() => {
      pollTimer = null;
      if (stopped || !state.polling) return;
      refetch();
      changed();
      schedulePoll();
    }, POLL_MS);
  }

  /** Apply one decoded event; false when it asked for a new snapshot. */
  function apply(event) {
    if (event.agentId !== agent) return true;
    if (pending || state.stale) {
      event.bytes ??= utf8Length(JSON.stringify(event.change) ?? "");
      buffer.push(event);
      bufferBytes += event.bytes;
      if (buffer.length > MAX_BUFFER || bufferBytes > MAX_BUFFER_BYTES) {
        setBuffer([]);
        // A read in flight may predate the dropped events: read again after it. A stale view reads again anyway.
        if (pending) load(true);
      }
      return true;
    }
    if (state.status !== "ready") return true;
    if (event.generation !== state.cursor.generation) return refetch(), false;
    if (event.seq <= state.snapshotSeq) return true;
    if (event.seq <= state.lastSeq) return refetch(), false;
    const turns = applyChange(state.turns, event);
    if (turns === null) return refetch(), false;
    state = { ...state, turns, lastSeq: event.seq, cursor: { ...state.cursor, seq: event.seq } };
    return true;
  }

  function findItem(turnId, itemId) {
    return state.turns.find((turn) => turn.id === turnId)?.items.find((item) => item.id === itemId) ?? null;
  }

  function settle(key) {
    const waiters = settlers.get(key);
    settlers.delete(key);
    for (const resolve of waiters ?? []) resolve();
  }

  /** Forget an item read: abort its read and release what it held. */
  function drop(key) {
    if (reading?.key === key) {
      reading.controller.abort();
      reading = null;
    }
    queue = queue.filter((entry) => entry !== key);
    full.get(key)?.pager?.drop();
    full.delete(key);
    positions.delete(key);
    budget.release(key);
    settle(key);
  }

  function dropAll() {
    reading?.controller.abort();
    reading = null;
    queue = [];
    for (const key of [...full.keys(), ...settlers.keys()]) settle(key);
    for (const [key, entry] of full) {
      entry.pager?.drop();
      budget.release(key);
    }
    full.clear();
    positions.clear();
  }

  /** How the budget frees an item read whole: as if it had never been read, so it offers Load in full again. */
  const evictor = (key) => () => {
    if (!full.has(key)) return;
    drop(key);
    changed();
  };

  /** Hold `bytes` for an item read whole, or, when the budget refuses (another request is the floor), forget it. */
  function holdWhole(key, bytes) {
    if (!budget.fits(key, bytes)) {
      // Past its allowance beside the floor, however recently asked for: read it in parts.
      wholeToParts(key);
      return false;
    }
    if (budget.reserve(key, bytes, evictor(key))) return true;
    drop(key);
    return false;
  }

  /**
   * Drop items read whose turn left the snapshot or that are no longer
   * paged. A newer version is not read by itself, as a running tool changes
   * often: the view offers Load the latest (as Channels does).
   */
  function refreshFull() {
    for (const [key, entry] of [...full]) {
      if (!findItem(entry.turnId, entry.itemId)?.paged) drop(key);
    }
    pump();
  }

  /**
   * Page reads go one at a time, whole and part reads alike: each waits for
   * the one before it to settle. A read whose signal aborted while it waited
   * is never sent.
   */
  let lane = Promise.resolve();
  const pageReader = (turnId, itemId, signal) => (page) => {
    const run = lane.then(() => {
      if (signal.aborted) throw new AbortedError();
      return untilAborted(client.activityItemPage(agent, turnId, itemId, page, signal), signal);
    });
    lane = run.catch(() => {});
    return run;
  };
  const readFailure = (failure) => (failure?.status === 404 ? "This turn is no longer on the gateway." : errorText(failure));

  /**
   * An item larger than the budget, read and shown a part (PART_PAGES pages)
   * at a time through a pager. Its parts are addressed by page number, so
   * any part can be read directly; each is pinned to the version shown.
   */
  function partsEntry(turnId, itemId, known) {
    const key = fullKey(turnId, itemId);
    const entry = { turnId, itemId, mode: "parts", generation: state.cursor?.generation ?? null, pager: null, replacing: known !== null };
    entry.pager = createPager({
      known,
      budget,
      key,
      partBytes: PART_MAX_BYTES,
      // A part draws one window at a time: at most WINDOW_BYTES as shown (SUB decision 8357ec55).
      drawn: (data) => Math.min(WINDOW_BYTES, shownBytes(data)),
      cursorAt: (part) => ({ from: part * PART_PAGES, offset: null }),
      read: async ({ cursor, version, signal }) => {
        const at = cursor ?? { from: 0, offset: 0 };
        const result = await readItemPart(pageReader(turnId, itemId, signal), { agent, turn: turnId, item: itemId, version, from: at.from, offset: at.offset });
        const pages = result.pages;
        return {
          data: result.data,
          next: result.from + PART_PAGES < pages ? { from: result.from + PART_PAGES, offset: result.end } : null,
          version: { seq: result.seq, bytes: result.bytes, pages },
          count: Math.max(1, Math.ceil(pages / PART_PAGES)),
          newer: result.newer,
        };
      },
      // Evicted: forgotten, so it offers Load in full again and reads from its first part.
      onEvict: () => {
        if (full.get(key) !== entry) return;
        full.delete(key);
        settle(key);
        changed();
      },
      onChange: () => {
        if (full.get(key) === entry) changed();
      },
    });
    return entry;
  }

  /**
   * Show an item too large to hold in parts, from its first part, or from
   * part `at` of a version already `known` (an item read whole).
   */
  function openParts(turnId, itemId, known = null, at = 0) {
    const key = fullKey(turnId, itemId);
    budget.release(key);
    const entry = partsEntry(turnId, itemId, known);
    full.set(key, entry);
    const done = entry.pager.open(at);
    done.then(() => settle(key));
    return done;
  }

  /**
   * An item read whole that no longer fits reads in parts instead (SUB
   * decision 533c2b29): of the version read, from the part at the place its
   * window shows (`place`). That window stays on show, and charged, until
   * the part arrives. Resolves once the part's read settles.
   */
  function wholeToParts(key) {
    const entry = full.get(key);
    if (reading?.key === key) {
      reading.controller.abort();
      reading = null;
    }
    queue = queue.filter((each) => each !== key);
    const shown = positions.get(key);
    positions.delete(key);
    if (!entry.item || !entry.pages) return openParts(entry.turnId, entry.itemId);
    const count = Math.max(1, Math.ceil(entry.pages / PART_PAGES));
    const known = { version: { seq: entry.seq, bytes: entry.bytes, pages: entry.pages }, count, carried: shown?.bytes ?? 0 };
    const done = openParts(entry.turnId, entry.itemId, known, Math.min(count - 1, Math.floor((shown?.at ?? 0) * count)));
    pump();
    return done;
  }

  /**
   * Show on an item read whole, at the floor (SUB decision 533c2b29): hold
   * the window the panel draws, `extra` bytes beyond the read's allowance,
   * in the read's own charge, freeing other entries for it; or, when the
   * whole and its window cannot be kept beside the pinned content, read it
   * in parts from where it shows. `extra` 0 gives the window's charge back.
   * Returns whether the window is held; false also when the item is being
   * read again, or not read whole.
   */
  function drawWhole(key, extra) {
    const entry = full.get(key);
    if (entry?.mode !== "whole" || !entry.item || reading?.key === key || queue.includes(key) || !budget.has(key)) return false;
    const bytes = entry.bytes * FORMAT_FACTOR + extra;
    if (budget.bytes(key) === bytes) return true;
    if (budget.fits(key, bytes) && budget.reserve(key, bytes, evictor(key))) return true;
    wholeToParts(key);
    changed();
    return false;
  }

  /** Start the next queued whole read if none is running. */
  function pump() {
    while (!reading && !stopped && queue.length) {
      const key = queue.shift();
      const entry = full.get(key);
      if (!entry || entry.mode !== "whole" || entry.state !== "queued") continue;
      const item = findItem(entry.turnId, entry.itemId);
      if (!item?.paged) {
        drop(key);
        continue;
      }
      // Room is made before the read starts; an item the budget cannot hold is read in parts.
      const held = entry.item ? entry.bytes * FORMAT_FACTOR : 0;
      const need = held + item.paged.bytes * FORMAT_FACTOR;
      if (!budget.fits(key, need) || !budget.reserve(key, need, evictor(key))) {
        openParts(entry.turnId, entry.itemId);
        changed();
        continue;
      }
      const own = new AbortController();
      reading = { key, controller: own };
      full.set(key, { ...entry, state: "loading" });
      readWhole(key, own, epoch);
    }
  }

  /** Whether `entry` was read in the snapshot generation on show. */
  const current = (entry) => entry.generation === (state.cursor?.generation ?? null);

  /** Whether a read finished for nothing: the view closed, the source or generation changed, or the item was dropped. */
  function abandoned(key, own, at, generation) {
    if (reading?.controller === own) reading = null;
    const entry = full.get(key);
    if (stopped || at !== epoch || own.signal.aborted || entry?.mode !== "whole" || entry.generation !== generation || !current(entry)) {
      pump();
      return true;
    }
    return false;
  }

  async function readWhole(key, own, at) {
    const entry = full.get(key);
    const generation = entry.generation;
    const held = entry.item ? entry.bytes * FORMAT_FACTOR : 0;
    let result = null;
    let failure = null;
    try {
      result = await readPagedItem(pageReader(entry.turnId, entry.itemId, own.signal), {
        agent,
        turn: entry.turnId,
        item: entry.itemId,
        // The version read may be larger than the snapshot said: make room for it before reading on.
        room: (bytes) => budget.fits(key, held + bytes * FORMAT_FACTOR) && budget.reserve(key, held + bytes * FORMAT_FACTOR, evictor(key)),
      });
    } catch (error) {
      failure = error;
    }
    if (abandoned(key, own, at, generation)) return;
    const now = full.get(key);
    if (result) {
      full.set(key, { ...now, seq: result.seq, state: "ready", item: result.item, bytes: result.bytes, pages: result.pages, error: null });
      holdWhole(key, result.bytes * FORMAT_FACTOR);
    } else if (failure instanceof TooLargeError) {
      // Larger than the budget: never read whole. Read its first part instead.
      openParts(now.turnId, now.itemId);
      refreshFull();
      changed();
      return;
    } else {
      // A failed read of a newer version keeps the version already read on show.
      full.set(key, { ...now, state: "failed", error: failure instanceof ChangingError ? failure.message : readFailure(failure) });
      if (now.item) holdWhole(key, now.bytes * FORMAT_FACTOR);
      else budget.release(key);
    }
    settle(key);
    refreshFull();
    changed();
  }

  /** Wait for `key`'s next read to settle. */
  const settled = (key) => new Promise((resolve) => settlers.set(key, [...(settlers.get(key) ?? []), resolve]));

  /**
   * Read a paged item on request: in full when it and its formatted copy
   * fit the budget, or else its first part. On an item already shown this
   * is Load the latest (or Try again). Resolves once that read settles.
   */
  function loadItem(turnId, itemId) {
    const item = findItem(turnId, itemId);
    if (!item?.paged || stopped) return Promise.resolve();
    const key = fullKey(turnId, itemId);
    const previous = full.get(key);
    // The latest request: with the inline content, the floor.
    budget.request(key);
    if (previous?.mode === "parts") {
      const shown = previous.pager.view;
      if (shown.state === "loading") return Promise.resolve();
      // Load the latest: from the first part of the newest version.
      if (shown.data !== null && shown.version && shown.version.seq < item.seq) return previous.pager.restart();
      if (shown.state === "failed") return previous.pager.open(shown.want);
      return Promise.resolve();
    }
    if (!previous && item.paged.bytes * FORMAT_FACTOR > budget.limit) {
      const done = openParts(turnId, itemId);
      changed();
      return done;
    }
    if (previous?.state === "ready" && previous.item && previous.seq >= item.seq) return Promise.resolve();
    const done = settled(key);
    if (previous?.state !== "loading" && previous?.state !== "queued") {
      full.set(key, { turnId, itemId, mode: "whole", generation: state.cursor?.generation ?? null, seq: previous?.seq ?? item.seq, state: "queued", item: previous?.item ?? null, bytes: previous?.bytes ?? 0, pages: previous?.pages ?? 0, error: null });
      queue.push(key);
      pump();
      changed();
    }
    return done;
  }

  /**
   * Show another part of an item read part by part: `to` is first,
   * previous, next, last or an index. The part is read when asked for,
   * pinned to the version shown. Resolves once that read settles.
   */
  function loadPart(turnId, itemId, to) {
    const key = fullKey(turnId, itemId);
    const entry = full.get(key);
    if (stopped || entry?.mode !== "parts") return Promise.resolve();
    budget.request(key);
    return entry.pager.open(to);
  }

  /** What the view shows of an item read: a whole entry as held, a parts entry as its pager sees it; nothing from another generation. */
  function shownFull(entry) {
    if (!entry || !current(entry)) return null;
    if (entry.mode !== "parts") return entry;
    const pager = entry.pager.view;
    const error = pager.error === null ? null : pager.error instanceof ChangingError ? pager.error.message : readFailure(pager.error);
    // An item read whole whose window stays on show until its first part arrives.
    const replacing = entry.replacing && pager.data === null && pager.state !== "evicted";
    return { turnId: entry.turnId, itemId: entry.itemId, mode: "parts", ...pager, seq: pager.version?.seq ?? null, error, replacing };
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
      polling: state.polling,
      retrying: retryTimer !== null,
      waiting,
      full: (turnId, itemId) => shownFull(full.get(fullKey(turnId, itemId))),
      /**
       * The view says where it shows an item read whole: `at`, the share of
       * its text before the window on show (0 to 1), and `bytes`, the
       * window's size as shown. Read in parts, it opens at that place.
       */
      place: (turnId, itemId, at, bytes) => {
        const key = fullKey(turnId, itemId);
        if (full.get(key)?.mode === "whole") positions.set(key, { at, bytes });
      },
      drawWhole: (turnId, itemId, extra) => drawWhole(fullKey(turnId, itemId), extra),
      /** Bytes held for items read (whole or a part); the panel's share of the budget is not counted here. */
      loadedBytes: [...full.keys()].reduce((total, key) => total + budget.bytes(key), 0),
      budget,
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
      chargeInline();
      if (!pending && !state.stale) refreshFull();
      changed();
    },
    /** The stream (re)opened with this agent: activity events have no replay, so read the snapshot again. */
    reopened() {
      if (stopped) return;
      waiting = false;
      clearRetry();
      if (state.polling) {
        stopPolling();
        state = { ...state, polling: false };
      }
      refetch();
      changed();
    },
    /** The stream attempt that carried this agent ended, or a new one began: wait for its answer before reading. */
    disconnected() {
      if (stopped || !awaitStream || state.polling || waiting) return;
      waiting = true;
      clearRetry();
      changed();
    },
    /** The gateway refused the stream subscription (400 or 404): read the snapshot every few seconds instead. */
    refused() {
      if (stopped) return;
      waiting = false;
      setBuffer([]);
      state = { ...state, polling: true };
      refetch();
      schedulePoll();
      changed();
    },
    /** The fleet's address or token changed: drop everything read from the old source and wait for the new stream. */
    reset() {
      if (stopped) return;
      epoch += 1;
      controller?.abort();
      controller = null;
      dropAll();
      clearRetry();
      stopPolling();
      failures = 0;
      setBuffer([]);
      pending = true;
      waiting = awaitStream;
      state = initial();
      if (!waiting) load(true);
      changed();
    },
    retry() {
      if (stopped || state.status === "unavailable") return;
      if (state.status === "failed") state = { ...state, status: "loading", error: null };
      // Queued: the snapshot is read once the current stream has answered.
      if (waiting) {
        changed();
        return;
      }
      clearRetry();
      refetch();
      changed();
    },
    loadItem,
    loadPart,
    get view() {
      return view();
    },
    stop() {
      stopped = true;
      controller?.abort();
      controller = null;
      dropAll();
      clearRetry();
      stopPolling();
      setBuffer([]);
      budget.clear();
      state = { ...state, turns: [] };
    },
  };
}
