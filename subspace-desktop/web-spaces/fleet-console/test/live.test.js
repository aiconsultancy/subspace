import assert from "node:assert/strict";
import { test } from "node:test";
import { createApiClient, HttpError, normalizeEndpoint } from "../src/api.js";
import { createLiveSource, WATCHDOG_MS } from "../src/live.js";
import { sampleFleet } from "./fixtures.js";

test("endpoints: HTTPS anywhere, plain HTTP only to loopback, no credentials", () => {
  assert.deepEqual(normalizeEndpoint("https://fleet.example.ts.net/"), { ok: true, url: "https://fleet.example.ts.net" });
  assert.deepEqual(normalizeEndpoint(" https://fleet.example.com/stargate/api/v1/ "), { ok: true, url: "https://fleet.example.com/stargate" });
  assert.deepEqual(normalizeEndpoint("http://localhost:8787"), { ok: true, url: "http://localhost:8787" });
  assert.equal(normalizeEndpoint("http://127.0.0.1:9000?x=1").url, "http://127.0.0.1:9000");
  assert.equal(normalizeEndpoint("http://[::1]:9000").ok, true);
  assert.equal(normalizeEndpoint("http://100.101.102.103:8787").reason, "insecure");
  assert.equal(normalizeEndpoint("https://user:pass@fleet.example.com").reason, "credentials");
  assert.equal(normalizeEndpoint("ftp://fleet.example.com").reason, "protocol");
  assert.equal(normalizeEndpoint("not a url").reason, "invalid");
});

function response(status, body, headers = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (name) => headers[name.toLowerCase()] ?? null },
    json: async () => body,
    body: typeof body === "string" || body instanceof ReadableStream ? (typeof body === "string" ? stream([body]) : body) : null,
  };
}

function stream(chunks, { hold = false } = {}) {
  const encoder = new TextEncoder();
  let controller;
  const readable = new ReadableStream({
    start(c) {
      controller = c;
      for (const chunk of chunks) c.enqueue(encoder.encode(chunk));
      if (!hold) c.close();
    },
  });
  readable.push = (chunk) => controller.enqueue(encoder.encode(chunk));
  readable.end = () => controller.close();
  return readable;
}

test("the client sends the bearer token, asks for no caching, and raises HttpError with Retry-After", async () => {
  const calls = [];
  const client = createApiClient({
    endpoint: "https://fleet.example.com",
    token: "t0ken",
    fetch: async (url, init) => {
      calls.push({ url, init });
      return url.endsWith("/fleet") ? response(200, { ok: 1 }) : response(503, {}, { "retry-after": "12" });
    },
  });
  assert.deepEqual(await client.fleet(), { ok: 1 });
  assert.equal(calls[0].url, "https://fleet.example.com/api/v1/fleet");
  assert.equal(calls[0].init.headers.Authorization, "Bearer t0ken");
  assert.equal(calls[0].init.cache, "no-store");
  await assert.rejects(client.ship("gal actica"), (error) => error instanceof HttpError && error.status === 503 && error.retryAfterMs === 12_000);
  assert.equal(calls[1].url, "https://fleet.example.com/api/v1/ships/gal%20actica");
  await assert.rejects(client.events(undefined, "41"), HttpError);
  assert.equal(calls[2].init.headers["Last-Event-ID"], "41");
  assert.equal(calls[2].init.headers.Accept, "text/event-stream");
});

function harness(fetchImpl) {
  const events = [];
  const states = [];
  const timers = new Map();
  let nextId = 1;
  let clock = 1_000_000;
  const source = createLiveSource({
    client: createApiClient({ endpoint: "https://fleet.example.com", token: "t", fetch: fetchImpl }),
    onEvent: (event) => events.push(event),
    onConnection: (state) => states.push(state),
    now: () => clock,
    random: () => 0,
    setTimer: (fn, ms) => {
      const id = nextId++;
      timers.set(id, { fn, at: clock + ms });
      return id;
    },
    clearTimer: (id) => timers.delete(id),
  });
  return {
    source,
    events,
    states,
    timers,
    advance(ms) {
      clock += ms;
      for (const [id, timer] of [...timers]) {
        if (timer.at <= clock) {
          timers.delete(id);
          timer.fn();
        }
      }
    },
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 5));
const frame = (type, data) => `event: ${type}\ndata: ${JSON.stringify(data)}\n\n`;

