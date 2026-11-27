// Content too large to hold, shown a part at a time (SUB decision a17870f4).
// A pager holds one part of one source (an activity item's pages, a crew
// diff's patch) and moves among its parts with First, Previous, Next and
// Last. Every part read is charged to the view's budget before it starts:
// the part on show and its drawn window, and the incoming part. Once the
// part arrives only it and its drawn window stay charged. The pager never
// decides what may pass the budget's limit; the budget does (its latest
// request), and a reservation it refuses evicts the pager. Every reply is
// fenced: a reply that arrives after the pager was
// evicted, dropped, restarted or sent elsewhere is discarded, so it can
// never append to or redirect a later read (the A3 race).
//
// Sources differ in how a part is addressed, so the pager knows parts only
// by index and by the opaque `cursor` its reader returned for the next one:
//   - a source paged by number (activity pages) also gives `cursorAt(part)`,
//     so any part, the last included, can be read directly;
//   - a source paged by cursor (a patch's `next`) reaches a part only
//     through the ones before it, and its page sizes may differ between
//     reads: after an eviction it is read again from the start, never from
//     a cursor kept from before.
// Nothing is cut: every part of every version stays reachable.

import { utf8Length } from "./utf8.js";

/**
 * The part index after a First, Previous, Next or Last move among `count`
 * parts from `at`; a number is taken as an index. Always within the parts.
 */
export function stepPart(to, at, count) {
  const last = Math.max(0, count - 1);
  const here = Math.min(Math.max(0, at), last);
  if (typeof to === "number" && Number.isInteger(to)) return Math.min(Math.max(0, to), last);
  return { first: 0, previous: Math.max(0, here - 1), next: Math.min(last, here + 1), last }[to] ?? here;
}

/**
 * Split `text` into windows of at most `maxBytes` UTF-8 bytes and at most
 * `maxLines` lines each, only between code points, so the windows join back
 * into `text` exactly. A view mounts one window at a time, so what it mounts
 * has both a byte and a line bound, however the content is shaped.
 */
export function splitBounded(text, { maxBytes, maxLines = Infinity }) {
  const out = [];
  let start = 0;
  let bytes = 0;
  let lines = 1;
  for (let i = 0; i < text.length; ) {
    const unit = text.charCodeAt(i);
    const pair = unit >= 0xd800 && unit <= 0xdbff && i + 1 < text.length && (text.charCodeAt(i + 1) & 0xfc00) === 0xdc00;
    const size = unit < 0x80 ? 1 : unit < 0x800 ? 2 : pair ? 4 : 3;
    if (i > start && (bytes + size > maxBytes || lines > maxLines)) {
      out.push(text.slice(start, i));
      start = i;
      bytes = 0;
      lines = 1;
    }
    bytes += size;
    // A newline ends its line: the next character starts another.
    if (unit === 0x0a) lines += 1;
    i += pair ? 2 : 1;
  }
  if (start < text.length || !out.length) out.push(text.slice(start));
  return out;
}

/** A reader's answer that a pager cannot use. */
export class PagerError extends Error {
  constructor(message) {
    super(message);
    this.name = "PagerError";
  }
}

/**
 * @typedef {object} PartRequest
 * @property {number} part        the index asked for
 * @property {unknown} cursor     null for the first part; else the `next` of the part before, or `cursorAt(part)`
 * @property {unknown} version    the version shown (null at first); the reader pins its pages to it
 * @property {AbortSignal} signal aborted when the reply is no longer wanted
 *
 * @typedef {object} PartReply
 * @property {string} data        the part's content, at most `partBytes` UTF-8 bytes
 * @property {unknown} next       the cursor of the next part, or null when this is the last
 * @property {unknown} [version]  the version the part belongs to
 * @property {number} [count]     the number of parts, when the source knows it
 * @property {boolean} [newer]    the version changed: this is part 0 of the newer one
 */

