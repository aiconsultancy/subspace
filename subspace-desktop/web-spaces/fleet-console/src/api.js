// The Fleet API v1 client: bearer-token reads and the event stream request.
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

  return {
    fleet: (signal) => getJson("/fleet", signal),
    ship: (id, signal) => getJson(`/ships/${encodeURIComponent(id)}`, signal),
    crew: (ship, member, signal) => getJson(`/ships/${encodeURIComponent(ship)}/crew/${encodeURIComponent(member)}`, signal),
    /** Open the event stream; resolves to a Response with a readable body, or throws HttpError. */
    async events(signal, lastEventId) {
      const response = await fetchImpl(url("/events"), {
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
