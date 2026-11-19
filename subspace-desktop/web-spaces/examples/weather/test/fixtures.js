// Synthetic provider responses. Values are invented and shaped like the real
// services; no test contacts a network.

/** 2026-10-31 11:45 in London (GMT, UTC+0 after the 25 October clock change). */
export const NOW = Date.UTC(2026, 9, 31, 11, 45);

function hourlyTimes(firstDate, days) {
  const times = [];
  const start = new Date(`${firstDate}T00:00:00Z`);
  for (let i = 0; i < days * 24; i += 1) {
    const moment = new Date(start.getTime() + i * 3600_000);
    times.push(`${moment.toISOString().slice(0, 13)}:00`);
  }
  return times;
}

/** Open-Meteo-shaped response for London on 31 October 2026. */
export function rawForecast({ code = 61, isDay = 1, timezone = "Europe/London", firstDate = "2026-10-31" } = {}) {
  const times = hourlyTimes(firstDate, 7);
  const dates = Array.from({ length: 7 }, (_, i) =>
    new Date(Date.UTC(2026, 9, 31 + i)).toISOString().slice(0, 10),
  );
  return {
    latitude: 51.5,
    longitude: -0.12,
    timezone,
    current: {
      time: "2026-10-31T11:45",
      interval: 900,
      temperature_2m: 11.4,
      relative_humidity_2m: 82,
      apparent_temperature: 8.6,
      is_day: isDay,
      precipitation: 0.3,
      weather_code: code,
      wind_speed_10m: 14.2,
      wind_direction_10m: 225,
      wind_gusts_10m: 27.5,
    },
    hourly: {
      time: times,
      temperature_2m: times.map((_, i) => 8 + (i % 24) / 4),
      precipitation_probability: times.map((_, i) => (i % 24 < 12 ? 70 : 5)),
      weather_code: times.map((_, i) => (i % 24 < 12 ? 61 : 3)),
      is_day: times.map((time) => {
        const hour = Number(time.slice(11, 13));
        return hour >= 7 && hour < 17 ? 1 : 0;
      }),
    },
    daily: {
      time: dates,
      weather_code: [61, 63, 3, 2, 0, 71, 95],
      temperature_2m_max: [12.1, 13.4, 11, 10.2, 9.9, 1.2, 12],
      temperature_2m_min: [7.2, 8.8, 6.1, 4.4, 2.2, -1.4, 6],
      apparent_temperature_max: [9.5, 11, 9, 8, 7, -2, 10],
      apparent_temperature_min: [4.5, 6, 3, 1, 0, -5, 4],
      precipitation_sum: [4.2, 8.8, 0.4, 0, 0, 3.1, 12.6],
      precipitation_probability_max: [90, 95, 30, 5, 0, 80, 95],
      sunrise: dates.map((date) => `${date}T07:02`),
      sunset: dates.map((date) => `${date}T16:38`),
      uv_index_max: [1.2, 0.9, 1.5, 2.1, 2.4, 0.6, 0.8],
      wind_speed_10m_max: [20, 26, 15, 12, 10, 18, 33],
    },
  };
}

/** Postcodes.io-shaped response. */
export function rawPostcode(overrides = {}) {
  return {
    status: 200,
    result: {
      postcode: "SW1A 1AA",
      latitude: 51.501009,
      longitude: -0.141588,
      country: "England",
      region: "London",
      admin_district: "Westminster",
      admin_ward: "St James's",
      parish: null,
      ...overrides,
    },
  };
}

/** Minimal fetch response. */
export function response(body, { status = 200, headers = {} } = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (name) => headers[name.toLowerCase()] ?? null },
    json: async () => {
      if (body instanceof Error) throw body;
      return body;
    },
  };
}

/** In-memory Storage stand-in. */
export function memoryStorage(initial = {}) {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (key) => (data.has(key) ? data.get(key) : null),
    setItem: (key, value) => void data.set(key, String(value)),
    removeItem: (key) => void data.delete(key),
  };
}
