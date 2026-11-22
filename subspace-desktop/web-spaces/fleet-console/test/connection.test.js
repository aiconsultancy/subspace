import assert from "node:assert/strict";
import { test } from "node:test";
import { BACKOFF_CAP_MS, backoffDelay, describeConnection, initialConnection, isStale, nextConnection, RECONNECT_GRACE } from "../src/connection.js";

const run = (events, random = () => 0.5) => {
  let state = initialConnection();
  let now = 1000;
  for (const event of events) {
    state = nextConnection(state, event, now, random);
    now += 1000;
  }
  return state;
};

test("backoff doubles, is jittered within half of its ceiling, and is capped", () => {
  assert.equal(backoffDelay(1, () => 0), 500);
  assert.equal(backoffDelay(1, () => 1), 1000);
  assert.equal(backoffDelay(3, () => 0), 2000);
  assert.equal(backoffDelay(3, () => 1), 4000);
  for (let attempt = 1; attempt < 20; attempt += 1) {
    const low = backoffDelay(attempt, () => 0);
    const high = backoffDelay(attempt, () => 0.999999);
    assert.ok(low >= 250 && high <= BACKOFF_CAP_MS && low <= high);
  }
  assert.equal(backoffDelay(30, () => 1), BACKOFF_CAP_MS);
});

test("connecting then open is connected; only a healthy stream clears the attempt count", () => {
  const state = run([{ type: "connecting" }, { type: "failed" }, { type: "connecting" }, { type: "open" }]);
  assert.equal(state.status, "connected");
  assert.equal(state.attempt, 1, "a 200 alone does not reset the backoff");
  assert.equal(state.connectedOnce, true);
  assert.equal(state.retryAt, null);
  assert.equal(nextConnection(state, { type: "healthy" }, 9000).attempt, 0);
});

test("a server that answers 200 and closes at once backs off further each time", () => {
  const cycle = [{ type: "connecting" }, { type: "open" }, { type: "closed" }];
  const state = run([...cycle, ...cycle, ...cycle, ...cycle], () => 0);
  assert.equal(state.attempt, 4);
  assert.ok(state.retryAt - (1000 + 11 * 1000) >= 4000, "the fourth retry waits seconds, not half of one");
});

test("a network failure before any connection is unreachable, with a retry scheduled", () => {
  const state = run([{ type: "connecting" }, { type: "failed" }], () => 0);
  assert.equal(state.status, "unreachable");
  assert.equal(state.attempt, 1);
  assert.equal(state.retryAt, 2000 + 500);
});

test("a dropped stream after connecting is reconnecting, then unreachable after the grace", () => {
  const events = [{ type: "connecting" }, { type: "open" }, { type: "closed" }];
  assert.equal(run(events).status, "reconnecting");
  const many = [...events];
  for (let i = 0; i < RECONNECT_GRACE; i += 1) many.push({ type: "connecting" }, { type: "failed" });
  assert.equal(run(many).status, "unreachable");
});

test("401 is signed out and 403 forbidden, neither retries on its own", () => {
  const signedOut = run([{ type: "connecting" }, { type: "failed", status: 401 }]);
  assert.equal(signedOut.status, "signed-out");
  assert.equal(signedOut.retryAt, null);
  const forbidden = run([{ type: "connecting" }, { type: "open" }, { type: "failed", status: 403 }]);
  assert.equal(forbidden.status, "forbidden");
  assert.equal(forbidden.retryAt, null);
});

test("404 is unreachable even after a connection; 503 honours Retry-After", () => {
  const notFound = run([{ type: "connecting" }, { type: "open" }, { type: "failed", status: 404 }]);
  assert.equal(notFound.status, "unreachable");
  assert.equal(notFound.httpStatus, 404);
  const busy = run([{ type: "connecting" }, { type: "open" }, { type: "failed", status: 503, retryAfterMs: 20_000 }], () => 0);
  assert.equal(busy.status, "reconnecting");
  assert.equal(busy.retryAt, 3000 + 20_000);
});

test("retry moves signed-out and unreachable back to connecting; sample and stop reset", () => {
  assert.equal(run([{ type: "failed", status: 401 }, { type: "retry" }]).status, "connecting");
  assert.equal(run([{ type: "open" }, { type: "retry" }]).status, "connected");
  const sample = run([{ type: "open" }, { type: "sample", reason: "no-endpoint" }]);
  assert.deepEqual([sample.status, sample.reason, sample.connectedOnce], ["sample", "no-endpoint", false]);
  assert.equal(run([{ type: "open" }, { type: "stop" }]).status, "idle");
});

test("stale while not connected", () => {
  assert.equal(isStale(run([{ type: "open" }])), false);
  assert.equal(isStale(run([{ type: "open" }, { type: "closed" }])), true);
});

test("every state says what it is and what to do", () => {
  const live = describeConnection(run([{ type: "open" }]));
  assert.deepEqual([live.label, live.tone, live.banner], ["Live", "green", null]);
  const signedOut = describeConnection(run([{ type: "failed", status: 401 }]));
  assert.equal(signedOut.label, "Signed out");
  assert.match(signedOut.banner.body, /stargate api token/);
  const forbidden = describeConnection(run([{ type: "failed", status: 403 }]));
  assert.match(forbidden.banner.body, /Tailscale|SSH tunnel/);
  const state = run([{ type: "connecting" }, { type: "failed" }], () => 0);
  const unreachable = describeConnection(state, { endpoint: "https://fleet.example.ts.net", now: state.retryAt - 2100 });
  assert.match(unreachable.banner.title, /fleet\.example\.ts\.net/);
  assert.match(unreachable.banner.body, /corsOrigins/);
  assert.match(unreachable.banner.body, /Retrying in 3 s/);
  const reconnecting = describeConnection(run([{ type: "open" }, { type: "closed" }], () => 0), { now: 0 });
  assert.match(reconnecting.label, /Reconnecting/);
  for (const reason of ["no-bridge", "no-settings", "no-endpoint", "bad-endpoint", "no-secrets", "no-secrets-api", "no-token"]) {
    const sample = describeConnection(run([{ type: "sample", reason }]));
    assert.equal(sample.label, "Sample");
    assert.ok(sample.banner.title.startsWith("Sample fleet"), reason);
    assert.equal(sample.banner.retry, reason !== "no-bridge");
  }
});

test("a fleet from the Subspace API says to set its token in Subspace Settings", async () => {
  const { tokenProblem } = await import("../src/connection.js");
  assert.match(tokenProblem("no-token", "api").body, /Subspace Settings › Fleets/);
  assert.match(tokenProblem("token-address", "api").title, /address changed/);
  assert.match(tokenProblem("token-store", "api").body, /keychain/);
  assert.match(tokenProblem("token-checking", "api").title, /reading/);
  assert.match(tokenProblem("token-address").body, /Edit the fleet/);
});
