// Refresh policy. Open-Meteo updates current conditions every 15 minutes, so
// asking more often gains nothing and only spends the shared free allowance.

export const REFRESH_INTERVAL_MS = 15 * 60 * 1000;
/** A saved forecast newer than this is used without a request. */
export const REUSE_WINDOW_MS = 10 * 60 * 1000;
/** Minimum gap between two manual refresh requests. */
export const MANUAL_COOLDOWN_MS = 60 * 1000;
/** Minimum gap between manual retries after a failed request. */
export const RETRY_COOLDOWN_MS = 15 * 1000;
/** Data older than this is labelled as saved instead of current. */
export const STALE_AFTER_MS = 30 * 60 * 1000;

const BACKOFF_BASE_MS = 60 * 1000;
const BACKOFF_MAX_MS = 30 * 60 * 1000;
const RATE_LIMIT_MIN_MS = 5 * 60 * 1000;

/**
 * Delay before retrying after `failures` consecutive failures.
 * A rate limit waits at least five minutes, or the server's Retry-After.
 */
export function retryDelay(failures, error) {
  if (error?.kind === "rate-limit") {
    return Math.max(RATE_LIMIT_MIN_MS, error.retryAfterMs ?? 0);
  }
  const exponent = Math.max(0, failures - 1);
  return Math.min(BACKOFF_MAX_MS, BACKOFF_BASE_MS * 2 ** exponent);
}

export const isStale = (fetchedAt, now) => now - fetchedAt > STALE_AFTER_MS;
