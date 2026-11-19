import assert from "node:assert/strict";
import { test } from "node:test";
import { WeatherError } from "../src/errors.js";
import { MANUAL_COOLDOWN_MS, REFRESH_INTERVAL_MS, REUSE_WINDOW_MS, STALE_AFTER_MS, isStale, retryDelay } from "../src/refresh.js";

const minute = 60_000;

test("refresh limits stay well inside the free provider allowance", () => {
  // The free API allows 600 calls per minute, 5,000 per hour and 10,000 per day.
  // One tab refreshing every 15 minutes uses 96 calls per day.
  assert.ok(REFRESH_INTERVAL_MS >= 15 * minute);
  assert.ok(REUSE_WINDOW_MS <= REFRESH_INTERVAL_MS);
  assert.ok(MANUAL_COOLDOWN_MS >= minute);
});

test("failures back off exponentially up to 30 minutes", () => {
  const delays = [1, 2, 3, 4, 5, 6, 7].map((failures) => retryDelay(failures, new WeatherError("network")));
  assert.deepEqual(delays, [1, 2, 4, 8, 16, 30, 30].map((m) => m * minute));
});

test("a rate limit waits at least five minutes or the server's Retry-After", () => {
  assert.equal(retryDelay(1, new WeatherError("rate-limit")), 5 * minute);
  assert.equal(retryDelay(1, new WeatherError("rate-limit", { retryAfterMs: 20 * minute })), 20 * minute);
  assert.equal(retryDelay(1, new WeatherError("rate-limit", { retryAfterMs: 10_000 })), 5 * minute);
});

test("data becomes stale after 30 minutes", () => {
  assert.equal(isStale(0, STALE_AFTER_MS), false);
  assert.equal(isStale(0, STALE_AFTER_MS + 1), true);
});
