// Round 8 of https://github.com/aiconsultancy/subspace/pull/115 (SUB
// decision cf21d510): Phobos's round 7 delta probes, rebuilt as tests,
// with the cases around them, each named for the finding it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { PART_MAX_BYTES } from "../src/activity.js";
import { createBudget } from "../src/budget.js";
import { servedPart, servedUnit } from "../src/document.js";
import { itemDocument, measureSource } from "../src/activity-view.js";
import { activityEvent, textItem, toolItem } from "./activity-fixtures.js";
import { buttons, clickOn, officer, pagedWith, pagesOf, prepare, settle, windowsIn } from "./activity-helpers.js";

const TURN = "turn-b";
const KEY = `${TURN}\u0000ia`;
const PAGING = { pageBytes: 65536, partPages: 4 };
/** A live item that grows the pinned inline charge by `size` bytes. */
const growth = (size, seq = 10) => activityEvent(seq, { type: "item", item: textItem(`growth-${seq}`, seq, "g".repeat(size)) });
/** The button that moves item `item`'s windows `to` first, previous, next or last. */
const windowButton = (panel, item, to) => panel.root.findAll((node) => node.attributes["data-activity-item"] === item && node.attributes["data-activity-window"] === to)[0];
/** A page reader that holds each read, with its signal, until the test answers it. */
function holding(client) {
  const held = [];
  client.activityItemPage = (agent, turn, id, page, signal) => new Promise((resolve, reject) => held.push({ id, page, signal, resolve, reject }));
  return held;
}
/** Answer the held reads from `from` on, in order, from `pages`, including those the answers lead to. */
async function answer(held, pages, from = 0) {
  for (let i = from; i < held.length; i += 1) {
    held[i].resolve(pages[held[i].page]);
    await settle();
  }
}
/** Announce a newer version of `item` (its pages `pages`) in place of the placeholder shown. */
function announce(watcher, item, pages) {
  const placeholder = watcher.view.turns.find((turn) => turn.id === TURN).items.find((each) => each.id === item.id);
  watcher.event(activityEvent(item.seq, { type: "item", item: { ...placeholder, seq: item.seq, paged: { bytes: pages[0].bytes, pages: pages.length } } }));
}

// ---------- P2-1: inline growth plans every refit against the final floor ----------

