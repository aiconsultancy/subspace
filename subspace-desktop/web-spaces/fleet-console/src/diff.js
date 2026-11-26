// Crew worktree diffs: what a crew member has changed in one project's
// checkout, from `GET /api/v1/ships/{id}/crew/{member}/diff` and its file
// route (STAR-5, SUB-DOC-1 §4). Nothing is redacted or cut: the file list is
// read page by page until every file is listed, a patch shows a page at a
// time with First, Previous, Next and Last (`diff-parts.js`), and a binary
// file's bytes are read page by page for the hex view. Content lives in
// memory only, for as long as the view is open, charged to the console's
// budget (SUB decision 75c3ed93); it is never stored, logged or shared with
// chat.
//
// One snapshot at a time. Every read of a snapshot (its list pages, patch
// pages and binary pages) is pinned to its fingerprint and shares one
// cancellation token, so Refresh, a mode change, closing the view, a source
// change or a stale fingerprint cancels whatever is still in flight or waiting
// to retry, and no late answer reaches the view.
//
// Busy answers (503) retry the same request up to twice, waiting the
// gateway's Retry-After (one second when it sends none). A wait over ten
// seconds, or a third 503, stops and offers Refresh (or Retry page).

import { DEFAULT_ROOM_BYTES, HttpError, InvalidResponseError } from "./api.js";
import { createBudget } from "./budget.js";
import { base64Length, createPartSource, PATCH_START, patchLines } from "./diff-parts.js";
import { shownBytes, WINDOW_BYTES } from "./document.js";

/** The gateway feature that says it serves the diff routes. */
export const DIFF_FEATURE = "api.crewDiffs";

/** The modes, as the PRs Space names them. */
export const DIFF_MODES = [
  { mode: "branch", label: "All changes" },
  { mode: "committed", label: "Committed only" },
  { mode: "working", label: "Uncommitted only" },
];

/** Attempts of one request while the gateway answers 503. */
export const BUSY_ATTEMPTS = 3;
/** The longest Retry-After waited for; longer offers Refresh instead. */
export const BUSY_MAX_WAIT_MS = 10_000;
export const BUSY_DEFAULT_WAIT_MS = 1_000;

/** A binary page holds at most 1 MiB of raw bytes (STAR-5). */
export const BINARY_PAGE_BYTES = 1024 * 1024;
/** A patch page holds at most 1 MiB (STAR-5): a patch part is one page. */
export const PATCH_PART_BYTES = 1024 * 1024;
/** The hex view shows this many bytes at a time, 8 to a row. */
export const HEX_WINDOW_BYTES = 4096;
export const HEX_ROW_BYTES = 8;
/** Image previews, as desktop #110: up to 10 MiB in at most 16 pages, at most 4,096 px a side. */
export const PREVIEW_MAX_BYTES = 10 * 1024 * 1024;
export const PREVIEW_MAX_PAGES = 16;
export const PREVIEW_MAX_PIXELS = 4096;

export const DIFF_MESSAGES = {
  unavailable: "Diff not available. The gateway may be older than crew diffs, or this crew member has no checkout for this project.",
  missing: "This file is no longer available. Refresh the diff.",
  stale: "The checkout changed while loading. Refresh to try again.",
  malformed: "Stargate refused the request as malformed (400).",
  failed: "Stargate could not compute the diff: Git or the crew plugin failed (500).",
  unstable: "Changed while loading",
  changed: "Changed since loaded",
};

const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const isStr = (v) => typeof v === "string";
const isCount = (v) => Number.isInteger(v) && v >= 0;
const optional = (check) => (v) => v === undefined || check(v);
const orNull = (check) => (v) => v === null || check(v);

/** Whether a fleet's gateway serves crew diffs: the feature is listed and on. */
export function diffsEnabled(fleet) {
  return (fleet?.gateway?.features ?? []).some((feature) => feature?.key === DIFF_FEATURE && feature.enabled === true);
}

/** Whether a project row offers View diff: diffs are on and its Git state was read without an error. */
export function canViewDiff(enabled, context) {
  return Boolean(enabled) && isObj(context?.git) && context.git.error === undefined;
}

// ---------- cancellation ----------

export class CancelledError extends Error {
  constructor() {
    super("The diff read was cancelled.");
    this.name = "CancelledError";
  }
}

