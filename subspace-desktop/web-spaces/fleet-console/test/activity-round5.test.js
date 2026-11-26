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
import { textItem, toolItem } from "./activity-fixtures.js";
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
