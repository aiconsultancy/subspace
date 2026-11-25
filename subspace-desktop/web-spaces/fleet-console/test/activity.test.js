import assert from "node:assert/strict";
import { test } from "node:test";
import { createApiClient, HttpError, readPagedItem } from "../src/api.js";
import { ACTIVITY_MESSAGES, applyChange, createActivityWatcher, decodeActivity, decodeActivityEvent } from "../src/activity.js";
import { createActivityPanel, headerTree, itemTree, mount, visibleSegments, visibleText } from "../src/activity-view.js";
import { createLiveSource } from "../src/live.js";
import { createFleetManager, createSampleFleetManager } from "../src/fleet-manager.js";
import { createSampleActivitySource } from "../src/sample-source.js";
import { createSampleActivitySim } from "../src/sample.js";
import { activityEvent, activitySnapshot, GENERATION, stepItem, T0, textItem, toolItem, turn } from "./activity-fixtures.js";
import { sampleFleet } from "./fixtures.js";

const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

// ---------- a small DOM, enough for mount() and the panel ----------

class FakeText {
  constructor(text) {
    this.nodeType = 3;
    this.data = text;
    this.parent = null;
  }
  get textContent() {
    return this.data;
  }
  remove() {
    this.parent?.removeChild(this);
  }
}

