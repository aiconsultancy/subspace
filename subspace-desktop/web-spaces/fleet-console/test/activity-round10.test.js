// Round 10 of https://github.com/aiconsultancy/subspace/pull/115 (SUB
// decision fbc47f97): Charon's round 9 delta probes, rebuilt as tests, with
// the cases around them, each named for the finding it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { PART_MAX_BYTES } from "../src/activity.js";
import { activityEvent, textItem } from "./activity-fixtures.js";
import { officer, pagedWith, prepare, settle, windowsIn } from "./activity-helpers.js";

const TURN = "turn-b";
const K = 1024;
const keyOf = (id) => `${TURN}\u0000${id}`;
/** A live item that grows the pinned inline charge by `size` bytes. */
const growth = (size, seq = 10) => activityEvent(seq, { type: "item", item: textItem(`growth-${seq}`, seq, "g".repeat(size)) });

/** Paged items of `sizes` KiB in one turn, named `ids`, with each one's pages. */
function several(ids, sizes) {
  const made = ids.map((id, index) => pagedWith(String.fromCharCode(97 + index).repeat(sizes[index] * K), id));
  for (const other of made.slice(1)) made[0].snapshot.turns[1].items.push(other.snapshot.turns[1].items[0]);
  return { snapshot: made[0].snapshot, pages: Object.fromEntries(ids.map((id, index) => [id, made[index].pages])) };
}

/** `ids` loaded whole in order, each window drawn; every page read is logged in `requested`. */
async function loadedWhole(snapshot, pages, ids, extra) {
  const prepared = await prepare(snapshot, extra);
  const { watcher, client, panel } = prepared;
  const requested = [];
  client.activityItemPage = async (agent, turn, id, page) => {
    requested.push([id, page]);
    return pages[id][page];
  };
  for (const id of ids) {
    await watcher.loadItem(TURN, id);
    panel.update(watcher.view, officer);
  }
  await settle();
  panel.update(watcher.view, officer);
  return { ...prepared, requested };
}

/**
 * Wholes of `sizes` KiB named `ids`, loaded in order (so the first is the
 * least recently used and the last is the latest request), all with their
 * windows on show, in a view `spare` bytes larger than they need.
 */
async function wholesOnShow(ids, sizes, spare) {
  const { snapshot, pages } = several(ids, sizes);
  // What the wholes and their windows hold, measured with room to spare.
  const roomy = await loadedWhole(snapshot, pages, ids, 50 * K * K);
  const held = roomy.watcher.view.budget.used - roomy.inline;
  roomy.watcher.stop();
  const loaded = await loadedWhole(snapshot, pages, ids, held + spare);
  assert.equal(windowsIn(loaded.panel.root).length, ids.length, "every window drawn");
  assert.equal(loaded.watcher.view.budget.requested, keyOf(ids.at(-1)));
  return loaded;
}

/** Each of `ids`' mode, or "Load in full" when it was freed. */
const modesOf = (watcher, ids) => Object.fromEntries(ids.map((id) => [id, watcher.view.full(TURN, id)?.mode ?? "Load in full"]));

/** What a refit of `id` gives back: its charge less its handover (the window on show and an incoming part). */
const givenBack = (watcher, id) => watcher.view.budget.bytes(keyOf(id)) - (64 * K + PART_MAX_BYTES);

/** Grow the inline charge by `size` and let every read settle. */
async function grow(watcher, panel, size) {
  watcher.event(growth(size));
  for (let i = 0; i < 4; i += 1) {
    await settle();
    panel.update(watcher.view, officer);
  }
}

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