test("r8 1 (probe): inline growth that needs both displayed wholes in parts turns both into pagers, each sending its first page", async () => {
  const a = pagedWith("a".repeat(600 * 1024), "ia");
  const b = pagedWith("b".repeat(600 * 1024), "ib");
  a.snapshot.turns[1].items.push(b.snapshot.turns[1].items[0]);
  const { watcher, client, panel } = await prepare(a.snapshot, 2 * (a.pages[0].bytes + b.pages[0].bytes) + 20_000);
  try {
    const pagesFor = (id) => (id === "ia" ? a.pages : b.pages);
    client.activityItemPage = async (agent, turn, id, page) => pagesFor(id)[page];
    await watcher.loadItem(TURN, "ia");
    await watcher.loadItem(TURN, "ib");
    panel.update(watcher.view, officer);
    assert.equal(windowsIn(panel.root).length, 2);
    assert.equal(watcher.view.budget.requested, `${TURN}\u0000ib`);
    const requested = [];
    client.activityItemPage = async (agent, turn, id, page) => {
      requested.push([id, page]);
      return pagesFor(id)[page];
    };
    watcher.event(growth(1400 * 1024));
    await settle();
    await settle();
    const { budget } = watcher.view;
    assert.ok(budget.limit - budget.bytes("inline") > 2 * (PART_MAX_BYTES + 65536), "both handovers fit once both wholes are in parts");
    assert.equal(watcher.view.full(TURN, "ib")?.mode, "parts");
    assert.deepEqual({ mode: watcher.view.full(TURN, "ia")?.mode, sent: requested.some(([id]) => id === "ia") }, { mode: "parts", sent: true }, "A did not go back to Load in full");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

// ---------- P2-2: a window inside a text part's other fields maps to that field ----------

test("r8 2 (probe): a window inside a text part's annotation opens the served part and unit that hold it", () => {
  const item = toolItem("ia", 9, { content: [{ type: "text", text: "t".repeat(300_000), annotation: { payload: "a".repeat(210_000) } }] });
  const doc = itemDocument(item);
  const json = JSON.stringify(item);
  const unit = measureSource({ kind: "whole", item }).bounds[1];
  const raw = json.indexOf("aaaaaaaa") + unit - doc.indexOf("aaaaaaaa");
  assert.equal(doc[unit], "a", "section 2 starts inside annotation.payload");
  assert.equal(json[raw], "a");
  assert.equal(servedUnit(item, unit), raw);
  assert.deepEqual(servedPart(item, servedUnit(item, unit), PAGING), { part: 1, unit: 103410 });
});

test("r8 2: every character of a text part's other fields, and of its text, maps to itself in the item as served", () => {
  const item = toolItem("ia", 9, { content: [{ type: "text", text: "T-start-" + "t".repeat(40) + "-T-end", annotation: { payload: "A-start-" + "a".repeat(40) + "-A-end", n: 7 }, cache: "C-mark" }] });
  const doc = itemDocument(item);
  const json = JSON.stringify(item);
  for (const mark of ["T-start", "-T-end", "A-start", "-A-end", "C-mark"]) {
    const at = doc.indexOf(mark);
    assert.ok(at >= 0, mark);
    for (let i = 0; i < mark.length; i += 1) assert.equal(json[servedUnit(item, at + i)], mark[i], `${mark}[${i}]`);
  }
});

// ---------- P2-3: a refit during Load the latest keeps asking for the fresh version ----------

test("r8 3 (probe): growth during Load the latest asks again for page 0 of the fresh version, and Try again repeats it", async () => {
  const { item, pages, snapshot } = pagedWith("a".repeat(600 * 1024), "ia");
  const newer = { ...item, seq: 10, content: [{ type: "text", text: "b".repeat(480 * 1024) }] };
  const newPages = pagesOf(newer);
  assert.equal(newPages[8], undefined, "page 8 is outside the fresh version");
  const { watcher, client, panel } = await prepare(snapshot, 2 * (pages[0].bytes + newPages[0].bytes) + 20_000);
  try {
    client.activityItemPage = async (agent, turn, id, page) => pages[page];
    await watcher.loadItem(TURN, "ia");
    panel.update(watcher.view, officer);
    clickOn(panel, windowButton(panel, "ia", "last"));
    announce(watcher, newer, newPages);
    panel.update(watcher.view, officer);
    assert.equal(buttons(panel.root, "Load the latest").length, 1);
    const held = holding(client);
    watcher.loadItem(TURN, "ia");
    await settle();
    assert.deepEqual(held.map((read) => read.page), [0]);
    watcher.event(growth(100 * 1024, 11));
    await settle();
    assert.equal(held[0].signal.aborted, true);
    assert.deepEqual(held.map((read) => read.page), [0, 0], "the refit asks for the fresh version's first page, not the old version's anchored part");
    assert.equal(watcher.view.full(TURN, "ia").replacing, true, "the old version's window stays on show meanwhile");
    held[1].reject(Object.assign(new Error("out of range"), { status: 404 }));
    await settle();
    assert.equal(watcher.view.full(TURN, "ia").state, "failed");
    watcher.loadItem(TURN, "ia");
    await settle();
    assert.equal(held[2]?.page, 0, "Try again repeats page 0");
    await answer(held, newPages, 2);
    await settle();
    const shown = watcher.view.full(TURN, "ia");
    assert.deepEqual({ state: shown.state, part: shown.part, seq: shown.seq }, { state: "ready", part: 0, seq: 10 });
  } finally {
    watcher.stop();
  }
});

// ---------- P3-4: closing a turn during a handover releases the outgoing window ----------

/** A whole of 600 KiB drawn, then a held Load the latest of a 600 KiB newer version, its window kept on show. */
async function heldHandover() {
  const { item, pages, snapshot } = pagedWith("a".repeat(600 * 1024), "ia");
  const newer = { ...item, seq: 10, content: [{ type: "text", text: "b".repeat(600 * 1024) }] };
  const newPages = pagesOf(newer);
  const prepared = await prepare(snapshot, 2 * pages[0].bytes + 20_000);
  const { watcher, client, panel } = prepared;
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem(TURN, "ia");
  panel.update(watcher.view, officer);
  announce(watcher, newer, newPages);
  const held = holding(client);
  watcher.loadItem(TURN, "ia");
  await settle();
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full(TURN, "ia").replacing, true);
  const details = panel.root.findAll((node) => node.tag === "details")[0];
  details.removeAttribute("open");
  details.listeners.toggle();
  assert.equal(windowsIn(panel.root).length, 0);
  return { ...prepared, held, newPages };
}

test("r8 4 (probe): closing a turn during a handover releases the outgoing window's charge, and a failed read leaves nothing held", async () => {
  const { watcher, held } = await heldHandover();
  try {
    assert.equal(watcher.view.budget.bytes(KEY), PART_MAX_BYTES, "only the incoming part is charged while it is read");
    assert.equal(held[0].signal.aborted, false, "the incoming read goes on");
    held[0].reject(new Error("gateway down"));
    await settle();
    const shown = watcher.view.full(TURN, "ia");
    assert.equal(shown.state, "failed");
    assert.deepEqual({ bytes: watcher.view.budget.bytes(KEY), replacing: shown.replacing }, { bytes: 0, replacing: false });
  } finally {
    watcher.stop();
  }
});

test("r8 4: closing a turn during a handover still shows the fresh version's first part once it arrives", async () => {
  const { watcher, panel, held, newPages } = await heldHandover();
  try {
    await answer(held, newPages);
    await settle();
    const shown = watcher.view.full(TURN, "ia");
    assert.deepEqual({ state: shown.state, part: shown.part, seq: shown.seq, replacing: shown.replacing }, { state: "ready", part: 0, seq: 10, replacing: false });
    const details = panel.root.findAll((node) => node.tag === "details")[0];
    details.setAttribute("open", "");
    details.listeners.toggle();
    panel.update(watcher.view, officer);
    assert.equal(windowsIn(panel.root).length, 1, "the part shows when the turn opens again");
  } finally {
    watcher.stop();
  }
});

// ---------- P3-5: wouldFree admits what reserve admits ----------

test("r8 5 (probe): wouldFree names nothing for a reservation that reserve refuses", () => {
  const budget = createBudget(100);
  budget.reserve("floor", 80, null, { pinned: true });
  budget.reserve("other", 10, null);
  const predicted = budget.wouldFree("plain", 30);
  assert.equal(budget.reserve("plain", 30, null), false);
  assert.equal(budget.has("other"), true);
  assert.deepEqual(predicted, []);
});

test("r8 5: wouldFree agrees with what reserve frees, for ordinary, pinned and latest-request reservations", () => {
  const cases = [
    ["plain", 10, {}],
    ["plain", 30, {}],
    ["plain", 50, {}],
    ["plain", 30, { pinned: true }],
    ["asked", 30, {}],
  ];
  for (const [key, bytes, options] of cases) {
    const budget = createBudget(100);
    const freed = [];
    budget.reserve("floor", 60, null, { pinned: true });
    for (const other of ["x", "y"]) budget.reserve(other, 15, () => freed.push(other));
    budget.request("asked");
    const predicted = budget.wouldFree(key, bytes, options);
    budget.reserve(key, bytes, null, options);
    assert.deepEqual(predicted, freed, `${key} ${bytes} ${JSON.stringify(options)}`);
  }
});
