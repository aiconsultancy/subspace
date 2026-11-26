// Regression tests for #115 review round 3's findings (SUB-69 comment
// 5d0227f2) and the decisions they follow (SUB ed71a299, 6f5e67dd,
// 0c13b784). Each fails without its fix. Every fake page reader stops
// itself after a few reads.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as activity from "../src/activity.js";
import * as view from "../src/activity-view.js";
import * as document from "../src/document.js";
import { utf8Length } from "../src/utf8.js";
import { activityEvent, activitySnapshot, T0, textItem, toolItem } from "./activity-fixtures.js";
import { buttons, clickOn, fakeActivityClient, fakeDoc, flush, pagesOf, settle } from "./activity-helpers.js";

const { createActivityWatcher, decodeActivity } = activity;
const officer = { name: "Example", role: "captain", live: true };
const turnsOf = (panel) => panel.root.findAll((node) => node.tag === "details");
const itemsOf = (element) => element.findAll((node) => node.tag === "li");
const windowsShown = (element) => element.findAll((node) => node.attributes.class === "act-pre act-part");

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

/** A snapshot whose running turn holds these items, those named in `paged` as paged placeholders. */
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

// ---------- P2: a turn whose items are not a list ----------

test("P2: a turn whose items is not a list stays an unreadable turn, its payload shown as sent", async () => {
  const snapshot = activitySnapshot({ seq: 9 });
  snapshot.turns[0].items = { note: "not a list" };
  const decoded = decodeActivity(snapshot);
  const kept = decoded.turns.find((each) => each.id === "turn-a");
  assert.equal(kept.status, "unreadable", "not a turn that observed nothing");
  assert.deepEqual(kept.items[0].raw, snapshot.turns[0], "its one item is the whole turn as sent");
  // Drawn through the bounded renderer, like any unreadable item.
  const client = fakeActivityClient([snapshot]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const unreadable = turnsOf(panel).find((details) => details.textContent.includes("turn-a"));
  unreadable.setAttribute("open", "");
  unreadable.listeners.toggle?.();
  assert.ok(unreadable.textContent.includes("not a list"), "its content still shows");
  assert.ok(!unreadable.textContent.includes("Nothing observed in this turn yet."));
  // A turn event with items that are not a list asks for the snapshot again.
  const events = fakeActivityClient([activitySnapshot({ seq: 9 }), activitySnapshot({ seq: 11 })]);
  const live = createActivityWatcher({ client: events, agent: "example-captain", awaitStream: false });
  live.start();
  await settle();
  live.event(activityEvent(10, { type: "turn-updated", turn: { ...activitySnapshot().turns[1], seq: 10, items: "nope" } }));
  await flush();
  assert.equal(events.calls.length, 2);
  watcher.stop();
  live.stop();
});

// ---------- P2: sizes and windows as shown ----------

test("P2: a body is measured and split by its bytes as shown, each hidden character a whole 12-byte marker", async () => {
  assert.equal(utf8Length(document.marker("\u0001")), document.MARKER_BYTES);
  const run = `${"\u0001".repeat(9_000)}${"\u202E".repeat(3_000)}end`;
  const item = textItem("ctl", 1, run);
  const shownText = view.visibleText(view.itemDocument(item));
  assert.equal(view.formatItem(item).bytes, utf8Length(shownText), "charged as shown, not as the source's bytes");
  const client = fakeActivityClient([snapshotOf([item])]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const body = () => itemsOf(turnsOf(panel)[0])[0];
  const seen = [];
  for (let guard = 0; guard < 10; guard += 1) {
    const [pre] = windowsShown(body());
    assert.ok(pre, "12 KiB of source shows as 144 KiB, so in windows");
    assert.ok(utf8Length(pre.textContent) <= view.WINDOW_BYTES, `a window holds at most WINDOW_BYTES as shown (${utf8Length(pre.textContent)})`);
    assert.match(pre.textContent, /^(⟨U\+[0-9A-F]{4}⟩|[^⟨⟩])*$/, "no marker is split");
    seen.push(pre.textContent);
    const next = buttons(body(), "Next")[0];
    if (!next || next.attributes["aria-disabled"]) break;
    clickOn(panel, next);
  }
  assert.ok(seen.length > 2);
  assert.ok(shownText.endsWith(seen.join("")) && seen.join("").endsWith("end"), "the windows join into exactly what shows");
  watcher.stop();
});

test("P2: the pieces a body is measured from join into exactly the text the view shows", () => {
  const tricky = { a: "x\u0000y\r\n\t\"\\", "k\u0001": [1, null, undefined, () => 1, { b: [] }, {}], lone: "\ud800z", pair: "🛰", n: NaN, d: new Date(T0), u: undefined };
  assert.equal([...document.shownJsonPieces(tricky)].join(""), view.json(tricky));
  assert.equal(document.jsonBytes(tricky), utf8Length(JSON.stringify(tricky)));
  const tool = toolItem("t", 1, { content: [{ type: "text", text: "out\u0007" }, { type: "file", name: "f" }, 3], metadata: null });
  const old = Object.entries(tool).map(([key, value]) => (key === "content" ? value.map((part, index) => `content[${index}]${part?.type === "text" ? " (text)" : ""}\n${part?.type === "text" ? part.text : view.json(part)}`).join("\n\n") : `${key}\n${typeof value === "string" ? value : view.json(value)}`)).join("\n\n");
  assert.equal(view.itemDocument(tool), old);
});

// ---------- P2: measures and windows kept by version ----------

test("P2: a window is kept by version across a snapshot read again, and a body is measured once per version", async () => {
  const big = textItem("big", 3, "w".repeat(200 * 1024));
  const client = fakeActivityClient([snapshotOf([big]), snapshotOf([big]), snapshotOf([{ ...big, seq: 10, text: "v".repeat(200 * 1024) }])]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc, onRetry: () => watcher.retry() });
  panel.update(watcher.view, officer);
  const body = () => itemsOf(turnsOf(panel)[0])[0];
  clickOn(panel, buttons(body(), "Next")[0]);
  assert.ok(body().textContent.includes("Section 2 of"));
  const measured = panel.held().measured;
  for (let i = 0; i < 5; i += 1) panel.update(watcher.view, officer);
  assert.equal(panel.held().measured, measured, "not measured again on each draw");
  // The same version read again, as a new object: the window stays.
  watcher.retry();
  await settle();
  panel.update(watcher.view, officer);
  assert.ok(body().textContent.includes("Section 2 of"), "the window follows the version, not the object");
  assert.equal(panel.held().measured, measured, "the same version is not measured again");
  // A newer version starts at its first window and is measured once.
  watcher.retry();
  await settle();
  panel.update(watcher.view, officer);
  assert.ok(body().textContent.includes("Section 1 of"));
  assert.ok(windowsShown(body())[0].textContent.includes("vvvvvvvv") && !windowsShown(body())[0].textContent.includes("wwwwwwww"), "the newer text");
  assert.equal(panel.held().measured, measured + 1);
  watcher.stop();
});

test("P2: an evicted read's measure and window are forgotten, and a read again starts afresh", { timeout: 10_000 }, async () => {
  const a = toolItem("ia", 9, { name: "read", content: [{ type: "text", text: "a".repeat(150 * 1024) }] });
  const b = toolItem("ib", 9, { name: "read", content: [{ type: "text", text: "b".repeat(150 * 1024) }] });
  const served = { ia: pagesOf(a), ib: pagesOf(b) };
  const client = fakeActivityClient([snapshotOf([a, b], { ia: placeholder(served.ia), ib: placeholder(served.ib) })]);
  const { read } = bounded(20, async (agent, turnId, item, page) => served[item][page]);
  client.activityItemPage = read;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: 500 * 1024 });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  const ofItem = (id) => itemsOf(turnsOf(panel)[0]).find((li) => li.findAll((node) => node.attributes["data-activity-item"] === id || node.attributes["data-activity-load"] === id).length);
  clickOn(panel, buttons(ofItem("ia"), "Last")[0]);
  const key = "turn-b\u0000ia";
  assert.ok(panel.held().measures.some(([held]) => held === key));
  assert.ok(panel.held().windows.some(([held]) => held === key));
  // B evicts A.
  await watcher.loadItem("turn-b", "ib");
  assert.equal(watcher.view.full("turn-b", "ia"), null, "A was evicted");
  panel.update(watcher.view, officer);
  assert.ok(!panel.held().measures.some(([held]) => held === key), "its measure is forgotten");
  assert.ok(!panel.held().windows.some(([held]) => held === key), "and its window");
  assert.equal(buttons(ofItem("ia"), "Load in full").length, 1);
  watcher.stop();
});

