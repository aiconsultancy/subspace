// Regression tests for #115 review round 2's findings and the round 3
// decisions (SUB a17870f4: every byte held is charged to a bounded budget;
// A3: every part read is fenced by revision and cursor). Each fails without
// its fix. Every fake page reader stops itself after a few reads.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as api from "../src/api.js";
import * as activity from "../src/activity.js";
import * as view from "../src/activity-view.js";
import { utf8Length } from "../src/utf8.js";
import { activityEvent, activitySnapshot, T0, textItem, toolItem, turn } from "./activity-fixtures.js";
import { buttons, clickOn, fakeActivityClient, fakeDoc, flush, pagesOf, settle } from "./activity-helpers.js";

const { createActivityWatcher, decodeActivity, MAX_BUFFER } = activity;
const { readPagedItem, readItemPart, InvalidResponseError } = api;
const PART_PAGES = api.PART_PAGES ?? 4;
const PAGE = 64 * 1024;
const WINDOW_BYTES = view.WINDOW_BYTES ?? 64 * 1024;
const WINDOW_LINES = view.WINDOW_LINES ?? 1000;
const ITEM_WINDOW = view.ITEM_WINDOW ?? 20;
const officer = { name: "Example", role: "captain", live: true };

/** A page reader that fails the test once it has been asked for more than `most` pages. */
function bounded(most, serve) {
  const reads = [];
  const read = (...args) => {
    reads.push(args);
    if (reads.length > most) throw new Error(`runaway read: more than ${most} pages`);
    return serve(...args);
  };
  return { read, reads };
}

/** A snapshot whose running turn holds these items, each as the placeholder of a paged item. */
function snapshotOf(items, paged = {}) {
  const snapshot = activitySnapshot({ seq: 9 });
  snapshot.turns[1].items = items.map((item) => {
    const size = paged[item.id];
    if (!size) return item;
    return { id: item.id, seq: item.seq, startedAt: T0 + 70, updatedAt: T0 + 80, kind: "tool", status: "completed", callId: `call_${item.id}`, name: "read", paged: size };
  });
  return snapshot;
}

const placeholder = (pages) => ({ pages: pages.length, bytes: pages[0].bytes });
const turnsOf = (panel) => panel.root.findAll((node) => node.tag === "details");
const itemsOf = (element) => element.findAll((node) => node.tag === "li");
const toggle = (details, open) => {
  if (open) details.setAttribute("open", "");
  else details.removeAttribute("open");
  details.listeners.toggle?.();
};

// ---------- finding 1: an item over 64 MiB ----------

test("finding 1: an item over 64 MiB is read and shown part by part, never refused and never read whole", { timeout: 10_000 }, async () => {
  const pages = 1600;
  const bytes = pages * PAGE;
  const data = "x".repeat(PAGE);
  const snapshot = snapshotOf([{ id: "huge", seq: 9 }], { huge: { pages, bytes } });
  const client = fakeActivityClient([snapshot]);
  const { read, reads } = bounded(12, async (agent, turnId, item, page) => ({ agent, turn: turnId, item, seq: 9, page, pages, bytes, data }));
  client.activityItemPage = read;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "huge");
  let shown = watcher.view.full("turn-b", "huge");
  assert.equal(shown.mode, "parts", "read in parts");
  assert.equal(shown.state, "ready");
  assert.equal(shown.count, pages / PART_PAGES);
  assert.equal(shown.data.length, PART_PAGES * PAGE, "its first part");
  assert.deepEqual(reads.map((args) => args[3]), [0, 1, 2, 3]);
  // The last part is reached directly, by page number.
  await watcher.loadPart("turn-b", "huge", "last");
  shown = watcher.view.full("turn-b", "huge");
  assert.equal(shown.part, pages / PART_PAGES - 1);
  assert.deepEqual(reads.slice(4).map((args) => args[3]), [1596, 1597, 1598, 1599]);
  assert.ok(watcher.view.budget.used <= watcher.view.budget.limit, "within the view's budget");
  watcher.stop();
});

