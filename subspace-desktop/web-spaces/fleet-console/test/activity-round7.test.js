// Round 7 of #115 (SUB decision a5275d81): Phobos's four round-6 probes,
// rebuilt from his review, and the two regressions the ruling adds, each
// named for what it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { PART_MAX_BYTES } from "../src/activity.js";
import { formatItem, itemDocument, itemTree, measureSource, mount } from "../src/activity-view.js";
import { splitUtf8 } from "../src/utf8.js";
import { activityEvent, GENERATION, textItem } from "./activity-fixtures.js";
import { buttons, clickOn, fakeDoc, officer, pagedSnapshot, pagedWith, pagesOf, prepare, settle, usePaged, windowsIn } from "./activity-helpers.js";

const KEY = "turn-b\u0000ia";
/** A live item that grows the pinned inline charge by about 100 KiB. */
const growth = (seq = 10) => activityEvent(seq, { type: "item", item: textItem(`grown-${seq}`, seq, "g".repeat(100 * 1024)) });
/** The button that moves item `item`'s windows `to` first, previous, next or last. */
const windowButton = (panel, item, to) => panel.root.findAll((node) => node.attributes["data-activity-window"] === to && node.attributes["data-activity-item"] === item)[0];
/** The window drawn for item `item`. */
const windowOf = (panel, item) => panel.root.findAll((node) => node.tag === "li" && node.findAll((each) => each.attributes["data-activity-item"] === item || each.attributes["data-activity-load"] === item).length).flatMap((node) => windowsIn(node))[0];
/** A page reader that holds each read until the test answers it. */
function holding(client) {
  const held = [];
  client.activityItemPage = (agent, turn, id, page) => new Promise((resolve, reject) => held.push({ id, page, resolve, reject }));
  return held;
}
/** Answer every held read, in order, from `pages`, including those the answers lead to. */
async function answer(held, pages) {
  while (held.length) {
    const { id, page, resolve } = held.shift();
    resolve((typeof pages === "function" ? pages(id) : pages)[page]);
    await settle();
  }
}

/** Two items read whole in the running turn, A (600 KiB) and then B (70 KiB), both drawn, A at its last window, with room for both and 20 kB more. */
async function twoWholes() {
  const a = pagedWith("a".repeat(600 * 1024), "ia");
  const b = pagedWith("b".repeat(70 * 1024), "ib");
  a.snapshot.turns[1].items.push(b.snapshot.turns[1].items[0]);
  const prepared = await prepare(a.snapshot, 2 * a.pages[0].bytes + 2 * b.pages[0].bytes + 20_000);
  const { watcher, client, panel } = prepared;
  const pagesFor = (id) => (id === "ia" ? a.pages : b.pages);
  client.activityItemPage = async (agent, turn, id, page) => pagesFor(id)[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  clickOn(panel, windowButton(panel, "ia", "last"));
  await watcher.loadItem("turn-b", "ib");
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full("turn-b", "ia").mode, "whole");
  assert.equal(watcher.view.full("turn-b", "ib").mode, "whole");
  assert.equal(watcher.view.budget.requested, "turn-b\u0000ib", "B is the latest request");
  assert.ok(windowOf(panel, "ia") && windowOf(panel, "ib"), "both are drawn");
  return { ...prepared, a, b, pagesFor };
}

// ---------- P2 1: inline growth refits a displayed whole before the budget can free it ----------

test("r7 1 (probe): inline growth gives a displayed whole that is not the latest request its handover to parts, instead of freeing it", async () => {
  const { watcher, client, panel, a, pagesFor } = await twoWholes();
  const asked = [];
  client.activityItemPage = async (agent, turn, id, page) => {
    asked.push([id, page]);
    return pagesFor(id)[page];
  };
  watcher.event(growth());
  await settle();
  await settle();
  const shown = watcher.view.full("turn-b", "ia");
  assert.ok(shown, "A is still read: not freed as if it had never been read");
  assert.equal(shown.mode, "parts");
  const count = Math.ceil(a.pages.length / 4);
  assert.deepEqual(
    asked.filter(([id]) => id === "ia").map(([, page]) => page)[0],
    (count - 1) * 4,
    "the gateway is asked for A's replacement, from the part its window shows",
  );
  assert.equal(shown.state, "ready");
  assert.equal(shown.part, count - 1);
  assert.equal(watcher.view.full("turn-b", "ib").mode, "whole", "B, the latest request, stays whole");
  panel.update(watcher.view, officer);
  assert.ok(windowOf(panel, "ia"), "A's part is drawn");
  watcher.stop();
});