// ---------- P1: the inline floor, a caption with Show, and the order bodies draw in ----------

/** The inline charge a snapshot makes once held: read from a watcher that only holds it. */
async function inlineOf(snapshot) {
  const probe = createActivityWatcher({ client: fakeActivityClient([snapshot]), agent: "example-captain", awaitStream: false });
  probe.start();
  await settle();
  const bytes = probe.view.budget.bytes(activity.INLINE_KEY);
  probe.stop();
  return bytes;
}

const captions = (element) => element.findAll((node) => node.tag === "p" && node.textContent.startsWith("Not drawn, to stay within the view's memory budget ("));
const showOf = (element) => buttons(element, "Show");
const big = (id, seq, letter) => textItem(id, seq, letter.repeat(100 * 1024));
const bodyWith = (element, letter) => itemsOf(element).find((li) => li.textContent.includes(letter.repeat(64)));
const heldBackWith = (element, id) => itemsOf(element).find((li) => li.findAll((node) => node.attributes["data-activity-show"] === id).length);

test("P1: the inline snapshot is charged first, pinned as the floor, and no read frees it", async () => {
  const a = toolItem("ia", 9, { name: "read", content: [{ type: "text", text: "a".repeat(150 * 1024) }] });
  const served = { ia: pagesOf(a) };
  const snapshot = snapshotOf([big("t1", 3, "K"), a], { ia: placeholder(served.ia) });
  const inline = await inlineOf(snapshot);
  const decoded = decodeActivity(snapshot);
  assert.equal(inline, decoded.turns.reduce((sum, each) => sum + document.jsonBytes(each), 0), "its compact JSON");
  assert.ok(inline > 100 * 1024);
  const client = fakeActivityClient([snapshot]);
  client.activityItemPage = bounded(20, async (agent, turnId, item, page) => served[item][page]).read;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: inline + 200 * 1024 });
  watcher.start();
  await settle();
  const budget = watcher.view.budget;
  assert.equal(budget.bytes(activity.INLINE_KEY), inline);
  // 150 KiB read whole needs 300 KiB: not in the 200 KiB left beside the floor, so it is read in parts, and the floor stays.
  await watcher.loadItem("turn-b", "ia");
  assert.equal(watcher.view.full("turn-b", "ia").mode, "parts");
  assert.equal(budget.bytes(activity.INLINE_KEY), inline, "never freed for a read");
  assert.equal(budget.freeOldest(() => false), true, "the read can be freed");
  assert.equal(budget.freeOldest(() => false), false, "the floor cannot");
  assert.equal(budget.bytes(activity.INLINE_KEY), inline);
  watcher.stop();
});

