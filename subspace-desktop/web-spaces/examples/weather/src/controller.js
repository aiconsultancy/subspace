import { WeatherError, toWeatherError } from "./errors.js";
import { normalisePostcode } from "./postcode.js";
import {
  MANUAL_COOLDOWN_MS,
  REFRESH_INTERVAL_MS,
  RETRY_COOLDOWN_MS,
  REUSE_WINDOW_MS,
  retryDelay,
} from "./refresh.js";

/**
 * @typedef {object} WeatherState
 * @property {"empty" | "loading" | "ready" | "error"} phase
 *   empty: no location. loading: location but no data yet. ready: has data
 *   (possibly stale, see `failure`). error: location but no data.
 * @property {object | null} location
 * @property {object | null} model
 * @property {number | null} fetchedAt
 * @property {boolean} refreshing
 * @property {WeatherError | null} failure Last failed refresh, cleared on success.
 * @property {{ pending: boolean, error: WeatherError | null }} lookup
 * @property {string | null} notice Short status text that is not an error.
 */

/**
 * Owns location, forecast, cache and refresh timing. It has no DOM access.
 * @param {object} deps
 * @param {ReturnType<import("./api.js").createClient>} deps.client
 * @param {ReturnType<import("./storage.js").createStore>} deps.store
 * @param {() => number} [deps.now]
 * @param {{ setTimeout: Function, clearTimeout: Function }} [deps.timers]
 * @param {(state: WeatherState) => void} deps.onChange
 */
export function createController({
  client,
  store,
  onChange,
  now = Date.now,
  timers = { setTimeout: globalThis.setTimeout.bind(globalThis), clearTimeout: globalThis.clearTimeout.bind(globalThis) },
}) {
  /** @type {WeatherState} */
  let state = {
    phase: "empty",
    location: null,
    model: null,
    fetchedAt: null,
    refreshing: false,
    failure: null,
    lookup: { pending: false, error: null },
    notice: null,
  };
  let hidden = false;
  let disposed = false;
  let timer = null;
  let nextAt = 0;
  let failures = 0;
  let lastRequestAt = -Infinity;
  let inflight = null;
  let forecastAbort = null;
  let forecastSeq = 0;
  let lookupAbort = null;
  let lookupSeq = 0;

  function update(patch) {
    state = { ...state, ...patch };
    if (!disposed) onChange(state);
  }

  function cancelTimer() {
    if (timer !== null) timers.clearTimeout(timer);
    timer = null;
  }

  function schedule() {
    cancelTimer();
    if (hidden || disposed || !state.location) return;
    timer = timers.setTimeout(() => {
      timer = null;
      refresh({ manual: false });
    }, Math.max(0, nextAt - now()));
  }

  /** Refresh now when due, otherwise wait for the next scheduled time. */
  function resume() {
    if (hidden || disposed || !state.location) return;
    if (now() >= nextAt) refresh({ manual: false });
    else schedule();
  }

  function refresh({ manual = false } = {}) {
    const { location } = state;
    if (!location) return Promise.resolve();
    if (inflight) return inflight;

    const started = now();
    if (manual && state.failure?.kind === "rate-limit" && started < nextAt) {
      update({ notice: "The weather service asked for a pause. Waiting before the next request." });
      return Promise.resolve();
    }
    const retrying = Boolean(state.failure);
    const cooldown = retrying ? RETRY_COOLDOWN_MS : MANUAL_COOLDOWN_MS;
    if (manual && (state.model || retrying) && started - lastRequestAt < cooldown) {
      update({
        notice: retrying
          ? "Just tried. Wait a few seconds before trying again."
          : "Already up to date. You can refresh again in a minute.",
      });
      return Promise.resolve();
    }

    lastRequestAt = started;
    const seq = ++forecastSeq;
    const controller = new AbortController();
    forecastAbort = controller;
    update({ refreshing: true, notice: null, phase: state.model ? "ready" : "loading" });

    const run = request(location, controller, seq).then(() => {
      if (inflight === run) inflight = null;
      if (seq === forecastSeq) schedule();
    });
    inflight = run;
    return run;
  }

  /** One forecast request. A result for a replaced location or request is dropped. */
  async function request(location, controller, seq) {
    try {
      const { raw, model } = await client.fetchForecast(location, { signal: controller.signal });
      if (seq !== forecastSeq) return;
      const fetchedAt = now();
      store.saveForecast(location, raw, fetchedAt);
      failures = 0;
      nextAt = fetchedAt + REFRESH_INTERVAL_MS;
      update({ phase: "ready", model, fetchedAt, refreshing: false, failure: null });
    } catch (caught) {
      if (seq !== forecastSeq) return;
      const failure = toWeatherError(caught);
      if (failure.kind === "aborted") {
        update({ refreshing: false });
        return;
      }
      failures += 1;
      nextAt = now() + retryDelay(failures, failure);
      update({ refreshing: false, failure, phase: state.model ? "ready" : "error" });
    }
  }

  function adopt(location) {
    forecastAbort?.abort();
    forecastSeq += 1;
    inflight = null;
    failures = 0;
    lastRequestAt = -Infinity;

    const cached = store.loadForecast(location);
    nextAt = cached ? cached.fetchedAt + REUSE_WINDOW_MS : 0;
    update({
      location,
      model: cached?.model ?? null,
      fetchedAt: cached?.fetchedAt ?? null,
      phase: cached ? "ready" : "loading",
      refreshing: false,
      failure: null,
      notice: null,
    });
    resume();
  }

  return {
    get state() {
      return state;
    },

    /** Load the saved location and show its cached forecast, then refresh if due. */
    start() {
      const location = store.loadLocation();
      if (location) adopt(location);
      else update({ phase: "empty" });
    },

    /**
     * Resolve a postcode and switch to it. Starting a new lookup cancels the
     * previous one, so an older answer can never replace a newer choice.
     */
    async submitPostcode(input) {
      lookupAbort?.abort();
      const seq = ++lookupSeq;
      const compact = normalisePostcode(input);
      if (!compact) {
        update({ lookup: { pending: false, error: new WeatherError("invalid-postcode") } });
        return false;
      }
      const controller = new AbortController();
      lookupAbort = controller;
      update({ lookup: { pending: true, error: null } });
      try {
        const location = await client.lookupPostcode(compact, { signal: controller.signal });
        if (seq !== lookupSeq) return false;
        store.saveLocation(location);
        update({ lookup: { pending: false, error: null } });
        adopt(location);
        return true;
      } catch (caught) {
        if (seq !== lookupSeq) return false;
        const error = toWeatherError(caught);
        update({ lookup: { pending: false, error: error.kind === "aborted" ? null : error } });
        return false;
      }
    },

    refresh: (options) => refresh(options),

    /** Stop periodic work while the page is hidden; catch up when shown. */
    setHidden(value) {
      hidden = value;
      if (hidden) cancelTimer();
      else resume();
    },

    dispose() {
      disposed = true;
      cancelTimer();
      forecastAbort?.abort();
      lookupAbort?.abort();
    },
  };
}