test("r7 1: with another item the latest request, a displayed whole's window stays on show and charged until its delayed replacement arrives", async () => {
  const { watcher, client, panel, a, pagesFor } = await twoWholes();
  const budget = watcher.view.budget;
  const window = windowOf(panel, "ia");
  const held = holding(client);
  watcher.event(growth());
  await settle();
  panel.update(watcher.view, officer);
  const waiting = watcher.view.full("turn-b", "ia");
  assert.ok(waiting, "A is not freed");
  assert.equal(waiting.mode, "parts");
  assert.equal(waiting.replacing, true);
  assert.equal(waiting.state, "loading");
  const count = Math.ceil(a.pages.length / 4);
  assert.deepEqual(
    held.map((each) => [each.id, each.page]),
    [["ia", (count - 1) * 4]],
    "A's replacement is asked for",
  );
  assert.equal(windowOf(panel, "ia"), window, "A's window stays mounted while the reply is delayed");
  assert.ok(budget.bytes(KEY) > PART_MAX_BYTES, `A's window stays charged beside the incoming part (${budget.bytes(KEY)})`);
  assert.equal(watcher.view.full("turn-b", "ib").mode, "whole", "B stays whole");
  assert.ok(budget.used <= budget.limit, `within the budget (${budget.used} of ${budget.limit})`);
  await answer(held, pagesFor);
  panel.update(watcher.view, officer);
  const after = watcher.view.full("turn-b", "ia");
  assert.equal(after.state, "ready");
  assert.equal(after.part, count - 1);
  assert.equal(after.replacing, false);
  assert.notEqual(windowOf(panel, "ia"), window, "the part's window replaced it");
  assert.ok(budget.used <= budget.limit);
  watcher.stop();
});

// ---------- P2 2: the handover opens the part, and the window in it, that hold the content shown ----------

/** Move item `item`'s window to `index` with Next, a click at a time. */
function moveTo(panel, watcher, item, index) {
  for (let i = 0; i < index; i += 1) {
    clickOn(panel, windowButton(panel, item, "next"));
    panel.update(watcher.view, officer);
  }
}

/**
 * Where the start of window `w` of `item`'s document is in the item as
 * served, worked out from JSON.stringify alone: the part that holds it, the
 * window of that part that holds it, and the part's data.
 */
function servedPlace(item, text, w) {
  const windows = formatItem(item).windows;
  const docStart = windows.slice(0, w).reduce((sum, each) => sum + each.length, 0);
  const t = docStart - itemDocument(item).indexOf(text.slice(0, 8));
  const json = JSON.stringify(item);
  const raw = json.indexOf(JSON.stringify(text.slice(0, 8)).slice(0, -1)) + 1 + JSON.stringify(text.slice(0, t)).length - 2;
  const pages = splitUtf8(json, 64 * 1024);
  let units = 0;
  let part = 0;
  let partStart = 0;
  for (let page = 0; page < pages.length && units + pages[page].length <= raw; page += 1) {
    units += pages[page].length;
    if ((page + 1) % 4 === 0) {
      part = (page + 1) / 4;
      partStart = units;
    }
  }
  const data = pages.slice(part * 4, part * 4 + 4).join("");
  const { bounds } = measureSource({ kind: "part", data });
  const within = raw - partStart;
  const window = bounds.findLastIndex((bound, i) => i < bounds.length - 1 && bound <= within);
  return { part, window, windows: bounds.length - 1, data };
}

/** Refit `item` (served from `text`) by inline growth with its window at `w`; returns what was asked for first, and what showed before and after. */
async function refitAt(text, w) {
  const { item, pages, snapshot } = pagedWith(text);
  const { watcher, client, panel } = await prepare(snapshot, 2 * pages[0].bytes + 20_000);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  moveTo(panel, watcher, "ia", w);
  assert.match(panel.root.textContent, new RegExp(`Section ${w + 1} of`));
  const before = windowOf(panel, "ia").textContent;
  const held = holding(client);
  watcher.event(growth());
  await settle();
  const first = held[0]?.page;
  await answer(held, pages);
  panel.update(watcher.view, officer);
  const part = watcher.view.full("turn-b", "ia");
  return { item, pages, watcher, panel, first, before, part, after: windowOf(panel, "ia")?.textContent ?? "" };
}

