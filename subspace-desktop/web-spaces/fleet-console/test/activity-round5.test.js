// Round 5 of #115 (SUB decision 1c19f413): one owner for the view's budget,
// the drawn window reserved in shown bytes, live row keys, generation-scoped
// bodies and one lossless block model. Phobos's eight probes are here as
// regressions, each named for what it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { createActivityWatcher, INLINE_KEY, PART_MAX_BYTES } from "../src/activity.js";
import { createActivityPanel, DRAW_KEY, WINDOW_BYTES } from "../src/activity-view.js";
import { createBudget } from "../src/budget.js";
import { createPager } from "../src/pager.js";
import { activityEvent, textItem, toolItem } from "./activity-fixtures.js";
import { buttons, clickOn, fakeActivityClient, fakeDoc, pagedSnapshot, pagesOf, settle } from "./activity-helpers.js";

const officer = { name: "Example", role: "captain", live: true };
const ITEM = "turn-b\u0000ia";

/** A watcher whose budget is the snapshot's inline charge plus `extra`, and a panel drawn from it. */
async function prepare(snapshot, extra) {
  const probe = createActivityWatcher({ client: fakeActivityClient([snapshot]), agent: "example-captain", awaitStream: false });
  probe.start();
  await settle();
  const inline = probe.view.budget.bytes(INLINE_KEY);
  probe.stop();
  const client = fakeActivityClient([snapshot]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: inline + extra });
  watcher.start();
  await settle();
  const panel = createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  return { watcher, client, panel, inline };
}

/** Make the running turn's one item a paged tool call served as `pages`. */
function usePaged(snapshot, item, pages) {
  snapshot.turns[1].items = [{ ...snapshot.turns[1].items[0], id: item.id, kind: "tool", name: "read", seq: item.seq, paged: { pages: pages.length, bytes: pages[0].bytes } }];
}

/** A snapshot whose running turn holds one paged item, `text` long, and the pages it is served as. */
function pagedWith(text, id = "ia") {
  const item = toolItem(id, 9, { content: [{ type: "text", text }] });
  const pages = pagesOf(item);
  const snapshot = pagedSnapshot([id]);
  usePaged(snapshot, item, pages);
  return { item, pages, snapshot };
}

const windowsIn = (root) => root.findAll((node) => node.attributes.class === "act-pre act-part");

// ---------- 1. One budget owner ----------

test("1: at the floor a settled 256 KiB part charges the part and one 64 KiB window, 320 KiB, not 512 (probe 1)", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const { watcher, client, panel, inline } = await prepare(snapshot, 0);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full("turn-b", "ia").size, PART_MAX_BYTES);
  assert.equal(watcher.view.budget.bytes(DRAW_KEY), 0, "its window is in the read's charge");
  assert.equal(watcher.view.budget.used - inline, PART_MAX_BYTES + WINDOW_BYTES, "the steady-state floor");
  assert.equal(windowsIn(panel.root).length, 1, "and it draws");
  watcher.stop();
});

test("1: a Load after a Show sends the Show back to its caption: one window at the floor, never two (probe 8)", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  snapshot.turns[1].items.push(textItem("inline", 8, "b".repeat(100 * 1024)));
  const { watcher, client, panel } = await prepare(snapshot, 0);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  clickOn(panel, buttons(panel.root, "Show")[0]);
  assert.equal(windowsIn(panel.root).length, 1, "the body asked for draws at the floor");
  assert.equal(watcher.view.budget.requested, "turn-b\u0000inline");
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.budget.requested, ITEM, "the Load is the latest request");
  assert.equal(windowsIn(panel.root).length, 1, "a later request must not keep two forced windows");
  assert.equal(buttons(panel.root, "Show").length, 1, "the earlier Show is back to its caption");
  watcher.stop();
});

test("1: a Show after a Load frees the read, no longer the latest request, and draws only the floor", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  snapshot.turns[1].items.push(textItem("inline", 8, "b".repeat(100 * 1024)));
  const { watcher, client, panel, inline } = await prepare(snapshot, PART_MAX_BYTES + WINDOW_BYTES);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.equal(buttons(panel.root, "Show").length, 1, "no room beside the part for the inline body");
  clickOn(panel, buttons(panel.root, "Show")[0]);
  const budget = watcher.view.budget;
  assert.equal(budget.requested, "turn-b\u0000inline");
  assert.equal(watcher.view.full("turn-b", "ia"), null, "the read, no longer the latest request, made room");
  // The window as shown, and its measure's numbers.
  assert.ok(budget.used - inline <= WINDOW_BYTES + 256, `only the floor: the inline content and the window shown (${budget.used - inline})`);
  assert.equal(budget.bytes(ITEM), 0);
  assert.equal(windowsIn(panel.root).length, 1);
  watcher.stop();
});

