import { compass, describeCondition, sceneFor } from "./conditions.js";
import { UNITS } from "./forecast.js";
import { isStale } from "./refresh.js";
import {
  clockOf,
  dateOf,
  dayLabel,
  describeAge,
  hourOf,
  longDate,
  weekdayName,
  zonedNow,
} from "./time.js";

/** Observed conditions stay "now" for this long after a fetch. */
const OBSERVED_FOR_MS = 45 * 60 * 1000;
export const HOURS_SHOWN = 24;

const DASH = "–";

export function degrees(value) {
  return Number.isFinite(value) ? `${Math.round(value)}°` : DASH;
}
const whole = (value, suffix = "") =>
  Number.isFinite(value) ? `${Math.round(value)}${suffix}` : DASH;
const tenths = (value) =>
  Number.isFinite(value) ? `${Math.round(value * 10) / 10}` : DASH;
const chanceText = (chance) =>
  Number.isFinite(chance) && chance >= 10 ? `${Math.round(chance)}%` : "";

export function uvLabel(index) {
  if (!Number.isFinite(index)) return "";
  if (index < 3) return "Low";
  if (index < 6) return "Moderate";
  if (index < 8) return "High";
  if (index < 11) return "Very high";
  return "Extreme";
}

export function feelsComparison(feelsLike, actual) {
  if (!Number.isFinite(feelsLike) || !Number.isFinite(actual)) return "";
  const difference = Math.round(feelsLike) - Math.round(actual);
  if (difference <= -2) return "Feels colder than the air temperature";
  if (difference >= 2) return "Feels warmer than the air temperature";
  return "Close to the air temperature";
}

function isDayAt(model, localNow) {
  const today = model.days.find((day) => day.date === dateOf(localNow));
  if (today?.sunrise && today?.sunset) {
    return localNow >= today.sunrise && localNow < today.sunset;
  }
  return model.current.isDay;
}

/**
 * Conditions to present as "now". Fresh data uses the observation. Older data
 * uses the forecast for the current hour and says so, never the old observation.
 */
export function currentConditions(model, fetchedAt, nowMs) {
  const localNow = zonedNow(nowMs, model.timezone);
  if (nowMs - fetchedAt <= OBSERVED_FOR_MS) {
    const { current } = model;
    return {
      observed: true,
      time: current.time,
      code: current.code,
      isDay: current.isDay,
      temperature: current.temperature,
      feelsLike: current.feelsLike,
    };
  }
  const hour = model.hours.find((entry) => hourOf(entry.time) === hourOf(localNow));
  return {
    observed: false,
    time: hour?.time ?? localNow,
    code: hour?.code ?? model.current.code,
    isDay: isDayAt(model, localNow),
    temperature: hour?.temperature ?? model.current.temperature,
    feelsLike: null,
  };
}

function updatedText(model, fetchedAt, nowMs) {
  const clock = zonedNow(fetchedAt, model.timezone);
  const sameDay = dateOf(clock) === dateOf(zonedNow(nowMs, model.timezone));
  const stamp = sameDay ? clockOf(clock) : `${weekdayName(dateOf(clock))} ${clockOf(clock)}`;
  return `Updated ${stamp} · ${describeAge(nowMs - fetchedAt)}`;
}

function hoursView(model, now, localNow) {
  const start = hourOf(localNow);
  return model.hours
    .filter((entry) => hourOf(entry.time) >= start)
    .slice(0, HOURS_SHOWN)
    .map((entry, index) => {
      const isNow = index === 0 && now.observed && hourOf(entry.time) === start;
      const code = isNow ? now.code : entry.code;
      const condition = describeCondition(code);
      const temperature = isNow ? now.temperature : entry.temperature;
      const chance = entry.precipitationChance;
      const time = isNow ? "Now" : clockOf(entry.time);
      return {
        key: entry.time,
        time,
        icon: { kind: condition.kind, isDay: isNow ? now.isDay : entry.isDay },
        chance: chanceText(chance),
        temperature: degrees(temperature),
        description: [
          time,
          condition.label,
          degrees(temperature),
          Number.isFinite(chance) ? `${Math.round(chance)}% chance of precipitation` : "",
        ].filter(Boolean).join(", "),
      };
    });
}

function daysView(model, localNow) {
  const today = dateOf(localNow);
  const upcoming = model.days.filter((day) => day.date >= today);
  const lows = upcoming.map((day) => day.low).filter(Number.isFinite);
  const highs = upcoming.map((day) => day.high).filter(Number.isFinite);
  const min = Math.min(...lows);
  const max = Math.max(...highs);
  const span = Math.max(max - min, 1);
  const percent = (value) =>
    Number.isFinite(value) && Number.isFinite(min) ? ((value - min) / span) * 100 : 0;

  return upcoming.map((day) => {
    const condition = describeCondition(day.code);
    const chance = day.precipitationChance;
    return {
      key: day.date,
      label: dayLabel(day.date, today),
      icon: { kind: condition.kind, isDay: true },
      chance: chanceText(chance),
      low: degrees(day.low),
      high: degrees(day.high),
      barFrom: percent(day.low),
      barTo: percent(day.high),
      description: [
        longDate(day.date),
        condition.label,
        `high ${degrees(day.high)}`,
        `low ${degrees(day.low)}`,
        Number.isFinite(chance) ? `${Math.round(chance)}% chance of precipitation` : "",
      ].filter(Boolean).join(", "),
    };
  });
}