test("r7 2 (probe): a whole with uneven content changes to parts at the part that holds its window's content, not at its share of the windows", async () => {
  const text = "\u202e".repeat(100_000) + "a".repeat(300_000);
  const place = servedPlace(pagedWith(text).item, text, 15);
  assert.equal(place.part, 0, "the fixture: window 15 starts within the first part as served");
  const { watcher, first, before, part, after } = await refitAt(text, 15);
  assert.equal(first, 0, "the first page asked for is the first part's");
  assert.equal(part.mode, "parts");
  assert.equal(part.part, 0);
  assert.ok(after.includes(before.slice(0, 64)), "the part's window shows the content that was on show");
  watcher.stop();
});

test("r7 2: a non-uniform item read in parts after the swap shows the window that holds the content on show before it", async () => {
  let text = "";
  // Markers early (12 bytes shown, 3 served), then newlines (a line each shown, two bytes served) and plain text.
  for (let i = 0; i < 160; i += 1) text += `#${i};` + (i < 60 ? "\u202e".repeat(1500) + "é".repeat(200) : i % 4 === 0 ? "\n".repeat(400) + "y".repeat(500) : "x".repeat(4000));
  const w = 19;
  const place = servedPlace(pagedWith(text).item, text, w);
  const windows = formatItem(pagedWith(text).item).windows.length;
  assert.notEqual(place.part, Math.min(2, Math.floor((w / windows) * 3)), "the fixture: its share of the windows names another part");
  const { watcher, panel, first, before, part, after } = await refitAt(text, w);
  assert.equal(first, place.part * 4, "the part that holds the window's start is read first");
  assert.equal(part.part, place.part);
  assert.match(panel.root.textContent, new RegExp(`Part ${place.part + 1} of 3`));
  assert.match(panel.root.textContent, new RegExp(`Section ${place.window + 1} of ${place.windows}`), "the window of the part that holds the same content");
  const expected = windowsIn(mount(itemTree("turn-b", watcher.view.turns.find((turn) => turn.id === "turn-b").items[0], part, { window: place.window }), fakeDoc))[0].textContent;
  assert.equal(after, expected, "that window's content is what is drawn");
  assert.ok(after.includes(before.slice(0, 100)), "and it holds the content that was on show");
  watcher.stop();
});

// ---------- P2 3: a whole's reload in parts keeps its window ----------

/** An item read whole (600 KiB) at its last window, a newer 600 KiB version announced, and room for one. */
async function wholeWithNewer({ advertised = null } = {}) {
  const { item, pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const prepared = await prepare(snapshot, 2 * pages[0].bytes + 20_000);
  const { watcher, client, panel } = prepared;
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  clickOn(panel, windowButton(panel, "ia", "last"));
  const window = windowOf(panel, "ia");
  assert.ok(window);
  const newer = { ...item, seq: 10, content: [{ type: "text", text: "b".repeat(600 * 1024) }] };
  const newPages = pagesOf(newer);
  const placeholder = watcher.view.turns.find((turn) => turn.id === "turn-b").items[0];
  const paged = advertised ?? { pages: newPages.length, bytes: newPages[0].bytes };
  watcher.event(activityEvent(10, { type: "item", item: { ...placeholder, seq: 10, paged } }));
  panel.update(watcher.view, officer);
  assert.equal(buttons(panel.root, "Load the latest").length, 1);
  return { ...prepared, window, newPages };
}

test("r7 3 (probe): Load the latest on a whole that must read in parts keeps its window on show and charged while the newest version is read", async () => {
  const { watcher, client, panel, window, newPages } = await wholeWithNewer();
  const budget = watcher.view.budget;
  const held = holding(client);
  watcher.loadItem("turn-b", "ia");
  await settle();
  panel.update(watcher.view, officer);
  const waiting = watcher.view.full("turn-b", "ia");
  assert.equal(waiting.mode, "parts");
  assert.deepEqual(
    held.map((each) => each.page),
    [0],
    "the newest version is asked for, from its first page",
  );
  assert.equal(waiting.replacing, true, "the watcher says the window stays");
  assert.equal(windowOf(panel, "ia"), window, "the window on show stays mounted");
  assert.ok(budget.bytes(KEY) > PART_MAX_BYTES, `and charged beside the incoming part (${budget.bytes(KEY)})`);
  held.shift().reject(new Error("gateway away"));
  await settle();
  panel.update(watcher.view, officer);
  const failed = watcher.view.full("turn-b", "ia");
  assert.equal(failed.state, "failed");
  assert.equal(windowOf(panel, "ia"), window, "a failure keeps the window");
  assert.ok(budget.bytes(KEY) > 0, "still charged");
  assert.match(panel.root.textContent, /gateway away/);
  assert.equal(buttons(panel.root, "Try again").length, 1);
  client.activityItemPage = async (agent, turn, id, page) => newPages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  const after = watcher.view.full("turn-b", "ia");
  assert.equal(after.state, "ready");
  assert.equal(after.seq, 10, "the newest version");
  assert.equal(after.part, 0);
  assert.ok(windowOf(panel, "ia").textContent.includes("bbbb"), "its first part replaced the window");
  assert.ok(budget.bytes(KEY) <= budget.limit - budget.bytes("inline"));
  watcher.stop();
});

test("r7 3: a read larger than its placeholder advertised keeps the window on show while it reads in parts", async () => {
  const { watcher, client, panel, window, newPages } = await wholeWithNewer({ advertised: { pages: 2, bytes: 1_000 } });
  const held = holding(client);
  watcher.loadItem("turn-b", "ia");
  await settle();
  // The whole read's first page says the version is far larger than advertised.
  held.shift().resolve(newPages[0]);
  await settle();
  panel.update(watcher.view, officer);
  const waiting = watcher.view.full("turn-b", "ia");
  assert.equal(waiting.mode, "parts");
  assert.equal(waiting.replacing, true);
  assert.equal(windowOf(panel, "ia"), window, "the window on show stays mounted");
  assert.deepEqual(
    held.map((each) => each.page),
    [0],
    "its first part is read",
  );
  await answer(held, newPages);
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full("turn-b", "ia").seq, 10);
  assert.notEqual(windowOf(panel, "ia"), window);
  watcher.stop();
});