test("1: the outgoing part and its window stay charged only until the next part arrives", async () => {
  const budget = createBudget(10_000);
  const release = [];
  const pager = createPager({
    budget,
    key: "p",
    partBytes: 100,
    drawn: () => 30,
    cursorAt: (part) => part,
    read: ({ part }) => new Promise((resolve) => release.push(() => resolve({ data: "x".repeat(part === 0 ? 100 : 40), next: part === 0 ? 1 : null, count: 2 }))),
  });
  budget.request("p");
  const first = pager.open("first");
  assert.equal(budget.bytes("p"), 100, "the incoming part only, before it is read");
  release.shift()();
  await first;
  assert.equal(budget.bytes("p"), 130, "the part and its window");
  const next = pager.open("next");
  assert.equal(budget.bytes("p"), 130 + 100, "the outgoing part and its window, beside the incoming part");
  release.shift()();
  await next;
  assert.equal(budget.bytes("p"), 40 + 30, "only the new part and its window once it arrives");
});

test("1: a part read that is no longer the latest request is refused past the limit and lets go", async () => {
  const budget = createBudget(150);
  const release = [];
  const pager = createPager({ budget, key: "p", partBytes: 100, read: () => new Promise((resolve) => release.push(() => resolve({ data: "x".repeat(100), next: null }))) });
  budget.request("p");
  const done = pager.open("first");
  // Something else is asked for while the part is read: it is the floor now.
  budget.request("q");
  budget.reserve("q", 100, null);
  release.shift()();
  await done;
  assert.equal(pager.view.state, "evicted", "100 + 100 does not fit beside the new request");
  assert.equal(budget.has("p"), false);
  assert.ok(budget.used <= budget.limit);
});

// ---------- 2. The drawn window is reserved in shown bytes ----------

test("2: a short final part heavy in bidi markers draws right after its read, with no second Show (probe 5)", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(4 * 65536) + "\u202e".repeat(13_000));
  assert.equal(pages.length, 5);
  const { watcher, client, panel, inline } = await prepare(snapshot, 0);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  await watcher.loadPart("turn-b", "ia", "last");
  panel.update(watcher.view, officer);
  const part = watcher.view.full("turn-b", "ia");
  assert.equal(part.part, 1);
  assert.equal(part.state, "ready");
  assert.ok(part.size < WINDOW_BYTES, "a short part, raw");
  assert.equal(buttons(panel.root, "Show").length, 0, "the part asked for draws");
  assert.equal(windowsIn(panel.root).length, 1);
  const budget = watcher.view.budget;
  assert.equal(budget.bytes(ITEM), part.size + WINDOW_BYTES, "its window is reserved as shown: markers at 12 bytes, at most 64 KiB");
  assert.equal(budget.bytes(DRAW_KEY), 0);
  assert.equal(budget.used - inline, part.size + WINDOW_BYTES);
  watcher.stop();
});

test("2: a short plain part reserves only the window it shows", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(PART_MAX_BYTES + 1000));
  const { watcher, client, panel } = await prepare(snapshot, 0);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  await watcher.loadPart("turn-b", "ia", "last");
  panel.update(watcher.view, officer);
  const part = watcher.view.full("turn-b", "ia");
  assert.equal(part.part, 1);
  assert.ok(part.size < WINDOW_BYTES);
  assert.equal(watcher.view.budget.bytes(ITEM), 2 * part.size, "the part and its whole drawn copy, below one window");
  assert.equal(windowsIn(panel.root).length, 1);
  watcher.stop();
});

// ---------- 3. Live row keys ----------

test("3: a newer unread placeholder redraws its status and size (probe 2)", async () => {
  const snapshot = pagedSnapshot(["ia"]);
  const { watcher, panel } = await prepare(snapshot, 1024 * 1024);
  assert.match(panel.root.textContent, /2 pages/);
  watcher.event(activityEvent(10, { type: "item", item: { ...snapshot.turns[1].items[0], status: "failed", seq: 10, paged: { pages: 4, bytes: 200_000 } } }));
  panel.update(watcher.view, officer);
  assert.match(panel.root.textContent, /4 pages/);
  assert.doesNotMatch(panel.root.textContent, /2 pages/);
  assert.match(panel.root.textContent, /Failed/, "and its status");
  watcher.stop();
});

test("3: an item read whole offers Load the latest once its seq is raised, and its body is not rebuilt (probe 3)", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(70_000));
  const { watcher, client, panel } = await prepare(snapshot, 1024 * 1024);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.equal(buttons(panel.root, "Load the latest").length, 0);
  const body = windowsIn(panel.root)[0];
  assert.ok(body, "drawn in windows");
  watcher.event(activityEvent(10, { type: "item", item: { ...snapshot.turns[1].items[0], status: "failed", seq: 10 } }));
  panel.update(watcher.view, officer);
  assert.equal(buttons(panel.root, "Load the latest").length, 1);
  assert.equal(windowsIn(panel.root)[0], body, "the same body node: a text selection in it survives");
  watcher.stop();
});

test("3: a part's body stays mounted while its placeholder's status and size change", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const { watcher, client, panel } = await prepare(snapshot, 0);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  const body = windowsIn(panel.root)[0];
  const placeholder = watcher.view.turns.find((turn) => turn.id === "turn-b").items[0];
  watcher.event(activityEvent(10, { type: "item", item: { ...placeholder, status: "completed", seq: 10, paged: { pages: 9, bytes: 700 * 1024 } } }));
  panel.update(watcher.view, officer);
  assert.match(panel.root.textContent, /9 pages/);
  assert.equal(buttons(panel.root, "Load the latest").length, 1);
  assert.equal(windowsIn(panel.root)[0], body, "the part's window is the same node");
  watcher.stop();
});