class StaleError extends Error {
  constructor() {
    super(DIFF_MESSAGES.stale);
    this.name = "StaleError";
  }
}

/**
 * A cancellation token: its signal aborts the fetch, its waits end at once,
 * and cancelling it cancels its children.
 */
export function createRead({ setTimer, clearTimer }, parent = null) {
  const controller = typeof AbortController === "function" ? new AbortController() : null;
  const waits = new Set();
  const children = new Set();
  let cancelled = false;
  const read = {
    signal: controller?.signal,
    get cancelled() {
      return cancelled;
    },
    check() {
      if (cancelled) throw new CancelledError();
    },
    /** Wait `ms`; rejects with CancelledError the moment the token is cancelled. */
    wait(ms) {
      return new Promise((resolve, reject) => {
        if (cancelled) {
          reject(new CancelledError());
          return;
        }
        const entry = { reject };
        entry.timer = setTimer(() => {
          waits.delete(entry);
          resolve();
        }, ms);
        waits.add(entry);
      });
    },
    child() {
      const child = createRead({ setTimer, clearTimer }, read);
      if (cancelled) child.cancel();
      else children.add(child);
      return child;
    },
    cancel() {
      if (cancelled) return;
      cancelled = true;
      controller?.abort();
      for (const entry of waits) {
        clearTimer(entry.timer);
        entry.reject(new CancelledError());
      }
      waits.clear();
      for (const child of children) child.cancel();
      children.clear();
      parent?.forget(read);
    },
    forget(child) {
      children.delete(child);
    },
  };
  return read;
}

/**
 * Run `request(signal)`, retrying the identical request while the gateway
 * answers 503: never a newer cursor or fingerprint.
 */
export async function retryBusy(read, request) {
  for (let attempt = 1; ; attempt += 1) {
    read.check();
    try {
      const answer = await request(read.signal);
      // A client that ignores the signal still answers late: drop it.
      read.check();
      return answer;
    } catch (error) {
      if (read.cancelled) throw new CancelledError();
      if (!(error instanceof HttpError) || error.status !== 503 || attempt >= BUSY_ATTEMPTS) throw error;
      const wait = error.retryAfterMs > 0 ? error.retryAfterMs : BUSY_DEFAULT_WAIT_MS;
      if (wait > BUSY_MAX_WAIT_MS) throw error;
      await read.wait(wait);
    }
  }
}

/**
 * What a failed read means. `first` is a read of the list's first page, where
 * 404 means no diff here; anywhere else in a snapshot 404 means the snapshot
 * moved on (a list page) or the file went (a file page). `action` names the
 * control that tries again.
 */
export function diffError(error, { first = false, file = false, action = "Refresh" } = {}) {
  if (error instanceof CancelledError || error?.name === "AbortError") return { kind: "cancelled", message: "" };
  if (error instanceof StaleError) return { kind: "stale", message: DIFF_MESSAGES.stale };
  if (error instanceof InvalidResponseError) return { kind: "invalid", message: `Stargate sent a response this console cannot read: ${error.message}` };
  if (error instanceof HttpError) {
    switch (error.status) {
      case 400:
        return { kind: "malformed", message: DIFF_MESSAGES.malformed };
      case 404:
        if (first) return { kind: "unavailable", message: DIFF_MESSAGES.unavailable };
        return file ? { kind: "missing", message: DIFF_MESSAGES.missing } : { kind: "stale", message: DIFF_MESSAGES.stale };
      case 409:
        return { kind: "stale", message: DIFF_MESSAGES.stale };
      case 503: {
        const seconds = Math.ceil((error.retryAfterMs || 0) / 1000);
        const message = seconds > 0 ? `Stargate is busy. Wait ${seconds} second${seconds === 1 ? "" : "s"}, then select ${action}.` : `Stargate is busy. Select ${action} to try again.`;
        return { kind: "busy", message };
      }
      case 500:
        return { kind: "failed", message: DIFF_MESSAGES.failed };
      default:
        return { kind: "failed", message: `Stargate answered ${error.status}.` };
    }
  }
  return { kind: "network", message: `Could not reach Stargate: ${error?.message ?? String(error)}` };
}

// ---------- wire shapes ----------

