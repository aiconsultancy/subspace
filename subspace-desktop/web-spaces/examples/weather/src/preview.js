import { addDays, addHours, dateOf, hourOf, zonedNow } from "./time.js";

// Synthetic data for checking each scene by eye: ?preview=rain or
// ?preview=clear-night. It is invented, never cached, and always labelled.

const PREVIEW_CODES = Object.freeze({
  clear: 0,
  partly: 2,
  cloudy: 3,
  fog: 45,
  drizzle: 53,
  rain: 63,
  heavyrain: 65,
  snow: 73,
  storm: 95,
});

export const PREVIEW_NAMES = Object.freeze(
  Object.keys(PREVIEW_CODES).flatMap((name) => [name, `${name}-night`]),
);

const PREVIEW_ZONE = "Europe/London";
const SUNRISE = "07:30";
const SUNSET = "17:00";

/** Preview name from a URL query string, or null when absent or unknown. */
export function previewNameFromSearch(search) {
  const name = new URLSearchParams(search).get("preview")?.toLowerCase();
  return name && PREVIEW_NAMES.includes(name) ? name : null;
}

/**
 * A complete state-shaped object holding invented data for a named scene.
 * @param {string} name one of PREVIEW_NAMES
 * @param {number} nowMs
 */
export function buildPreview(name, nowMs) {
  const night = name.endsWith("-night");
  const base = PREVIEW_CODES[night ? name.slice(0, -6) : name];
  const localNow = zonedNow(nowMs, PREVIEW_ZONE);
  const today = dateOf(localNow);
  const startHour = `${hourOf(localNow)}:00`;

  const hours = Array.from({ length: 48 }, (_, i) => {
    const time = addHours(startHour, i);
    const clock = Number(time.slice(11, 13));
    const daylight = i === 0 ? !night : clock >= 7 && clock < 17;
    const settles = i > 8 && i % 7 < 3;
    return {
      time,
      temperature: 8 + 5 * Math.sin(((clock - 9) / 24) * 2 * Math.PI) + (night ? -2 : 0),
      precipitationChance: base >= 51 && !settles ? 60 + ((i * 7) % 35) : (i * 5) % 20,
      code: settles ? 3 : base,
      isDay: daylight,
    };
  });

  const dayCodes = [base, base, 3, 2, base, 1, 3];
  const days = dayCodes.map((code, i) => ({
    date: addDays(today, i),
    code,
    high: 13 + ((i * 3) % 5),
    low: 6 + ((i * 2) % 4),
    feelsHigh: 11 + ((i * 3) % 5),
    feelsLow: 4 + ((i * 2) % 4),
    precipitationSum: code >= 51 ? 4.2 + i : 0.2,
    precipitationChance: code >= 51 ? 70 : 15,
    sunrise: `${addDays(today, i)}T${SUNRISE}`,
    sunset: `${addDays(today, i)}T${SUNSET}`,
    uvIndex: 1.5 + i * 0.5,
    windMax: 14 + i,
  }));

  return {
    phase: "ready",
    synthetic: true,
    refreshing: false,
    failure: null,
    notice: null,
    lookup: { pending: false, error: null },
    fetchedAt: nowMs,
    location: {
      postcode: "Preview",
      latitude: 0,
      longitude: 0,
      name: "Sample location",
      area: ["Invented data"],
      country: null,
    },
    model: {
      timezone: PREVIEW_ZONE,
      current: {
        time: localNow,
        temperature: hours[0].temperature,
        feelsLike: hours[0].temperature - 2,
        humidity: base >= 51 ? 88 : 58,
        precipitation: base >= 51 ? 1.4 : 0,
        code: base,
        isDay: !night,
        windSpeed: base === 95 ? 28 : 11,
        windDirection: 230,
        windGusts: base === 95 ? 46 : 19,
      },
      hours,
      days,
    },
  };
}