/**
 * @param {object} options
 * @param {import("./budget.js").ViewBudget} options.budget  the view's budget
 * @param {string} options.key            this pager's key in the budget
 * @param {(request: PartRequest) => Promise<PartReply>} options.read
 * @param {number} options.partBytes      the most one part holds; reserved before each read
 * @param {(data: string) => number} [options.drawn]  the bytes a part takes once drawn, charged beside
 *   the part (SUB decisions 6f5e67dd, 8357ec55): its drawn window, as shown. One more copy by default.
 * @param {((part: number, version: unknown) => unknown)|null} [options.cursorAt]  a cursor for any part, for sources paged by number
 * @param {() => void} [options.onChange] the pager's view changed
 * @param {() => void} [options.onEvict]  the budget freed it; it reads from the start when opened again
 * @param {{version: unknown, count: number, carried: number}|null} [options.known]  a version already known (its
 *   part count), so any of its parts can be opened first, and the bytes of a window still on show from
 *   before the pager (an item read whole), held beside the first read until a part arrives
 */
export function createPager({ budget, key, read, partBytes, drawn = (data) => utf8Length(data), cursorAt = null, onChange = () => {}, onEvict = () => {}, known = null }) {
  /** A part's charge once it shows: the part held and what it draws. */
  const charge = (part) => utf8Length(part) + drawn(part);
  /** Bumped by every read started, eviction, restart and drop: a reply from before is discarded. */
  let revision = 0;
  let controller = null;
  /** starts[i] is the cursor of part i, once known; part 0 starts at null. */
  let starts = [null];
  let count = known?.count ?? null;
  let version = known?.version ?? null;
  /** The window on show from before the pager, charged until its first part arrives. */
  let carried = known?.carried ?? 0;
  let part = 0;
  let want = 0;
  let data = null;
  let state = "idle";
  let error = null;
  let newer = false;
  /** Whether the read in flight is of the newest version's first part (`restart`). */
  let latest = false;

  function forget() {
    revision += 1;
    controller?.abort();
    controller = null;
    starts = [null];
    count = null;
    version = null;
    part = 0;
    want = 0;
    data = null;
    carried = 0;
  }

  function evicted() {
    budget.release(key);
    forget();
    state = "evicted";
    error = null;
    onEvict();
    onChange();
  }

  /** How many parts can be asked for from here. */
  const reach = () => (count !== null ? count : starts.length);
  const cursorFor = (index) => {
    if (index === 0) return null;
    if (starts[index] !== undefined) return starts[index];
    if (cursorAt && count !== null && version !== null) return cursorAt(index, version);
    return undefined;
  };

  /** Hold `bytes` for what shows, or, when the budget refuses, let it go as if evicted. */
  function hold(bytes) {
    if (budget.reserve(key, bytes, evicted)) return true;
    evicted();
    return false;
  }

  function fail(failure) {
    state = "failed";
    error = failure;
    // The part already shown stays, charged as it is, with its drawn window.
    if (data !== null) hold(charge(data));
    else if (carried > 0) hold(carried);
    else budget.release(key);
  }

  function accept(reply, index, cursor, fromLatest) {
    if (!reply || typeof reply.data !== "string") throw new PagerError("The part read has no content.");
    const size = utf8Length(reply.data);
    if (size > partBytes) throw new PagerError(`The part read holds ${size} bytes, more than ${partBytes}.`);
    const fresh = reply.newer === true;
    const at = fresh ? 0 : index;
    // A part that is not the last must hold something and move on: else Next would read it forever.
    if (reply.next !== null && reply.next !== undefined && (size === 0 || (!fresh && reply.next === cursor))) {
      throw new PagerError("The part read does not advance.");
    }
    if (fromLatest) {
      // The newest version, read unpinned: nothing known of the one shown before applies.
      starts = [null];
      count = null;
      version = null;
      newer = false;
    } else if (fresh) {
      starts = [null];
      count = null;
      newer = true;
    }
    if (reply.version !== undefined) version = reply.version;
    if (Number.isInteger(reply.count) && reply.count >= 1) count = reply.count;
    if (reply.next === null || reply.next === undefined) {
      count = at + 1;
      starts.length = Math.min(starts.length, at + 1);
    } else starts[at + 1] = reply.next;
    part = at;
    want = at;
    data = reply.data;
    carried = 0;
    state = "ready";
    error = null;
    // The part on show before it is let go: only this part and its window stay charged.
    hold(charge(data));
  }

  /**
   * Show part `to` (first, previous, next, last or an index). The current
   * part stays on show until the new one arrives. Resolves once the read
   * settles, or at once when there is nothing to read. An evicted pager
   * starts again from its first part. `fromLatest` reads the first part of
   * the newest version, unpinned, even when part 0 is on show (`restart`).
   */
  async function open(to = "first", { fromLatest = false } = {}) {
    if (state === "dropped") return;
    if (state === "evicted") to = 0;
    const index = fromLatest ? 0 : stepPart(to, state === "idle" ? 0 : part, reach());
    budget.touch(key);
    if (!fromLatest && state === "ready" && index === part && data !== null) return;
    const cursor = fromLatest ? null : cursorFor(index);
    if (cursor === undefined) return;
    revision += 1;
    const rev = revision;
    controller?.abort();
    const own = new AbortController();
    controller = own;
    // Room for the part about to be read, beside the one on show and its
    // drawn window, before the read starts. Its own window is charged once
    // it arrives, as the outgoing part goes. The budget decides whether this
    // may pass its limit (its latest request); when it refuses, the pager
    // lets go of what it held and reads from the start when opened again.
    if (!budget.reserve(key, (data === null ? carried : charge(data)) + partBytes, evicted)) {
      evicted();
      return;
    }
    want = index;
    state = "loading";
    latest = fromLatest;
    error = null;
    onChange();
    let reply = null;
    let failure = null;
    try {
      reply = await read({ part: index, cursor, version: fromLatest ? null : version, signal: own.signal });
    } catch (caught) {
      failure = caught;
    }
    // Fenced: evicted, dropped, restarted or another read started meanwhile.
    if (rev !== revision) return;
    controller = null;
    if (failure === null) {
      try {
        accept(reply, index, cursor, fromLatest);
      } catch (caught) {
        failure = caught;
      }
    }
    if (failure !== null) fail(failure);
    onChange();
  }

  return {
    open,
    /**
     * Read the first part of the newest version (Load the latest), beside
     * the part on show: it stays on show and charged, with its drawn window,
     * until the new part arrives, and stays with the error when the read
     * fails, so it can be tried again (SUB decision 533c2b29). A pager of a
     * source that is gone (another stream generation) is dropped instead.
     */
    restart() {
      if (state === "dropped") return Promise.resolve();
      return open("first", { fromLatest: true });
    },
    /**
     * The window on show from before the pager is gone (its body
     * unmounted): stop charging it (SUB decision cf21d510). A read in
     * flight goes on, charged for its part alone; with nothing in flight
     * or on show, nothing stays held.
     */
    dropCarried() {
      if (carried === 0 || data !== null) return;
      carried = 0;
      if (state === "loading") budget.reserve(key, partBytes, evicted, { makeRoom: false });
      else if (state !== "evicted" && state !== "dropped") budget.release(key);
    },
    /** Stop: abort any read, discard its reply and release what is held. */
    drop() {
      forget();
      state = "dropped";
      budget.release(key);
    },
    get view() {
      const reachable = reach();
      return {
        state,
        part,
        want,
        count,
        known: reachable,
        data,
        /** The part's UTF-8 length, or 0 when none is on show. */
        size: data === null ? 0 : utf8Length(data),
        version,
        newer,
        /** Reading the newest version's first part, the part on show kept meanwhile. */
        reloading: state === "loading" && latest,
        error,
        hasPrevious: data !== null && part > 0,
        hasNext: data !== null && part + 1 < reachable,
        hasLast: data !== null && count !== null && part < count - 1 && cursorFor(count - 1) !== undefined,
      };
    },
  };
}