const DIFF_STATUSES = ["added", "removed", "modified", "renamed"];
const FILE_KINDS = ["submodule", "symlink"];

/** A `V1DiffFile`. An unknown status is kept and shown as written, never dropped (SUB-70). */
export function isDiffFile(f) {
  return (
    isObj(f) &&
    isStr(f.path) &&
    optional(orNull(isStr))(f.previousPath) &&
    isStr(f.status) &&
    orNull(isCount)(f.additions ?? null) &&
    orNull(isCount)(f.deletions ?? null) &&
    optional((v) => typeof v === "boolean")(f.untracked) &&
    optional((v) => typeof v === "boolean")(f.binary) &&
    optional(isCount)(f.size) &&
    orNull(isStr)(f.patch ?? null) &&
    optional(isStr)(f.patchNextPage) &&
    optional(isStr)(f.kind)
  );
}

export const knownStatus = (status) => DIFF_STATUSES.includes(status);
export const knownKind = (kind) => kind === undefined || FILE_KINDS.includes(kind);

const isRef = (v) => v === null || (isObj(v) && orNull(isStr)(v.ref ?? null) && orNull(isStr)(v.commit ?? null));

function checkDiffPage(page, target, mode) {
  const ok =
    isObj(page) &&
    isStr(page.fingerprint) &&
    page.ship === target.ship &&
    page.member === target.member &&
    page.mode === mode &&
    orNull(isStr)(page.project ?? null) &&
    orNull(isStr)(page.branch ?? null) &&
    isRef(page.base ?? null) &&
    orNull(isStr)(page.head ?? null) &&
    isCount(page.total) &&
    Array.isArray(page.files) &&
    optional(isStr)(page.nextCursor ?? undefined);
  if (!ok) throw new InvalidResponseError("A page of the file list is not a valid diff.");
  if (!page.files.every(isDiffFile)) throw new InvalidResponseError("A file in the list is not a valid diff file.");
}

const sameCheckout = (a, b) => a.ship === b.ship && a.member === b.member && (a.project ?? null) === (b.project ?? null) && a.mode === b.mode && a.total === b.total;

/**
 * Read every page of the file list for one snapshot. Each page must carry the
 * first page's fingerprint (else the snapshot moved on: stale) and checkout,
 * add files, and never repeat a cursor; the pages must list exactly `total`
 * files. Nothing is cut: it returns the whole list or throws.
 */
export async function loadDiffList(client, target, mode, read) {
  const query = (cursor) => ({ project: target.project, mode, cursor });
  let first;
  try {
    first = await retryBusy(read, (signal) => client.diff(target.ship, target.member, query(undefined), signal));
  } catch (error) {
    throw tagFirst(error);
  }
  checkDiffPage(first, target, mode);
  const files = [...first.files];
  let unstable = first.unstable === true;
  let cursor = first.nextCursor ?? null;
  const seen = new Set();
  while (cursor !== null) {
    read.check();
    if (files.length >= first.total || seen.has(cursor)) throw new InvalidResponseError("The file list repeats a page or runs past its total.");
    seen.add(cursor);
    // A 404 here means the snapshot moved on: diffError reads it as stale.
    const page = await retryBusy(read, (signal) => client.diff(target.ship, target.member, query(cursor), signal));
    checkDiffPage(page, target, mode);
    if (page.fingerprint !== first.fingerprint) throw new StaleError();
    if (!page.files.length || !sameCheckout(first, page)) throw new InvalidResponseError("A later page of the file list is for another checkout or is empty.");
    files.push(...page.files);
    unstable ||= page.unstable === true;
    cursor = page.nextCursor ?? null;
  }
  if (files.length !== first.total) throw new InvalidResponseError(`The file list has ${files.length} files, not ${first.total}.`);
  const { nextCursor: _, files: __, ...rest } = first;
  return { ...rest, unstable, files };
}

/** Marks an error from the list's first page, where 404 means no diff here. */
function tagFirst(error) {
  if (error instanceof HttpError) error.firstPage = true;
  return error;
}

