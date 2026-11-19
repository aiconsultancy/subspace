import assert from "node:assert/strict";
import { test } from "node:test";
import { createClient, parseRetryAfter } from "../src/api.js";
import { NOW, rawForecast, rawPostcode, response } from "./fixtures.js";

function clientWith(handler, options = {}) {
  const calls = [];
  const fetch = async (url, init) => {
    calls.push({ url, init });
    return handler(url, init);
  };
  return { calls, client: createClient({ fetch, now: () => NOW, ...options }) };
}

test("lookupPostcode requests only Postcodes.io and returns a location", async () => {
  const { client, calls } = clientWith(() => response(rawPostcode()));
  const location = await client.lookupPostcode("sw1a 1aa");
  assert.equal(location.name, "Westminster");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, "https://api.postcodes.io/postcodes/SW1A1AA");
  assert.equal(calls[0].init.credentials, "omit");
  assert.equal(calls[0].init.referrerPolicy, "no-referrer");
});

test("lookupPostcode rejects invalid text without any request", async () => {
  const { client, calls } = clientWith(() => response(rawPostcode()));
  await assert.rejects(client.lookupPostcode("hello"), { kind: "invalid-postcode" });
  assert.equal(calls.length, 0);
});

test("a 404 means the postcode was not found", async () => {
  const { client } = clientWith(() => response({ status: 404 }, { status: 404 }));
  await assert.rejects(client.lookupPostcode("SW1A 1AA"), { kind: "not-found" });
});

test("fetchForecast sends coordinates only and returns raw plus model", async () => {
  const { client, calls } = clientWith(() => response(rawForecast()));
  const { raw, model } = await client.fetchForecast({ latitude: 51.5, longitude: -0.12 });
  assert.equal(model.current.temperature, 11.4);
  assert.equal(raw.timezone, "Europe/London");
  assert.ok(calls[0].url.startsWith("https://api.open-meteo.com/v1/forecast?"));
  assert.ok(!/SW1A/i.test(calls[0].url));
});

test("429 becomes a rate-limit error carrying Retry-After", async () => {
  const { client } = clientWith(() => response({}, { status: 429, headers: { "retry-after": "120" } }));
  await assert.rejects(client.fetchForecast({ latitude: 51.5, longitude: -0.12 }), (error) => {
    assert.equal(error.kind, "rate-limit");
    assert.equal(error.retryAfterMs, 120_000);
    return true;
  });
});

test("server errors are reported as server problems", async () => {
  const { client } = clientWith(() => response({}, { status: 503 }));
  await assert.rejects(client.fetchForecast({ latitude: 1, longitude: 1 }), { kind: "server", status: 503 });
});

test("a failed connection is a network error", async () => {
  const { client } = clientWith(() => {
    throw new TypeError("Failed to fetch");
  });
  await assert.rejects(client.fetchForecast({ latitude: 1, longitude: 1 }), { kind: "network" });
});

test("an unreadable body is bad data", async () => {
  const { client } = clientWith(() => response(new SyntaxError("Unexpected token")));
  await assert.rejects(client.fetchForecast({ latitude: 1, longitude: 1 }), { kind: "bad-data" });
});

test("a response that is not a forecast is bad data", async () => {
  const { client } = clientWith(() => response({ error: true, reason: "nope" }));
  await assert.rejects(client.fetchForecast({ latitude: 1, longitude: 1 }), { kind: "bad-data" });
});

test("a request that never answers times out", async () => {
  const { client } = clientWith(
    (_url, init) => new Promise((_resolve, reject) => {
      init.signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
    }),
    { timeoutMs: 20 },
  );
  await assert.rejects(client.fetchForecast({ latitude: 1, longitude: 1 }), { kind: "timeout" });
});

test("aborting the caller's signal cancels the request and reports aborted", async () => {
  let seen;
  const { client } = clientWith((_url, init) => {
    seen = init.signal;
    return new Promise((_resolve, reject) => {
      init.signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
    });
  });
  const controller = new AbortController();
  const pending = client.lookupPostcode("SW1A 1AA", { signal: controller.signal });
  controller.abort();
  await assert.rejects(pending, { kind: "aborted" });
  assert.equal(seen.aborted, true);
});

test("an already aborted signal never reaches the network as a success", async () => {
  const { client } = clientWith((_url, init) => (init.signal.aborted ? Promise.reject(new DOMException("a", "AbortError")) : response(rawPostcode())));
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(client.lookupPostcode("SW1A 1AA", { signal: controller.signal }), { kind: "aborted" });
});

test("parseRetryAfter reads seconds and dates, capped at one hour", () => {
  assert.equal(parseRetryAfter("30", NOW), 30_000);
  assert.equal(parseRetryAfter(new Date(NOW + 90_000).toUTCString(), NOW), 90_000);
  assert.equal(parseRetryAfter("999999", NOW), 3_600_000);
  assert.equal(parseRetryAfter(new Date(NOW - 5_000).toUTCString(), NOW), 0);
  assert.equal(parseRetryAfter("soon", NOW), null);
  assert.equal(parseRetryAfter(null, NOW), null);
});