test("P1: a body past the room shows its size and Show, its turn stays open, and Show draws it, freeing bodies shown earlier", async () => {
  const snapshot = snapshotOf([big("b1", 3, "P"), big("b2", 4, "Q"), big("b3", 5, "R")]);
  const inline = await inlineOf(snapshot);
  const client = fakeActivityClient([snapshot]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: inline + 100 * 1024 });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const running = () => turnsOf(panel)[watcher.view.turns.findIndex((each) => each.id === "turn-b")];
  const budget = watcher.view.budget;
  assert.notEqual(running().getAttribute("open"), null, "the turn stays open");
  assert.ok(bodyWith(running(), "P"), "the first body draws, a window of it");
  assert.equal(captions(running()).length, 2, "the two that do not fit show a caption");
  const measured = view.formatItem(big("b3", 5, "R")).bytes;
  assert.ok(heldBackWith(running(), "b3").textContent.includes(`(${measured.toLocaleString("en-GB")} bytes)`), "with their size as shown");
  assert.ok(budget.used <= budget.limit, "within the budget");
  assert.ok(budget.bytes(view.DRAW_KEY) > 0, "what draws is charged");
  // Show the third: it draws, and the first is held back to make room.
  clickOn(panel, showOf(heldBackWith(running(), "b3"))[0]);
  assert.ok(bodyWith(running(), "R"));
  assert.ok(heldBackWith(running(), "b1") && heldBackWith(running(), "b2"));
  assert.ok(budget.used <= budget.limit);
  // Show the second: the third, shown earlier, gives way.
  clickOn(panel, showOf(heldBackWith(running(), "b2"))[0]);
  assert.ok(bodyWith(running(), "Q"));
  assert.ok(heldBackWith(running(), "b3"), "the body shown earlier gives way");
  assert.notEqual(running().getAttribute("open"), null);
  watcher.stop();
});

test("P1: Show frees loaded items first, the least recently opened, and never the body's own", async () => {
  const ia = toolItem("ia", 9, { name: "read", content: [{ type: "text", text: "a".repeat(30 * 1024) }] });
  const ib = toolItem("ib", 9, { name: "read", content: [{ type: "text", text: "b".repeat(30 * 1024) }] });
  const served = { ia: pagesOf(ia), ib: pagesOf(ib) };
  const snapshot = snapshotOf([big("k", 3, "K"), ia, ib], { ia: placeholder(served.ia), ib: placeholder(served.ib) });
  const inline = await inlineOf(snapshot);
  const client = fakeActivityClient([snapshot]);
  client.activityItemPage = bounded(20, async (agent, turnId, item, page) => served[item][page]).read;
  const one = 2 * utf8Length(JSON.stringify(ia));
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: inline + 2 * one + 20 * 1024 });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "ia");
  await watcher.loadItem("turn-b", "ib");
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const running = () => turnsOf(panel)[0];
  assert.ok(heldBackWith(running(), "k"), "the inline body does not fit beside two loaded items");
  clickOn(panel, showOf(heldBackWith(running(), "k"))[0]);
  assert.equal(watcher.view.full("turn-b", "ia"), null, "the least recently opened is freed");
  assert.equal(watcher.view.full("turn-b", "ib").state, "ready", "one is enough");
  panel.update(watcher.view, officer);
  assert.ok(bodyWith(running(), "K"));
  const budget = watcher.view.budget;
  assert.ok(budget.used <= budget.limit);
  watcher.stop();
});