/** A patch continuation: pinned to `at`, for `path`, never pointing back at itself. */
export function validatePatchPage(answer, { path, page, at }) {
  if (!isObj(answer) || !isStr(answer.fingerprint) || !isDiffFile(answer.file)) throw new InvalidResponseError("The file page is not a valid answer.");
  if (answer.fingerprint !== at) throw new StaleError();
  if (answer.file.path !== path) throw new InvalidResponseError("The file page is for another path.");
  if (!isStr(answer.file.patch)) throw new InvalidResponseError("The file page has no patch text.");
  if (answer.file.patchNextPage !== undefined && answer.file.patchNextPage === page) throw new InvalidResponseError("The file page points back at itself.");
  return { patch: answer.file.patch, next: answer.file.patchNextPage ?? null };
}

const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
/** The longest base64 a binary page may hold: 1 MiB decoded. A binary part is held as this text. */
export const BINARY_PART_BYTES = Math.ceil(BINARY_PAGE_BYTES / 3) * 4;

/** Check `data` is base64 of at most 1 MiB, without decoding it; returns the bytes it decodes to. */
function checkBase64(data) {
  if (data.length > BINARY_PART_BYTES || !BASE64.test(data)) throw new InvalidResponseError("A binary page is not valid base64 of at most 1 MiB.");
  return base64Length(data);
}

export function decodeBase64(data) {
  checkBase64(data);
  const raw = atob(data);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

/**
 * A binary page: pinned to `at`, for `path` and `side`, starting where the
 * last page ended, inside the file's size. A page that ends at the size is
 * the last one, whatever else it says; one that ends before it must name the
 * next page. An empty file is one page of size 0 with no data. Its bytes
 * stay base64 (`data`, `length` decoded): the hex view decodes one window
 * at a time.
 */
export function validateContent(answer, { path, at, side, page, offset, size }) {
  if (!isObj(answer) || !isStr(answer.fingerprint) || !isObj(answer.file)) throw new InvalidResponseError("The file page is not a valid answer.");
  if (answer.fingerprint !== at) throw new StaleError();
  if (answer.file.path !== path) throw new InvalidResponseError("The file page is for another path.");
  const content = answer.content;
  const ok =
    isObj(content) &&
    content.side === side &&
    content.encoding === "base64" &&
    isCount(content.offset) &&
    isCount(content.size) &&
    optional(isStr)(content.data) &&
    optional(isStr)(content.nextPage ?? undefined);
  if (!ok) throw new InvalidResponseError("The binary page is not valid content for this side.");
  if (content.offset !== offset) throw new InvalidResponseError(`The binary page starts at ${content.offset}, not ${offset}.`);
  if (size !== null && content.size !== size) throw new InvalidResponseError("The binary page gives another size for the same file.");
  const data = content.data ?? "";
  const length = checkBase64(data);
  const end = content.offset + length;
  if (end > content.size) throw new InvalidResponseError("The binary page runs past the file's size.");
  if (end < content.size) {
    if (!isStr(content.nextPage)) throw new InvalidResponseError("The binary page ends before the file does but names no next page.");
    if (!length) throw new InvalidResponseError("The binary page is empty but the file goes on.");
    if (content.nextPage === page) throw new InvalidResponseError("The binary page points back at itself.");
  }
  return { offset: content.offset, size: content.size, data, length, nextPage: end < content.size ? content.nextPage : null };
}

const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp)$/i;
/** Whether #110 would offer Preview image for this path. */
export const previewable = (path) => IMAGE_EXTENSION.test(path);

/** The image type of `bytes` from its signature, or null. */
export function imageType(bytes) {
  const at = (i, ...values) => values.every((value, k) => bytes[i + k] === value);
  if (at(0, 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)) return "image/png";
  if (at(0, 0xff, 0xd8, 0xff)) return "image/jpeg";
  if (at(0, 0x47, 0x49, 0x46, 0x38)) return "image/gif";
  if (at(0, 0x52, 0x49, 0x46, 0x46) && at(8, 0x57, 0x45, 0x42, 0x50)) return "image/webp";
  return null;
}

/** The sides a binary file has: an added file only its new side, a removed one only its old. */
export function binarySides(file) {
  if (file.status === "added") return ["new"];
  if (file.status === "removed") return ["old"];
  return ["old", "new"];
}

/** The text a hex window of `bytes` bytes draws: a row of 8 is 16 + 2 + 23 + 2 + 8 characters and a newline. */
export const hexShownBytes = (bytes) => Math.ceil(bytes / HEX_ROW_BYTES) * 52;

