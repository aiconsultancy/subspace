// The Fleet API v1 client: bearer-token reads, officer activity and the event
// stream request.
// The only module that calls fetch. Requests never follow redirects, so the
// token only goes to the address it was given for.

import { utf8Length } from "./utf8.js";

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]", "::1"]);

/**
 * Check an endpoint the web Space can reach under its Content Security
 * Policy: HTTPS anywhere, or plain HTTP to loopback (an SSH tunnel).
 * Returns {ok: true, url} with no trailing slash, or {ok: false, reason}.
 */
export function normalizeEndpoint(raw) {
  let url;
  try {
    url = new URL(String(raw ?? "").trim());
  } catch {
    return { ok: false, reason: "invalid" };
  }
  if (url.username || url.password) return { ok: false, reason: "credentials" };
  if (url.protocol === "http:" && !LOOPBACK.has(url.hostname)) return { ok: false, reason: "insecure" };
  if (url.protocol !== "https:" && url.protocol !== "http:") return { ok: false, reason: "protocol" };
  url.search = "";
  url.hash = "";
  const path = url.pathname.replace(/\/+$/, "").replace(/\/api\/v1$/, "");
  return { ok: true, url: `${url.origin}${path}` };
}

export class HttpError extends Error {
  constructor(status, retryAfterMs = 0) {
    super(`Stargate answered ${status}.`);
    this.name = "HttpError";
    this.status = status;
    this.retryAfterMs = retryAfterMs;
  }
}

function retryAfter(response) {
  const value = response.headers?.get?.("retry-after");
  if (!value) return 0;
  const seconds = Number(value);
  if (Number.isFinite(seconds)) return Math.max(0, seconds * 1000);
  const date = Date.parse(value);
  return Number.isFinite(date) ? Math.max(0, date - Date.now()) : 0;
}

/**
 * @param {{endpoint: string, token: string, fetch: typeof fetch}} options
 */
export function createApiClient({ endpoint, token, fetch: fetchImpl }) {
  const headers = { Authorization: `Bearer ${token}` };
  const url = (path) => `${endpoint}/api/v1${path}`;

  async function getJson(path, signal, maxBytes = 0) {
    const response = await fetchImpl(url(path), { headers: { ...headers, Accept: "application/json" }, signal, cache: "no-store", credentials: "omit", redirect: "error" });
    if (!response.ok) throw new HttpError(response.status, retryAfter(response));
    return maxBytes ? readJsonCapped(response, maxBytes) : response.json();
  }

  const activityPath = (agent, query) => {
    const params = new URLSearchParams(Object.entries(query).filter(([, value]) => value !== undefined && value !== null).map(([key, value]) => [key, String(value)]));
    const search = params.toString();
    return `/agents/${encodeURIComponent(agent)}/activity${search ? `?${search}` : ""}`;
  };

  return {
    fleet: (signal) => getJson("/fleet", signal),
    /** An officer's recent turns (`V1AgentActivity`), or one turn's with `{turn}`. 404 when the gateway lacks the route or the agent or turn. */
    activity: (agent, { turn } = {}, signal) => getJson(activityPath(agent, { turn }), signal, MAX_SNAPSHOT_BYTES),
    /** One page (`V1ActivityItemPage`) of a paged item's complete JSON; a body over MAX_PAGE_BODY_BYTES is refused unread. */
    activityItemPage: (agent, turn, item, page, signal) => getJson(activityPath(agent, { turn, item, page }), signal, MAX_PAGE_BODY_BYTES),
    ship: (id, signal) => getJson(`/ships/${encodeURIComponent(id)}`, signal),
    crew: (ship, member, signal) => getJson(`/ships/${encodeURIComponent(ship)}/crew/${encodeURIComponent(member)}`, signal),
    /**
     * Open the event stream; resolves to a Response with a readable body, or
     * throws HttpError. `agents` adds `agent-activity` events for those
     * registration IDs, one repeated `agent` parameter each.
     */
    async events(signal, lastEventId, agents = []) {
      const params = new URLSearchParams();
      for (const agent of agents) params.append("agent", agent);
      const search = params.toString();
      const response = await fetchImpl(url(`/events${search ? `?${search}` : ""}`), {
        headers: { ...headers, Accept: "text/event-stream", ...(lastEventId ? { "Last-Event-ID": lastEventId } : {}) },
        signal,
        cache: "no-store",
        credentials: "omit",
        // A redirect would carry the bearer token to another address: refuse it.
        redirect: "error",
      });
      if (!response.ok) throw new HttpError(response.status, retryAfter(response));
      // A proxy's sign-in page or error page can answer 200: that is not the stream.
      const type = response.headers?.get?.("content-type");
      if (type && !/^\s*text\/event-stream/i.test(type)) throw new Error(`Expected an event stream, got ${type.split(";")[0]}.`);
      if (!response.body) throw new Error("The event stream has no body.");
      return response;
    },
  };
}

