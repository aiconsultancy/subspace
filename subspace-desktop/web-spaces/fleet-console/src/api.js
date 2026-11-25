// The Fleet API v1 client: bearer-token reads, officer activity and the event
// stream request.
// The only module that calls fetch. Requests never follow redirects, so the
// token only goes to the address it was given for.

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

  async function getJson(path, signal) {
    const response = await fetchImpl(url(path), { headers: { ...headers, Accept: "application/json" }, signal, cache: "no-store", credentials: "omit", redirect: "error" });
    if (!response.ok) throw new HttpError(response.status, retryAfter(response));
    return response.json();
  }

  const activityPath = (agent, query) => {
    const params = new URLSearchParams(Object.entries(query).filter(([, value]) => value !== undefined && value !== null).map(([key, value]) => [key, String(value)]));
    const search = params.toString();
    return `/agents/${encodeURIComponent(agent)}/activity${search ? `?${search}` : ""}`;
  };

  return {
    fleet: (signal) => getJson("/fleet", signal),
    /** An officer's recent turns (`V1AgentActivity`), or one turn's with `{turn}`. 404 when the gateway lacks the route or the agent or turn. */
    activity: (agent, { turn } = {}, signal) => getJson(activityPath(agent, { turn }), signal),
    /** One page (`V1ActivityItemPage`) of a paged item's complete JSON. */
    activityItemPage: (agent, turn, item, page, signal) => getJson(activityPath(agent, { turn, item, page }), signal),
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

/** How many times reading a paged item starts again because it changed between pages. */
export const PAGE_RESTARTS = 8;

/**
 * Read every page of a paged activity item and return its complete JSON,
 * parsed. Pages with a different `seq` belong to another version of the item,
 * and joined data whose UTF-8 length is not the page's `bytes` is incomplete,
 * so either way the read starts again from page 0. Nothing is cut: it either returns the
 * whole item or throws.
 *
 * @param {(page: number) => Promise<{seq: number, page: number, pages: number, bytes: number, data: string}>} readPage
 * @returns {Promise<{seq: number, item: object}>}
 */
export async function readPagedItem(readPage, restarts = PAGE_RESTARTS) {
  for (let attempt = 0; attempt <= restarts; attempt += 1) {
    const first = await readPage(0);
    checkPage(first, 0);
    const parts = [first.data];
    let changed = false;
    for (let page = 1; page < first.pages; page += 1) {
      const next = await readPage(page);
      checkPage(next, page);
      if (next.seq !== first.seq || next.pages !== first.pages) {
        changed = true;
        break;
      }
      parts.push(next.data);
    }
    if (changed) continue;
    const data = parts.join("");
    // A short or long read is another version, or a broken one: never parse it as the item.
    if (utf8Length(data) !== first.bytes) continue;
    return { seq: first.seq, item: JSON.parse(data) };
  }
  throw new Error("The item kept changing while its pages were read.");
}

const utf8Length = (text) => new TextEncoder().encode(text).length;

function checkPage(page, number) {
  const ok = page && typeof page.data === "string" && Number.isInteger(page.pages) && page.pages >= 1 && page.page === number && Number.isFinite(page.seq) && Number.isInteger(page.bytes) && page.bytes >= 0;
  if (!ok) throw new Error(`Page ${number} of the item is not a valid page.`);
}