/** Rows of a hex and ASCII dump: a 16-digit offset, 8 bytes in hex, and the bytes as ASCII with `.` for the rest. */
export function hexRows(bytes, offset) {
  const rows = [];
  for (let i = 0; i < bytes.length; i += HEX_ROW_BYTES) {
    const row = bytes.subarray(i, i + HEX_ROW_BYTES);
    const hex = [...row].map((byte) => byte.toString(16).padStart(2, "0")).join(" ").padEnd(HEX_ROW_BYTES * 3 - 1, " ");
    const ascii = [...row].map((byte) => (byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : ".")).join("");
    rows.push(`${(offset + i).toString(16).padStart(16, "0")}  ${hex}  ${ascii}`);
  }
  return rows;
}

// ---------- the watcher ----------

/** A timer that never keeps a process alive on its own (a no-op in the page). */
function defaultTimer(callback, ms) {
  const timer = setTimeout(callback, ms);
  timer?.unref?.();
  return timer;
}

/**
 * Follow one crew member's diff for one project. `client` has `diff` and
 * `diffFile` as in api.js; `target` is {ship, member, project}. `observe`
 * takes the fingerprint the `ship` event carries for this checkout. `budget`
 * is the diff view's scope of the console's budget (SUB decision 75c3ed93
 * §3); without one the view has a budget of its own.
 */
