// Fixtures: the sample fleet at a fixed time, so tests are deterministic.

import { buildSampleFleet } from "../src/sample.js";

export const NOW = Date.UTC(2026, 10, 19, 12, 0, 0);

export function sampleFleet() {
  return buildSampleFleet(NOW);
}

/** A reducer state holding the whole sample fleet. */
export async function loadedState() {
  const { initialFleetState, reduceFleet } = await import("../src/reducer.js");
  const { fleet, ships } = sampleFleet();
  let state = reduceFleet(initialFleetState(), { type: "fleet", data: fleet }, NOW);
  for (const detail of Object.values(ships)) state = reduceFleet(state, { type: "ship", data: detail }, NOW);
  return state;
}

/** A deterministic random source. */
export function seeded(seed = 7) {
  let value = seed;
  return () => {
    value = (value * 16807) % 2147483647;
    return (value - 1) / 2147483646;
  };
}
