/**
 * Maps WMO weather interpretation codes (as used by Open-Meteo) to a label,
 * a scene kind and a rain/snow intensity.
 *
 * Scene kinds: clear, partly, cloudy, fog, rain, snow, storm.
 */

const CONDITIONS = new Map([
  [0, ["Clear sky", "clear", 0]],
  [1, ["Mainly clear", "clear", 0]],
  [2, ["Partly cloudy", "partly", 0]],
  [3, ["Overcast", "cloudy", 0]],
  [45, ["Fog", "fog", 0]],
  [48, ["Freezing fog", "fog", 0]],
  [51, ["Light drizzle", "rain", 1]],
  [53, ["Drizzle", "rain", 1]],
  [55, ["Heavy drizzle", "rain", 2]],
  [56, ["Light freezing drizzle", "rain", 1]],
  [57, ["Freezing drizzle", "rain", 2]],
  [61, ["Light rain", "rain", 1]],
  [63, ["Rain", "rain", 2]],
  [65, ["Heavy rain", "rain", 3]],
  [66, ["Light freezing rain", "rain", 2]],
  [67, ["Freezing rain", "rain", 3]],
  [71, ["Light snow", "snow", 1]],
  [73, ["Snow", "snow", 2]],
  [75, ["Heavy snow", "snow", 3]],
  [77, ["Snow grains", "snow", 1]],
  [80, ["Light rain showers", "rain", 1]],
  [81, ["Rain showers", "rain", 2]],
  [82, ["Violent rain showers", "rain", 3]],
  [85, ["Light snow showers", "snow", 1]],
  [86, ["Heavy snow showers", "snow", 3]],
  [95, ["Thunderstorm", "storm", 2]],
  [96, ["Thunderstorm with hail", "storm", 3]],
  [99, ["Severe thunderstorm with hail", "storm", 3]],
]);

export const SCENE_KINDS = Object.freeze([
  "clear",
  "partly",
  "cloudy",
  "fog",
  "rain",
  "snow",
  "storm",
]);

/**
 * @param {number | null | undefined} code WMO weather code
 * @returns {{ label: string, kind: string, intensity: number, known: boolean }}
 */
export function describeCondition(code) {
  const entry = CONDITIONS.get(code);
  if (!entry) {
    return {
      label: "Conditions unavailable",
      kind: "cloudy",
      intensity: 0,
      known: false,
    };
  }
  const [label, kind, intensity] = entry;
  return { label, kind, intensity, known: true };
}

/**
 * Pick the full-window scene for a weather code.
 * @param {number | null | undefined} code
 * @param {boolean} isDay
 */
export function sceneFor(code, isDay) {
  const { kind, intensity, label } = describeCondition(code);
  return { kind, intensity, label, time: isDay ? "day" : "night" };
}

/** Wind direction in degrees to a 16-point compass name. */
export function compass(degrees) {
  if (!Number.isFinite(degrees)) return "";
  const names = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
    "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW",
  ];
  const index = Math.round((((degrees % 360) + 360) % 360) / 22.5) % 16;
  return names[index];
}
