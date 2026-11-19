import assert from "node:assert/strict";
import { test } from "node:test";
import { forecastUrl, parseForecast } from "../src/forecast.js";
import { rawForecast } from "./fixtures.js";

test("forecastUrl asks Open-Meteo for the needed fields in the location's zone", () => {
  const url = new URL(forecastUrl({ latitude: 51.501009, longitude: -0.141588 }));
  assert.equal(url.origin, "https://api.open-meteo.com");
  assert.equal(url.pathname, "/v1/forecast");
  assert.equal(url.searchParams.get("latitude"), "51.5010");
  assert.equal(url.searchParams.get("longitude"), "-0.1416");
  assert.equal(url.searchParams.get("timezone"), "auto");
  assert.equal(url.searchParams.get("wind_speed_unit"), "mph");
  assert.equal(url.searchParams.get("forecast_days"), "7");
  for (const field of ["apparent_temperature", "relative_humidity_2m", "precipitation", "wind_gusts_10m"]) {
    assert.ok(url.searchParams.get("current").includes(field), `current includes ${field}`);
  }
  assert.ok(url.searchParams.get("daily").includes("sunrise"));
  assert.ok(url.searchParams.get("hourly").includes("precipitation_probability"));
});

test("forecastUrl sends no postcode or other identifying text", () => {
  const url = forecastUrl({ latitude: 51.5, longitude: -0.12 });
  assert.ok(!/postcode/i.test(url));
});

test("parseForecast reduces a response to the app model", () => {
  const model = parseForecast(rawForecast());
  assert.equal(model.timezone, "Europe/London");
  assert.equal(model.current.temperature, 11.4);
  assert.equal(model.current.feelsLike, 8.6);
  assert.equal(model.current.humidity, 82);
  assert.equal(model.current.isDay, true);
  assert.equal(model.hours.length, 168);
  assert.equal(model.hours[0].time, "2026-10-31T00:00");
  assert.equal(model.days.length, 7);
  assert.equal(model.days[0].high, 12.1);
  assert.equal(model.days[0].sunrise, "2026-10-31T07:02");
  assert.equal(model.days[5].low, -1.4);
});

test("parseForecast turns missing values into null instead of inventing them", () => {
  const raw = rawForecast();
  raw.current.apparent_temperature = null;
  delete raw.current.relative_humidity_2m;
  raw.daily.uv_index_max = [];
  const model = parseForecast(raw);
  assert.equal(model.current.feelsLike, null);
  assert.equal(model.current.humidity, null);
  assert.equal(model.days[0].uvIndex, null);
});

test("parseForecast drops entries with malformed times", () => {
  const raw = rawForecast();
  raw.hourly.time[3] = "not a time";
  assert.equal(parseForecast(raw).hours.length, 167);
});

test("parseForecast rejects unusable responses", () => {
  const cases = {
    "not an object": null,
    "reported error": { error: true, reason: "Too many requests" },
    "unknown zone": rawForecast({ timezone: "Mars/Olympus" }),
    "no current": { ...rawForecast(), current: undefined },
    "no daily": { ...rawForecast(), daily: undefined },
    "no days": { ...rawForecast(), daily: { time: [] } },
    "no temperature": (() => {
      const raw = rawForecast();
      raw.current.temperature_2m = "warm";
      return raw;
    })(),
    "no weather code": (() => {
      const raw = rawForecast();
      delete raw.current.weather_code;
      return raw;
    })(),
  };
  for (const [name, json] of Object.entries(cases)) {
    assert.throws(() => parseForecast(json), { kind: "bad-data" }, name);
  }
});
