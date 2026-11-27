// Refits planned before the inline charge grows (SUB decisions 8346cf82,
// fbc47f97): a planned refit that keeps no window on show is dropped,
// also when the growth frees that refit itself, counting every window the
// growth frees, planned refits included (SUB-69).

import assert from "node:assert/strict";
import { test } from "node:test";
import { applyChange, INLINE_KEY, PART_MAX_BYTES } from "../src/activity.js";
import { jsonBytes } from "../src/document.js";
import { activityEvent, textItem } from "./activity-fixtures.js";
import { givenBack, grow, keyOf, modesOf, officer, settle, wholesOnShow, windowsIn } from "./activity-helpers.js";

const K = 1024;
/** The live item `grow` sends, `size` bytes of text. */
const growth = (size) => activityEvent(10, { type: "item", item: textItem("growth-10", 10, "g".repeat(size)) });

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

/** The ids the growth to `bytes` more would free, with the entries in `planned` held as a handover of `handover` bytes. */
function namedBy(budget, bytes, planned = [], handover = 0) {
  const as = new Map(planned.map((id) => [keyOf(id), handover]));
  return budget.wouldFree(INLINE_KEY, budget.bytes(INLINE_KEY) + bytes, { pinned: true, as }).map((key) => key.split("\u0000").at(-1));
}

test("Deimos's case: when either planned refit or both free more wholes than none, nothing is refitted and the small whole stays", async () => {
  // Deimos's case: C, D, S and R of 200, 210, 100 and 100 KiB, R the latest
  // request. After the growth, R fits with 240,000 bytes left beside it:
  // room for S whole, not for a handover of C or D.
  const ids = ["ic", "id", "is", "ir"];
  const { watcher, client, panel, snapshot } = await wholesOnShow(ids, [200, 210, 100, 100], 20_000);
  try {
    const { budget } = watcher.view;
    const handover = 64 * K + PART_MAX_BYTES;
    const bytes = budget.limit - budget.bytes(INLINE_KEY) - budget.bytes(keyOf("ir")) - 240_000;
    assert.ok(budget.fits(keyOf("ir"), budget.bytes(keyOf("ir")) + bytes), "R still fits");
    assert.ok(givenBack(watcher, "ic") > 0 && givenBack(watcher, "id") > 0, "both refits give something back, so both are planned");
    assert.deepEqual(namedBy(budget, bytes), ["ic", "id"], "without refits, C and D are freed and S stays");
    assert.deepEqual(namedBy(budget, bytes, ["ic", "id"], handover).sort(), ["ic", "id", "is"], "with both refits, all three are freed");
    assert.deepEqual(namedBy(budget, bytes, ["id"], handover).sort(), ["ic", "id", "is"], "with D's alone, all three");
    assert.deepEqual(namedBy(budget, bytes, ["ic"], handover).sort(), ["ic", "id", "is"], "with C's alone, all three");

    // A reconnect snapshot that grows the inline charge by `bytes`, in text items of at most 31 KiB.
    const calls = [];
    client.activityItemPage = (agent, turn, id, page) => {
      calls.push([id, page]);
      return new Promise(() => {});
    };
    const { items } = snapshot.turns[0];
    const count = Math.ceil(bytes / (31 * K));
    const added = Array.from({ length: count }, (_, i) => textItem(`more-${i}`, 10 + i, ""));
    const framing = JSON.stringify(added).length - 1;
    const each = Math.floor((bytes - framing) / count);
    for (const item of added) item.text = "g".repeat(each);
    added.at(-1).text += "g".repeat(bytes - framing - count * each);
    items.push(...added);
    snapshot.cursor.seq = 9 + count;
    client.activity = async () => structuredClone(snapshot);
    const before = budget.bytes(INLINE_KEY);
    watcher.reopened();
    for (let i = 0; i < 4; i += 1) {
      await settle();
      panel.update(watcher.view, officer);
    }
    assert.equal(budget.bytes(INLINE_KEY) - before, bytes);
    assert.deepEqual(modesOf(watcher, ids), { ic: "Load in full", id: "Load in full", is: "whole", ir: "whole" });
    assert.equal(windowsIn(panel.root).length, 2, "S and R stay on show");
    assert.deepEqual(calls, [], "nothing reads in parts");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

/** Grow the inline charge by exactly `bytes` with one live item, and let every read settle. */
async function growBy(watcher, panel, bytes) {
  const { budget, turns } = watcher.view;
  // The item's text adds its own length; the rest of the item and its comma, what an empty one adds.
  const framing = jsonBytes(applyChange(turns, growth(0))) - jsonBytes(turns);
  const before = budget.bytes(INLINE_KEY);
  await grow(watcher, panel, bytes - framing);
  assert.equal(budget.bytes(INLINE_KEY) - before, bytes, "the inline charge grew by exactly that");
}

/** Wholes of `sizes` KiB, the last the latest request, in a view `spare` bytes larger than they need, grown by `bytes`: what each ends as and which read in parts. */
async function grownBy(sizes, spare, bytes) {
  const ids = sizes.map((_, i) => `w${i}`);
  const { watcher, panel, requested } = await wholesOnShow(ids, sizes, spare);
  try {
    const { budget } = watcher.view;
    assert.ok(budget.fits(keyOf(ids.at(-1)), budget.bytes(keyOf(ids.at(-1))) + bytes), "the latest request still fits");
    requested.length = 0;
    await growBy(watcher, panel, bytes);
    assert.ok(budget.used <= budget.limit);
    return { modes: modesOf(watcher, ids), read: [...new Set(requested.map(([id]) => id))] };
  } finally {
    watcher.stop();
  }
}

test("a planned refit is dropped when every window the growth frees, planned refits included, is no more without it", async () => {
  // Five wholes, W4 the latest request. Without refits the growth frees W0
  // and W1. W1's refit is planned first, as it gives back the most, then
  // W0's; with both, it frees W2, W3 and W1's handover. Counting only the
  // wholes not refitted, W0's refit looked as if it kept a window (two lost
  // against three without it), so W0 read in parts while W1, W2 and W3 were
  // freed. Counted in full, either refit or both lose three windows and
  // none loses two: nothing reads, and W2, W3 and W4 stay whole.
  assert.deepEqual(await grownBy([190, 540, 80, 130, 280], 9_000, 1_374_000), {
    modes: { w0: "Load in full", w1: "Load in full", w2: "whole", w3: "whole", w4: "whole" },
    read: [],
  });
});

test("the refits kept are the set that loses fewest windows, not the first a fixed order reaches", async () => {
  // Six wholes, W5 the latest request. Without refits the growth frees W0
  // and W1. W0's refit is planned, then W2's; with both, it frees W1 and W3.
  // Dropping the latest planned first, W2's goes (W0's alone frees W1 and
  // W2, as many), then W0's (none frees W0 and W1, as many): four of the
  // six stay whole. Dropping W0's first, as W2's alone frees only W0, keeps
  // W2's: five stay on show.
  assert.deepEqual(await grownBy([340, 100, 260, 130, 120, 140], 90_000, 971_000), {
    modes: { w0: "Load in full", w1: "whole", w2: "parts", w3: "whole", w4: "whole", w5: "whole" },
    read: ["w2"],
  });
});
