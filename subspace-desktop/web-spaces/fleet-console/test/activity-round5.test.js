// Round 5 of #115 (SUB decision 1c19f413): one owner for the view's budget,
// the drawn window reserved in shown bytes, live row keys, generation-scoped
// bodies and one lossless block model. Phobos's eight probes are here as
// regressions, each named for what it checks.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { createActivityWatcher, INLINE_KEY, PART_MAX_BYTES } from "../src/activity.js";
import { createActivityController } from "../src/activity-controller.js";
import { createActivityPanel, DRAW_KEY, formatItem, itemDocument, itemTree, mount, WINDOW_BYTES } from "../src/activity-view.js";
import { createBudget } from "../src/budget.js";
import { createPager } from "../src/pager.js";
import { activityEvent, stepItem, textItem, toolItem } from "./activity-fixtures.js";
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

// ---------- 4. Items read belong to their snapshot generation ----------

/** The snapshot `snapshot` as a new generation, its item at `seq`. */
function nextGeneration(snapshot, seq = 3) {
  const next = structuredClone(snapshot);
  next.cursor = { generation: "gen-new", seq: 4 };
  next.turns[1].items[0].seq = seq;
  return next;
}

test("4: a new stream generation drops the item read whole in the old one (probe 7)", async () => {
  const { pages, snapshot } = pagedWith("PREVIOUS FIELD " + "a".repeat(70_000));
  const { watcher, client, panel } = await prepare(snapshot, 1024 * 1024);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  assert.match(panel.root.textContent, /PREVIOUS FIELD/);
  client.activity = async () => nextGeneration(snapshot);
  watcher.retry();
  await settle();
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.cursor.generation, "gen-new");
  assert.equal(watcher.view.full("turn-b", "ia"), null, "seq 9 of the old generation is not newer than seq 3 of this one");
  assert.equal(watcher.view.budget.bytes(ITEM), 0, "and what it held is released");
  assert.doesNotMatch(panel.root.textContent, /PREVIOUS FIELD/);
  assert.equal(buttons(panel.root, "Load in full").length, 1);
  watcher.stop();
});

test("4: a whole read in flight when the generation changes is aborted, its waiter settles and its reply is never shown", async () => {
  const { pages, snapshot } = pagedWith("OLD " + "a".repeat(70_000));
  const { watcher, client } = await prepare(snapshot, 1024 * 1024);
  const held = [];
  const signals = [];
  client.activityItemPage = (agent, turn, id, page, signal) => {
    signals.push(signal);
    return new Promise((resolve) => held.push(() => resolve(pages[page])));
  };
  let done = false;
  const load = watcher.loadItem("turn-b", "ia").then(() => (done = true));
  await settle();
  assert.equal(held.length, 1, "the first page is asked for");
  client.activity = async () => nextGeneration(snapshot);
  watcher.retry();
  await settle();
  await settle();
  assert.equal(done, true, "the waiter settled with the old generation");
  assert.equal(signals[0].aborted, true, "the read was aborted");
  for (const release of held.splice(0)) release();
  await load;
  await settle();
  assert.equal(watcher.view.full("turn-b", "ia"), null, "the old reply is not shown");
  assert.equal(watcher.view.budget.bytes(ITEM), 0);
  watcher.stop();
});

test("4: a part read in flight when the generation changes is aborted and dropped", async () => {
  const { pages, snapshot } = pagedWith("a".repeat(600 * 1024));
  const { watcher, client } = await prepare(snapshot, 0);
  const held = [];
  const signals = [];
  client.activityItemPage = (agent, turn, id, page, signal) => {
    signals.push(signal);
    return new Promise((resolve) => held.push(() => resolve(pages[page])));
  };
  const load = watcher.loadItem("turn-b", "ia");
  await settle();
  assert.equal(watcher.view.full("turn-b", "ia").mode, "parts");
  client.activity = async () => nextGeneration(snapshot);
  watcher.retry();
  await settle();
  assert.equal(watcher.view.full("turn-b", "ia"), null);
  assert.equal(signals[0].aborted, true);
  for (const release of held.splice(0)) release();
  await load;
  await settle();
  assert.equal(watcher.view.full("turn-b", "ia"), null, "the old part never shows");
  assert.equal(watcher.view.budget.bytes(ITEM), 0);
  watcher.stop();
});