function detailsView(model, now, today) {
  const { current } = model;
  const saved = "Not available for saved data";
  return [
    {
      id: "feels",
      title: "Feels like",
      value: degrees(now.feelsLike),
      note: now.observed ? feelsComparison(now.feelsLike, now.temperature) : saved,
    },
    {
      id: "precipitation",
      title: "Precipitation",
      value: Number.isFinite(today?.precipitationSum)
        ? `${tenths(today.precipitationSum)} ${UNITS.precipitation}`
        : DASH,
      note: [
        Number.isFinite(today?.precipitationChance)
          ? `${whole(today.precipitationChance)}% chance today`
          : "",
        now.observed && Number.isFinite(current.precipitation)
          ? `${tenths(current.precipitation)} ${UNITS.precipitation} now`
          : "",
      ].filter(Boolean).join(" · "),
    },
    {
      id: "humidity",
      title: "Humidity",
      value: now.observed ? whole(current.humidity, "%") : DASH,
      note: now.observed ? "Relative humidity" : saved,
    },
    {
      id: "wind",
      title: "Wind",
      value: now.observed && Number.isFinite(current.windSpeed)
        ? `${whole(current.windSpeed)} ${UNITS.wind} ${compass(current.windDirection)}`.trim()
        : DASH,
      note: now.observed && Number.isFinite(current.windGusts)
        ? `Gusts up to ${whole(current.windGusts)} ${UNITS.wind}`
        : now.observed ? "" : saved,
    },
    {
      id: "sun",
      title: "Sunrise",
      value: today?.sunrise ? clockOf(today.sunrise) : DASH,
      note: today?.sunset ? `Sunset ${clockOf(today.sunset)}` : "",
    },
    {
      id: "uv",
      title: "UV index",
      value: tenths(today?.uvIndex),
      note: Number.isFinite(today?.uvIndex) ? `${uvLabel(today.uvIndex)} · highest today` : "",
    },
  ];
}

function bannerFor(state, ageText, stale, synthetic) {
  if (synthetic) {
    return {
      tone: "synthetic",
      text: "Synthetic preview. This is invented sample data, not a live forecast.",
    };
  }
  const withNotice = (text) => (state.notice ? `${text} ${state.notice}` : text);
  if (state.failure && state.model) {
    return {
      tone: "warning",
      text: withNotice(`${state.failure.message} Showing the saved forecast from ${ageText}.`),
    };
  }
  if (state.failure) return { tone: "error", text: withNotice(state.failure.message) };
  if (stale) {
    return {
      tone: "warning",
      text: withNotice(
        state.refreshing
          ? `Showing a saved forecast from ${ageText} while updating.`
          : `Showing a saved forecast from ${ageText}.`,
      ),
    };
  }
  if (state.notice) return { tone: "info", text: state.notice };
  return null;
}

/**
 * Everything the page renders, as plain data.
 * @param {import("./controller.js").WeatherState & { synthetic?: boolean }} state
 * @param {number} nowMs
 */
export function buildView(state, nowMs) {
  const synthetic = state.synthetic === true;
  const { model, location } = state;

  if (!model || !location) {
    const failed = state.phase === "error";
    return {
      mode: state.phase === "empty" ? "empty" : failed ? "error" : "loading",
      synthetic: false,
      scene: { kind: "idle", time: "night", intensity: 0, label: "" },
      banner: failed && state.failure
        ? { tone: "error", text: state.notice ? `${state.failure.message} ${state.notice}` : state.failure.message }
        : null,
      location,
      hero: null,
      hours: [],
      days: [],
      details: [],
      updated: "",
    };
  }

  // Invented preview data is never presented as saved or aged.
  const fetchedAt = synthetic ? nowMs : (state.fetchedAt ?? nowMs);
  const stale = !synthetic && isStale(fetchedAt, nowMs);
  const localNow = zonedNow(nowMs, model.timezone);
  const now = currentConditions(model, fetchedAt, nowMs);
  const today = model.days.find((day) => day.date === dateOf(localNow));
  const condition = describeCondition(now.code);
  const ageText = describeAge(nowMs - fetchedAt);

  return {
    mode: "ready",
    synthetic,
    stale,
    scene: sceneFor(now.code, now.isDay),
    banner: bannerFor(state, ageText, stale, synthetic),
    location,
    hero: {
      title: location.name,
      subtitle: [location.postcode, ...location.area].filter(Boolean).join(" · "),
      basis: now.observed ? "Now" : `Saved forecast for ${clockOf(now.time)}`,
      temperature: degrees(now.temperature),
      condition: condition.label,
      high: degrees(today?.high),
      low: degrees(today?.low),
      feelsLike: now.observed ? degrees(now.feelsLike) : null,
    },
    hours: hoursView(model, now, localNow),
    days: daysView(model, localNow),
    details: detailsView(model, now, today),
    updated: synthetic
      ? "Synthetic sample generated on this device"
      : updatedText(model, fetchedAt, nowMs),
    northernIreland: location.country === "Northern Ireland",
  };
}