test("live source: reads the fleet, opens the stream, delivers events, and reconnects after it closes", async () => {
  const { fleet, ships } = sampleFleet();
  const opened = [];
  const h = harness(async (url, init) => {
    if (url.endsWith("/fleet")) return response(200, fleet);
    opened.push(init.headers["Last-Event-ID"] ?? null);
    return response(200, stream([`id: ${opened.length}\n${frame("ship", ships.galactica)}`, frame("heartbeat", { at: 5 }), ": comment\n\n"]));
  });
  h.source.start();
  await settle();
  assert.deepEqual(h.events.map((event) => event.type), ["fleet", "ship", "heartbeat"]);
  assert.deepEqual(h.states.map((state) => state.status), ["connecting", "connected", "reconnecting"]);
  const retry = [...h.timers.values()].find((timer) => timer.at > 1_000_000 && timer.at <= 1_000_000 + 1000);
  assert.ok(retry, "a quick reconnect is scheduled");
  h.advance(1000);
  await settle();
  assert.deepEqual(opened, [null, "1"], "the reconnect resumes from the last event id and skips the fleet read");
  assert.equal(h.events.filter((event) => event.type === "fleet").length, 1);
  h.source.stop();
});

test("live source: 401 stops at signed out with no automatic retry, until retried", async () => {
  let status = 401;
  const h = harness(async () => response(status, {}));
  h.source.start();
  await settle();
  assert.equal(h.states.at(-1).status, "signed-out");
  assert.equal(h.timers.size, 0);
  status = 403;
  h.source.retryNow();
  await settle();
  assert.equal(h.states.at(-1).status, "forbidden");
  h.source.stop();
});

test("live source: network failures back off as unreachable", async () => {
  const h = harness(async () => {
    throw new TypeError("Load failed");
  });
  h.source.start();
  await settle();
  const state = h.states.at(-1);
  assert.equal(state.status, "unreachable");
  assert.equal(state.attempt, 1);
  assert.equal(h.timers.size, 1);
  h.advance(600);
  await settle();
  assert.equal(h.states.at(-1).attempt, 2);
  h.source.stop();
});

test("live source: a silent stream is dropped by the watchdog and retried", async () => {
  const { fleet } = sampleFleet();
  let body;
  const h = harness(async (url, init) => {
    if (url.endsWith("/fleet")) return response(200, fleet);
    body = stream([], { hold: true });
    init.signal?.addEventListener("abort", () => body.end?.());
    return response(200, body);
  });
  h.source.start();
  await settle();
  assert.equal(h.states.at(-1).status, "connected");
  h.advance(WATCHDOG_MS);
  await settle();
  assert.equal(h.states.at(-1).status, "reconnecting");
  h.source.stop();
});

test("live source: malformed JSON frames are skipped", async () => {
  const { fleet } = sampleFleet();
  const h = harness(async (url) => (url.endsWith("/fleet") ? response(200, fleet) : response(200, stream(["event: ship\ndata: {nope\n\n", frame("heartbeat", { at: 1 })]))));
  h.source.start();
  await settle();
  assert.deepEqual(h.events.map((event) => event.type), ["fleet", "heartbeat"]);
  h.source.stop();
});

test("detail reads emit ship-detail and crew-detail, and swallow failures", async () => {
  const { ships } = sampleFleet();
  const h = harness(async (url) => {
    if (url.endsWith("/ships/galactica")) return response(200, ships.galactica);
    if (url.includes("/crew/")) return response(200, { ship: "galactica", member: ships.galactica.crewMembers[0] });
    return response(404, {});
  });
  assert.equal(await h.source.readShip("galactica"), true);
  assert.equal(await h.source.readCrew("galactica", "starbuck"), true);
  assert.equal(await h.source.readShip("missing"), false);
  assert.deepEqual(h.events.map((event) => event.type), ["ship-detail", "crew-detail"]);
});
