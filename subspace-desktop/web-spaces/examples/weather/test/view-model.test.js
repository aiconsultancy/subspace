import assert from "node:assert/strict";
import { test } from "node:test";
import { WeatherError } from "../src/errors.js";
import { parseForecast } from "../src/forecast.js";
import { parsePostcodeResponse } from "../src/postcode.js";
import { buildView, currentConditions, feelsComparison, uvLabel } from "../src/view-model.js";
import { NOW, rawForecast, rawPostcode } from "./fixtures.js";

const location = parsePostcodeResponse(rawPostcode());
const minute = 60_000;

function readyState(overrides = {}, raw = rawForecast()) {
  return {
    phase: "ready",
    location,
    model: parseForecast(raw),
    fetchedAt: NOW - 3 * minute,
    refreshing: false,
    failure: null,
    lookup: { pending: false, error: null },
    notice: null,
    ...overrides,
  };
}

test("fresh data shows observed conditions, high and low, and details", () => {
  const view = buildView(readyState(), NOW);
  assert.equal(view.mode, "ready");
  assert.equal(view.banner, null);
  assert.equal(view.hero.title, "Westminster");
  assert.equal(view.hero.subtitle, "SW1A 1AA · St James's · London");
  assert.equal(view.hero.basis, "Now");
  assert.equal(view.hero.temperature, "11°");
  assert.equal(view.hero.condition, "Light rain");
  assert.equal(view.hero.high, "12°");
  assert.equal(view.hero.low, "7°");
  assert.equal(view.hero.feelsLike, "9°");
  assert.deepEqual(view.scene, { kind: "rain", intensity: 1, time: "day", label: "Light rain" });
  assert.equal(view.updated, "Updated 11:42 · 3 min ago");

  const byId = Object.fromEntries(view.details.map((detail) => [detail.id, detail]));
  assert.equal(byId.feels.value, "9°");
  assert.equal(byId.feels.note, "Feels colder than the air temperature");
  assert.equal(byId.humidity.value, "82%");
  assert.equal(byId.wind.value, "14 mph SW");
  assert.equal(byId.wind.note, "Gusts up to 28 mph");
  assert.equal(byId.precipitation.value, "4.2 mm");
  assert.equal(byId.precipitation.note, "90% chance today · 0.3 mm now");
  assert.equal(byId.sun.value, "07:02");
  assert.equal(byId.sun.note, "Sunset 16:38");
  assert.equal(byId.uv.value, "1.2");
});

test("hourly forecast starts at the current hour in the location's zone with a Now cell", () => {
  const view = buildView(readyState(), NOW);
  assert.equal(view.hours.length, 24);
  assert.equal(view.hours[0].time, "Now");
  assert.equal(view.hours[0].temperature, "11°");
  assert.equal(view.hours[1].time, "12:00");
  assert.equal(view.hours[12].time, "23:00");
  assert.equal(view.hours[13].time, "00:00");
  assert.equal(view.hours[1].description, "12:00, Overcast, 11°, 5% chance of precipitation");
  assert.equal(view.hours[0].chance, "70%");
  assert.equal(view.hours[1].chance, "", "chances under 10% are not drawn");
});

test("the hourly start follows the zone, not the viewer's clock", () => {
  const raw = rawForecast({ timezone: "Asia/Tokyo" });
  const tokyoNow = Date.UTC(2026, 9, 31, 11, 45); // 20:45 in Tokyo
  const view = buildView(readyState({ fetchedAt: tokyoNow - minute }, raw), tokyoNow);
  assert.equal(view.hours[0].time, "Now");
  assert.equal(view.hours[1].time, "21:00");
  assert.equal(view.hours[4].time, "00:00");
});

test("daily forecast begins today and labels days", () => {
  const view = buildView(readyState(), NOW);
  assert.equal(view.days.length, 7);
  assert.equal(view.days[0].label, "Today");
  assert.equal(view.days[1].label, "Sun");
  assert.equal(view.days[5].low, "-1°");
  assert.ok(view.days.every((day) => day.barFrom >= 0 && day.barTo <= 100 && day.barFrom <= day.barTo));
  assert.match(view.days[0].description, /^Sat 31 Oct, Light rain, high 12°, low 7°/);
});

test("past days are not listed once the date moves on", () => {
  const nextDay = NOW + 26 * 60 * minute;
  const view = buildView(readyState({ fetchedAt: nextDay - minute }), nextDay);
  assert.equal(view.days[0].label, "Today");
  assert.equal(view.days[0].key, "2026-11-01");
  assert.equal(view.days.length, 6);
});

test("data older than 30 minutes is labelled as saved", () => {
  const view = buildView(readyState({ fetchedAt: NOW - 2 * 60 * minute }), NOW);
  assert.equal(view.stale, true);
  assert.equal(view.banner.tone, "warning");
  assert.equal(view.banner.text, "Showing a saved forecast from 2 h ago.");
  assert.equal(view.hero.basis, "Saved forecast for 11:00");
  assert.equal(view.hero.feelsLike, null);
  assert.match(view.updated, /^Updated 09:45 · 2 h ago$/);
});

