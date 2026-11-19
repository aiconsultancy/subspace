import assert from "node:assert/strict";
import { test } from "node:test";
import { createController } from "../src/controller.js";
import { WeatherError } from "../src/errors.js";
import { parseForecast } from "../src/forecast.js";
import { parsePostcodeResponse } from "../src/postcode.js";
import { createStore } from "../src/storage.js";
import { NOW, memoryStorage, rawForecast, rawPostcode } from "./fixtures.js";

const minute = 60_000;
const westminster = parsePostcodeResponse(rawPostcode());
const manchester = parsePostcodeResponse(
  rawPostcode({ postcode: "M1 1AE", latitude: 53.4808, longitude: -2.2426, admin_district: "Manchester", region: "North West" }),
);

function deferred() {
  const handle = {};
  handle.promise = new Promise((resolve, reject) => {
    handle.resolve = resolve;
    handle.reject = reject;
  });
  return handle;
}

/** Client whose calls wait for the test to settle them. */
function manualClient() {
  const forecasts = [];
  const lookups = [];
  return {
    forecasts,
    lookups,
    client: {
      lookupPostcode(postcode, { signal }) {
        const call = { postcode, signal, ...deferred() };
        lookups.push(call);
        return call.promise;
      },
      fetchForecast(location, { signal }) {
        const call = { location, signal, ...deferred() };
        forecasts.push(call);
        return call.promise;
      },
    },
  };
}

function fakeTimers() {
  const pending = new Map();
  let nextId = 0;
  return {
    pending,
    setTimeout(callback, delay) {
      nextId += 1;
      pending.set(nextId, { callback, delay });
      return nextId;
    },
    clearTimeout(id) {
      pending.delete(id);
    },
    /** Run the single scheduled timer. */
    fire() {
      assert.equal(pending.size, 1, "exactly one timer is scheduled");
      const [[id, timer]] = [...pending];
      pending.delete(id);
      timer.callback();
    },
    delays: () => [...pending.values()].map((timer) => timer.delay),
  };
}

function setup({ storage = memoryStorage(), hidden = false } = {}) {
  const clock = { now: NOW };
  const timers = fakeTimers();
  const fake = manualClient();
  const store = createStore(storage, () => clock.now);
  const states = [];
  const controller = createController({
    client: fake.client,
    store,
    now: () => clock.now,
    timers,
    onChange: (state) => states.push(state),
  });
  controller.setHidden(hidden);
  return { ...fake, clock, timers, store, storage, states, controller };
}

const settle = () => new Promise((resolve) => setImmediate(resolve));

async function succeedForecast(call, raw = rawForecast()) {
  call.resolve({ raw, model: parseForecast(raw) });
  await settle();
}

test("with no saved location it stays empty and makes no request", async () => {
  const { controller, forecasts, lookups, timers } = setup();
  controller.start();
  assert.equal(controller.state.phase, "empty");
  assert.equal(forecasts.length + lookups.length, 0);
  assert.equal(timers.pending.size, 0);
});

test("an invalid postcode is reported without a request", async () => {
  const { controller, lookups } = setup();
  assert.equal(await controller.submitPostcode("nope"), false);
  assert.equal(controller.state.lookup.error.kind, "invalid-postcode");
  assert.equal(lookups.length, 0);
});

test("a valid postcode saves the location, loads, caches and schedules the next refresh", async () => {
  const { controller, lookups, forecasts, timers, store, clock } = setup();
  const submitted = controller.submitPostcode("sw1a 1aa");
  assert.equal(controller.state.lookup.pending, true);
  assert.equal(lookups[0].postcode, "SW1A1AA");
  lookups[0].resolve(westminster);
  assert.equal(await submitted, true);

  assert.equal(controller.state.phase, "loading");
  assert.equal(forecasts.length, 1);
  await succeedForecast(forecasts[0]);

  assert.equal(controller.state.phase, "ready");
  assert.equal(controller.state.failure, null);
  assert.equal(controller.state.fetchedAt, clock.now);
  assert.deepEqual(store.loadLocation(), westminster);
  assert.notEqual(store.loadForecast(westminster), null);
  assert.deepEqual(timers.delays(), [15 * minute]);
});

test("a fresh saved forecast is shown without any request", async () => {
  const { controller, forecasts, timers, store, clock } = setup();
  store.saveLocation(westminster);
  store.saveForecast(westminster, rawForecast(), clock.now - 4 * minute);
  controller.start();
  assert.equal(controller.state.phase, "ready");
  assert.equal(forecasts.length, 0);
  assert.deepEqual(timers.delays(), [6 * minute]);
});

