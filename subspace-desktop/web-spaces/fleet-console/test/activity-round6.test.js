// Round 6 of #115 (SUB decision 533c2b29): Phobos's six round-5 findings,
// each a regression named for what it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { INLINE_KEY, PART_MAX_BYTES } from "../src/activity.js";
import { formatItem, itemDocument, itemTree, json, mount } from "../src/activity-view.js";
import { createBudget } from "../src/budget.js";
import { partBlocks, restOf, shownJsonPieces } from "../src/document.js";
import { createPager } from "../src/pager.js";
import { activityEvent, stepItem, textItem, tokens, toolItem } from "./activity-fixtures.js";
import { buttons, clickOn, fakeDoc, officer, pagedWith, pagesOf, prepare, settle, windowsIn } from "./activity-helpers.js";

// ---------- 2. A whole read is rechecked when the inline charge grows ----------

test("2: an item read whole that no longer fits beside grown inline content reads in parts from where its window shows, the window kept until the part arrives", async () => {
  const { item, pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const whole = pages[0].bytes * 2;
  const { watcher, client, panel, inline } = await prepare(snapshot, whole + 20_000);
  const budget = watcher.view.budget;
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full("turn-b", "ia").mode, "whole", "it fits whole at first");
  assert.equal(budget.requested, "turn-b\u0000ia", "and is the latest request");
  clickOn(panel, panel.root.findAll((node) => node.attributes["data-activity-window"] === "last")[0]);
  const window = windowsIn(panel.root)[0];
  assert.ok(window, "its last window is drawn");
  // Live content grows the pinned inline charge past what the whole leaves.
  const held = [];
  client.activityItemPage = (agent, turn, id, page) => new Promise((resolve) => held.push({ page, resolve }));
  watcher.event(activityEvent(10, { type: "item", item: textItem("grown", 10, "b".repeat(100 * 1024)) }));
  await settle();
  const steady = budget.limit - budget.bytes(INLINE_KEY);
  assert.ok(budget.bytes(INLINE_KEY) > inline, "the inline charge grew");
  const switching = watcher.view.full("turn-b", "ia");
  assert.equal(switching.mode, "parts", "the latest request does not keep a whole read past the floor");
  assert.equal(switching.replacing, true);
  assert.ok(budget.bytes("turn-b\u0000ia") <= steady, `within the steady allowance (${budget.bytes("turn-b\u0000ia")} of ${steady})`);
  const count = Math.ceil(pages.length / 4);
  assert.deepEqual(held.map((each) => each.page), [(count - 1) * 4], "the part where its window shows, the last");
  panel.update(watcher.view, officer);
  assert.equal(windowsIn(panel.root)[0], window, "the window shown stays until the part arrives");
  assert.match(panel.root.textContent, /the window below stays on show until it arrives/);
  for (let page = (count - 1) * 4; held.length; page += 1) {
    held.shift().resolve(pages[page]);
    await settle();
  }
  panel.update(watcher.view, officer);
  const part = watcher.view.full("turn-b", "ia");
  assert.equal(part.state, "ready");
  assert.equal(part.part, count - 1);
  assert.equal(part.replacing, false);
  assert.notEqual(windowsIn(panel.root)[0], window, "the part's window replaced it");
  assert.match(panel.root.textContent, new RegExp(`Part ${count} of ${count}`));
  assert.ok(budget.bytes("turn-b\u0000ia") <= steady);
  watcher.stop();
});

test("2: inline content that grows while an item is read whole sends the read to parts, though it is the latest request", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const { watcher, client } = await prepare(snapshot, pages[0].bytes * 2 + 20_000);
  const budget = watcher.view.budget;
  const held = [];
  client.activityItemPage = (agent, turn, id, page) => new Promise((resolve) => held.push({ page, resolve }));
  const load = watcher.loadItem("turn-b", "ia");
  await settle();
  held.shift().resolve(pages[0]);
  await settle();
  watcher.event(activityEvent(10, { type: "item", item: textItem("grown", 10, "b".repeat(100 * 1024)) }));
  while (held.length) {
    const { page, resolve } = held.shift();
    resolve(pages[page]);
    await settle();
  }
  await load;
  await settle();
  assert.equal(budget.requested, "turn-b\u0000ia");
  assert.equal(watcher.view.full("turn-b", "ia").mode, "parts", "never held whole past the floor");
  assert.ok(budget.bytes("turn-b\u0000ia") <= budget.limit - budget.bytes(INLINE_KEY), "within the steady allowance");
  watcher.stop();
});

// ---------- 3. Load the latest keeps the part on show until its replacement arrives ----------