test("P1: bodies draw asked first, then turns opened by hand (newest first), then the running turn", async () => {
  const snapshot = snapshotOf([big("rb", 7, "B")]);
  snapshot.turns[0].items = [big("ra", 1, "A")];
  snapshot.turns.unshift({ ...snapshot.turns[0], id: "turn-x", seq: 2, items: [big("rx", 1, "X")] });
  const inline = await inlineOf(snapshot);
  const client = fakeActivityClient([snapshot]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: inline + 100 * 1024 });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const of = (id) => turnsOf(panel)[watcher.view.turns.findIndex((each) => each.id === id)];
  const toggle = (details, open) => {
    if (open) details.setAttribute("open", "");
    else details.removeAttribute("open");
    details.listeners.toggle?.();
  };
  assert.ok(bodyWith(of("turn-b"), "B"), "only the running turn is open");
  toggle(of("turn-a"), true);
  assert.ok(bodyWith(of("turn-a"), "A"), "a turn opened by hand draws before the running turn");
  assert.ok(heldBackWith(of("turn-b"), "rb"), "the running turn's body is held back, its turn still open");
  assert.notEqual(of("turn-b").getAttribute("open"), null);
  toggle(of("turn-x"), true);
  assert.ok(bodyWith(of("turn-x"), "X"), "the turn opened last first");
  assert.ok(heldBackWith(of("turn-a"), "ra"));
  // Asked for draws before any of them.
  clickOn(panel, showOf(heldBackWith(of("turn-b"), "rb"))[0]);
  assert.ok(bodyWith(of("turn-b"), "B"));
  assert.ok(heldBackWith(of("turn-x"), "rx"));
  // Closing a turn forgets its ask and its opening.
  toggle(of("turn-b"), false);
  assert.ok(bodyWith(of("turn-x"), "X"));
  toggle(of("turn-x"), false);
  assert.ok(bodyWith(of("turn-a"), "A"));
  watcher.stop();
});

test("P1: a body asked for that is larger than the room still draws, a window at a time: the floor", async () => {
  const snapshot = snapshotOf([big("f", 3, "F")]);
  const inline = await inlineOf(snapshot);
  const client = fakeActivityClient([snapshot]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: inline + 10 * 1024 });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const body = () => itemsOf(turnsOf(panel)[0])[0];
  assert.equal(captions(body()).length, 1, "no room for one window");
  clickOn(panel, showOf(body())[0]);
  assert.ok(body().textContent.includes("Section 1 of 2"));
  assert.deepEqual(["First", "Previous", "Next", "Last"].map((words) => buttons(body(), words).length), [1, 1, 1, 1]);
  clickOn(panel, buttons(body(), "Next")[0]);
  assert.ok(body().textContent.includes("Section 2 of 2"), "every window reachable");
  const budget = watcher.view.budget;
  assert.ok(budget.used > budget.limit, "only the floor passes the limit");
  assert.ok(budget.used <= inline + view.WINDOW_BYTES + 4096, "the inline content and one window of what was asked for");
  watcher.stop();
});

// ---------- P3: a part read reserves the part and its drawn copy ----------

test("P3: a part read reserves the part held and its drawn copy before it reads, and keeps both", async () => {
  const a = toolItem("ia", 9, { name: "read", content: [{ type: "text", text: "a".repeat(600 * 1024) }] });
  const served = { ia: pagesOf(a) };
  const client = fakeActivityClient([snapshotOf([a], { ia: placeholder(served.ia) })]);
  let reserved = null;
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: 1024 * 1024 });
  client.activityItemPage = bounded(20, async (agent, turnId, item, page) => {
    reserved ??= watcher.view.budget.bytes("turn-b\u0000ia");
    return served[item][page];
  }).read;
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "ia");
  const full = watcher.view.full("turn-b", "ia");
  assert.equal(full.mode, "parts", "1.2 MiB whole does not fit 1 MiB");
  assert.equal(reserved, 2 * activity.PART_MAX_BYTES, "the part and its drawn copy, before the first page is read");
  assert.equal(watcher.view.budget.bytes("turn-b\u0000ia"), 2 * full.size, "both kept once read");
  assert.equal(full.size, utf8Length(full.data));
  watcher.stop();
});
