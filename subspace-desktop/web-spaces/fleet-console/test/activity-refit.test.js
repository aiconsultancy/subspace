// Refits planned before the inline charge grows (SUB decisions 8346cf82,
// fbc47f97): a planned refit that keeps no window on show is dropped,
// also when the growth frees that refit itself (SUB-69).

import assert from "node:assert/strict";
import { test } from "node:test";
import { givenBack, grow, keyOf, modesOf, wholesOnShow } from "./activity-helpers.js";

/** Every `reserve` the budget is asked for, as [key, options]. */
function reservesOf(budget) {
  const log = [];
  const reserve = budget.reserve.bind(budget);
  budget.reserve = (key, bytes, onEvict, options) => {
    log.push([key, options ?? null]);
    return reserve(key, bytes, onEvict, options);
  };
  return log;
}

test("a planned refit the growth frees anyway is dropped: no handover is reserved for it, and it is freed whole", async () => {
  // Charon's probe A (S, C and R of 100, 600 and 100 KiB, R the latest
  // request), grown past everything the floor can keep beside R.
  const ids = ["is", "ic", "ir"];
  const { watcher, panel, requested } = await wholesOnShow(ids, [100, 600, 100], 20_000);
  try {
    const { budget } = watcher.view;
    const size = 1_200_000;
    assert.ok(givenBack(watcher, "ic") > 0, "C's refit gives something back, so it is planned");
    assert.ok(budget.fits(keyOf("ir"), budget.bytes(keyOf("ir")) + size), "R still fits beside the growth");
    const reserves = reservesOf(budget);
    requested.length = 0;
    await grow(watcher, panel, size);
    assert.deepEqual(
      reserves.filter(([key]) => key === keyOf("ic")),
      [],
      "no handover is reserved for C",
    );
    assert.deepEqual(modesOf(watcher, ids), { is: "Load in full", ic: "Load in full", ir: "whole" });
    assert.deepEqual(requested, [], "nothing reads");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});