export function createDiffWatcher({ client, target, mode: startMode = "branch", onChange, budget = createBudget(DEFAULT_ROOM_BYTES), setTimer = defaultTimer, clearTimer = clearTimeout }) {
  const timers = { setTimer, clearTimer };
  let mode = startMode;
  let status = "loading";
  let error = null;
  let diff = null;
  let files = [];
  let unstable = false;
  let stalePage = false;
  let busy = null;
  let binary = null;
  let preview = null;
  let revision = 0;
  let generation = 0;
  let read = null;
  let stopped = false;
  /** The parts read at this snapshot, one source per file patch: key -> createPartSource. */
  const sources = new Map();
  // The fingerprint banner, as desktop #110's FleetState: a repeat of the
  // value already seen proves nothing, nor does a missing one.
  let fingerprint = null;
  let live = null;
  let liveRevision = 0;
  let liveChanged = false;

  const changed = () => stalePage || liveChanged;
  const emit = () => {
    revision += 1;
    if (!stopped) onChange?.();
  };
  const current = (gen) => !stopped && gen === generation;

  /** Stop every read of this snapshot: it can no longer be read at its fingerprint. */
  function cancelSnapshotReads() {
    read?.cancel();
  }

  /** Let go of every part read at this snapshot, and what it held of the budget. */
  function dropSources() {
    for (const source of sources.values()) source.drop();
    sources.clear();
  }

  /** A file of the list: its inline patch is its first part, `first` the cursor of the page after it. */
  function entryFor(file, index) {
    const { patch, patchNextPage, ...rest } = file;
    return { index, file: { ...rest, previousPath: file.previousPath ?? null }, inline: patch ?? null, first: patchNextPage ?? null };
  }

  async function load() {
    cancelSnapshotReads();
    dropSources();
    generation += 1;
    const gen = generation;
    read = createRead(timers);
    const mine = read;
    const baseline = liveRevision;
    status = "loading";
    error = null;
    files = [];
    unstable = false;
    stalePage = false;
    liveChanged = false;
    busy = null;
    binary?.source.drop();
    binary = null;
    if (preview) budget.release(preview.key);
    preview = null;
    emit();
    try {
      const loaded = await loadDiffList(client, target, mode, mine);
      if (!current(gen)) return;
      diff = loaded;
      unstable = loaded.unstable;
      files = loaded.files.map(entryFor);
      status = "ready";
      liveChanged = liveRevision !== baseline && live !== null && live !== loaded.fingerprint;
      fingerprint = loaded.fingerprint;
      if (liveChanged) cancelSnapshotReads();
    } catch (failure) {
      if (!current(gen)) return;
      const problem = diffError(failure, { first: failure?.firstPage === true });
      if (problem.kind === "cancelled") return;
      diff = null;
      status = problem.kind === "unavailable" ? "unavailable" : "failed";
      error = problem;
      if (problem.kind === "busy") busy = problem.message;
    }
    emit();
  }

  // ----- parts: a file page at a time, pinned to the snapshot's fingerprint -----

  /** Read one file page at the snapshot's fingerprint, under the snapshot's token and the pager's signal. */
  async function readFile(snapshot, at, query, signal) {
    const token = snapshot.child();
    const abort = () => token.cancel();
    if (signal?.aborted) abort();
    signal?.addEventListener("abort", abort, { once: true });
    try {
      return await retryBusy(token, (inner) => client.diffFile(target.ship, target.member, { project: target.project, mode, ...query, at }, inner));
    } finally {
      signal?.removeEventListener("abort", abort);
      token.cancel();
    }
  }

  /**
   * Run one part read and say what its end means for the snapshot: a stale
   * `at` (409, another fingerprint) cancels the snapshot's reads and shows
   * the Refresh banner; a busy gateway says so. The pager keeps the part on
   * show, and offers Retry page for any other failure.
   */
  async function partRead(gen, body) {
    try {
      const reply = await body();
      if (current(gen)) busy = null;
      return reply;
    } catch (failure) {
      const problem = diffError(failure, { file: true, action: "Retry page" });
      if (current(gen) && problem.kind === "stale") {
        stalePage = true;
        cancelSnapshotReads();
      } else if (current(gen) && problem.kind === "busy") busy = problem.message;
      throw failure;
    }
  }

  const patchKey = (entry) => ["patch", fingerprint, mode, entry.file.path].join("\u0000");
  const hasPatch = (entry) => Boolean(entry) && !entry.file.binary && (entry.inline !== null || entry.first !== null);

  /**
   * The patch of file `index` as parts (SUB decision 75c3ed93 §1): its
   * inline patch, then each page `patchNextPage` names, read at the
   * snapshot's fingerprint. A patch with no next page is one part.
   */
  function patchSource(index) {
    const entry = files[index];
    if (status !== "ready" || !hasPatch(entry)) return null;
    const key = patchKey(entry);
    if (sources.has(key)) return sources.get(key);
    const gen = generation;
    const at = fingerprint;
    const snapshot = read;
    const path = entry.file.path;
    const source = createPartSource({
      budget,
      key,
      version: at,
      partBytes: PATCH_PART_BYTES,
      // A part draws one window at a time: at most WINDOW_BYTES as shown, as an Activity part.
      drawn: (data) => Math.min(WINDOW_BYTES, shownBytes(data)),
      first: PATCH_START,
      onChange: emit,
      read: ({ part, cursor, start, signal }) =>
        partRead(gen, async () => {
          let text = entry.inline;
          let next = entry.first;
          if (part > 0 || text === null) {
            const page = cursor ?? entry.first;
            const answer = await readFile(snapshot, at, { path, page }, signal);
            ({ patch: text, next } = validatePatchPage(answer, { path, page, at }));
          }
          return { data: text, next, after: patchLines(text, start).end };
        }),
    });
    sources.set(key, source);
    return source;
  }

  // ----- binary bytes: one hex view or one preview at a time -----

  function closeBinary() {
    binary?.source.drop();
    preview?.read.cancel();
    if (preview) budget.release(preview.key);
    const had = binary || preview;
    binary = null;
    preview = null;
    if (had) emit();
  }

  /**
   * One side of a binary file as parts (SUB decision 75c3ed93 §2): each a
   * gateway page of at most 1 MiB decoded, held as its base64, following the
   * gateway's cursors and pinned to the snapshot's fingerprint. Each part
   * knows the offset it starts at; `onSize` hears the file's size.
   */
  function binarySource(entry, side, onChange, onSize) {
    const gen = generation;
    const at = fingerprint;
    const snapshot = read;
    const path = entry.file.path;
    let size = null;
    return createPartSource({
      budget,
      key: ["binary", at, mode, path, side].join("\u0000"),
      version: at,
      partBytes: BINARY_PART_BYTES,
      // A part draws one 4 KiB hex window at a time.
      drawn: (data) => hexShownBytes(Math.min(HEX_WINDOW_BYTES, base64Length(data))),
      first: { offset: 0 },
      onChange,
      read: ({ cursor, start, signal }) =>
        partRead(gen, async () => {
          const answer = await readFile(snapshot, at, { path, page: cursor ?? undefined, side }, signal);
          const content = validateContent(answer, { path, at, side, page: cursor, offset: start.offset, size });
          size = content.size;
          onSize(size);
          return { data: content.data, next: content.nextPage, after: { offset: start.offset + content.length } };
        }),
    });
  }

  /** The hex view's windows in the part on show: 4 KiB each, at least one. */
  const windowsIn = (data) => Math.max(1, Math.ceil(base64Length(data ?? "") / HEX_WINDOW_BYTES));

  function openBinary(index, side) {
    const entry = files[index];
    if (status !== "ready" || !entry?.file.binary || !binarySides(entry.file).includes(side) || changed()) return;
    // Bytes the budget let go are read again where they were, at the same snapshot.
    if (binary?.index === index && binary.side === side && binary.source.view?.evicted) return binary.source.show();
    closeBinary();
    const view = { index, path: entry.file.path, side, window: 0, land: "first", part: null, moves: 0, size: null, source: null };
    // A part arrives: land on its first window, or its last after Previous or Last.
    view.source = binarySource(entry, side, () => {
      const shown = view.source?.view;
      if (shown?.data !== null && shown?.data !== undefined && shown.part !== view.part) {
        view.window = view.land === "last" ? windowsIn(shown.data) - 1 : 0;
        view.part = shown.part;
      }
      emit();
    }, (size) => (view.size = size));
    binary = view;
    return view.source.show();
  }

  /**
   * Move the hex view a window: First, Previous, Next or Last. A move past
   * the part's first or last window reads the part before or after; Last
   * only where its page can be read without the pages between.
   */
  function moveBinary(to) {
    const view = binary;
    const shown = view?.source.view;
    if (!shown || shown.data === null || shown.state === "loading") return;
    const windows = windowsIn(shown.data);
    const step = (window) => {
      view.window = window;
      view.moves += 1;
      emit();
    };
    const turn = (part, land) => {
      view.land = land;
      return view.source.move(part);
    };
    if (to === "first") return shown.part === 0 ? step(0) : turn("first", "first");
    if (to === "previous") return view.window > 0 ? step(view.window - 1) : shown.hasPrevious ? turn("previous", "last") : undefined;
    if (to === "next") return view.window < windows - 1 ? step(view.window + 1) : shown.hasNext ? turn("next", "first") : undefined;
    if (to === "last" && shown.count !== null) return shown.part === shown.count - 1 ? step(windows - 1) : shown.hasLast ? turn("last", "last") : undefined;
  }

  function retryBinary() {
    return binary?.source.retry();
  }

  /** The open hex view as the panel draws it. */
  function binaryView() {
    if (!binary) return null;
    const shown = binary.source.view;
    const { index, path, side, window, moves, size } = binary;
    return { index, path, side, window, size, windows: windowsIn(shown?.data), part: shown, offset: shown?.start.offset ?? 0, state: shown?.state ?? "idle", version: `${shown?.changes}.${moves}` };
  }

  /**
   * Assemble one side of an image in memory for a still preview, as #110: a
   * whole read (SUB decisions feb0d839, 75c3ed93 §3). Once the first page
   * gives its size, it is charged to the budget, but only in room no other
   * read holds: it frees nothing, and gives its room back before anything
   * else when a later read needs it. If its size does not fit, the view
   * says so; Show bytes still pages through it.
   */
  async function openPreview(index, side) {
    const entry = files[index];
    if (status !== "ready" || !entry?.file.binary || !binarySides(entry.file).includes(side) || changed()) return;
    closeBinary();
    const key = ["preview", fingerprint, mode, entry.file.path, side].join("\u0000");
    const view = { index, path: entry.file.path, side, key, state: "loading", error: null, type: null, bytes: null, size: null, room: null, read: read.child(), version: 0 };
    // The room given back to a later read: stop reading, let the bytes go.
    const giveBack = () => {
      view.read.cancel();
      view.bytes = null;
      view.state = "given-back";
      view.version += 1;
      emit();
    };
    preview = view;
    const token = view.read;
    const gen = generation;
    emit();
    try {
      const parts = [];
      let cursor;
      let offset = 0;
      let size = null;
      for (let number = 0; ; number += 1) {
        const answer = await retryBusy(token, (signal) =>
          client.diffFile(target.ship, target.member, { project: target.project, mode, path: view.path, page: cursor, at: fingerprint, side }, signal),
        );
        const content = validateContent(answer, { path: view.path, at: fingerprint, side, page: cursor ?? null, offset, size });
        token.check();
        size = content.size;
        if (size > PREVIEW_MAX_BYTES || Math.ceil(size / BINARY_PAGE_BYTES) > PREVIEW_MAX_PAGES) {
          view.state = "too-large";
          break;
        }
        if (number === 0) {
          const room = budget.limit - budget.used;
          if (size > room) {
            Object.assign(view, { state: "no-room", size, room });
            break;
          }
          budget.reserve(key, size, giveBack, { makeRoom: false, yields: true });
        }
        const bytes = decodeBase64(content.data);
        if (number === 0 && !imageType(bytes)) {
          view.state = "unsupported";
          budget.release(key);
          break;
        }
        parts.push(bytes);
        offset += bytes.length;
        if (!content.nextPage) {
          const bytes = new Uint8Array(offset);
          let at = 0;
          for (const part of parts) {
            bytes.set(part, at);
            at += part.length;
          }
          view.bytes = bytes;
          view.type = imageType(bytes);
          view.state = "ready";
          break;
        }
        cursor = content.nextPage;
      }
    } catch (failure) {
      if (!current(gen) || preview !== view || view.state === "given-back") return;
      budget.release(key);
      const problem = diffError(failure, { file: true, action: "Retry page" });
      if (problem.kind === "cancelled") {
        preview = null;
      } else if (problem.kind === "stale") {
        preview = null;
        stalePage = true;
        cancelSnapshotReads();
      } else {
        view.state = "failed";
        view.error = problem;
        if (problem.kind === "busy") busy = problem.message;
      }
    }
    if (!current(gen)) return;
    view.version += 1;
    emit();
  }

  return {
    /** A read-only picture of the view for the panel. */
    get view() {
      return {
        status,
        mode,
        target,
        error,
        diff,
        files,
        unstable,
        changed: changed(),
        banner: unstable ? DIFF_MESSAGES.unstable : changed() ? DIFF_MESSAGES.changed : null,
        busy,
        binary: binaryView(),
        preview,
        revision,
        budget,
        /** File `index`'s patch as parts: the pager's view and where its part starts, or null before it is opened. */
        patch: (index) => (status === "ready" && hasPatch(files[index]) ? (sources.get(patchKey(files[index]))?.view ?? null) : null),
      };
    },
    start() {
      load();
    },
    /** Read the snapshot again from the start; everything in flight is cancelled. */
    refresh() {
      if (!stopped) load();
    },
    setMode(next) {
      if (stopped || next === mode || !DIFF_MODES.some((entry) => entry.mode === next)) return;
      mode = next;
      load();
    },
    /** Show file `index`'s patch: the part on show before, read again at the same `at` if the budget freed it. */
    openPatch: (index) => patchSource(index)?.show(),
    /** First, Previous, Next or Last part of file `index`'s patch. */
    movePatch: (index, to) => patchSource(index)?.move(to),
    /** Retry page: the part of file `index`'s patch that failed. */
    retryPatch: (index) => patchSource(index)?.retry(),
    openBinary,
    moveBinary,
    retryBinary,
    openPreview,
    closeBinary,
    /** The fingerprint the `ship` event reports for this checkout now. */
    observe(value) {
      if (!isStr(value) || value === live) return;
      live = value;
      liveRevision += 1;
      const was = liveChanged;
      liveChanged = status === "ready" && fingerprint !== null && fingerprint !== value;
      if (liveChanged && status === "ready") cancelSnapshotReads();
      if (liveChanged !== was) emit();
    },
    /** The fleet's address or token changed: drop what was read and read again from the new one. */
    reset() {
      if (stopped) return;
      diff = null;
      fingerprint = null;
      live = null;
      liveChanged = false;
      load();
    },
    stop() {
      stopped = true;
      generation += 1;
      read?.cancel();
      dropSources();
      binary?.source.drop();
      budget.clear();
      binary = null;
      preview = null;
      files = [];
      diff = null;
    },
  };
}