test("4: a snapshot of the same generation keeps what was read", async () => {
  const { pages, snapshot } = pagedWith("KEPT " + "a".repeat(70_000));
  const { watcher, client } = await prepare(snapshot, 1024 * 1024);
  client.activityItemPage = async (agent, turn, id, page) => pages[page];
  await watcher.loadItem("turn-b", "ia");
  const again = structuredClone(snapshot);
  again.cursor = { ...again.cursor, seq: again.cursor.seq + 1 };
  client.activity = async () => again;
  watcher.retry();
  await settle();
  assert.equal(watcher.view.full("turn-b", "ia")?.state, "ready");
  watcher.stop();
});

// ---------- 5. One lossless block model ----------

const annotated = (repeat) => toolItem("ia", 9, { content: [{ type: "text", text: "a".repeat(repeat), annotation: "fixture-side-data" }] });

test("5: a text output part's other fields show in the short view and in the first window (probe 4)", () => {
  for (const repeat of [100, 70_000]) {
    const shown = mount(itemTree("turn-b", annotated(repeat)), fakeDoc).textContent;
    assert.match(shown, /fixture-side-data/, `at ${repeat} characters`);
    assert.match(shown, /a{100}/, "beside the friendly text");
  }
  assert.equal(formatItem(annotated(70_000)).windows.length > 1, true, "the long one shows in windows");
});

test("5: the item's document, which the windows and the measure are made from, keeps them (probe 6)", () => {
  const document = itemDocument(annotated(70_000));
  assert.match(document, /content\[0\] \(other fields\)\n\{\n {2}"annotation": "fixture-side-data"\n\}\n\ncontent\[0\] \(text\)\na/);
  assert.equal(formatItem(annotated(70_000)).windows.join(""), document, "the windows join into the document");
});

test("5: fields a friendly view leaves out of an item, its error or a file part still show", () => {
  const tool = toolItem("t", 1, {
    content: [{ type: "file", name: "f.txt", mime: "text/plain", uri: "file:///f.txt", size: 12 }],
    error: { type: "Timeout", message: "late", code: "E-SIDE" },
    origin: "TOOL-SIDE",
  });
  const shown = mount(itemTree("turn-a", tool), fakeDoc).textContent;
  for (const text of ['"size": 12', "f.txt", "file:///f.txt", "E-SIDE", "TOOL-SIDE"]) assert.ok(shown.includes(text), text);
  const reply = mount(itemTree("turn-a", textItem("r", 2, "hello", { citations: ["REPLY-SIDE"] })), fakeDoc).textContent;
  assert.ok(reply.includes("REPLY-SIDE"));
  const step = mount(itemTree("turn-a", stepItem("s", 3, { cache: "STEP-SIDE" })), fakeDoc).textContent;
  assert.ok(step.includes("STEP-SIDE") && step.includes("firstTokenAt"));
  const plain = mount(itemTree("turn-a", toolItem("p", 4, { content: [{ type: "text", text: "only" }] })), fakeDoc).textContent;
  assert.ok(!plain.includes("Other fields"), "nothing extra when nothing is left out");
});

// ---------- 6. The activity controller ----------

