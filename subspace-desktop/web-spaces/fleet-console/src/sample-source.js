// Sample mode's source: feeds the simulation's events through the same path
// as the live stream, on the same cadence as the mock-up.

import { createSampleSim } from "./sample.js";

export const SAMPLE_TICK_MS = 5600;
export const SAMPLE_FIRST_TICK_MS = 2600;
export const SAMPLE_HOST_MS = 10_000;
export const SAMPLE_HEARTBEAT_MS = 15_000;

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
