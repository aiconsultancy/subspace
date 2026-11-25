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
/** The largest paged item this console reads: its view keeps no more than this loaded in all. */
export const MAX_ITEM_BYTES = 64 * 1024 * 1024;

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

/** How many times reading a paged item starts again because it changed between pages. */
export const PAGE_RESTARTS = 8;

/**
 * Read every page of a paged activity item and return its complete JSON,
 * parsed. Each page is checked as it arrives, before more is held: it must be
 * the page asked for (agent, turn, item and index), hold at most 64 KiB, and
 * make progress for a multi-page item. The item's `bytes` must fit its page
 * count and this console's whole-item limit. The data joined so far may
 * never pass `bytes`, and the whole must equal it.
 * Anything else is an invalid response and the read stops. Pages with a
 * different `seq` or page count belong to another version of the item, so
 * the read starts again from page 0. Nothing is cut: it either returns the
 * whole item or throws.
 *
 * @param {(page: number) => Promise<object>} readPage  one `V1ActivityItemPage`
 * @param {{agent?: string, turn?: string, item?: string, restarts?: number}} [expect]
 * @returns {Promise<{seq: number, bytes: number, item: object}>}
 */
export async function readPagedItem(readPage, { agent, turn, item, restarts = PAGE_RESTARTS } = {}) {
  const expect = { agent, turn, item };
  for (let attempt = 0; attempt <= restarts; attempt += 1) {
    const first = await readPage(0);
    checkPage(first, 0, expect);
    if (first.bytes > first.pages * PAGE_DATA_BYTES) throw new InvalidResponseError(`The item claims ${first.bytes} bytes in ${first.pages} pages.`);
    if (first.bytes > MAX_ITEM_BYTES) throw new InvalidResponseError(`The item is ${first.bytes} bytes, over this console's ${MAX_ITEM_BYTES}-byte limit.`);
    if (first.pages > Math.max(1, first.bytes)) throw new InvalidResponseError(`The item claims ${first.pages} pages for ${first.bytes} bytes.`);
    let total = pageBytes(first, first.bytes);
    const parts = [first.data];
    let changed = false;
    for (let page = 1; page < first.pages; page += 1) {
      const next = await readPage(page);
      checkPage(next, page, expect);
      if (next.seq !== first.seq || next.pages !== first.pages) {
        changed = true;
        break;
      }
      if (next.bytes !== first.bytes) throw new InvalidResponseError(`Page ${page} of the item gives another length for the same version.`);
      total += pageBytes(next, first.bytes - total);
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
    return { seq: first.seq, bytes: first.bytes, item: parsed };
  }
  throw new Error("The item kept changing while its pages were read.");
}

/** A page must fit in 64 KiB and within the remaining item, and must make progress. */
function pageBytes(page, left) {
  const bytes = utf8Length(page.data);
  if (bytes > PAGE_DATA_BYTES) throw new InvalidResponseError(`Page ${page.page} of the item holds more than 64 KiB.`);
  if (page.pages > 1 && bytes === 0) throw new InvalidResponseError(`Page ${page.page} of the item is empty.`);
  if (bytes > left) throw new InvalidResponseError(`Page ${page.page} of the item runs past its length.`);
  return bytes;
}

function checkPage(page, number, expect) {
  const ok = page && typeof page.data === "string" && Number.isInteger(page.pages) && page.pages >= 1 && page.page === number && Number.isFinite(page.seq) && Number.isInteger(page.bytes) && page.bytes >= 0;
  if (!ok) throw new InvalidResponseError(`Page ${number} of the item is not a valid page.`);
  for (const field of ["agent", "turn", "item"]) {
    if (expect[field] !== undefined && page[field] !== expect[field]) throw new InvalidResponseError(`Page ${number} is for another ${field}.`);
  }
}
