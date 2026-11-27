// A crew diff's content a part at a time (SUB decision 75c3ed93 §1–§4): a
// file's patch, or one side of a binary file, shown through #115's
// `createPager` and charged to the console's budget. A part is one gateway
// page, pinned to the snapshot's fingerprint; one shows at a time, with the
// window drawn from it, and both are charged.
//
// What a source learns of its pages at that fingerprint stays with it: each
// part's cursor and what starts there (a patch's line numbers, a binary
// side's offset). Pages at a fixed fingerprint are fixed, so when the budget
// frees a part it is read again at the same `at` when next opened, never
// from the start: once the part count is known, a fresh pager opens the
// part the person was on by its remembered cursor. Until the count is known
// the pager can reach a part only through the ones before it, so a part
// freed then is read again from the first.

import { createPager } from "./pager.js";

/**
 * @typedef {object} SourceRead
 * @property {number} part       the part asked for
 * @property {unknown} cursor    its page cursor; null for the first
 * @property {object} start      what is known of where it starts (`first` for part 0)
 * @property {AbortSignal} signal
 *
 * @typedef {object} SourceReply
 * @property {string} data       the part
 * @property {unknown} next      the next part's cursor, or null when this is the last
 * @property {object} after      what starts the next part
 */

/**
 * @param {object} options
 * @param {import("./budget.js").ViewBudget} options.budget  the diff view's scope of the console's budget
 * @param {string} options.key        the source's key: fingerprint, mode, path (and side)
 * @param {string} options.version    the fingerprint every page is pinned to
 * @param {number} options.partBytes  the most one part holds
 * @param {(data: string) => number} options.drawn  what a part's drawn window takes
 * @param {object} options.first      what starts part 0
 * @param {(request: SourceRead) => Promise<SourceReply>} options.read
 * @param {() => void} options.onChange
 */
export function createPartSource({ budget, key, version, partBytes, drawn, first, read, onChange }) {
  /** table[i]: part i's cursor and what starts it, once known. */
  const table = [{ cursor: null, start: first }];
  let count = null;
  let pager = null;
  /** The part on show, or last asked for: opened again after an eviction. */
  let part = 0;
  let evicted = false;
  /** Bumped by every change, so a panel can tell when to draw the source again. */
  let changes = 0;

  function make() {
    evicted = false;
    pager = createPager({
      budget,
      key,
      partBytes,
      drawn,
      known: count !== null ? { version, count, carried: 0 } : null,
      cursorAt: (index) => table[index]?.cursor,
      read: async ({ part: index, cursor, signal }) => {
        const reply = await read({ part: index, cursor, start: table[index].start, signal });
        if (reply.next === null || reply.next === undefined) count = index + 1;
        else table[index + 1] = { cursor: reply.next, start: reply.after };
        return { data: reply.data, next: reply.next ?? null, version };
      },
      onChange: () => {
        if (pager?.view.data !== null) part = pager.view.part;
        changes += 1;
        onChange();
      },
      onEvict: () => {
        evicted = true;
      },
    });
  }

  return {
    key,
    /** Show the part on show before, or part 0: read again at the same `at` when the budget freed it. */
    show() {
      budget.request(key);
      if (!pager || evicted) {
        make();
        return pager.open(count !== null ? part : 0);
      }
      // Reading or failed: what is on show stays, a failure with its Retry page. Ready: only touched.
      return pager.view.state === "ready" ? pager.open(pager.view.part) : Promise.resolve();
    },
    /** First, previous, next or last part, or an index; the part on show stays until the new one arrives. */
    move(to) {
      if (!pager || evicted || pager.view.data === null) return Promise.resolve();
      budget.request(key);
      return pager.open(to);
    },
    /** Retry page: the part that failed, again. */
    retry() {
      if (!pager || pager.view.state !== "failed") return Promise.resolve();
      budget.request(key);
      return pager.open(pager.view.want);
    },
    drop() {
      pager?.drop();
      pager = null;
    },
    get view() {
      if (!pager) return null;
      const view = pager.view;
      return { ...view, evicted, changes, start: table[view.part]?.start ?? first };
    },
  };
}

// ---------- a patch's lines ----------

/**
 * Where a stretch of patch text starts: the old and new line numbers of
 * its next line, and the kind of the line it starts inside, when a page or
 * window boundary split one (else null).
 *
 * @typedef {{old: number, new: number, open: string|null}} PatchStart
 * @type {Readonly<PatchStart>}
 */
export const PATCH_START = Object.freeze({ old: 0, new: 0, open: null });

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;
const lineKind = (line) => (HUNK.test(line) ? "hunk" : line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : line.startsWith("\\") ? "note" : "ctx");

/**
 * Lines of patch text with their old and new line numbers, from the hunk
 * headers, continuing from `from`. A line split by the boundary before the
 * text continues as its kind with no numbers (`more`), so the parts and
 * windows of one patch number as the whole would. `end` is where the next
 * stretch starts.
 */
export function patchLines(text, from = PATCH_START) {
  if (text === "") return { lines: [], end: from };
  const lines = text.split("\n");
  const ended = lines[lines.length - 1] === "";
  if (ended && lines.length > 1) lines.pop();
  let oldLine = from.old;
  let newLine = from.new;
  const out = lines.map((line, index) => {
    if (index === 0 && from.open) return { kind: from.open, text: line, old: null, new: null, more: true };
    const kind = lineKind(line);
    if (kind === "hunk") {
      const hunk = HUNK.exec(line);
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      return { kind, text: line, old: null, new: null };
    }
    if (kind === "add") return { kind, text: line, old: null, new: newLine++ };
    if (kind === "del") return { kind, text: line, old: oldLine++, new: null };
    if (kind === "note") return { kind, text: line, old: null, new: null };
    return { kind, text: line, old: oldLine++, new: newLine++ };
  });
  const last = out[out.length - 1];
  return { lines: out, end: { old: oldLine, new: newLine, open: ended ? null : last.kind } };
}

// ---------- a binary side's bytes ----------

/** The bytes base64 `data` decodes to, counted without decoding it. */
export const base64Length = (data) => (data.length / 4) * 3 - (data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0);

/**
 * `length` bytes from `start` of the bytes base64 `data` holds, decoding
 * only the groups that hold them: a window is drawn from the page without
 * decoding the whole page.
 */
export function bytesOf(data, start, length) {
  const from = Math.floor(start / 3) * 4;
  const to = Math.min(data.length, Math.ceil((start + length) / 3) * 4);
  const raw = atob(data.slice(from, to));
  const skip = start % 3;
  const bytes = new Uint8Array(Math.max(0, Math.min(length, raw.length - skip)));
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = raw.charCodeAt(skip + i);
  return bytes;
}