test("3: a pager's restart keeps the part on show and charged beside the read, swaps on arrival and keeps it on failure", async () => {
  const budget = createBudget(10_000);
  const replies = [];
  const asked = [];
  const pager = createPager({
    budget,
    key: "p",
    partBytes: 100,
    drawn: () => 30,
    cursorAt: (part) => part,
    read: (request) => {
      asked.push({ part: request.part, version: request.version });
      return new Promise((resolve, reject) => replies.push({ resolve, reject }));
    },
  });
  budget.request("p");
  const first = pager.open("first");
  replies.shift().resolve({ data: "old-0", next: 1, version: 1, count: 2 });
  await first;
  const shown = budget.bytes("p");
  let reload = pager.restart();
  assert.deepEqual(asked.at(-1), { part: 0, version: null }, "the newest version, unpinned, even with part 0 on show");
  assert.equal(pager.view.data, "old-0", "the outgoing part stays on show");
  assert.equal(pager.view.reloading, true);
  assert.equal(budget.bytes("p"), shown + 100, "the outgoing part and its window stay charged beside the incoming part");
  replies.shift().reject(new Error("gateway away"));
  await reload;
  assert.equal(pager.view.state, "failed");
  assert.equal(pager.view.data, "old-0", "a failed read keeps the outgoing part");
  assert.equal(pager.view.version, 1);
  assert.equal(budget.bytes("p"), shown, "charged as it was");
  reload = pager.restart();
  replies.shift().resolve({ data: "new-0", next: null, version: 2, count: 1 });
  await reload;
  assert.equal(pager.view.data, "new-0", "swapped once the new part arrived");
  assert.equal(pager.view.version, 2);
  assert.equal(pager.view.count, 1);
  assert.equal(pager.view.newer, false);
  assert.equal(budget.bytes("p"), 5 + 30, "only the new part and its window");
});

