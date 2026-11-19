import { WeatherError } from "./errors.js";
import { isValidTimeZone } from "./time.js";

const FORECAST_API = "https://api.open-meteo.com/v1/forecast";
export const FORECAST_DAYS = 7;

const CURRENT_FIELDS = [
  "temperature_2m",
  "relative_humidity_2m",
  "apparent_temperature",
  "is_day",
  "precipitation",
  "weather_code",
  "wind_speed_10m",
  "wind_direction_10m",
  "wind_gusts_10m",
];
const HOURLY_FIELDS = [
  "temperature_2m",
  "precipitation_probability",
  "weather_code",
  "is_day",
];
const DAILY_FIELDS = [
  "weather_code",
  "temperature_2m_max",
  "temperature_2m_min",
  "apparent_temperature_max",
  "apparent_temperature_min",
  "precipitation_sum",
  "precipitation_probability_max",
  "sunrise",
  "sunset",
  "uv_index_max",
  "wind_speed_10m_max",
];

/** Units are fixed: degrees Celsius, miles per hour and millimetres. */
export const UNITS = Object.freeze({ temperature: "°C", wind: "mph", precipitation: "mm" });

/** Open-Meteo request URL for a location. */
export function forecastUrl({ latitude, longitude }) {
  const query = new URLSearchParams({
    latitude: latitude.toFixed(4),
    longitude: longitude.toFixed(4),
    current: CURRENT_FIELDS.join(","),
    hourly: HOURLY_FIELDS.join(","),
    daily: DAILY_FIELDS.join(","),
    timezone: "auto",
    forecast_days: String(FORECAST_DAYS),
    temperature_unit: "celsius",
    wind_speed_unit: "mph",
    precipitation_unit: "mm",
  });
  return `${FORECAST_API}?${query}`;
}

const number = (value) => (typeof value === "number" && Number.isFinite(value) ? value : null);
const wallTime = (value) =>
  typeof value === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/.test(value) ? value : null;
const calendarDate = (value) =>
  typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : null;

function column(section, name) {
  const values = section?.[name];
  return Array.isArray(values) ? values : [];
}

/**
 * Validate an Open-Meteo response and reduce it to the model the app renders.
 * Missing values become null; a response without usable current conditions or
 * daily entries is rejected.
 * @param {unknown} json
 * @throws WeatherError("bad-data")
 */
export function parseForecast(json) {
  if (!json || typeof json !== "object" || json.error === true) {
    throw new WeatherError("bad-data");
  }
  const { current, hourly, daily } = json;
  if (!isValidTimeZone(json.timezone) || !current || !daily) {
    throw new WeatherError("bad-data");
  }

  const currentTime = wallTime(current.time);
  const temperature = number(current.temperature_2m);
  const code = number(current.weather_code);
  if (!currentTime || temperature === null || code === null) {
    throw new WeatherError("bad-data");
  }

  const hours = column(hourly, "time").flatMap((time, i) => {
    const valid = wallTime(time);
    return valid
      ? [{
          time: valid,
          temperature: number(column(hourly, "temperature_2m")[i]),
          precipitationChance: number(column(hourly, "precipitation_probability")[i]),
          code: number(column(hourly, "weather_code")[i]),
          isDay: column(hourly, "is_day")[i] === 1,
        }]
      : [];
  });

  const days = column(daily, "time").flatMap((date, i) => {
    const valid = calendarDate(date);
    if (!valid) return [];
    const at = (name) => column(daily, name)[i];
    return [{
      date: valid,
      code: number(at("weather_code")),
      high: number(at("temperature_2m_max")),
      low: number(at("temperature_2m_min")),
      feelsHigh: number(at("apparent_temperature_max")),
      feelsLow: number(at("apparent_temperature_min")),
      precipitationSum: number(at("precipitation_sum")),
      precipitationChance: number(at("precipitation_probability_max")),
      sunrise: wallTime(at("sunrise")),
      sunset: wallTime(at("sunset")),
      uvIndex: number(at("uv_index_max")),
      windMax: number(at("wind_speed_10m_max")),
    }];
  });
  if (days.length === 0) throw new WeatherError("bad-data");

  return {
    timezone: json.timezone,
    current: {
      time: currentTime,
      temperature,
      feelsLike: number(current.apparent_temperature),
      humidity: number(current.relative_humidity_2m),
      precipitation: number(current.precipitation),
      code,
      isDay: current.is_day === 1,
      windSpeed: number(current.wind_speed_10m),
      windDirection: number(current.wind_direction_10m),
      windGusts: number(current.wind_gusts_10m),
    },
    hours,
    days,
  };
}
