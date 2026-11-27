// Round 9 of https://github.com/aiconsultancy/subspace/pull/115 (SUB
// decision 8346cf82): Charon's and Phobos's round 8 delta probes, rebuilt
// as tests, with the cases around them, each named for the finding it
// checks.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { PART_MAX_BYTES } from "../src/activity.js";
import { createBudget } from "../src/budget.js";
import { activityEvent, textItem } from "./activity-fixtures.js";
import { buttons, clickOn, officer, pagedWith, pagesOf, prepare, settle, windowsIn } from "./activity-helpers.js";

const TURN = "turn-b";
const K = 1024;
const keyOf = (id) => `${TURN}\u0000${id}`;
/** A live item that grows the pinned inline charge by `size` bytes. */
const growth = (size, seq = 10) => activityEvent(seq, { type: "item", item: textItem(`growth-${seq}`, seq, "g".repeat(size)) });

/** Three paged items of `sizes` KiB in one turn, named `ids`, with each one's pages. */
function three(ids, sizes) {
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

/** Each of `ids`' mode, or "Load in full" when it was freed. */
const modesOf = (watcher, ids) => Object.fromEntries(ids.map((id) => [id, watcher.view.full(TURN, id)?.mode ?? "Load in full"]));

// ---------- P2 (both reviews): plan every refit and its charge before opening any pager ----------

test("r9 1 (Charon's probe): growth refits the large displayed whole that gives back the most, and the small ones stay whole", async () => {
  const ids = ["is", "ic", "ir"];
  const { snapshot, pages } = three(ids, [100, 600, 100]);
  // What the three wholes and their windows hold, measured with room to spare.
  const roomy = await loadedWhole(snapshot, pages, ids, 50 * K * K);
  const held = roomy.watcher.view.budget.used - roomy.inline;
  roomy.watcher.stop();
  const { watcher, panel, requested } = await loadedWhole(snapshot, pages, ids, held + 20_000);
  try {
    assert.equal(windowsIn(panel.root).length, 3, "all three windows drawn");
    assert.equal(watcher.view.budget.requested, keyOf("ir"));
    requested.length = 0;
    watcher.event(growth(400 * K));
    await settle();
    await settle();
    panel.update(watcher.view, officer);
    assert.deepEqual(modesOf(watcher, ids), { is: "whole", ic: "parts", ir: "whole" }, "C's refit alone covers the growth");
    assert.deepEqual(requested, [["ic", 0], ["ic", 1], ["ic", 2], ["ic", 3]], "only C reads, the part holding its window");
    assert.equal(watcher.view.full(TURN, "ic").state, "ready");
    assert.equal(windowsIn(panel.root).length, 4, "every window stays on show, beside the growth's own");
    const { budget } = watcher.view;
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

test("r9 1 (Phobos's probe): growth with mixed whole sizes keeps every displayed window, refitting the 600 KiB whole first", async () => {
  const ids = ["ia", "ib", "ic"];
  const { snapshot, pages } = three(ids, [96, 600, 96]);
  const extra = 2 * (pages.ia[0].bytes + pages.ib[0].bytes + pages.ic[0].bytes) + 30 * K;
  const { watcher, panel, requested } = await loadedWhole(snapshot, pages, ids, extra);
  try {
    assert.equal(windowsIn(panel.root).length, 3);
    const { budget } = watcher.view;
    assert.equal(budget.requested, keyOf("ic"));
    requested.length = 0;
    watcher.event(growth(240 * K));
    await settle();
    await settle();
    panel.update(watcher.view, officer);
    const futureRoom = budget.limit - budget.bytes("inline") - budget.bytes(keyOf("ic"));
    assert.ok(futureRoom > 2 * (PART_MAX_BYTES + 64 * K), "both handovers would fit");
    assert.deepEqual(modesOf(watcher, ids), { ia: "whole", ib: "parts", ic: "whole" }, "no displayed whole goes back to Load in full");
    assert.ok(requested.length > 0 && requested.every(([id]) => id === "ib"), "only B reads");
    assert.equal(windowsIn(panel.root).length, 4, "every window stays on show, beside the growth's own");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

test("r9 1: when no single refit is enough, the next largest give-back refits too, and nothing on show is freed", async () => {
  const ids = ["ia", "ib", "ic"];
  const { snapshot, pages } = three(ids, [400, 600, 100]);
  const extra = 2 * (pages.ia[0].bytes + pages.ib[0].bytes + pages.ic[0].bytes) + 30 * K;
  const { watcher, panel, requested } = await loadedWhole(snapshot, pages, ids, extra);
  try {
    assert.equal(windowsIn(panel.root).length, 3);
    requested.length = 0;
    // More than B's refit gives back (about 900 KiB), less than A's and B's together.
    watcher.event(growth(1200 * K));
    await settle();
    await settle();
    panel.update(watcher.view, officer);
    assert.deepEqual(modesOf(watcher, ids), { ia: "parts", ib: "parts", ic: "whole" });
    assert.deepEqual([...new Set(requested.map(([id]) => id))].sort(), ["ia", "ib"]);
    assert.equal(windowsIn(panel.root).length, 4, "every window stays on show, beside the growth's own");
    const { budget } = watcher.view;
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

test("r9 1: wouldFree can ask as if some entries held other sizes, reserved just now, changing nothing", () => {
  const budget = createBudget(100);
  budget.reserve("floor", 40, null, { pinned: true });
  for (const key of ["a", "b", "c"]) budget.reserve(key, 20, null);
  assert.deepEqual(budget.wouldFree("floor", 90, { pinned: true }), ["a", "b", "c"]);
  // b at 5 bytes gives back 15: only a and c need to go, and b, as if reserved just now, would go last.
  assert.deepEqual(budget.wouldFree("floor", 90, { pinned: true, as: new Map([["b", 5]]) }), ["a", "c"]);
  // Resized entries go in the order given, after every other.
  assert.deepEqual(budget.wouldFree("floor", 90, { pinned: true, as: new Map([["b", 5], ["a", 5]]) }), ["c"]);
  assert.deepEqual(budget.wouldFree("floor", 95, { pinned: true, as: new Map([["b", 5], ["a", 5], ["c", 5]]) }), ["b", "a"]);
  assert.deepEqual(budget.wouldFree("floor", 85, { pinned: true, as: new Map([["b", 5], ["a", 5], ["c", 5]]) }), []);
  assert.deepEqual({ used: budget.used, a: budget.bytes("a"), b: budget.bytes("b") }, { used: 100, a: 20, b: 20 }, "asking changes nothing");
});

// ---------- P2 (Phobos): keep the fresh-read intent through a failed whole reload ----------

/**
 * A 600 KiB whole drawn at its last window, then a newer 480 KiB version
 * (eight pages, two parts) announced: Phobos's setup. Page reads after
 * that go through `serve`, which is logged in `calls`.
 */
async function reloadSetup() {
  const { item, pages, snapshot } = pagedWith("a".repeat(600 * K), "ia");
  const newer = { ...item, seq: 10, content: [{ type: "text", text: "b".repeat(480 * K) }] };
  const fresh = pagesOf(newer);
  const prepared = await prepare(snapshot, 2 * (pages[0].bytes + fresh[0].bytes) + 30 * K);
  const { watcher, client, panel } = prepared;
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem(TURN, "ia");
  panel.update(watcher.view, officer);
  clickOn(panel, panel.root.findAll((node) => node.attributes["data-activity-item"] === "ia" && node.attributes["data-activity-window"] === "last")[0]);
  const placeholder = watcher.view.turns.find((each) => each.id === TURN).items[0];
  watcher.event(activityEvent(10, { type: "item", item: { ...placeholder, seq: 10, paged: { bytes: fresh[0].bytes, pages: fresh.length } } }));
  const calls = [];
  const control = { serve: (page) => fresh[page] };
  client.activityItemPage = async (agent, turn, id, page, signal) => {
    calls.push(page);
    // The newer version has no page past its last: as the gateway, 404.
    if (!fresh[page]) throw Object.assign(new Error("out of range"), { status: 404 });
    return control.serve(page, signal);
  };
  return { ...prepared, fresh, calls, control };
}

test("r9 2 (Phobos's probe): growth after a failed whole reload reads the newest version from page 0, and so does Try again", async () => {
  const { watcher, panel, fresh, calls, control } = await reloadSetup();
  try {
    let failures = 2;
    control.serve = (page) => {
      if (failures > 0) {
        failures -= 1;
        throw new Error("gateway error");
      }
      return fresh[page];
    };
    await watcher.loadItem(TURN, "ia");
    panel.update(watcher.view, officer);
    assert.deepEqual([watcher.view.full(TURN, "ia").mode, watcher.view.full(TURN, "ia").state], ["whole", "failed"]);
    assert.equal(buttons(panel.root, "Try again").length, 1);
    watcher.event(growth(1100 * K, 11));
    await settle();
    await settle();
    panel.update(watcher.view, officer);
    assert.deepEqual(calls, [0, 0], "the handover reads the newest version's page 0, not the old version's part");
    const handed = watcher.view.full(TURN, "ia");
    assert.deepEqual([handed.mode, handed.state], ["parts", "failed"]);
    assert.equal(buttons(panel.root, "Try again").length, 1);
    await watcher.loadItem(TURN, "ia");
    await settle();
    assert.deepEqual(calls, [0, 0, 0, 1, 2, 3], "Try again reads the newest version's first part");
    const shown = watcher.view.full(TURN, "ia");
    assert.deepEqual([shown.mode, shown.state, shown.seq, shown.part], ["parts", "ready", 10, 0]);
  } finally {
    watcher.stop();
  }
});

test("r9 2: growth during a whole reload, then a failed part read: Try again reads the newest version from page 0", async () => {
  const { watcher, panel, fresh, calls, control } = await reloadSetup();
  try {
    let first = true;
    let failNext = false;
    control.serve = (page, signal) => {
      if (first) {
        first = false;
        failNext = true;
        // The whole reload is still reading when the snapshot grows.
        return new Promise((resolve, reject) => signal.addEventListener("abort", () => reject(new Error("aborted"))));
      }
      if (failNext) {
        failNext = false;
        throw new Error("gateway error");
      }
      return fresh[page];
    };
    watcher.loadItem(TURN, "ia");
    await settle();
    assert.equal(watcher.view.full(TURN, "ia").state, "loading");
    watcher.event(growth(1100 * K, 11));
    await settle();
    await settle();
    panel.update(watcher.view, officer);
    assert.deepEqual(calls, [0, 0]);
    assert.deepEqual([watcher.view.full(TURN, "ia").mode, watcher.view.full(TURN, "ia").state], ["parts", "failed"]);
    await watcher.loadItem(TURN, "ia");
    await settle();
    assert.deepEqual(calls, [0, 0, 0, 1, 2, 3]);
    const shown = watcher.view.full(TURN, "ia");
    assert.deepEqual([shown.state, shown.seq, shown.part], ["ready", 10, 0]);
  } finally {
    watcher.stop();
  }
});

// ---------- P3s ----------

test("r9 3: the README keeps the shown part only for growth and Show, and puts a too-large newest version under Load the latest", () => {
  const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8").replace(/\s+/g, " ");
  assert.match(readme, /One already on show that stops fitting \(the snapshot grew or \*\*Show\*\* needs more\) is read in parts from the part holding what it shows/);
  assert.doesNotMatch(readme, /or its newest version is too large\)/);
  assert.match(readme, /\*\*Load the latest\*\*, also when the newest version is too large to read whole, reads the newest version from its first part/);
});