test("3: Load the latest on an item read in parts keeps the part and its window on show until the first new page arrives; a failure keeps them with Try again", async () => {
  const { item, pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const { watcher, client, panel } = await prepare(snapshot, 0);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  const before = watcher.view.full("turn-b", "ia");
  const window = windowsIn(panel.root)[0];
  assert.ok(window, "the first part's window is drawn");
  const charged = watcher.view.budget.bytes("turn-b\u0000ia");
  // The item changes on the gateway; its first new page is held.
  const newer = { ...item, seq: 10, content: [{ type: "text", text: "b".repeat(600 * 1024) }] };
  const newPages = pagesOf(newer);
  watcher.event(activityEvent(10, { type: "item", item: { ...placeholderOf(watcher), seq: 10, paged: { pages: newPages.length, bytes: newPages[0].bytes } } }));
  panel.update(watcher.view, officer);
  const held = [];
  client.activityItemPage = (agent, turn, id, page) => new Promise((resolve, reject) => held.push({ page, resolve, reject }));
  assert.equal(buttons(panel.root, "Load the latest").length, 1);
  let reload = watcher.loadItem("turn-b", "ia");
  await settle();
  panel.update(watcher.view, officer);
  const waiting = watcher.view.full("turn-b", "ia");
  assert.equal(waiting.state, "loading");
  assert.equal(waiting.data, before.data, "the outgoing part stays");
  assert.equal(windowsIn(panel.root)[0], window, "and its window stays mounted");
  assert.ok(watcher.view.budget.bytes("turn-b\u0000ia") >= charged + PART_MAX_BYTES, "both stay charged while the read waits");
  assert.match(panel.root.textContent, /Reading the latest version/);
  held.shift().reject(new Error("gateway away"));
  await reload;
  panel.update(watcher.view, officer);
  const failed = watcher.view.full("turn-b", "ia");
  assert.equal(failed.state, "failed");
  assert.equal(failed.data, before.data, "a failed read keeps the outgoing part");
  assert.equal(windowsIn(panel.root)[0], window);
  assert.match(panel.root.textContent, /gateway away/);
  assert.equal(watcher.view.budget.bytes("turn-b\u0000ia"), charged, "charged as before");
  client.activityItemPage = async (agent, turn, id, page) => newPages[page];
  assert.equal(buttons(panel.root, "Try again").length, 1);
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  const after = watcher.view.full("turn-b", "ia");
  assert.equal(after.state, "ready");
  assert.equal(after.seq, 10, "Try again reads the latest version");
  assert.ok(after.data.includes("bbbb"), "swapped to the new part");
  watcher.stop();
});

// ---------- 4. A retained whole body keeps the live status and size ----------

/** The running turn's paged placeholder in `watcher`'s view. */
const placeholderOf = (watcher) => watcher.view.turns.find((turn) => turn.id === "turn-b").items[0];

for (const [what, repeat] of [
  ["in windows", 70_000],
  ["field by field", 100],
]) {
  test(`4: an item read whole (${what}) shows its live status and size above the earlier version, and keeps its body node`, async () => {
    const { pages, snapshot } = pagedWith("a".repeat(repeat));
    const { watcher, client, panel } = await prepare(snapshot, 1024 * 1024);
    client.activityItemPage = async (agent, turn, id, page) => pages[page];
    await watcher.loadItem("turn-b", "ia");
    panel.update(watcher.view, officer);
    const bodyNode = () => (repeat > 1000 ? windowsIn(panel.root)[0] : panel.root.findAll((node) => node.attributes.class === "act-pre")[0]);
    const body = bodyNode();
    assert.ok(body, "the body is drawn");
    assert.match(panel.root.textContent, /Read in full \(/);
    watcher.event(activityEvent(10, { type: "item", item: { ...placeholderOf(watcher), status: "failed", seq: 10, paged: { pages: 4, bytes: 200_000 } } }));
    panel.update(watcher.view, officer);
    const shown = panel.root.textContent;
    assert.match(shown, /Failed/, "the live status");
    assert.match(shown, /4 pages/, "the live size");
    assert.match(shown, /Below is the earlier version, as read/, "the body is labelled as the earlier version");
    assert.equal(buttons(panel.root, "Load the latest").length, 1);
    assert.equal(bodyNode(), body, "the same body node: a text selection in it survives");
    watcher.stop();
  });
}

// ---------- 5. The short step view leaves nothing out ----------

test("5: a step's model and tokens members the short view does not put in words still show, as the document has them", () => {
  const step = stepItem("s", 3, { model: { provider: "example", id: "model-large", variant: null, region: "MODEL-SIDE" }, tokens: { ...tokens(1200, 80), detail: { cached: "TOKENS-SIDE" } } });
  const shown = mount(itemTree("turn-a", step), fakeDoc).textContent;
  assert.match(shown, /example\/model-large/, "the model in words");
  assert.match(shown, /model \(other fields\)regionMODEL-SIDE/, "the model's other member");
  assert.match(shown, /tokens \(other fields\)detail\{\n {2}"cached": "TOKENS-SIDE"\n\}/, "the tokens' other member, as the document's JSON");
  const document = itemDocument(step);
  for (const text of ["MODEL-SIDE", "TOKENS-SIDE"]) assert.ok(document.includes(text), `the document, which is measured, holds ${text}`);
  const plain = mount(itemTree("turn-a", stepItem("p", 4)), fakeDoc).textContent;
  assert.ok(!plain.includes("other fields"), "nothing extra when every member is in words");
});

test("5: a step's model or tokens that is not a record shows whole", () => {
  const shown = mount(itemTree("turn-a", stepItem("s", 3, { model: "MODEL-AS-TEXT", tokens: [7, 8] })), fakeDoc).textContent;
  assert.match(shown, /Model not reported/);
  assert.match(shown, /modelMODEL-AS-TEXT/);
  assert.match(shown, /tokens\[\n {2}7,\n {2}8\n\]/);
});

test("5: the short view's JSON is the document's formatter", () => {
  const value = { outer: { "k\u202Eey": ["a\u0000b", "\ud800"], typed: "\\u0000", gone: undefined, nested: [undefined, 1.5] } };
  assert.equal(json(value), [...shownJsonPieces(value)].join(""));
});

// ---------- 6. Every own field survives the projection ----------

/** A tool call whose text part, parsed from JSON, has an own `__proto__` field. */
const withProto = (repeat) => JSON.parse(JSON.stringify(toolItem("ia", 9)).replace('"content":[{"type":"text","text":"README.md\\nsrc\\n"}]', `"content":[{"type":"text","text":"${"a".repeat(repeat)}","__proto__":"PROTO-SIDE"}],"__proto__":{"item":"ITEM-SIDE"}`));

test("6: restOf and partBlocks keep an own __proto__ field as a field", () => {
  const part = withProto(4).content[0];
  assert.ok(Object.hasOwn(part, "__proto__"), "the fixture has the own field");
  const rest = restOf(part, ["type", "text"]);
  assert.ok(rest && Object.hasOwn(rest, "__proto__"));
  assert.equal(rest.__proto__, "PROTO-SIDE");
  const [other] = partBlocks(part, 0);
  assert.equal(JSON.stringify(other.json), '{"__proto__":"PROTO-SIDE"}');
});

test("6: an own __proto__ field shows in the short view and in the windows", () => {
  const short = mount(itemTree("turn-b", withProto(100)), fakeDoc).textContent;
  assert.match(short, /"__proto__": "PROTO-SIDE"/, "the part's field in the short view");
  assert.match(short, /ITEM-SIDE/, "the item's field in the short view");
  const long = withProto(70_000);
  const { windows } = formatItem(long);
  assert.ok(windows.length > 1, "the long one shows in windows");
  const joined = windows.join("");
  assert.equal(joined, itemDocument(long));
  assert.match(joined, /content\[0\] \(other fields\)\n\{\n {2}"__proto__": "PROTO-SIDE"\n\}/, "the part's field in the windows");
  assert.match(joined, /__proto__\n\{\n {2}"item": "ITEM-SIDE"\n\}/, "the item's field in the windows");
});
