import assert from "node:assert/strict";
import { test } from "node:test";
import { parsePostcodeResponse } from "../src/postcode.js";
import { MAX_CACHE_AGE_MS, createStore } from "../src/storage.js";
import { NOW, memoryStorage, rawForecast, rawPostcode } from "./fixtures.js";

const location = parsePostcodeResponse(rawPostcode());

test("the chosen location survives a reload", () => {
  const storage = memoryStorage();
  createStore(storage, () => NOW).saveLocation(location);
  assert.deepEqual(createStore(storage, () => NOW).loadLocation(), location);
});

test("a forecast is returned for its own location with its fetch time", () => {
  const storage = memoryStorage();
  const store = createStore(storage, () => NOW);
  store.saveForecast(location, rawForecast(), NOW - 5 * 60_000);
  const cached = store.loadForecast(location);
  assert.equal(cached.fetchedAt, NOW - 5 * 60_000);
  assert.equal(cached.model.current.temperature, 11.4);
});

test("a forecast for another location is not reused", () => {
  const store = createStore(memoryStorage(), () => NOW);
  store.saveForecast(location, rawForecast(), NOW);
  const elsewhere = { ...location, latitude: 53.4808, longitude: -2.2426 };
  assert.equal(store.loadForecast(elsewhere), null);
});

test("a forecast older than the cache limit is discarded", () => {
  const store = createStore(memoryStorage(), () => NOW);
  store.saveForecast(location, rawForecast(), NOW - MAX_CACHE_AGE_MS - 1);
  assert.equal(store.loadForecast(location), null);
  store.saveForecast(location, rawForecast(), NOW - MAX_CACHE_AGE_MS + 1);
  assert.notEqual(store.loadForecast(location), null);
});

test("a forecast dated in the future is rejected", () => {
  const store = createStore(memoryStorage(), () => NOW);
  store.saveForecast(location, rawForecast(), NOW + 60_000);
  assert.equal(store.loadForecast(location), null);
});

test("corrupt or tampered records are ignored", () => {
  const storage = memoryStorage({
    "subspace.weather.v1.location": "{not json",
    "subspace.weather.v1.forecast": JSON.stringify({ key: "51.5010,-0.1416", fetchedAt: NOW, raw: { current: {} } }),
  });
  const store = createStore(storage, () => NOW);
  assert.equal(store.loadLocation(), null);
  assert.equal(store.loadForecast(location), null);

  storage.setItem("subspace.weather.v1.location", JSON.stringify({ ...location, latitude: "north" }));
  assert.equal(store.loadLocation(), null);
});

test("unavailable storage never throws", () => {
  const broken = {
    getItem() { throw new Error("blocked"); },
    setItem() { throw new Error("full"); },
    removeItem() { throw new Error("blocked"); },
  };
  for (const storage of [broken, null]) {
    const store = createStore(storage, () => NOW);
    store.saveLocation(location);
    store.saveForecast(location, rawForecast(), NOW);
    store.clearForecast();
    assert.equal(store.loadLocation(), null);
    assert.equal(store.loadForecast(location), null);
  }
});