test("an old saved forecast is shown while a new one loads, and a failure keeps it labelled", async () => {
  const { controller, forecasts, timers, store, clock } = setup();
  store.saveLocation(westminster);
  store.saveForecast(westminster, rawForecast(), clock.now - 3 * 60 * minute);
  controller.start();

  assert.equal(controller.state.phase, "ready");
  assert.equal(controller.state.refreshing, true);
  assert.equal(forecasts.length, 1);

  forecasts[0].reject(new WeatherError("network"));
  await settle();

  assert.equal(controller.state.phase, "ready", "saved data stays visible");
  assert.equal(controller.state.fetchedAt, clock.now - 3 * 60 * minute, "its age is not reset");
  assert.equal(controller.state.failure.kind, "network");
  assert.deepEqual(timers.delays(), [minute], "retries after one minute");
});

test("a failure with nothing saved is an error state", async () => {
  const { controller, forecasts, store } = setup();
  store.saveLocation(westminster);
  controller.start();
  forecasts[0].reject(new WeatherError("server"));
  await settle();
  assert.equal(controller.state.phase, "error");
  assert.equal(controller.state.model, null);
  assert.equal(controller.state.failure.kind, "server");
});

test("repeated failures back off", async () => {
  const { controller, forecasts, timers, store } = setup();
  store.saveLocation(westminster);
  controller.start();
  const delays = [];
  for (let i = 0; i < 3; i += 1) {
    forecasts[i].reject(new WeatherError("network"));
    await settle();
    delays.push(...timers.delays());
    timers.fire();
  }
  assert.deepEqual(delays, [minute, 2 * minute, 4 * minute]);
});

test("a newer postcode lookup aborts the older one and ignores its late answer", async () => {
  const { controller, lookups, forecasts } = setup();
  const first = controller.submitPostcode("SW1A 1AA");
  const second = controller.submitPostcode("M1 1AE");

  assert.equal(lookups[0].signal.aborted, true);
  assert.equal(lookups[1].signal.aborted, false);

  lookups[0].resolve(westminster); // late answer for the abandoned request
  assert.equal(await first, false);
  assert.equal(controller.state.location, null);

  lookups[1].resolve(manchester);
  assert.equal(await second, true);
  assert.equal(controller.state.location.name, "Manchester");
  assert.equal(forecasts.length, 1);
  assert.equal(forecasts[0].location.name, "Manchester");
});

test("a rejected stale lookup does not show an error for the newer one", async () => {
  const { controller, lookups } = setup();
  const first = controller.submitPostcode("SW1A 1AA");
  const second = controller.submitPostcode("M1 1AE");
  lookups[0].reject(new WeatherError("network"));
  await first;
  assert.equal(controller.state.lookup.error, null);
  assert.equal(controller.state.lookup.pending, true);
  lookups[1].reject(new WeatherError("not-found"));
  await second;
  assert.equal(controller.state.lookup.error.kind, "not-found");
});

test("a failed lookup keeps the previous location and forecast", async () => {
  const { controller, lookups, forecasts } = setup();
  const first = controller.submitPostcode("SW1A 1AA");
  lookups[0].resolve(westminster);
  await first;
  await succeedForecast(forecasts[0]);

  const second = controller.submitPostcode("M1 1AE");
  lookups[1].reject(new WeatherError("not-found"));
  assert.equal(await second, false);
  assert.equal(controller.state.location.name, "Westminster");
  assert.equal(controller.state.phase, "ready");
  assert.equal(controller.state.lookup.error.kind, "not-found");
});

test("switching location aborts the old forecast request and drops its result", async () => {
  const { controller, lookups, forecasts } = setup();
  const first = controller.submitPostcode("SW1A 1AA");
  lookups[0].resolve(westminster);
  await first;
  assert.equal(forecasts.length, 1);

  const second = controller.submitPostcode("M1 1AE");
  lookups[1].resolve(manchester);
  await second;
  assert.equal(forecasts[0].signal.aborted, true);
  assert.equal(forecasts.length, 2);

  await succeedForecast(forecasts[0]); // late answer for Westminster
  assert.equal(controller.state.model, null, "old forecast is not shown for the new place");

  await succeedForecast(forecasts[1]);
  assert.equal(controller.state.location.name, "Manchester");
  assert.equal(controller.state.phase, "ready");
});

test("refresh is paused while hidden and resumes when due", async () => {
  const { controller, forecasts, timers, store, clock } = setup();
  store.saveLocation(westminster);
  controller.start();
  await succeedForecast(forecasts[0]);
  assert.equal(timers.pending.size, 1);

  controller.setHidden(true);
  assert.equal(timers.pending.size, 0, "timer is cleared when hidden");

  clock.now += 20 * minute;
  controller.setHidden(false);
  assert.equal(forecasts.length, 2, "an overdue refresh runs when shown again");
  assert.equal(timers.pending.size, 0, "no timer while the request is in flight");
  await succeedForecast(forecasts[1]);
  assert.equal(timers.pending.size, 1);
});

