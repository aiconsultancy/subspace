// Sample mode's source: feeds the simulation's events through the same path
// as the live stream, on the same cadence as the mock-up.

import { HttpError } from "./api.js";
import { createSampleActivitySim, createSampleSim } from "./sample.js";

export const SAMPLE_TICK_MS = 5600;
export const SAMPLE_FIRST_TICK_MS = 2600;
export const SAMPLE_HOST_MS = 10_000;
export const SAMPLE_HEARTBEAT_MS = 15_000;
/** How often the sample officer's running turn changes. */
export const SAMPLE_ACTIVITY_MS = 900;

export function createSampleSource({ onEvent, now = Date.now, random = Math.random, setTimer = setTimeout, clearTimer = clearTimeout, every = setInterval, stopEvery = clearInterval, variant = "colonial" }) {
  const sim = createSampleSim({ now, random, variant });
  const timers = [];
  let first = null;
  const emit = (events) => events.forEach((event) => onEvent(event));
  return {
    sim,
    start() {
      emit(sim.initial());
      first = setTimer(() => emit(sim.tick()), SAMPLE_FIRST_TICK_MS);
      timers.push(every(() => emit(sim.tick()), SAMPLE_TICK_MS));
      timers.push(every(() => emit(sim.hostTick()), SAMPLE_HOST_MS));
      timers.push(every(() => emit(sim.heartbeat()), SAMPLE_HEARTBEAT_MS));
    },
    stop() {
      clearTimer(first);
      timers.splice(0).forEach((timer) => stopEvery(timer));
    },
    readShip: async () => true,
    readCrew: async () => true,
    retryNow() {},
  };
}

/**
 * Sample officer activity: a client with the live client's activity reads,
 * and `agent-activity` events from a running turn, delivered as the live
 * stream would deliver them.
 */
export function createSampleActivitySource({ agent, role, backend, onEvent, onOpen, now = Date.now, every = setInterval, stopEvery = clearInterval }) {
  const sim = createSampleActivitySim({ agent, role, backend, now });
  let timer = null;
  const answer = (value) => (value === null ? Promise.reject(new HttpError(404)) : Promise.resolve(value));
  return {
    sim,
    client: {
      activity: (id, { turn } = {}) => answer(id === agent ? sim.snapshot(turn ?? null) : null),
      activityItemPage: (id, turn, item, page) => answer(id === agent ? sim.page(turn, item, page) : null),
    },
    start() {
      // As the live stream does: it answers after the view has subscribed, then the snapshot is read.
      Promise.resolve().then(() => onOpen?.());
      timer = every(() => sim.tick().forEach((event) => onEvent(event)), SAMPLE_ACTIVITY_MS);
    },
    stop() {
      stopEvery(timer);
      timer = null;
    },
  };
}
