import { WeatherError } from "./errors.js";
import { forecastUrl, parseForecast } from "./forecast.js";
import { normalisePostcode, parsePostcodeResponse, postcodeUrl } from "./postcode.js";

export const REQUEST_TIMEOUT_MS = 10_000;
const MAX_RETRY_AFTER_MS = 60 * 60 * 1000;

/** Retry-After as milliseconds, from delta-seconds or an HTTP date. */
export function parseRetryAfter(header, nowMs) {
  if (typeof header !== "string" || header.trim() === "") return null;
  const seconds = Number(header);
  const ms = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(header) - nowMs;
  if (!Number.isFinite(ms)) return null;
  return Math.min(Math.max(ms, 0), MAX_RETRY_AFTER_MS);
}

function statusError(response, nowMs) {
  const { status } = response;
  if (status === 429) {
    return new WeatherError("rate-limit", {
      status,
      retryAfterMs: parseRetryAfter(response.headers?.get("retry-after"), nowMs),
    });
  }
  return new WeatherError(status === 404 ? "not-found" : "server", { status });
}

/**
 * Network access for the two providers. Everything is injected so tests run
 * without a network.
 * @param {{ fetch: typeof fetch, now?: () => number, timeoutMs?: number }} options
 */
export function createClient({ fetch, now = Date.now, timeoutMs = REQUEST_TIMEOUT_MS }) {
  async function getJson(url, signal) {
    const timeout = new AbortController();
    const timer = setTimeout(() => timeout.abort(), timeoutMs);
    const onAbort = () => timeout.abort();
    signal?.addEventListener("abort", onAbort, { once: true });
    if (signal?.aborted) timeout.abort();

    try {
      const response = await fetch(url, {
        signal: timeout.signal,
        credentials: "omit",
        referrerPolicy: "no-referrer",
        headers: { accept: "application/json" },
      });
      if (!response.ok) throw statusError(response, now());
      try {
        return await response.json();
      } catch {
        throw new WeatherError("bad-data");
      }
    } catch (error) {
      if (error instanceof WeatherError) throw error;
      if (signal?.aborted) throw new WeatherError("aborted");
      throw new WeatherError(timeout.signal.aborted ? "timeout" : "network");
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
    }
  }

  return {
    /** Look up a UK postcode. Rejects invalid text before any request. */
    async lookupPostcode(input, { signal } = {}) {
      const compact = normalisePostcode(input);
      if (!compact) throw new WeatherError("invalid-postcode");
      return parsePostcodeResponse(await getJson(postcodeUrl(compact), signal));
    },

    /** Fetch a forecast. Resolves to `{ raw, model }` so callers can cache raw. */
    async fetchForecast(location, { signal } = {}) {
      const raw = await getJson(forecastUrl(location), signal);
      return { raw, model: parseForecast(raw) };
    },
  };
}