/** The most a snapshot body may be; Channels uses the same cap. */
export const MAX_SNAPSHOT_BYTES = 128 * 1024 * 1024;
/** A page holds at most 64 KiB of UTF-8 data (Stargate `docs/api.md`). */
export const PAGE_DATA_BYTES = 64 * 1024;
/** A page's JSON body: 64 KiB of data, escaped, and its envelope. */
export const MAX_PAGE_BODY_BYTES = 512 * 1024;
/** A response this console refuses to read further: too large, or not the shape it asked for. */
export class InvalidResponseError extends Error {
  constructor(message) {
    super(message);
    this.name = "InvalidResponseError";
  }
}

/** Read a JSON body, refusing it once it passes `maxBytes`, before it is decoded. */
async function readJsonCapped(response, maxBytes) {
  const tooLarge = () => new InvalidResponseError(`Stargate sent a response over ${maxBytes} bytes.`);
  const declared = Number(response.headers?.get?.("content-length"));
  if (Number.isFinite(declared) && declared > maxBytes) {
    await response.body?.cancel?.().catch(() => {});
    throw tooLarge();
  }
  let text;
  if (typeof response.body?.getReader === "function") {
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxBytes) {
        await reader.cancel().catch(() => {});
        throw tooLarge();
      }
      chunks.push(value);
    }
    const joined = new Uint8Array(size);
    let at = 0;
    for (const chunk of chunks) {
      joined.set(chunk, at);
      at += chunk.byteLength;
    }
    text = new TextDecoder().decode(joined);
  } else if (typeof response.text === "function") {
    text = await response.text();
    if (utf8Length(text) > maxBytes) throw tooLarge();
  } else {
    return response.json();
  }
  return JSON.parse(text);
}

/** How many times a paged item is read from page 0 before the read gives up for now, as Channels (`PAGE_ATTEMPTS`). */
export const PAGE_ATTEMPTS = 5;
/**
 * The most one item read whole may be, whatever room its view has: one
 * larger is refused with TooLargeError after page 0, before page 1 is asked
 * for, and is read part by part instead (decision a17870f4).
 */
export const MAX_ITEM_BYTES = 64 * 1024 * 1024;
/** The most a read holds when its caller gives no `room`: a view's default budget. */
export const DEFAULT_ROOM_BYTES = MAX_ITEM_BYTES;
const defaultRoom = (bytes) => bytes <= DEFAULT_ROOM_BYTES;

/** A part of an item too large to hold is this many pages (256 KiB of UTF-8 at most). */
export const PART_PAGES = 4;

/**
 * An item larger than MAX_ITEM_BYTES or the room its view can make: refused
 * as a whole read (an invalid response for one), so it is read part by part.
 */
export class TooLargeError extends InvalidResponseError {
  constructor(bytes) {
    super(`The item is ${bytes} bytes, more than this view holds at once.`);
    this.name = "TooLargeError";
    this.bytes = bytes;
  }
}

/** The item changed on every attempt: nothing is wrong with it, so it can be read again later. */
export class ChangingError extends Error {
  constructor() {
    super("The item kept changing while its pages were read.");
    this.name = "ChangingError";
  }
}

const sameVersion = (a, b) => a.seq === b.seq && a.bytes === b.bytes && a.pages === b.pages;

/**
 * Read every page of a paged activity item and return its complete JSON,
 * parsed. Each page is checked as it arrives, before the next is asked for
 * (`checkPage`). After page 0, an item over MAX_ITEM_BYTES is refused with
 * TooLargeError, and `room(bytes)` is asked for the item's length:
 * the view makes room before it holds more, or answers false, and the read
 * stops with TooLargeError so the view can read it part by part. Without
 * `room`, the item must fit DEFAULT_ROOM_BYTES: no read is unbounded. A page of
 * another `seq`, `bytes` or page count belongs to another version, so the
 * read starts again from page 0, at most PAGE_ATTEMPTS reads in all (as
 * Channels). The joined JSON must be an object for the item asked for.
 * Nothing is cut: it either returns the whole item or throws.
 *
 * @param {(page: number) => Promise<object>} readPage  one `V1ActivityItemPage`
 * @param {{agent?: string, turn?: string, item?: string, attempts?: number, room?: (bytes: number) => boolean}} [expect]
 * @returns {Promise<{seq: number, bytes: number, pages: number, item: object}>}
 */