/** A controller over fakes: its frames run when `runFrames()` is called. */
function controllerOver({ token = true } = {}) {
  const snapshot = pagedSnapshot(["ia"]);
  const client = fakeActivityClient([snapshot, snapshot, snapshot]);
  const inspector = fakeDoc.createElement("aside");
  inspector.querySelector = () => null;
  const frames = new Map();
  let nextFrame = 0;
  const log = { subscribed: [], stopped: 0, toasts: [], closed: [], drawn: 0 };
  const hooks = [];
  const controller = createActivityController({
    inspector,
    doc: fakeDoc,
    watch: (agent, hook) => {
      log.subscribed.push(agent);
      hooks.push(hook);
      return token ? { client, stop: () => (log.stopped += 1) } : null;
    },
    live: (subscribed) => subscribed,
    toast: (words) => log.toasts.push(words),
    announce: () => {},
    onDrawn: () => (log.drawn += 1),
    onClosed: (agent, restore) => log.closed.push([agent, restore]),
    frame: (draw) => {
      nextFrame += 1;
      frames.set(nextFrame, draw);
      return nextFrame;
    },
    cancelFrame: (id) => frames.delete(id),
  });
  const runFrames = () => {
    const due = [...frames.values()];
    frames.clear();
    for (const draw of due) draw();
  };
  return { controller, inspector, client, frames, runFrames, log, hooks };
}

test("6: the controller opens the view in the inspector, draws on change in one frame and tears it all down on close", async () => {
  const { controller, inspector, frames, runFrames, log, hooks } = controllerOver();
  assert.equal(controller.open("example-captain", "Example", "captain"), true);
  assert.equal(controller.isOpen, true);
  assert.equal(controller.agent, "example-captain");
  assert.equal(inspector.children.length, 1, "the panel is in the inspector");
  await settle();
  assert.ok(frames.size <= 1, "changes share one frame");
  runFrames();
  assert.match(inspector.textContent, /Example/);
  const drawn = log.drawn;
  controller.close(true);
  assert.equal(controller.isOpen, false);
  assert.equal(log.stopped, 1, "the stream is given back its plain form");
  assert.deepEqual(log.closed, [["example-captain", true]]);
  assert.equal(frames.size, 0, "the frame asked for is cancelled");
  // Late callbacks of the closed view do nothing.
  hooks[0].onEvent({ agentId: "example-captain", generation: "g", seq: 99, change: { type: "turn" } });
  hooks[0].onEnded();
  runFrames();
  assert.equal(log.drawn, drawn, "nothing draws after close");
  assert.equal(log.closed.length, 1, "and it closes once");
  assert.equal(log.toasts.length, 0);
});

test("6: opening another officer closes the first, whose callbacks then do nothing", async () => {
  const { controller, log, hooks } = controllerOver();
  controller.open("example-captain", "Example", "captain");
  controller.open("example-admiral", "Admiral", "rear_admiral");
  assert.deepEqual(log.subscribed, ["example-captain", "example-admiral"]);
  assert.deepEqual(log.closed, [["example-captain", false]]);
  assert.equal(log.stopped, 1);
  hooks[0].onEnded();
  assert.equal(controller.agent, "example-admiral", "the first view's end does not close the second");
  controller.close(false);
  assert.equal(log.stopped, 2);
});

test("6: the fleet's stream ending closes the view and says why; with no token it never opens", () => {
  const open = controllerOver();
  open.controller.open("example-captain", "Example", "captain");
  open.hooks[0].onEnded();
  assert.equal(open.controller.isOpen, false);
  assert.deepEqual(open.log.closed, [["example-captain", true]]);
  assert.equal(open.log.stopped, 0, "the ended subscription is not stopped again");
  assert.match(open.log.toasts[0], /address or token changed/);
  const refused = controllerOver({ token: false });
  assert.equal(refused.controller.open("example-captain", "Example", "captain"), false);
  assert.equal(refused.controller.isOpen, false);
  assert.match(refused.log.toasts[0], /needs the fleet's token/);
  assert.equal(refused.inspector.children.length, 0);
});

test("6: main.js keeps selection, focus and shell wiring, under 1,000 lines, and leaves the activity lifecycle to the controller", () => {
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const lines = main.split("\n").length - (main.endsWith("\n") ? 1 : 0);
  assert.ok(lines < 1000, `main.js has ${lines} lines`);
  for (const gone of ["createActivityWatcher", "createActivityPanel", "requestAnimationFrame(drawActivity", "app.activity"]) assert.ok(!main.includes(gone), gone);
  assert.ok(main.includes("createActivityController"));
});