class FakeElement {
  constructor(tag) {
    this.nodeType = 1;
    this.tag = tag;
    this.attributes = {};
    this.childNodes = [];
    this.parent = null;
    this.listeners = {};
  }
  set innerHTML(_) {
    throw new Error("innerHTML is never used by the activity view");
  }
  get children() {
    return this.childNodes.filter((node) => node.nodeType === 1);
  }
  get dataset() {
    const out = {};
    for (const [name, value] of Object.entries(this.attributes)) if (name.startsWith("data-")) out[name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
    return out;
  }
  get textContent() {
    return this.childNodes.map((node) => node.textContent).join("");
  }
  setAttribute(name, value) {
    this.attributes[name] = String(value);
  }
  getAttribute(name) {
    return this.attributes[name] ?? null;
  }
  appendChild(node) {
    node.parent?.removeChild(node);
    node.parent = this;
    this.childNodes.push(node);
    return node;
  }
  insertBefore(node, before) {
    if (!before) return this.appendChild(node);
    node.parent?.removeChild(node);
    node.parent = this;
    this.childNodes.splice(this.childNodes.indexOf(before), 0, node);
    return node;
  }
  removeChild(node) {
    this.childNodes = this.childNodes.filter((child) => child !== node);
    node.parent = null;
  }
  remove() {
    this.parent?.removeChild(this);
  }
  replaceChildren(...nodes) {
    for (const node of this.childNodes) node.parent = null;
    this.childNodes = [];
    for (const node of nodes) this.appendChild(node);
  }
  replaceWith(node) {
    const parent = this.parent;
    parent.insertBefore(node, this);
    parent.removeChild(this);
  }
  addEventListener(type, fn) {
    this.listeners[type] = fn;
  }
  closest(selector) {
    const name = selector.slice(1, -1);
    for (let node = this; node; node = node.parent) if (node.nodeType === 1 && name in node.attributes) return node;
    return null;
  }
  findAll(predicate, out = []) {
    for (const node of this.childNodes) {
      if (node.nodeType !== 1) continue;
      if (predicate(node)) out.push(node);
      node.findAll(predicate, out);
    }
    return out;
  }
}

const fakeDoc = { createElement: (tag) => new FakeElement(tag), createTextNode: (text) => new FakeText(text) };

// ---------- decoding ----------

test("activity: decodes a #355-shaped snapshot and drops a malformed item alone", () => {
  const json = activitySnapshot();
  json.turns[0].items.push({ id: "bad" });
  const decoded = decodeActivity(json);
  assert.deepEqual(decoded.agent, { id: "example-captain", role: "captain", backend: "opencode", collected: true });
  assert.deepEqual(decoded.cursor, { generation: GENERATION, seq: 9 });
  assert.deepEqual(decoded.turns.map((t) => t.id), ["turn-a", "turn-b"]);
  assert.deepEqual(decoded.turns[0].items.map((i) => i.id), ["i1", "i2", "i3"], "the malformed item is dropped");
  assert.equal(decoded.turns[1].items[1].paged.pages, 2);
  assert.throws(() => decodeActivity({ agent: {}, turns: [] }));
  assert.equal(decodeActivity(activitySnapshot({ collected: false })).agent.collected, false);
  assert.equal(decodeActivityEvent({ agentId: "a" }), null);
  assert.equal(decodeActivityEvent(activityEvent(10, { type: "item", item: toolItem("i3", 10) })).seq, 10);
});

test("activity: turn events upsert the turn and keep its items; item events keep the higher seq", () => {
  const { turns } = decodeActivity(activitySnapshot());
  const ended = applyChange(turns, activityEvent(10, { type: "turn-ended", turn: turn("turn-b", 10, { items: undefined }) }));
  assert.equal(ended[1].status, "completed");
  assert.deepEqual(ended[1].items.map((i) => i.id), ["i1", "i2"], "a turn event keeps the items held");
  const stale = applyChange(turns, activityEvent(11, { type: "item", item: stepItem("i1", 5) }));
  assert.equal(stale[1].items[0].seq, 7, "an older version of an item does not replace a newer one");
  const added = applyChange(turns, activityEvent(12, { type: "turn-started", turn: turn("turn-c", 12, { status: "running", items: undefined }) }, { turnId: "turn-c" }));
  assert.deepEqual(added.map((t) => t.id), ["turn-a", "turn-b", "turn-c"]);
  assert.deepEqual(added[2].items, []);
  assert.equal(applyChange(turns, activityEvent(13, { type: "item", item: textItem("i1", 13, "x") }, { turnId: "turn-gone" })), null, "an item for an unknown turn asks for a snapshot");
  assert.equal(applyChange(turns, activityEvent(14, { type: "something-new" })), turns, "an unknown change type is ignored");
});

// ---------- the API client ----------

function recordingClient(answer) {
  const calls = [];
  const client = createApiClient({
    endpoint: "https://fleet.example.com",
    token: "t",
    fetch: async (url, init) => {
      calls.push({ url, init });
      return answer(url);
    },
  });
  return { client, calls };
}

const json = (status, body) => ({ ok: status >= 200 && status < 300, status, headers: { get: () => null }, json: async () => body });

test("api: activity routes, and events() sends each agent as a repeated query parameter", async () => {
  const { client, calls } = recordingClient(() => json(200, {}));
  await client.activity("example captain");
  await client.activity("example-captain", { turn: "turn-b" });
  await client.activityItemPage("example-captain", "turn-b", "i2", 0);
  assert.deepEqual(
    calls.map((call) => call.url),
    [
      "https://fleet.example.com/api/v1/agents/example%20captain/activity",
      "https://fleet.example.com/api/v1/agents/example-captain/activity?turn=turn-b",
      "https://fleet.example.com/api/v1/agents/example-captain/activity?turn=turn-b&item=i2&page=0",
    ],
  );
  const stream = recordingClient(() => ({ ok: true, status: 200, headers: { get: () => "text/event-stream" }, body: new ReadableStream() }));
  await stream.client.events(undefined, "", ["example-admiral", "example captain"]);
  await stream.client.events(undefined, "");
  assert.equal(stream.calls[0].url, "https://fleet.example.com/api/v1/events?agent=example-admiral&agent=example+captain");
  assert.deepEqual(new URL(stream.calls[0].url).searchParams.getAll("agent"), ["example-admiral", "example captain"]);
  assert.equal(stream.calls[1].url, "https://fleet.example.com/api/v1/events");
});

test("api: paged items join in full, and a seq change between pages starts again from page 0", async () => {
  const v1 = JSON.stringify(toolItem("i2", 9, { content: [{ type: "text", text: "a".repeat(100) }] }));
  const v2 = JSON.stringify(toolItem("i2", 11, { content: [{ type: "text", text: "b".repeat(100) }] }));
  const pages = (text, seq) => [text.slice(0, 60), text.slice(60, 120), text.slice(120)].map((data, page) => ({ seq, page, pages: 3, bytes: text.length, data }));
  const reads = [];
  let version = 1;
  const item = await readPagedItem(async (page) => {
    reads.push(page);
    const answer = (version === 1 ? pages(v1, 9) : pages(v2, 11))[page];
    if (page === 1 && version === 1) version = 2; // the item changes after page 1 is read
    return answer;
  });
  assert.deepEqual(reads, [0, 1, 2, 0, 1, 2]);
  assert.equal(item.seq, 11);
  assert.deepEqual(item.item, JSON.parse(v2), "the whole item, not cut");
  assert.equal(item.item.content[0].text.length, 100);
  await assert.rejects(readPagedItem(async (page) => ({ seq: page, page, pages: 2, bytes: 1, data: "x" }), 2), /kept changing/);
});

test("api: joined pages must have the item's UTF-8 byte length, or the read starts again", async () => {
  const text = JSON.stringify(textItem("i3", 9, "héllo ✓ wörld"));
  const bytes = new TextEncoder().encode(text).length;
  assert.notEqual(bytes, text.length, "the fixture has multi-byte characters");
  let attempt = 0;
  const reads = [];
  const item = await readPagedItem(async (page) => {
    reads.push(page);
    if (page === 0) attempt += 1;
    // The first attempt's second page comes back short.
    const second = attempt === 1 ? text.slice(20, -5) : text.slice(20);
    return { seq: 9, page, pages: 2, bytes, data: page === 0 ? text.slice(0, 20) : second };
  });
  assert.deepEqual(reads, [0, 1, 0, 1]);
  assert.equal(item.item.text, "héllo ✓ wörld");
  // Always short: give up rather than parse a smaller item.
  const short = '{"id":"i3"}';
  await assert.rejects(readPagedItem(async (page) => ({ seq: 9, page, pages: 1, bytes: short.length + 40, data: short }), 2), /kept changing/);
});

// ---------- the watcher: snapshot and stream joined ----------

function fakeActivityClient(snapshots) {
  const calls = [];
  const pending = [];
  return {
    calls,
    pending,
    activity(agent, query, signal) {
      calls.push({ agent, query });
      const next = snapshots.shift();
      if (next === "hold") return new Promise((resolve) => pending.push(resolve));
      return next instanceof Error ? Promise.reject(next) : Promise.resolve(structuredClone(next));
    },
    activityItemPage: async () => {
      throw new HttpError(404);
    },
  };
}

test("watcher: events buffered during the snapshot join with no gap and no duplicate", async () => {
  const client = fakeActivityClient(["hold"]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  // Arrive before the snapshot answers: one already in it, two newer (seq skips are fine).
  watcher.event(activityEvent(8, { type: "item", item: stepItem("i1", 8, { status: "running", tokens: null }) }));
  watcher.event(activityEvent(10, { type: "item", item: textItem("i3", 10, "Hel") }));
  watcher.event(activityEvent(13, { type: "item", item: textItem("i3", 13, "Hello") }));
  watcher.event(activityEvent(99, { type: "item", item: textItem("x", 99, "other officer") }, { agentId: "example-admiral" }));
  assert.equal(watcher.view.status, "loading");
  client.pending[0](activitySnapshot({ seq: 9 }));
  await settle();
  let view = watcher.view;
  assert.equal(view.status, "ready");
  assert.deepEqual(view.turns.map((t) => t.id), ["turn-b", "turn-a"], "newest first");
  const running = view.turns[0];
  assert.deepEqual(running.items.map((i) => i.id), ["i1", "i2", "i3"], "no duplicate item");
  assert.equal(running.items[0].seq, 7, "the event at seq 8 was already in the snapshot and is dropped");
  assert.equal(running.items[2].text, "Hello");
  // A later event, skipping seq values, is applied without a refetch.
  watcher.event(activityEvent(20, { type: "turn-ended", turn: turn("turn-b", 20, { items: undefined }) }));
  view = watcher.view;
  assert.equal(view.turns[0].status, "completed");
  assert.equal(view.turns[0].items.length, 3);
  assert.equal(client.calls.length, 1);
  watcher.stop();
});

test("watcher: a seq that fails to rise, a generation change and a re-opened stream each read the snapshot again", async () => {
  const client = fakeActivityClient([activitySnapshot({ seq: 9 }), activitySnapshot({ seq: 30 }), activitySnapshot({ seq: 40, generation: "gen-example-2" }), activitySnapshot({ seq: 41, generation: "gen-example-2" })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  watcher.event(activityEvent(15, { type: "item", item: textItem("i3", 15, "a") }));
  assert.equal(client.calls.length, 1);
  watcher.event(activityEvent(12, { type: "item", item: textItem("i3", 12, "b") }));
  await settle();
  assert.equal(client.calls.length, 2, "seq fell: refetch");
  assert.equal(watcher.view.cursor.seq, 30);
  watcher.event(activityEvent(31, { type: "item", item: textItem("i3", 31, "c") }, { generation: "gen-example-2" }));
  await settle();
  assert.equal(client.calls.length, 3, "generation changed: refetch");
  assert.equal(watcher.view.cursor.generation, "gen-example-2");
  assert.ok(!watcher.view.turns[0].items.some((i) => i.id === "i3"), "the event that triggered it is not applied to the old snapshot");
  watcher.reopened();
  await settle();
  assert.equal(client.calls.length, 4, "the stream re-opened: refetch");
  assert.equal(watcher.view.cursor.seq, 41);
  watcher.stop();
});

test("watcher: a failed refetch while ready goes stale, applies no events, and Try again brings it back in step", async () => {
  const client = fakeActivityClient([activitySnapshot({ seq: 9 }), new Error("network down"), "hold"]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  assert.equal(watcher.view.stale, false);
  // A new generation asks for a snapshot, and that read fails.
  watcher.event(activityEvent(20, { type: "item", item: textItem("i3", 20, "lost") }, { generation: "gen-example-2" }));
  await settle();
  assert.equal(client.calls.length, 2);
  let view = watcher.view;
  assert.equal(view.status, "ready");
  assert.equal(view.stale, true);
  assert.equal(view.error, "network down");
  assert.equal(view.turns[0].items.length, 2, "the last snapshot stays on show");
  // A later event in the old generation is not applied on top of it.
  watcher.event(activityEvent(21, { type: "item", item: textItem("i4", 21, "not applied") }));
  assert.ok(!watcher.view.turns[0].items.some((item) => item.id === "i4"));
  assert.equal(client.calls.length, 2, "no refetch storm while stale");
  const header = mount(headerTree(watcher.view, { name: "Example", role: "captain", live: true }), fakeDoc);
  assert.ok(header.findAll((node) => "data-activity-retry" in node.attributes).length === 1, "Try again is offered");
  // Try again: events during the read are buffered and joined after it.
  watcher.retry();
  assert.equal(client.calls.length, 3);
  watcher.event(activityEvent(42, { type: "item", item: textItem("i3", 42, "after") }, { generation: "gen-example-2" }));
  client.pending[0](activitySnapshot({ seq: 41, generation: "gen-example-2" }));
  await settle();
  view = watcher.view;
  assert.equal(view.stale, false);
  assert.equal(view.error, null);
  assert.equal(view.cursor.generation, "gen-example-2");
  assert.equal(view.turns[0].items.find((item) => item.id === "i3")?.text, "after", "back in step");
  watcher.event(activityEvent(43, { type: "item", item: textItem("i4", 43, "applied") }, { generation: "gen-example-2" }));
  assert.ok(watcher.view.turns[0].items.some((item) => item.id === "i4"));
  watcher.stop();
});

test("watcher: refetches are single-flight", async () => {
  const client = fakeActivityClient(["hold", activitySnapshot({ seq: 50 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  watcher.reopened();
  watcher.reopened();
  watcher.reopened();
  assert.equal(client.calls.length, 1);
  client.pending[0](activitySnapshot({ seq: 9 }));
  await settle();
  assert.equal(client.calls.length, 2, "one more read covers every request made meanwhile");
  assert.equal(watcher.view.cursor.seq, 50);
  watcher.stop();
});

test("watcher: 404 reads as unavailable, collected false as not collected", async () => {
  const gone = createActivityWatcher({ client: fakeActivityClient([new HttpError(404)]), agent: "example-captain", awaitStream: false });
  gone.start();
  await settle();
  assert.equal(gone.view.status, "unavailable");
  const header = mount(headerTree(gone.view, { name: "Example", role: "captain", live: true }), fakeDoc);
  assert.match(header.textContent, new RegExp(ACTIVITY_MESSAGES.unavailable.replace(/[.']/g, "\\$&")));
  assert.equal(ACTIVITY_MESSAGES.unavailable, "Activity isn't available on this gateway.");

  const other = createActivityWatcher({ client: fakeActivityClient([activitySnapshot({ collected: false })]), agent: "example-captain", awaitStream: false });
  other.start();
  await settle();
  assert.equal(other.view.agent.collected, false);
  assert.ok(mount(headerTree(other.view, { name: "Example", role: "captain", live: true }), fakeDoc).textContent.includes("This officer's backend isn't collected yet."));

  const failed = createActivityWatcher({ client: fakeActivityClient([new HttpError(500)]), agent: "example-captain", awaitStream: false });
  failed.start();
  await settle();
  assert.equal(failed.view.status, "failed");
});

test("watcher: a paged item loads on request, in full", async () => {
  const big = toolItem("i2", 9, { name: "read", content: [{ type: "text", text: "line\n".repeat(20_000) }] });
  const text = JSON.stringify(big);
  const client = fakeActivityClient([activitySnapshot({ seq: 9 })]);
  const pages = [];
  client.activityItemPage = async (agent, turnId, item, page) => {
    pages.push([agent, turnId, item, page]);
    return { agent, turn: turnId, item, seq: 9, page, pages: 2, bytes: text.length, data: page === 0 ? text.slice(0, 50_000) : text.slice(50_000) };
  };
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  assert.equal(watcher.view.full("turn-b", "i2"), null, "nothing read before it is asked for");
  await watcher.loadItem("turn-b", "i2");
  assert.deepEqual(pages, [["example-captain", "turn-b", "i2", 0], ["example-captain", "turn-b", "i2", 1]]);
  const full = watcher.view.full("turn-b", "i2");
  assert.equal(full.state, "ready");
  assert.equal(full.item.content[0].text.length, 100_000);
  watcher.stop();
});

// ---------- rendering ----------

test("view: control and bidi characters show as markers; newline and tab stay", () => {
  assert.deepEqual(visibleSegments("a\u001b[31mb\u202Ec\n\td"), ["a", { ctl: "⟨U+001B⟩" }, "[31mb", { ctl: "⟨U+202E⟩" }, "c\n\td"]);
  assert.equal(visibleText("x\u0000y\u200F\u2066z\r\n"), "x⟨U+0000⟩y⟨U+200F⟩⟨U+2066⟩z⟨U+000D⟩\n");
  assert.equal(visibleText("plain ünïcode ✓"), "plain ünïcode ✓");
});

test("view: content is set as text, never parsed as HTML, and marked", () => {
  const hostile = '<img src=x onerror="alert(1)">\u001b[0m\u202Etxt.exe';
  const tool = toolItem("i2", 2, { name: "<b>bash</b>", input: { command: hostile }, metadata: { output: hostile }, content: [{ type: "text", text: hostile }], error: { type: "ToolError", message: hostile, status: 500, responseBody: hostile, retryAfterMs: null } });
  const element = mount(itemTree("turn-a", tool), fakeDoc);
  const all = element.findAll(() => true);
  assert.ok(all.every((node) => ["li", "div", "span", "b", "p", "pre", "h4", "button"].includes(node.tag)), "only the view's own elements");
  assert.ok(!all.some((node) => node.tag === "img"), "markup in content is never an element");
  const shown = element.textContent;
  assert.ok(shown.includes('<img src=x onerror="alert(1)">'), "markup shows as text");
  assert.ok(shown.includes("⟨U+001B⟩[0m⟨U+202E⟩txt.exe"));
  assert.ok(!/[\u001b\u202E]/.test(shown), "no hidden character reaches the page");
  assert.ok(shown.includes("<b>bash</b>"));
  const markers = all.filter((node) => node.attributes.class === "ctl");
  assert.ok(markers.length >= 10);
  assert.throws(() => mount({ tag: "a", props: { href: "javascript:x" }, children: [] }, fakeDoc), /not allowed/);
});

test("view: steps show settled tokens and cost; text and reasoning show in full", () => {
  const step = mount(itemTree("t", stepItem("i1", 1, { tokens: { input: 1200, output: 80, reasoning: 10, cacheRead: 900, cacheWrite: 5 }, cost: 0.0123 })), fakeDoc).textContent;
  assert.match(step, /1,200 in · 80 out · 10 reasoning · 900 cache read · 5 cache write · \$0\.01/);
  const long = "word ".repeat(10_000);
  assert.equal(mount(itemTree("t", textItem("i3", 3, long)), fakeDoc).findAll((node) => node.tag === "pre")[0].textContent, long);
  const paged = mount(itemTree("turn-b", activitySnapshot().turns[1].items[1]), fakeDoc);
  const button = paged.findAll((node) => node.tag === "button")[0];
  assert.equal(button.dataset.activityLoad, "i2");
  assert.equal(button.dataset.activityTurn, "turn-b");
  assert.match(paged.textContent, /2 pages/);
});

test("panel: newest first, running turn open, unchanged items keep their nodes", async () => {
  const client = fakeActivityClient([activitySnapshot({ seq: 9 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = createActivityPanel({ doc: fakeDoc });
  const officer = { name: "Example", role: "captain", live: true };
  panel.update(watcher.view, officer);
  const turnsOf = () => panel.root.findAll((node) => node.tag === "details");
  assert.deepEqual(turnsOf().map((node) => "open" in node.attributes), [true, false], "the running turn is open");
  const before = turnsOf()[0].findAll((node) => node.tag === "li");
  watcher.event(activityEvent(10, { type: "item", item: textItem("i3", 10, "Reply") }));
  watcher.event(activityEvent(11, { type: "turn-updated", turn: turn("turn-b", 11, { status: "running", reconnects: 1, endedAt: null, items: undefined }) }));
  panel.update(watcher.view, officer);
  const after = turnsOf()[0].findAll((node) => node.tag === "li");
  assert.equal(after.length, 3);
  assert.equal(after[0], before[0], "an unchanged item is not rebuilt, so a selection in it survives");
  assert.ok(turnsOf()[0].textContent.includes(ACTIVITY_MESSAGES.reconnected));
  assert.equal(ACTIVITY_MESSAGES.reconnected, "Some changes may be missing (the stream reconnected)");
  watcher.stop();
});

// ---------- the one stream ----------

const streamResponse = () => {
  let controller;
  const body = new ReadableStream({ start: (c) => (controller = c) });
  return { response: { ok: true, status: 200, headers: { get: () => "text/event-stream" }, body }, push: (text) => controller.enqueue(new TextEncoder().encode(text)) };
};

test("live source: setAgents re-opens the one stream with ?agent= and without it on close", async () => {
  const { fleet } = sampleFleet();
  const opened = [];
  const streams = [];
  const live = createLiveSource({
    client: createApiClient({
      endpoint: "https://fleet.example.com",
      token: "t",
      fetch: async (url, init) => {
        if (url.endsWith("/fleet")) return json(200, fleet);
        opened.push(url);
        init.signal.addEventListener("abort", () => streams.at(-1)?.aborted?.());
        const s = streamResponse();
        streams.push(s);
        return s.response;
      },
    }),
    onEvent: () => {},
    onConnection: () => {},
    onOpen: (agents) => opens.push(agents),
  });
  const opens = [];
  live.start();
  await settle();
  live.setAgents(["example-captain"]);
  await settle();
  live.setAgents(["example-captain"]);
  await settle();
  live.setAgents([]);
  await settle();
  assert.deepEqual(opened.map((url) => new URL(url).search), ["", "?agent=example-captain", ""], "one stream at a time, re-opened only on a change");
  assert.deepEqual(opens, [[], ["example-captain"], []]);
  live.stop();
});

test("join order: the snapshot is read only after the stream answered with ?agent=, on open and on every re-open", async () => {
  const { fleet } = sampleFleet();
  const log = [];
  const held = [];
  const fetchImpl = async (url) => {
    const path = new URL(url).pathname + new URL(url).search;
    log.push(path);
    if (path.endsWith("/fleet")) return json(200, fleet);
    if (path.includes("/activity")) return json(200, activitySnapshot());
    const s = streamResponse();
    if (!path.includes("agent=")) return s.response;
    // Hold the subscribed stream's answer to show the snapshot waits for it.
    return new Promise((resolve) => held.push(() => resolve(s.response)));
  };
  const manager = createFleetManager({
    entries: [{ id: "f1", endpoint: "https://fleet.example.com", token: "t", origin: "local" }],
    createClient: (entry) => createApiClient({ endpoint: entry.endpoint, token: entry.token, fetch: fetchImpl }),
    createLive: (options) => createLiveSource(options),
    setTimer: () => 0,
    clearTimer: () => {},
  });
  manager.activate("f1");
  await settle();
  const hooks = { onEvent: (data) => watcher?.event(data), onOpen: () => watcher?.reopened() };
  let watcher = null;
  const handle = manager.watchActivity("f1", "example-captain", hooks);
  watcher = createActivityWatcher({ client: handle.client, agent: "example-captain" });
  watcher.start();
  await settle();
  assert.ok(!log.some((path) => path.includes("/activity")), "no snapshot while the stream has not answered");
  assert.equal(watcher.view.waiting, true);
  held.shift()();
  await settle();
  const subscribed = log.indexOf("/api/v1/events?agent=example-captain");
  const snapshot = log.findIndex((path) => path.includes("/activity"));
  assert.ok(subscribed >= 0 && snapshot > subscribed, "subscribe, then read the snapshot");
  assert.equal(watcher.view.status, "ready");
  // A re-open (here a retry) reads the snapshot again only after the new stream answers.
  manager.retry("f1");
  await settle();
  const reads = log.filter((path) => path.includes("/activity")).length;
  assert.equal(reads, 1);
  held.shift()();
  await settle();
  assert.equal(log.filter((path) => path.includes("/activity")).length, 2);
  handle.stop();
  watcher.stop();
  manager.stop();
});

test("live source: a refused subscription (400 or 404) drops the agent and keeps the fleet stream", async () => {
  const { fleet } = sampleFleet();
  const opened = [];
  const refused = [];
  const states = [];
  const live = createLiveSource({
    client: createApiClient({
      endpoint: "https://fleet.example.com",
      token: "t",
      fetch: async (url) => {
        if (url.endsWith("/fleet")) return json(200, fleet);
        opened.push(new URL(url).search);
        return url.includes("agent=") ? json(404, {}) : streamResponse().response;
      },
    }),
    onEvent: () => {},
    onConnection: (state) => states.push(state.status),
    onAgentsRefused: (agents, status) => refused.push([agents, status]),
  });
  live.start();
  await settle();
  live.setAgents(["example-unknown"]);
  await settle();
  assert.deepEqual(opened, ["", "?agent=example-unknown", ""]);
  assert.deepEqual(refused, [[["example-unknown"], 404]]);
  assert.equal(states.at(-1), "connected");
  assert.deepEqual(live.agents, []);
  live.stop();
});

test("watcher: a refused subscription reads as unavailable", () => {
  const watcher = createActivityWatcher({ client: fakeActivityClient([]), agent: "example-captain" });
  watcher.start();
  watcher.refused();
  assert.equal(watcher.view.status, "unavailable");
  assert.equal(watcher.view.waiting, false);
});

test("fleet manager: agent-activity events go to the open view, not the reducer", async () => {
  const { fleet } = sampleFleet();
  const lives = [];
  const manager = createFleetManager({
    entries: [{ id: "f1", endpoint: "https://fleet.example.com", token: "t", origin: "local" }],
    createClient: () => ({ fleet: async () => fleet }),
    createLive: (options) => {
      const live = { options, agents: options.agents, start() {}, stop() {}, setAgents: (ids) => (live.agents = ids) };
      lives.push(live);
      return live;
    },
    setTimer: () => 0,
    clearTimer: () => {},
  });
  manager.activate("f1");
  const seen = [];
  const opened = [];
  const handle = manager.watchActivity("f1", "example-captain", { onEvent: (data) => seen.push(data), onOpen: () => opened.push(true) });
  assert.deepEqual(lives[0].agents, ["example-captain"]);
  lives[0].options.onEvent({ type: "agent-activity", data: { seq: 1 } });
  lives[0].options.onOpen(["example-captain"]);
  assert.deepEqual(seen, [{ seq: 1 }]);
  assert.equal(opened.length, 1);
  handle.stop();
  assert.deepEqual(lives[0].agents, []);
  lives[0].options.onEvent({ type: "agent-activity", data: { seq: 2 } });
  assert.equal(seen.length, 1, "nothing is delivered once the view closed");
  manager.stop();
});

// ---------- sample mode ----------

test("sample: the officer's activity decodes, its paged item joins, and its running turn advances through the watcher", async () => {
  let clock = T0;
  const sim = createSampleActivitySim({ agent: "galactica", now: () => clock });
  const snapshot = decodeActivity(sim.snapshot());
  assert.equal(snapshot.turns.length, 3);
  assert.equal(snapshot.turns.at(-1).status, "running");
  assert.equal(snapshot.turns[1].reconnects, 1);
  const paged = snapshot.turns[0].items.find((item) => item.paged);
  assert.ok(paged && paged.paged.pages > 1, "the long log is paged");
  const joined = await readPagedItem(async (page) => sim.page(snapshot.turns[0].id, paged.id, page));
  assert.equal(joined.item.content[0].text.split("\n").length > 1400, true);
  assert.equal(decodeActivity(createSampleActivitySim({ agent: "kirrin", backend: "claude" }).snapshot()).agent.collected, false);

  const ticks = [];
  const source = createSampleActivitySource({ agent: "galactica", role: "captain", backend: "opencode", onEvent: (event) => watcher.event(event), onOpen: () => watcher.reopened(), now: () => clock, every: (fn) => ticks.push(fn), stopEvery: () => {} });
  const watcher = createActivityWatcher({ client: source.client, agent: "galactica" });
  assert.equal(watcher.view.waiting, true);
  watcher.start();
  source.start();
  await settle();
  for (let i = 0; i < 60; i += 1) {
    clock += 900;
    ticks[0]();
  }
  const view = watcher.view;
  assert.equal(view.status, "ready");
  assert.ok(view.turns.some((t) => t.status === "completed" && t.items.some((item) => item.kind === "text" && item.text.startsWith("The orbit layout tests pass"))), "the running turn ended with its reply");
  assert.ok(view.turns.every((t) => new Set(t.items.map((item) => item.id)).size === t.items.length));
  source.stop();
  watcher.stop();
});

test("sample manager: watchActivity serves a simulated officer", async () => {
  const manager = createSampleFleetManager({
    createSource: ({ onEvent }) => ({ start: () => onEvent({ type: "fleet", data: sampleFleet().fleet }), stop() {} }),
    createActivity: (options) => createSampleActivitySource({ ...options, every: () => 0, stopEvery: () => {} }),
  });
  manager.start();
  const captain = sampleFleet().fleet.ships[0].captain;
  const handle = manager.watchActivity("sample-colonial", captain.id, { onEvent() {}, onOpen() {} });
  assert.ok(handle);
  const snapshot = await handle.client.activity(captain.id);
  assert.equal(snapshot.agent.id, captain.id);
  await assert.rejects(handle.client.activity(captain.id, { turn: "turn-gone" }), (error) => error.status === 404);
  handle.stop();
  manager.stop();
});