test("saved data does not present old observations as current", () => {
  const model = parseForecast(rawForecast());
  const now = currentConditions(model, NOW - 5 * 60 * minute, NOW);
  assert.equal(now.observed, false);
  assert.equal(now.feelsLike, null);
  const view = buildView(readyState({ fetchedAt: NOW - 5 * 60 * minute }), NOW);
  const byId = Object.fromEntries(view.details.map((detail) => [detail.id, detail]));
  assert.equal(byId.humidity.value, "–");
  assert.equal(byId.wind.value, "–");
  assert.equal(view.hours[0].time, "11:00", "no Now cell for saved data");
});

test("saved data picks day or night from sunrise and sunset", () => {
  const model = parseForecast(rawForecast());
  const evening = Date.UTC(2026, 9, 31, 19, 0);
  assert.equal(currentConditions(model, NOW - 6 * 60 * minute, evening).isDay, false);
  const noon = Date.UTC(2026, 9, 31, 12, 0);
  assert.equal(currentConditions(model, NOW - 6 * 60 * minute, noon).isDay, true);
});

test("a failed refresh keeps saved data and says why", () => {
  const failure = new WeatherError("network");
  const view = buildView(readyState({ failure, fetchedAt: NOW - 3 * 60 * minute }), NOW);
  assert.equal(view.banner.tone, "warning");
  assert.match(view.banner.text, /Could not reach the weather service/);
  assert.match(view.banner.text, /Showing the saved forecast from 3 h ago/);
});

test("a rate-limit pause notice is shown with the failure instead of being hidden by it", () => {
  const failure = new WeatherError("rate-limit");
  const notice = "The weather service asked for a pause. Waiting before the next request.";
  const withData = buildView(readyState({ failure, notice }), NOW);
  assert.match(withData.banner.text, /limiting requests/);
  assert.match(withData.banner.text, /asked for a pause/);
  const noData = buildView({ phase: "error", location, model: null, failure, notice, lookup: {} }, NOW);
  assert.match(noData.banner.text, /asked for a pause/);
});

test("with no data a failure is an error and no forecast is drawn", () => {
  const view = buildView(
    { phase: "error", location, model: null, fetchedAt: null, failure: new WeatherError("server"), lookup: {}, notice: null },
    NOW,
  );
  assert.equal(view.mode, "error");
  assert.equal(view.banner.tone, "error");
  assert.equal(view.hero, null);
  assert.deepEqual(view.hours, []);
  assert.equal(view.scene.kind, "idle");
});

test("empty and loading states carry no weather values", () => {
  const empty = buildView({ phase: "empty", location: null, model: null }, NOW);
  assert.equal(empty.mode, "empty");
  assert.equal(empty.hero, null);
  const loading = buildView({ phase: "loading", location, model: null, failure: null }, NOW);
  assert.equal(loading.mode, "loading");
  assert.equal(loading.hero, null);
  assert.equal(loading.days.length + loading.hours.length + loading.details.length, 0);
  assert.equal(loading.scene.kind, "idle");
});

test("a pending refresh over old data says it is updating", () => {
  const view = buildView(readyState({ refreshing: true, fetchedAt: NOW - 60 * minute }), NOW);
  assert.equal(view.banner.text, "Showing a saved forecast from 1 h ago while updating.");
});

test("synthetic data is always labelled and never shows an age", () => {
  const view = buildView({ ...readyState(), synthetic: true }, NOW);
  assert.equal(view.synthetic, true);
  assert.equal(view.banner.tone, "synthetic");
  assert.match(view.banner.text, /Synthetic preview/);
  assert.match(view.updated, /Synthetic/);
});

test("night scenes come from the observation's is_day flag", () => {
  const view = buildView(readyState({}, rawForecast({ code: 0, isDay: 0 })), NOW);
  assert.equal(view.scene.kind, "clear");
  assert.equal(view.scene.time, "night");
});

test("Northern Ireland postcodes get the data licence note", () => {
  const belfast = { ...location, country: "Northern Ireland" };
  assert.equal(buildView(readyState({ location: belfast }), NOW).northernIreland, true);
  assert.equal(buildView(readyState(), NOW).northernIreland, false);
});

test("helpers describe UV and feels-like", () => {
  assert.equal(uvLabel(1), "Low");
  assert.equal(uvLabel(4), "Moderate");
  assert.equal(uvLabel(7), "High");
  assert.equal(uvLabel(9), "Very high");
  assert.equal(uvLabel(12), "Extreme");
  assert.equal(feelsComparison(5, 10), "Feels colder than the air temperature");
  assert.equal(feelsComparison(14, 10), "Feels warmer than the air temperature");
  assert.equal(feelsComparison(10, 10.4), "Close to the air temperature");
});