test("r7 3 (control): a new generation still discards a whole's window at once while its reload in parts is read", async () => {
  const { watcher, client, panel } = await wholeWithNewer();
  holding(client);
  watcher.loadItem("turn-b", "ia");
  await settle();
  const next = pagedSnapshot(["ia"], { seq: 3, generation: `${GENERATION}-next` });
  client.activity = async () => structuredClone(next);
  watcher.retry();
  await settle();
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full("turn-b", "ia"), null, "nothing read in the old generation carries over");
  assert.equal(windowOf(panel, "ia"), undefined, "its window is gone");
  assert.equal(watcher.view.budget.bytes(KEY), 0);
  watcher.stop();
});

// ---------- P3: a window that unmounts owns no bytes ----------

test("r7 4 (probe): closing a turn leaves no charge for its window when a whole of it is refitted later", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const { watcher, client, panel } = await prepare(snapshot, 2 * pages[0].bytes + 20_000);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.ok(windowOf(panel, "ia"));
  const details = panel.root.findAll((node) => node.tag === "details")[0];
  details.removeAttribute("open");
  details.listeners.toggle?.();
  assert.equal(windowsIn(panel.root).length, 0, "the closed turn mounts nothing");
  client.activityItemPage = async () => {
    throw new Error("gateway away");
  };
  watcher.event(growth());
  await settle();
  const failed = watcher.view.full("turn-b", "ia");
  assert.equal(failed?.mode, "parts");
  assert.equal(failed.replacing, false, "no window is on show to keep");
  assert.equal(watcher.view.budget.bytes(KEY), 0, "and none is charged");
  watcher.stop();
});

test("r7 4: moving the items window off a whole leaves no charge for its window when it is refitted later", async () => {
  const { item, pages } = pagedWith("a".repeat(600 * 1024));
  const snapshot = pagedSnapshot(["ia"]);
  usePaged(snapshot, item, pages);
  for (let i = 0; i < 20; i += 1) snapshot.turns[1].items.push(textItem(`t${i}`, 9, `note ${i}`));
  const { watcher, client, panel } = await prepare(snapshot, 2 * pages[0].bytes + 20_000);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  clickOn(panel, buttons(panel.root, "Earlier items")[0]);
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.ok(windowOf(panel, "ia"), "drawn in the earlier items");
  clickOn(panel, buttons(panel.root, "Later items")[0]);
  assert.equal(windowOf(panel, "ia"), undefined, "unmounted with the later items");
  client.activityItemPage = async () => {
    throw new Error("gateway away");
  };
  watcher.event(growth());
  await settle();
  assert.equal(watcher.view.full("turn-b", "ia")?.replacing, false);
  assert.equal(watcher.view.budget.bytes(KEY), 0, "no charge for the window that unmounted");
  watcher.stop();
});