test("showing the page before a refresh is due only schedules it", async () => {
  const { controller, forecasts, timers, store, clock } = setup();
  store.saveLocation(westminster);
  controller.start();
  await succeedForecast(forecasts[0]);
  controller.setHidden(true);
  clock.now += 5 * minute;
  controller.setHidden(false);
  assert.equal(forecasts.length, 1);
  assert.deepEqual(timers.delays(), [10 * minute]);
});

test("a request that finishes while hidden does not schedule more work", async () => {
  const { controller, forecasts, timers, store } = setup();
  store.saveLocation(westminster);
  controller.start();
  controller.setHidden(true);
  await succeedForecast(forecasts[0]);
  assert.equal(timers.pending.size, 0);
  assert.equal(controller.state.phase, "ready");
});

test("starting hidden makes no request until shown", async () => {
  const { controller, forecasts, store } = setup({ hidden: true });
  store.saveLocation(westminster);
  controller.start();
  assert.equal(forecasts.length, 0);
  controller.setHidden(false);
  assert.equal(forecasts.length, 1);
});

test("the timer triggers a background refresh", async () => {
  const { controller, forecasts, timers, store, clock } = setup();
  store.saveLocation(westminster);
  controller.start();
  await succeedForecast(forecasts[0]);
  clock.now += 15 * minute;
  timers.fire();
  assert.equal(forecasts.length, 2);
});

test("manual refresh is limited to one request a minute", async () => {
  const { controller, forecasts, store, clock } = setup();
  store.saveLocation(westminster);
  controller.start();
  await succeedForecast(forecasts[0]);

  clock.now += 20_000;
  await controller.refresh({ manual: true });
  assert.equal(forecasts.length, 1);
  assert.match(controller.state.notice, /Already up to date/);

  clock.now += 41_000;
  controller.refresh({ manual: true });
  assert.equal(forecasts.length, 2);
  await succeedForecast(forecasts[1]);
});

test("after a failure a manual retry waits 15 seconds and then goes through", async () => {
  const { controller, forecasts, store, clock } = setup();
  store.saveLocation(westminster);
  controller.start();
  forecasts[0].reject(new WeatherError("network"));
  await settle();
  assert.equal(controller.state.phase, "error");

  clock.now += 5_000;
  await controller.refresh({ manual: true });
  assert.equal(forecasts.length, 1, "too soon after the failed request");
  assert.match(controller.state.notice, /Wait a few seconds/);

  clock.now += 11_000;
  controller.refresh({ manual: true });
  assert.equal(forecasts.length, 2);
  assert.equal(controller.state.notice, null);
});

test("overlapping refreshes share one request", async () => {
  const { controller, forecasts, store } = setup();
  store.saveLocation(westminster);
  controller.start();
  const again = controller.refresh({ manual: true });
  await Promise.resolve();
  assert.equal(forecasts.length, 1);
  await succeedForecast(forecasts[0]);
  await again;
});

test("a rate limit pauses requests and blocks manual refresh until it passes", async () => {
  const { controller, forecasts, timers, store, clock } = setup();
  store.saveLocation(westminster);
  controller.start();
  await succeedForecast(forecasts[0]);

  clock.now += 15 * minute;
  timers.fire();
  forecasts[1].reject(new WeatherError("rate-limit", { retryAfterMs: 12 * minute }));
  await settle();
  assert.equal(controller.state.failure.kind, "rate-limit");
  assert.equal(controller.state.phase, "ready", "saved data stays visible");
  assert.deepEqual(timers.delays(), [12 * minute]);

  clock.now += 2 * minute;
  await controller.refresh({ manual: true });
  assert.equal(forecasts.length, 2, "no request during the pause");
  assert.match(controller.state.notice, /pause/);
});

test("a successful refresh clears an earlier failure", async () => {
  const { controller, forecasts, timers, store } = setup();
  store.saveLocation(westminster);
  controller.start();
  forecasts[0].reject(new WeatherError("timeout"));
  await settle();
  assert.equal(controller.state.failure.kind, "timeout");
  timers.fire();
  await succeedForecast(forecasts[1]);
  assert.equal(controller.state.failure, null);
  assert.equal(controller.state.phase, "ready");
});

test("dispose cancels timers and in-flight requests", async () => {
  const { controller, forecasts, timers, store } = setup();
  store.saveLocation(westminster);
  controller.start();
  controller.dispose();
  assert.equal(forecasts[0].signal.aborted, true);
  assert.equal(timers.pending.size, 0);
});
