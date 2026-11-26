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