export async function readPagedItem(readPage, { agent, turn, item, attempts = PAGE_ATTEMPTS, room = defaultRoom } = {}) {
  const expect = { agent, turn, item };
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const first = await readPage(0);
    checkPage(first, 0, expect);
    if (first.bytes > MAX_ITEM_BYTES) throw new TooLargeError(first.bytes);
    if (!room(first.bytes)) throw new TooLargeError(first.bytes);
    let total = pageBytes(first, 0, 0, first.bytes);
    const parts = [first.data];
    let changed = false;
    for (let page = 1; page < first.pages; page += 1) {
      const next = await readPage(page);
      checkPage(next, page, expect);
      if (!sameVersion(next, first)) {
        changed = true;
        break;
      }
      total += pageBytes(next, page, total, first.bytes);
      parts.push(next.data);
    }
    if (changed) continue;
    if (total !== first.bytes) throw new InvalidResponseError(`The item's pages join to ${total} bytes, not ${first.bytes}.`);
    let parsed;
    try {
      parsed = JSON.parse(parts.join(""));
    } catch {
      throw new InvalidResponseError("The item's pages do not join into JSON.");
    }
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed) || (item !== undefined && parsed.id !== item)) {
      throw new InvalidResponseError("The item's pages do not join into the item asked for.");
    }
    return { seq: first.seq, bytes: first.bytes, pages: first.pages, item: parsed };
  }
  throw new ChangingError();
}

/**
 * Read one part of an item too large to hold: pages `from` to `from +
 * count - 1` (fewer at the end), joined as the JSON text served. Every page
 * is pinned to `version` ({seq, bytes, pages}; the first page read sets it
 * when absent). A page of another version restarts the read once, from page
 * 0 of that version, and the result says `newer`; a second change throws
 * ChangingError. Each page is checked before the next is asked for, and the
 * running length is kept within `bytes`: exactly from `offset` (the bytes
 * before `from`, when known), or else allowing one byte for each page before.
 *
 * @returns {Promise<{seq: number, bytes: number, pages: number, from: number, data: string, end: number|null, newer: boolean}>}
 *   `end` is the bytes up to the part's end, when `offset` was known.
 */
export async function readItemPart(readPage, { agent, turn, item, version = null, from = 0, count = PART_PAGES, offset = null } = {}) {
  const expect = { agent, turn, item };
  let pin = version;
  let start = from;
  let before = from === 0 ? 0 : offset;
  let newer = false;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const parts = [];
    let total = 0;
    let restart = false;
    const last = () => Math.min(pin ? pin.pages : start + count, start + count) - 1;
    for (let page = start; page <= last(); page += 1) {
      const next = await readPage(page);
      checkPage(next, page, expect);
      if (pin && !sameVersion(next, pin)) {
        restart = true;
        pin = { seq: next.seq, bytes: next.bytes, pages: next.pages };
        break;
      }
      pin ??= { seq: next.seq, bytes: next.bytes, pages: next.pages };
      // Unknown: pages before this part hold at least a byte each after page 0.
      const floor = before ?? Math.max(0, start - 1);
      total += pageBytes(next, page, floor + total, pin.bytes);
      parts.push(next.data);
    }
    if (!restart) {
      const end = before === null ? null : before + total;
      if (end !== null && last() === pin.pages - 1 && end !== pin.bytes) throw new InvalidResponseError(`The item's pages join to ${end} bytes, not ${pin.bytes}.`);
      return { ...pin, from: start, data: parts.join(""), end, newer };
    }
    newer = true;
    start = 0;
    before = 0;
  }
  throw new ChangingError();
}

/** A page's data length, checked against 64 KiB, an empty page in a multi-page item, and the item's length from `held` bytes before it. */
function pageBytes(page, number, held, bytes) {
  const size = utf8Length(page.data);
  if (size > PAGE_DATA_BYTES) throw new InvalidResponseError(`Page ${number} of the item holds more than 64 KiB.`);
  if (page.pages > 1 && size === 0) throw new InvalidResponseError(`Page ${number} of the item is empty.`);
  if (held + size > bytes) throw new InvalidResponseError(`Page ${number} of the item runs past its length.`);
  return size;
}

/**
 * The page asked for, of the agent, turn and item asked for, whose length
 * fits its page count both ways: `bytes ≤ pages × 64 KiB` and `pages ≤
 * max(1, bytes)`, since no page after the first may be empty (as Channels).
 */
function checkPage(page, number, expect) {
  const ok = page && typeof page.data === "string" && Number.isInteger(page.pages) && page.pages >= 1 && page.page === number && page.page < page.pages && Number.isFinite(page.seq) && Number.isInteger(page.bytes) && page.bytes >= 0;
  if (!ok) throw new InvalidResponseError(`Page ${number} of the item is not a valid page.`);
  for (const field of ["agent", "turn", "item"]) {
    if (expect[field] !== undefined && page[field] !== expect[field]) throw new InvalidResponseError(`Page ${number} is for another ${field}.`);
  }
  if (page.bytes > page.pages * PAGE_DATA_BYTES) throw new InvalidResponseError(`The item claims ${page.bytes} bytes in ${page.pages} pages.`);
  if (page.pages > Math.max(1, page.bytes)) throw new InvalidResponseError(`The item claims ${page.pages} pages for ${page.bytes} bytes.`);
}