// ---------- finding 2: the render working set ----------

test("finding 2: a run of hidden characters is one marker element, each character still shown, and a copy is exactly what shows", () => {
  assert.deepEqual(view.visibleSegments("a\u0000\u0001\u202Eb\u200Fc"), ["a", { ctl: "⟨U+0000⟩⟨U+0001⟩⟨U+202E⟩" }, "b", { ctl: "⟨U+200F⟩" }, "c"]);
  const run = "\u0001".repeat(10_000);
  const element = view.mount(view.itemTree("turn-a", textItem("i9", 1, `x${run}y`)), fakeDoc);
  const markers = element.findAll((node) => node.attributes.class === "ctl");
  assert.equal(markers.length, 1, "one element for the run, not one per character");
  const shown = `x${"⟨U+0001⟩".repeat(10_000)}y`;
  assert.equal(markers[0].textContent, "⟨U+0001⟩".repeat(10_000));
  // What a selection copies is the elements' text: exactly what shows.
  assert.ok(element.textContent.includes(shown));
  assert.equal(view.visibleText(`x${run}y`), shown);
});

test("finding 2: a closed turn mounts no item bodies; opening mounts them and closing unmounts them", async () => {
  const client = fakeActivityClient([activitySnapshot({ seq: 9 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const [running, ended] = turnsOf(panel);
  assert.ok("open" in running.attributes && !("open" in ended.attributes));
  assert.equal(itemsOf(ended).length, 0, "the closed turn builds no items");
  assert.ok(itemsOf(running).length > 0);
  toggle(ended, true);
  assert.equal(itemsOf(ended).length, 3, "opening it mounts its items");
  toggle(ended, false);
  assert.equal(itemsOf(ended).length, 0, "closing it unmounts them");
  watcher.stop();
});

test("finding 2: an open turn mounts a bounded window of items, each a bounded window of its text, charged to the budget", async () => {
  const small = Array.from({ length: 45 }, (_, index) => textItem(`s${index}`, 1, `Reply ${index}`));
  const lines = textItem("lines", 1, Array.from({ length: 3000 }, (_, index) => `row ${index}`).join("\n"));
  const wide = textItem("wide", 1, "q".repeat(200 * 1024));
  const client = fakeActivityClient([snapshotOf([...small, lines, wide])]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const running = turnsOf(panel)[0];
  const mounted = itemsOf(running);
  assert.equal(mounted.length, ITEM_WINDOW, "at most ITEM_WINDOW items mounted");
  assert.equal(buttons(running, "Earlier items").length, 1, "the rest are reachable");
  for (const item of mounted) {
    const text = item.textContent;
    assert.ok(utf8Length(text) <= WINDOW_BYTES + 1024, `an item mounts at most one window (${utf8Length(text)} bytes)`);
    assert.ok(text.split("\n").length <= WINDOW_LINES + 20, "and at most WINDOW_LINES lines");
  }
  // Every byte stays reachable: the wide item's last window ends with its last byte.
  const wideItem = mounted.at(-1);
  clickOn(panel, buttons(wideItem, "Last")[0]);
  assert.ok(itemsOf(turnsOf(panel)[0]).at(-1).textContent.endsWith("q"));
  assert.ok(watcher.view.budget.used > 0, "mounted content and formats are charged to the view's budget");
  watcher.stop();
});

// ---------- finding 3: the rerun gate ----------

test("finding 3: an overflow refetch queued behind a held snapshot waits for the new stream after a disconnect, then reads once", async () => {
  const client = fakeActivityClient(["hold", activitySnapshot({ seq: 30 }), activitySnapshot({ seq: 31 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain" });
  watcher.start();
  watcher.reopened();
  assert.equal(client.calls.length, 1, "the first snapshot is held");
  // Overflow the buffer: a refetch is queued behind the held read.
  for (let seq = 10; seq <= 10 + MAX_BUFFER; seq += 1) watcher.event(activityEvent(seq, { type: "item", item: textItem(`e${seq}`, seq, "x") }));
  watcher.disconnected();
  client.pending[0](activitySnapshot({ seq: 9 }));
  await flush();
  assert.equal(client.calls.length, 1, "no read until the new stream answers");
  watcher.reopened();
  await flush();
  assert.equal(client.calls.length, 2, "then exactly one");
  watcher.stop();
});

test("finding 3: a retry queued behind a held snapshot waits for the new stream after a reset, then reads once", async () => {
  const client = fakeActivityClient(["hold", activitySnapshot({ seq: 30 }), activitySnapshot({ seq: 31 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain" });
  watcher.start();
  watcher.reopened();
  watcher.retry();
  watcher.reset();
  client.pending[0](activitySnapshot({ seq: 9 }));
  await flush();
  assert.equal(client.calls.length, 1, "no read until the new stream answers");
  watcher.reopened();
  await flush();
  assert.equal(client.calls.length, 2, "then exactly one");
  assert.equal(watcher.view.status, "ready");
  watcher.stop();
});

// ---------- finding 4: page-count checks ----------

test("finding 4: pages that together run past the item's length are refused at the page that passes it", async () => {
  const { read, reads } = bounded(5, async (page) => ({ seq: 1, page, pages: 3, bytes: 5, data: ["ab", "cd", "ef"][page] }));
  await assert.rejects(readPagedItem(read), /Page 2 of the item runs past its length/);
  assert.deepEqual(reads.map((args) => args[0]), [0, 1, 2], "each page alone fits; their sum does not");
  // A part read with its offset known is held to the same sum.
  const part = bounded(5, async (page) => ({ seq: 1, page, pages: 6, bytes: 9, data: "ab" }));
  await assert.rejects(readItemPart(part.read, { from: 4, offset: 8, version: { seq: 1, bytes: 9, pages: 6 } }), /Page 4 of the item runs past its length/);
  assert.equal(part.reads.length, 1);
  // A page count that needs empty pages is refused before any further read.
  const sparse = bounded(5, async (page) => ({ seq: 1, page, pages: 3, bytes: 2, data: "{" }));
  await assert.rejects(readPagedItem(sparse.read), /claims 3 pages for 2 bytes/);
  assert.equal(sparse.reads.length, 1);
  // Without its offset, a part allows a byte for each page before it, and no more.
  const late = bounded(5, async (page) => ({ seq: 1, page, pages: 6, bytes: 9, data: "abcde" }));
  await assert.rejects(readItemPart(late.read, { from: 4, version: { seq: 1, bytes: 9, pages: 6 } }), /runs past its length/);
});

test("finding 4: a page boundary between multibyte characters is valid, and the item joins in full", async () => {
  const text = JSON.stringify({ id: "i", t: "é🛰✓" });
  const cut = text.indexOf("🛰");
  const parts = [text.slice(0, cut), text.slice(cut)];
  const bytes = utf8Length(text);
  const { read } = bounded(4, async (page) => ({ item: "i", seq: 1, page, pages: 2, bytes, data: parts[page] }));
  const item = await readPagedItem(read, { item: "i" });
  assert.equal(item.item.t, "é🛰✓");
  assert.equal(item.bytes, bytes);
  assert.ok(utf8Length(parts[0]) !== parts[0].length, "the first page ends after a multibyte character");
});

// ---------- finding 5 ----------

test("finding 5a: a turn with no startedAt shows as unreadable, as sent", () => {
  const snapshot = activitySnapshot({ seq: 9 });
  const { startedAt, ...noStart } = snapshot.turns[0];
  snapshot.turns[0] = noStart;
  const decoded = decodeActivity(snapshot);
  assert.equal(decoded.turns.length, 2, "kept, not dropped");
  const kept = decoded.turns.find((each) => each.id === "turn-a");
  assert.equal(kept.status, "unreadable");
  assert.deepEqual(kept.items[0].raw, noStart, "its one item is the whole turn as sent");
  const shown = view.mount(view.itemTree("turn-a", kept.items[0]), fakeDoc).textContent;
  assert.ok(shown.includes("doesn't have the fields this console reads"));
  assert.ok(shown.includes("Done."), "its content still shows");
  const summary = view.turnSummary(kept).map((node) => view.mount(node, fakeDoc).textContent).join(" ");
  assert.ok(summary.includes("Unreadable") && summary.includes("Start not known"));
});

test("finding 5b: an item event with no id reads the snapshot again", async () => {
  const client = fakeActivityClient([activitySnapshot({ seq: 9 }), activitySnapshot({ seq: 10 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  assert.equal(client.calls.length, 1);
  watcher.event(activityEvent(10, { type: "item", item: { seq: 10, kind: "text", status: "completed", text: "no id" } }));
  await flush();
  assert.equal(client.calls.length, 2, "read again, where the item shows as unreadable");
  watcher.stop();
});

test("finding 5c: a paged item whose pages join into another item shows failed with Try again", async () => {
  const other = toolItem("other", 9, { content: [{ type: "text", text: "a".repeat(70_000) }] });
  const pages = pagesOf(other, { id: "i2" });
  const client = fakeActivityClient([snapshotOf([{ id: "i2", seq: 9 }], { i2: placeholder(pages) })]);
  const { read } = bounded(6, async (agent, turnId, item, page) => pages[page]);
  client.activityItemPage = read;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "i2");
  const shown = watcher.view.full("turn-b", "i2");
  assert.equal(shown.state, "failed");
  assert.match(shown.error, /not join into the item asked for/);
  const loads = [];
  const panel = view.createActivityPanel({ doc: fakeDoc, onLoad: (turnId, item) => loads.push([turnId, item]) });
  panel.update(watcher.view, officer);
  const again = buttons(panel.root, "Try again");
  assert.equal(again.length, 1);
  clickOn(panel, again[0]);
  assert.deepEqual(loads, [["turn-b", "i2"]]);
  watcher.stop();
});

// ---------- A3: every part read fenced by revision and cursor ----------

/** An item of about 1 MiB of multibyte lines, its pages, and a snapshot holding it and a 100 KiB item. */
function largeAndSmall() {
  const text = Array.from({ length: 16_000 }, (_, index) => `line ${index} ünï ✓ 🛰 ${"·".repeat(20)}`).join("\n");
  const a = toolItem("ia", 9, { name: "read", content: [{ type: "text", text }] });
  const b = toolItem("ib", 9, { name: "read", content: [{ type: "text", text: "b".repeat(100 * 1024) }] });
  const served = { ia: pagesOf(a), ib: pagesOf(b) };
  const client = fakeActivityClient([snapshotOf([a, b], { ia: placeholder(served.ia), ib: placeholder(served.ib) })]);
  return { a, b, served, client };
}

/** Read every part of `itemId` from the one shown, in order, with Next. */
async function readOnward(watcher, itemId) {
  const parts = [];
  for (let guard = 0; guard < 20; guard += 1) {
    const shown = watcher.view.full("turn-b", itemId);
    assert.equal(shown.state, "ready");
    assert.equal(shown.part, parts.length, "in order");
    parts.push(shown.data);
    if (!shown.hasNext) return parts;
    await watcher.loadPart("turn-b", itemId, "next");
  }
  throw new Error("too many parts");
}

test("A3: a part reply that arrives after its item was evicted is dropped; the item then reads in full and in order", { timeout: 10_000 }, async () => {
  const { a, served, client } = largeAndSmall();
  const reads = [];
  let release = null;
  client.activityItemPage = (agent, turnId, item, page) => {
    reads.push(`${item}:${page}`);
    if (reads.length > 60) throw new Error("runaway read");
    // Hold A's second part.
    if (item === "ia" && page === PART_PAGES && !release) return new Promise((resolve) => (release = () => resolve(served.ia[page])));
    return Promise.resolve(served[item][page]);
  };
  const limit = 300 * 1024;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: limit });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "ia");
  assert.equal(watcher.view.full("turn-b", "ia").mode, "parts");
  const first = watcher.view.full("turn-b", "ia").data;
  const second = watcher.loadPart("turn-b", "ia", "next");
  await settle();
  assert.ok(release, "A's second part is held");
  assert.equal(watcher.view.full("turn-b", "ia").state, "loading");
  // B is read whole; making room for it evicts A, whose read in flight is fenced off.
  await watcher.loadItem("turn-b", "ib");
  assert.equal(watcher.view.full("turn-b", "ib").state, "ready");
  assert.equal(watcher.view.full("turn-b", "ia"), null, "A was evicted");
  release();
  await second;
  await flush();
  assert.equal(watcher.view.full("turn-b", "ia"), null, "the late reply is dropped");
  assert.ok(watcher.view.budget.used <= limit);
  // A again: from its first part, every part in order, joining to its content exactly.
  const before = reads.length;
  await watcher.loadItem("turn-b", "ia");
  const parts = await readOnward(watcher, "ia");
  assert.equal(parts[0], first);
  assert.equal(parts.join(""), JSON.stringify(a));
  assert.deepEqual(reads.slice(before), served.ia.map((_, page) => `ia:${page}`), "each page once, in order");
  assert.ok(watcher.view.budget.used <= limit + api.PAGE_DATA_BYTES * PART_PAGES);
  watcher.stop();
});

test("A3: a line split across a part boundary joins exactly once after the part before it was evicted", { timeout: 10_000 }, async () => {
  const { a, served, client } = largeAndSmall();
  const { read, reads } = bounded(60, async (agent, turnId, item, page) => served[item][page]);
  client.activityItemPage = read;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: 300 * 1024 });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "ia");
  await watcher.loadPart("turn-b", "ia", "next");
  const shownSecond = watcher.view.full("turn-b", "ia").data;
  // Evict A while its second part shows, and the first part with it.
  await watcher.loadItem("turn-b", "ib");
  assert.equal(watcher.view.full("turn-b", "ia"), null);
  const before = reads.length;
  await watcher.loadItem("turn-b", "ia");
  assert.equal(watcher.view.full("turn-b", "ia").part, 0, "an evicted item starts again from its first part, with no cursor");
  const first = watcher.view.full("turn-b", "ia").data;
  await watcher.loadPart("turn-b", "ia", "next");
  const second = watcher.view.full("turn-b", "ia").data;
  assert.equal(second, shownSecond, "the same part as before");
  assert.deepEqual(reads.slice(before).map((args) => args[3]), Array.from({ length: 2 * PART_PAGES }, (_, page) => page));
  // The line across the boundary: its head ends the first part and its tail starts the second.
  const whole = JSON.stringify(a);
  const head = first.slice(first.lastIndexOf("\\n") + 2);
  const tail = second.slice(0, second.indexOf("\\n"));
  assert.ok(head && tail, "the boundary falls inside a line");
  const at = whole.lastIndexOf("\\n", first.length - 1) + 2;
  assert.equal(head + tail, whole.slice(at, whole.indexOf("\\n", first.length)), "joined exactly once");
  // Shown in windows, the second part starts with the line's tail.
  const panel = view.createActivityPanel({ doc: fakeDoc, onPart: () => {} });
  panel.update(watcher.view, officer);
  const pre = turnsOf(panel)[0].findAll((node) => node.tag === "pre" && node.textContent.startsWith(tail));
  assert.equal(pre.length, 1);
  watcher.stop();
});

// ---------- the turn decode keeps a well-formed turn with its items ----------

test("finding 5a: a well-formed turn still decodes with its items", () => {
  const decoded = decodeActivity({ ...activitySnapshot(), turns: [turn("turn-x", 1, { items: [textItem("i1", 1, "Hello")] })] });
  assert.equal(decoded.turns[0].status, "completed");
  assert.equal(decoded.turns[0].items[0].text, "Hello");
});
