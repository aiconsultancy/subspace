import { locationKey, validateLocation } from "./postcode.js";
import { parseForecast } from "./forecast.js";

const LOCATION_KEY = "subspace.weather.v1.location";
const FORECAST_KEY = "subspace.weather.v1.forecast";

/** Saved forecasts older than this are discarded rather than shown as stale. */
export const MAX_CACHE_AGE_MS = 24 * 60 * 60 * 1000;

/** Browser storage, or null when the page may not use it. */
export function browserStorage() {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

/**
 * Persist the chosen location and the last good forecast. Storage failures
 * (blocked, full, corrupt) never throw; the app then works from memory only.
 * @param {Pick<Storage, "getItem" | "setItem" | "removeItem"> | null} storage
 */
export function createStore(storage, now = Date.now) {
  function read(key) {
    try {
      const text = storage?.getItem(key);
      return text ? JSON.parse(text) : null;
    } catch {
      return null;
    }
  }
  function write(key, value) {
    try {
      storage?.setItem(key, JSON.stringify(value));
    } catch {
      // Storage unavailable or full: continue without persistence.
    }
  }
  function remove(key) {
    try {
      storage?.removeItem(key);
    } catch {
      // Nothing to clean up.
    }
  }

  return {
    loadLocation() {
      try {
        return validateLocation(read(LOCATION_KEY));
      } catch {
        return null;
      }
    },

    saveLocation(location) {
      write(LOCATION_KEY, location);
    },

    /**
     * The saved forecast for this location, re-validated, with its fetch time.
     * Returns null for another location, an old record or invalid data.
     */
    loadForecast(location) {
      const record = read(FORECAST_KEY);
      if (!record || record.key !== locationKey(location)) return null;
      const age = now() - record.fetchedAt;
      if (!Number.isFinite(age) || age < 0 || age > MAX_CACHE_AGE_MS) return null;
      try {
        return { model: parseForecast(record.raw), fetchedAt: record.fetchedAt };
      } catch {
        return null;
      }
    },

    saveForecast(location, raw, fetchedAt) {
      write(FORECAST_KEY, { key: locationKey(location), fetchedAt, raw });
    },

    clearForecast() {
      remove(FORECAST_KEY);
    },
  };
}
