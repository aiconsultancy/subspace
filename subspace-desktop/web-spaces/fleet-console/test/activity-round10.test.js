// Round 10 of https://github.com/aiconsultancy/subspace/pull/115 (SUB
// decision fbc47f97): Charon's round 9 delta probes, rebuilt as tests, with
// the cases around them, each named for the finding it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { givenBack, grow, keyOf, modesOf, windowsIn, wholesOnShow } from "./activity-helpers.js";

// ---------- P2-1 (Charon): plan from every whole on show, not only the ones named ----------

/**
 * Charon's probe A: S, C and R, wholes of 100, 600 and 100 KiB, R the latest
 * request. Growth that freeing S alone covers names only S, whose refit
 * gives nothing back; refitting C, never named, keeps every window.
 */
async function probeA(sizeOf, windows) {
  const ids = ["is", "ic", "ir"];
  const { watcher, panel, requested } = await wholesOnShow(ids, [100, 600, 100], 20_000);
  try {
    const { budget } = watcher.view;
    const size = sizeOf(budget.bytes(keyOf("is")));
    assert.ok(givenBack(watcher, "is") <= 0, "S's refit gives nothing back");
    assert.ok(givenBack(watcher, "ic") > size, "C's refit alone covers the growth");
    assert.deepEqual(budget.wouldFree("inline", budget.bytes("inline") + size, { pinned: true }), [keyOf("is")], "the growth names only S");
    requested.length = 0;
    await grow(watcher, panel, size);
    assert.deepEqual(modesOf(watcher, ids), { is: "whole", ic: "parts", ir: "whole" }, "C reads in parts, and S and R stay whole");
    assert.deepEqual(requested, [["ic", 0], ["ic", 1], ["ic", 2], ["ic", 3]], "only C reads, the part holding its window");
    assert.equal(windowsIn(panel.root).length, windows, "every window stays on show");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
}

test("r10 1 (Charon's probe A): growth that freeing the small least recently used whole covers refits the large whole instead, keeping every window", () =>
  // The growth is large enough to draw its own window beside the three.
  probeA((small) => small - 10_000, 4));

test("r10 1 (Charon's probe A2): the same with growth just past the spare", () => probeA(() => 40_000, 3));

test("r10 1: a refit that keeps no window is dropped: when the growth frees the small whole anyway, the whole whose refit gives back too little stays whole", async () => {
  const ids = ["is", "ic", "ir"];
  const { watcher, panel, requested } = await wholesOnShow(ids, [100, 200, 100], 20_000);
  try {
    const { budget } = watcher.view;
    // More than C's refit gives back with the spare, less than freeing S gives.
    const size = givenBack(watcher, "ic") + 40_000;
    assert.ok(givenBack(watcher, "ic") > 0 && givenBack(watcher, "ic") + 20_000 < size);
    assert.ok(size < budget.bytes(keyOf("is")) + 20_000);
    requested.length = 0;
    await grow(watcher, panel, size);
    assert.deepEqual(modesOf(watcher, ids), { is: "Load in full", ic: "whole", ir: "whole" }, "S is freed with or without C's refit, so C is not refitted");
    assert.deepEqual(requested, [], "nothing reads");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

// ---------- P3-1 (Charon): the latest request is never refitted to make room for another ----------

test("r10 2: growth that frees the small whole refits the large whole on show, never the larger latest request that still fits", async () => {
  const ids = ["is", "ic", "ir"];
  const { watcher, panel, requested } = await wholesOnShow(ids, [100, 600, 700], 20_000);
  try {
    const { budget } = watcher.view;
    const size = 40_000;
    assert.ok(givenBack(watcher, "ir") > givenBack(watcher, "ic"), "R's refit would give back the most");
    assert.ok(budget.fits(keyOf("ir"), budget.bytes(keyOf("ir")) + size), "R still fits beside the growth");
    requested.length = 0;
    await grow(watcher, panel, size);
    assert.deepEqual(modesOf(watcher, ids), { is: "whole", ic: "parts", ir: "whole" }, "C reads in parts; R, what the person asked for last, stays whole");
    assert.deepEqual(requested, [["ic", 0], ["ic", 1], ["ic", 2], ["ic", 3]], "only C reads");
    assert.equal(windowsIn(panel.root).length, 3, "every window stays on show");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});
