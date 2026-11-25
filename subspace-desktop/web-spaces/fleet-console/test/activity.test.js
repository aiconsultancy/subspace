import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { createApiClient, HttpError, InvalidResponseError, MAX_PAGE_BODY_BYTES, readPagedItem } from "../src/api.js";
import { ACTIVITY_MESSAGES, applyChange, createActivityWatcher, decodeActivity, decodeActivityEvent, MAX_BUFFER, MAX_BUFFER_BYTES, MAX_TURNS, POLL_MS } from "../src/activity.js";
import { createActivityPanel, headerTree, itemDocument, itemParts, itemTree, json as viewJson, mount, PART_BYTES, visibleSegments, visibleText } from "../src/activity-view.js";
import { crewContext } from "../src/effects.js";
import { splitUtf8, utf8Length } from "../src/utf8.js";
import { createLiveSource } from "../src/live.js";
import { createFleetManager, createSampleFleetManager } from "../src/fleet-manager.js";
import { createSampleActivitySource } from "../src/sample-source.js";
import { createSampleActivitySim } from "../src/sample.js";
import { activityEvent, activitySnapshot, GENERATION, stepItem, T0, textItem, toolItem, turn } from "./activity-fixtures.js";
import { sampleFleet } from "./fixtures.js";

const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

/** Timers a test fires by hand. */
function fakeTimers() {
  let next = 1;
  const live = new Map();
  return {
    options: {
      setTimer: (fn, ms) => {
        const id = next++;
        live.set(id, { fn, ms });
        return id;
      },
      clearTimer: (id) => live.delete(id),
    },
    delays: () => [...live.values()].map((timer) => timer.ms),
    pending: () => live.size,
    /** Fire every timer set so far. */
    fire() {
      const due = [...live.entries()];
      for (const [id, timer] of due) {
        live.delete(id);
        timer.fn();
      }
    },
  };
}

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

test("activity: decodes a #355-shaped snapshot and keeps a malformed item as unreadable", () => {
  const json = activitySnapshot();
  json.turns[0].items.push({ id: "bad" });
  const decoded = decodeActivity(json);
  assert.deepEqual(decoded.agent, { id: "example-captain", role: "captain", backend: "opencode", collected: true });
  assert.deepEqual(decoded.cursor, { generation: GENERATION, seq: 9 });
  assert.deepEqual(decoded.turns.map((t) => t.id), ["turn-a", "turn-b"]);
  assert.deepEqual(decoded.turns[0].items.map((i) => i.id), ["i1", "i2", "i3", "bad"], "the malformed item is kept, never withheld");
  assert.deepEqual(decoded.turns[0].items[3], { id: "bad", seq: 4, kind: "unreadable", raw: { id: "bad" } });
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
  await assert.rejects(readPagedItem(async (page) => ({ seq: page, page, pages: 2, bytes: 2, data: "x" }), { restarts: 2 }), /kept changing/);
});

test("api: joined pages must have the item's UTF-8 byte length, or the read is invalid", async () => {
  const text = JSON.stringify(textItem("i3", 9, "héllo ✓ wörld"));
  const bytes = new TextEncoder().encode(text).length;
  assert.notEqual(bytes, text.length, "the fixture has multi-byte characters");
  const reads = [];
  const item = await readPagedItem(async (page) => {
    reads.push(page);
    return { seq: 9, page, pages: 2, bytes, data: page === 0 ? text.slice(0, 20) : text.slice(20) };
  });
  assert.deepEqual(reads, [0, 1]);
  assert.equal(item.item.text, "héllo ✓ wörld");
  // Short of `bytes`: refused rather than parse a smaller item.
  const short = '{"id":"i3"}';
  await assert.rejects(readPagedItem(async (page) => ({ seq: 9, page, pages: 1, bytes: short.length + 40, data: short })), /join to 11 bytes, not 51/);
  // Past `bytes`: refused at the page that overruns it.
  await assert.rejects(readPagedItem(async (page) => ({ seq: 9, page, pages: 1, bytes: 3, data: short })), /runs past its length/);
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
  const big = toolItem("i2", 9, { name: "read", content: [{ type: "text", text: "line\n".repeat(18_000) }] });
  const text = JSON.stringify(big);
  const client = fakeActivityClient([activitySnapshot({ seq: 9 })]);
  const pages = [];
  client.activityItemPage = async (agent, turnId, item, page) => {
    pages.push([agent, turnId, item, page]);
    return { agent, turn: turnId, item, seq: 9, page, pages: 2, bytes: text.length, data: page === 0 ? text.slice(0, 60_000) : text.slice(60_000) };
  };
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  assert.equal(watcher.view.full("turn-b", "i2"), null, "nothing read before it is asked for");
  await watcher.loadItem("turn-b", "i2");
  assert.deepEqual(pages, [["example-captain", "turn-b", "i2", 0], ["example-captain", "turn-b", "i2", 1]]);
  const full = watcher.view.full("turn-b", "i2");
  assert.equal(full.state, "ready");
  assert.equal(full.item.content[0].text.length, 90_000);
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

test("watcher: a refused subscription polls the snapshot every five seconds until it is accepted or closed", async () => {
  const timers = fakeTimers();
  const client = fakeActivityClient([activitySnapshot({ seq: 9 }), activitySnapshot({ seq: 12 }), activitySnapshot({ seq: 15 }), activitySnapshot({ seq: 20 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", ...timers.options });
  watcher.start();
  assert.equal(client.calls.length, 0, "waiting for the stream");
  watcher.refused();
  await settle();
  assert.equal(watcher.view.status, "ready");
  assert.equal(watcher.view.polling, true);
  assert.equal(watcher.view.waiting, false);
  assert.equal(client.calls.length, 1);
  const header = mount(headerTree(watcher.view, { name: "Example", role: "captain", live: true }), fakeDoc);
  assert.ok(header.textContent.includes(ACTIVITY_MESSAGES.polling));
  assert.deepEqual(timers.delays(), [POLL_MS]);
  timers.fire();
  await settle();
  assert.equal(client.calls.length, 2);
  assert.equal(watcher.view.cursor.seq, 12);
  // Accepted: polling stops and the snapshot follows the stream.
  watcher.reopened();
  await settle();
  assert.equal(watcher.view.polling, false);
  assert.equal(timers.pending(), 0, "no poll left");
  assert.equal(client.calls.length, 3);
  // Refused again, then closed: no more reads.
  watcher.refused();
  await settle();
  assert.equal(client.calls.length, 4);
  watcher.stop();
  assert.equal(timers.pending(), 0);
  timers.fire();
  await settle();
  assert.equal(client.calls.length, 4);
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

// ---------- round 1: source change, paging, retry, polling and rendering ----------

const flush = async () => {
  for (let i = 0; i < 4; i += 1) await settle();
};

/** Pages of `item` as the gateway sends them: at most 64 KiB of UTF-8 each, split between code points. */
function pagesOf(item, { agent = "example-captain", turn: turnId = "turn-b", id = item.id, seq = item.seq } = {}) {
  const text = JSON.stringify(item);
  const parts = splitUtf8(text, 64 * 1024);
  return parts.map((data, page) => ({ agent, turn: turnId, item: id, seq, page, pages: parts.length, bytes: utf8Length(text), data }));
}

/** A snapshot whose running turn holds paged items with these IDs. */
function pagedSnapshot(ids, { seq = 9, generation = GENERATION } = {}) {
  const snapshot = activitySnapshot({ seq, generation });
  snapshot.turns[1].items = ids.map((id) => ({ id, seq: 9, startedAt: T0 + 70, updatedAt: T0 + 80, kind: "tool", status: "completed", callId: `call_${id}`, name: "read", paged: { pages: 2, bytes: 70_000 } }));
  return snapshot;
}

const clickOn = (panel, node) => panel.root.listeners.click({ target: node });
const buttons = (root, words) => root.findAll((node) => node.tag === "button" && node.textContent === words);

test("fix 1: a source change drops the old snapshot and page reads, and no later request carries the old token", async () => {
  const { fleet } = sampleFleet();
  const log = [];
  const held = [];
  const big = toolItem("i2", 9, { name: "read", content: [{ type: "text", text: "x".repeat(70_000) }] });
  const fetchImpl = async (url, init) => {
    const path = new URL(url).pathname + new URL(url).search;
    const token = init.headers.Authorization;
    log.push({ path, token });
    if (path.endsWith("/fleet")) return json(200, fleet);
    if (path.startsWith("/api/v1/events")) return streamResponse().response;
    if (token === "Bearer old" && log.filter((entry) => entry.path.endsWith("/activity")).length > 1) {
      // Hold the old source's second snapshot and its page reads.
      return new Promise((resolve) => held.push(() => resolve(json(200, path.includes("page=") ? pagesOf(big)[0] : activitySnapshot({ seq: 50 })))));
    }
    if (path.includes("page=")) return json(200, pagesOf(big)[Number(new URL(url).searchParams.get("page"))]);
    return json(200, activitySnapshot({ seq: 9 }));
  };
  const entry = (token) => ({ id: "f1", endpoint: "https://fleet.example.com", token, origin: "local" });
  const manager = createFleetManager({
    entries: [entry("old")],
    createClient: (fleetEntry) => createApiClient({ endpoint: fleetEntry.endpoint, token: fleetEntry.token, fetch: fetchImpl }),
    createLive: (options) => createLiveSource(options),
    setTimer: () => 0,
    clearTimer: () => {},
  });
  manager.activate("f1");
  await flush();
  let watcher = null;
  const ended = [];
  const handle = manager.watchActivity("f1", "example-captain", {
    onEvent: (data) => watcher?.event(data),
    onOpen: () => watcher?.reopened(),
    onDown: () => watcher?.disconnected(),
    onReset: () => watcher?.reset(),
    onEnded: () => ended.push(true),
  });
  watcher = createActivityWatcher({ client: handle.client, agent: "example-captain" });
  watcher.start();
  await flush();
  assert.equal(watcher.view.status, "ready");
  // An old snapshot read and an old page read, both held.
  watcher.reopened();
  watcher.loadItem("turn-b", "i2");
  await flush();
  assert.equal(held.length, 2, "a snapshot and a page held on the old source");
  const before = log.length;
  manager.update([entry("new")]);
  assert.equal(watcher.view.status, "loading", "nothing read from the old source stays on show");
  assert.deepEqual(watcher.view.turns, []);
  assert.equal(watcher.view.full("turn-b", "i2"), null);
  for (const release of held) release();
  await flush();
  assert.equal(watcher.view.cursor?.seq, 9, "the old held snapshot (seq 50) was never published");
  assert.equal(watcher.view.full("turn-b", "i2"), null, "the old page read was never published");
  const after = log.slice(before);
  assert.ok(after.length > 0);
  assert.ok(after.every((request) => request.token === "Bearer new"), "no request after the change uses the old token");
  const stream = after.findIndex((request) => request.path === "/api/v1/events?agent=example-captain");
  const snapshot = after.findIndex((request) => request.path.endsWith("/activity"));
  assert.ok(stream >= 0 && snapshot > stream, "the new source's snapshot waits for its stream");
  // The token removed: the view ends.
  manager.update([entry("")]);
  assert.equal(ended.length, 1);
  await assert.rejects(handle.client.activity("example-captain"), /no longer has a fleet/);
  watcher.stop();
  manager.stop();
});

test("fix 2: a page claiming more than it may hold is refused before another page is read", async () => {
  let reads = 0;
  await assert.rejects(
    readPagedItem(async (page) => {
      reads += 1;
      assert.equal(reads, 1, "reject the oversized item on its first read");
      assert.equal(page, 0);
      return { agent: "a", turn: "t", item: "i", seq: 1, page, pages: 1_000_000, bytes: 60_000_000_000, data: "{" };
    }, { agent: "a", turn: "t", item: "i" }),
    InvalidResponseError,
  );
  assert.equal(reads, 1);
  const page = (fields) => async (n) => ({ agent: "a", turn: "t", item: "i", seq: 1, page: n, pages: 1, bytes: 2, data: "{}", ...fields });
  await assert.rejects(readPagedItem(page({ pages: 0 })), /not a valid page/);
  await assert.rejects(readPagedItem(page({ bytes: 70_000 })), /claims 70000 bytes in 1 pages/);
  await assert.rejects(readPagedItem(page({ pages: 3000, bytes: 100 * 1024 * 1024 })), /over this console's/);
  await assert.rejects(readPagedItem(page({ item: "other" }), { agent: "a", turn: "t", item: "i" }), /another item/);
  await assert.rejects(readPagedItem(page({ page: 1 })), /not a valid page/);
  // A page body is capped before it is decoded.
  const { client } = recordingClient(() => ({ ok: true, status: 200, headers: { get: () => null }, text: async () => `"${"x".repeat(MAX_PAGE_BODY_BYTES)}"`, json: async () => assert.fail("decoded") }));
  await assert.rejects(client.activityItemPage("a", "t", "i", 0), /over 524288 bytes/);
  const declared = recordingClient(() => ({ ok: true, status: 200, headers: { get: (name) => (name === "content-length" ? String(MAX_PAGE_BODY_BYTES + 1) : null) }, json: async () => assert.fail("decoded") }));
  await assert.rejects(declared.client.activityItemPage("a", "t", "i", 0), /over 524288 bytes/);
});

test("api: a multi-page item must have enough bytes for its pages and make progress", async () => {
  const reads = [];
  const readPage = async (page, fields) => {
    assert.ok(reads.length < 2, "stop acquiring pages at the first invalid page");
    reads.push(page);
    return { seq: 9, page, ...fields };
  };

  await assert.rejects(readPagedItem((page) => readPage(page, { pages: 3, bytes: 2, data: "{" })), /claims 3 pages for 2 bytes/);
  assert.deepEqual(reads, [0]);

  reads.length = 0;
  await assert.rejects(readPagedItem((page) => readPage(page, { pages: 3, bytes: 3, data: page === 0 ? "{" : "" })), /Page 1 of the item is empty/);
  assert.deepEqual(reads, [0, 1]);
});

test("fix 2: one item read at a time, and a large item shows in parts that reach its last byte", async () => {
  const sentinel = "END-OF-ITEM-SENTINEL";
  // Multi-byte text, so parts must split between code points.
  const big = toolItem("i2", 9, { name: "read", content: [{ type: "text", text: `${"ünï ✓ 🛰 ".repeat(40_000)}${sentinel}` }] });
  const other = toolItem("i4", 9, { name: "read", content: [{ type: "text", text: "y".repeat(70_000) }] });
  const client = fakeActivityClient([pagedSnapshot(["i2", "i4"])]);
  const inflight = [];
  let most = 0;
  client.activityItemPage = async (agent, turnId, item, page) => {
    inflight.push(item);
    most = Math.max(most, inflight.length);
    await settle();
    inflight.splice(inflight.indexOf(item), 1);
    return pagesOf(item === "i2" ? big : other, { id: item })[page];
  };
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  await Promise.all([watcher.loadItem("turn-b", "i2"), watcher.loadItem("turn-b", "i4")]);
  assert.equal(most, 1, "one page read in flight at a time");
  const full = watcher.view.full("turn-b", "i2");
  assert.equal(full.state, "ready");
  assert.ok(full.bytes > 256 * 1024);
  const panel = createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, { name: "Example", role: "captain", live: true });
  const parts = itemParts(full.item);
  assert.ok(parts.length > 4);
  assert.ok(parts.every((part) => utf8Length(part) <= PART_BYTES), "each part at most 64 KiB");
  assert.ok(parts.every((part) => !/^[\uDC00-\uDFFF]/.test(part) && !/[\uD800-\uDBFF]$/.test(part)), "split between code points");
  const mounted = () => panel.root.findAll((node) => node.attributes.class === "act-pre act-part");
  assert.equal(mounted().length, 1, "only the current part is mounted");
  assert.ok(panel.root.textContent.includes(`Part 1 of ${parts.length}`));
  assert.ok(!panel.root.textContent.includes(sentinel));
  clickOn(panel, buttons(panel.root, "Next")[0]);
  assert.ok(panel.root.textContent.includes(`Part 2 of ${parts.length}`));
  clickOn(panel, buttons(panel.root, "Last")[0]);
  assert.ok(panel.root.textContent.includes(`Part ${parts.length} of ${parts.length}`));
  assert.ok(mounted()[0].textContent.includes(sentinel), "the final page's sentinel is reachable");
  clickOn(panel, buttons(panel.root, "Previous")[0]);
  assert.ok(panel.root.textContent.includes(`Part ${parts.length - 1} of ${parts.length}`));
  clickOn(panel, buttons(panel.root, "First")[0]);
  assert.ok(panel.root.textContent.includes(`Part 1 of ${parts.length}`));
  assert.equal(mounted().length, 1);
  watcher.stop();
});

test("fix 2: loaded items are evicted least recently opened first, and released when their turn leaves", async () => {
  const items = { i2: toolItem("i2", 9, { content: [{ type: "text", text: "a".repeat(70_000) }] }), i4: toolItem("i4", 9, { content: [{ type: "text", text: "b".repeat(70_000) }] }), i5: toolItem("i5", 9, { content: [{ type: "text", text: "c".repeat(70_000) }] }) };
  const withoutTurnB = activitySnapshot({ seq: 30 });
  withoutTurnB.turns = withoutTurnB.turns.slice(0, 1);
  const client = fakeActivityClient([pagedSnapshot(["i2", "i4", "i5"]), withoutTurnB]);
  client.activityItemPage = async (agent, turnId, item, page) => pagesOf(items[item], { id: item })[page];
  const one = utf8Length(JSON.stringify(items.i2));
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, maxLoadedBytes: one * 2 + 10 });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "i2");
  await watcher.loadItem("turn-b", "i4");
  // Opening i2 again makes i4 the least recently opened.
  await watcher.loadItem("turn-b", "i2");
  await watcher.loadItem("turn-b", "i5");
  assert.equal(watcher.view.full("turn-b", "i4"), null, "the least recently opened is freed");
  assert.equal(watcher.view.full("turn-b", "i2").state, "ready");
  assert.equal(watcher.view.full("turn-b", "i5").state, "ready");
  assert.ok(watcher.view.loadedBytes <= one * 2 + 10);
  const evicted = mount(itemTree("turn-b", watcher.view.turns[0].items.find((item) => item.id === "i4"), watcher.view.full("turn-b", "i4")), fakeDoc);
  assert.equal(buttons(evicted, "Load in full").length, 1, "an evicted item offers Load in full again");
  // The turn leaves the snapshot: its loaded items go with it.
  watcher.reopened();
  await settle();
  assert.equal(watcher.view.full("turn-b", "i2"), null);
  assert.equal(watcher.view.full("turn-b", "i5"), null);
  assert.equal(watcher.view.loadedBytes, 0);
  watcher.stop();
});

test("fix 2: closing the view or changing source drops loaded items", async () => {
  const item = toolItem("i2", 9, { content: [{ type: "text", text: "a".repeat(70_000) }] });
  const client = fakeActivityClient([pagedSnapshot(["i2"])]);
  client.activityItemPage = async (agent, turnId, id, page) => pagesOf(item)[page];
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  await watcher.loadItem("turn-b", "i2");
  assert.ok(watcher.view.loadedBytes > 0);
  watcher.reset();
  assert.equal(watcher.view.loadedBytes, 0);
  watcher.stop();
});

test("fix 3: a tool call shows its parsed and its streamed input, inline and paged", () => {
  const sentinel = "RAW-ONLY-SENTINEL";
  const tool = toolItem("i2", 2, { rawInput: `{"command":"ls","note":"${sentinel}"}`, input: { command: "ls" } });
  for (const element of [mount(itemTree("turn-a", tool), fakeDoc), mount(itemTree("turn-b", { id: "i2", seq: 2, kind: "tool", status: "completed", name: "bash", paged: { pages: 2, bytes: 70_000 } }, { state: "ready", seq: 2, item: tool, bytes: 70_000 }), fakeDoc)]) {
    const shown = element.textContent;
    assert.ok(shown.includes("Input (parsed)"));
    assert.ok(shown.includes("Input (as streamed)"));
    assert.ok(shown.includes(sentinel), "the streamed input's own content shows");
  }
});

test("fix 4: Try again after a disconnect waits for the new stream's answer", async () => {
  const { fleet } = sampleFleet();
  const log = [];
  const held = [];
  let snapshots = [new HttpError(500), activitySnapshot()];
  const fetchImpl = async (url) => {
    const path = new URL(url).pathname + new URL(url).search;
    log.push(path);
    if (path.endsWith("/fleet")) return json(200, fleet);
    if (path.includes("/activity")) {
      const next = snapshots.shift();
      return next instanceof Error ? { ok: false, status: next.status, headers: { get: () => null } } : json(200, next);
    }
    const s = streamResponse();
    if (!path.includes("agent=")) return s.response;
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
  let watcher = null;
  const handle = manager.watchActivity("f1", "example-captain", { onEvent: (data) => watcher?.event(data), onOpen: () => watcher?.reopened(), onDown: () => watcher?.disconnected() });
  const timers = fakeTimers();
  watcher = createActivityWatcher({ client: handle.client, agent: "example-captain", ...timers.options });
  const panel = createActivityPanel({ doc: fakeDoc, onRetry: () => watcher.retry() });
  watcher.start();
  held.shift()();
  await flush();
  assert.equal(watcher.view.status, "failed");
  // The stream drops and a new attempt begins; its answer is held.
  manager.retry("f1");
  await settle();
  assert.equal(watcher.view.waiting, true, "waiting again for the new attempt");
  const reads = () => log.filter((path) => path.includes("/activity")).length;
  assert.equal(reads(), 1);
  panel.update(watcher.view, { name: "Example", role: "captain", live: true });
  clickOn(panel, panel.root.findAll((node) => "data-activity-retry" in node.attributes)[0]);
  timers.fire();
  await flush();
  assert.equal(reads(), 1, "no snapshot before the new response");
  held.shift()();
  await flush();
  assert.equal(reads(), 2);
  assert.equal(watcher.view.status, "ready");
  handle.stop();
  watcher.stop();
  manager.stop();
});

test("fix 5: a snapshot 503 recovers with no click and no new stream, applying the events held meanwhile", async () => {
  const { fleet } = sampleFleet();
  const opened = [];
  const streams = [];
  const answers = [activitySnapshot({ seq: 9 }), { status: 503, retryAfter: "3" }, { status: 503, retryAfter: "120" }, activitySnapshot({ seq: 12 })];
  const fetchImpl = async (url) => {
    const path = new URL(url).pathname + new URL(url).search;
    if (path.endsWith("/fleet")) return json(200, fleet);
    if (path.includes("/activity")) {
      const next = answers.shift();
      return next.status ? { ok: false, status: next.status, headers: { get: (name) => (name.toLowerCase() === "retry-after" ? next.retryAfter : null) } } : json(200, next);
    }
    opened.push(path);
    const s = streamResponse();
    streams.push(s);
    return s.response;
  };
  const manager = createFleetManager({
    entries: [{ id: "f1", endpoint: "https://fleet.example.com", token: "t", origin: "local" }],
    createClient: (entry) => createApiClient({ endpoint: entry.endpoint, token: entry.token, fetch: fetchImpl }),
    createLive: (options) => createLiveSource({ ...options, setTimer: () => 0, clearTimer: () => {} }),
    setTimer: () => 0,
    clearTimer: () => {},
  });
  manager.activate("f1");
  await settle();
  const timers = fakeTimers();
  let watcher = null;
  const handle = manager.watchActivity("f1", "example-captain", { onEvent: (data) => watcher?.event(data), onOpen: () => watcher?.reopened(), onDown: () => watcher?.disconnected() });
  watcher = createActivityWatcher({ client: handle.client, agent: "example-captain", ...timers.options });
  watcher.start();
  await flush();
  assert.equal(watcher.view.status, "ready");
  const send = (data) => streams.at(-1).push(`event: agent-activity\ndata: ${JSON.stringify(data)}\n\n`);
  // A generation change asks for a snapshot, which answers 503.
  send(activityEvent(10, { type: "item", item: textItem("i3", 10, "x") }, { generation: "gen-example-2" }));
  await flush();
  assert.equal(watcher.view.stale, true);
  assert.equal(watcher.view.turns.length, 2, "the last snapshot stays on show");
  assert.match(watcher.view.error, /503/);
  assert.deepEqual(timers.delays(), [3000], "Retry-After counts");
  const header = mount(headerTree(watcher.view, { name: "Example", role: "captain", live: true }), fakeDoc);
  assert.equal(header.findAll((node) => "data-activity-retry" in node.attributes).length, 1, "Try again stays available");
  // Events in between are held, not applied.
  send(activityEvent(11, { type: "item", item: textItem("i3", 11, "old") }));
  send(activityEvent(13, { type: "item", item: textItem("i4", 13, "held") }));
  await flush();
  assert.ok(!watcher.view.turns[0].items.some((item) => item.id === "i4"));
  timers.fire();
  await flush();
  assert.equal(watcher.view.stale, true, "a second 503");
  assert.deepEqual(timers.delays(), [30_000], "Retry-After is capped at 30 s");
  timers.fire();
  await flush();
  const view = watcher.view;
  assert.equal(view.stale, false);
  assert.equal(view.error, null);
  assert.equal(view.cursor.seq, 13);
  assert.equal(view.turns[0].items.find((item) => item.id === "i4")?.text, "held", "a held event above the snapshot's seq is applied");
  assert.ok(!view.turns[0].items.some((item) => item.text === "old"), "a held event at or below it is not");
  assert.equal(opened.length, 2, "no new stream: the plain one and the subscribed one");
  assert.equal(timers.pending(), 0);
  handle.stop();
  watcher.stop();
  manager.stop();
});

test("fix 5: backoff doubles to 30 s, and a 404 is unavailable with no retry", async () => {
  const timers = fakeTimers();
  const client = fakeActivityClient([new HttpError(503), new HttpError(503), new HttpError(503), new HttpError(503), new HttpError(503), new HttpError(503), new HttpError(404)]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, ...timers.options });
  watcher.start();
  const seen = [];
  for (let i = 0; i < 6; i += 1) {
    await settle();
    seen.push(...timers.delays());
    timers.fire();
  }
  assert.deepEqual(seen, [1000, 2000, 4000, 8000, 16000, 30000]);
  await settle();
  assert.equal(watcher.view.status, "unavailable");
  assert.equal(timers.pending(), 0);
  watcher.retry();
  assert.equal(client.calls.length, 7, "Try again does nothing for unavailable");
  watcher.stop();
});

test("fix 6: a refetch queued behind a held snapshot never starts after close", async () => {
  const signals = [];
  const client = fakeActivityClient(["hold", activitySnapshot({ seq: 50 })]);
  const activity = client.activity;
  client.activity = (agent, query, signal) => {
    signals.push(signal);
    return activity(agent, query, signal);
  };
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  watcher.reopened();
  watcher.stop();
  assert.equal(signals[0].aborted, true, "the read in flight is aborted");
  client.pending[0](activitySnapshot({ seq: 9 }));
  await flush();
  assert.equal(client.calls.length, 1, "no follow-up request");
});

test("fix 7: hidden characters are marked in nested JSON keys and values and in the backend label", () => {
  const value = { outer: { "k\u202Eey": ["a\u0000b"], typed: "\\u0000" } };
  const text = viewJson(value);
  assert.ok(text.includes("a\u0000b"), "a real NUL is the character itself, to be marked");
  assert.ok(text.includes("\\\\u0000"), "a typed \\u0000 stays escaped");
  const element = mount(itemTree("t", toolItem("i2", 2, { input: value, rawInput: null })), fakeDoc);
  const shown = element.textContent;
  assert.ok(shown.includes("a⟨U+0000⟩b"), "nested NUL marked");
  assert.ok(shown.includes("k⟨U+202E⟩ey"), "bidi control in a nested key marked");
  assert.ok(shown.includes('"typed": "\\\\u0000"'), "the typed form reads differently from the marker");
  assert.ok(!/[\u0000\u202E]/.test(shown));
  const header = mount(headerTree({ agentId: "a", status: "ready", agent: { id: "a", backend: "open\u202Ecode", collected: true }, turns: [], full: () => null }, { name: "Example", role: "captain", live: true }), fakeDoc);
  assert.ok(header.textContent.includes("open⟨U+202E⟩code"));
});

test("fix 9: opening activity leaves the chat context unchanged, and nothing in it reaches the context", async () => {
  const { fleet } = sampleFleet();
  const ships = new Map();
  const ship = fleet.ships[0];
  const member = ship.crew?.[0];
  const selection = { level: 2, ship: ship.id, crew: member?.id };
  const before = crewContext(fleet, ships, selection, [], T0);
  const sentinel = "ACTIVITY-ONLY-SENTINEL";
  const client = fakeActivityClient([activitySnapshot({ seq: 9 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  watcher.event(activityEvent(10, { type: "item", item: textItem("i3", 10, sentinel) }));
  const after = crewContext(fleet, ships, selection, [], T0);
  assert.deepEqual(after, before);
  assert.ok(!JSON.stringify(after ?? "").includes(sentinel));
  // shareContext reads only the fleet state, never the activity view.
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const share = main.slice(main.indexOf("function shareContext()"), main.indexOf("// ---------- issues"));
  assert.ok(share.length > 0 && !/activity/i.test(share), "shareContext does not touch activity");
  watcher.stop();
});

test("fix 10: an unknown kind, an invalid known kind and an unknown content type show as raw JSON", () => {
  const bad = { id: "i9", seq: 3, kind: "tool", status: 7, name: "bash", content: [{ type: "image", data: "BASE64-SENTINEL" }] };
  const decoded = decodeActivity({ ...activitySnapshot(), turns: [turn("turn-a", 4, { items: [bad, { id: "i10", seq: 4, kind: "hologram", beam: "FUTURE-SENTINEL" }] })] });
  const [invalid, unknown] = decoded.turns[0].items;
  const shownInvalid = mount(itemTree("turn-a", invalid), fakeDoc).textContent;
  assert.ok(shownInvalid.includes('"status": 7'), "the invalid item shows as its JSON");
  assert.ok(shownInvalid.includes('"type": "image"') && shownInvalid.includes("BASE64-SENTINEL"));
  const shownUnknown = mount(itemTree("turn-a", unknown), fakeDoc).textContent;
  assert.ok(shownUnknown.includes("hologram") && shownUnknown.includes("FUTURE-SENTINEL"));
  const image = mount(itemTree("turn-a", toolItem("i2", 2, { content: [{ type: "image", mime: "image/png", data: "PIXELS" }] })), fakeDoc).textContent;
  assert.ok(image.includes('"type": "image"') && image.includes("PIXELS"), "an unknown content type in a valid tool shows as JSON");
});

test("fix 11: a turn session, unavailable markers and child turns decode", () => {
  const json = activitySnapshot();
  json.agent.unavailable = { reason: "example" };
  json.turns[0].session = { id: "ses-root", parent: null };
  json.turns[0].items[1].unavailable = ["content"];
  json.turns.push(turn("turn-child", 8, { session: { id: "ses-child", parent: "ses-root" }, items: [textItem("c1", 8, "child reply")] }));
  const decoded = decodeActivity(json);
  assert.deepEqual(decoded.turns.map((t) => t.id), ["turn-a", "turn-b", "turn-child"]);
  assert.deepEqual(decoded.turns[2].session, { id: "ses-child", parent: "ses-root" });
  assert.equal(decoded.turns[0].items[1].kind, "tool", "an item with unavailable stays a tool call");
  assert.ok(mount(itemTree("turn-a", decoded.turns[0].items[1]), fakeDoc).textContent.includes("bash"));
  assert.equal(decoded.agent.collected, true);
});

// ---------- phase 1b: claude-2's review of 1a7937d2 ----------

test("1b-1: a rawInput-only marker shows inline, in a paged item read in full, and in its parts", () => {
  const sentinel = "RAW-ONLY-SENTINEL";
  const tool = toolItem("i2", 2, { rawInput: `{"command":"ls","note":"${sentinel}"}`, input: { command: "ls" } });
  const placeholder = { id: "i2", seq: 2, kind: "tool", status: "completed", name: "bash", paged: { pages: 2, bytes: 70_000 } };
  assert.ok(mount(itemTree("turn-a", tool), fakeDoc).textContent.includes(sentinel), "inline");
  assert.ok(mount(itemTree("turn-b", placeholder, { state: "ready", seq: 2, item: tool, bytes: 70_000 }), fakeDoc).textContent.includes(sentinel), "paged, through itemBody");
  const document = itemDocument(tool);
  assert.ok(document.includes("input\n") && document.includes("rawInput\n") && document.includes(sentinel), "both inputs in the item's document");
  // Over the part threshold the item shows in parts, built from itemDocument.
  const big = { ...tool, content: [{ type: "text", text: "z".repeat(300 * 1024) }] };
  const parts = itemParts(big);
  assert.ok(parts.length > 1 && parts.join("").includes(sentinel), "the parts reach the streamed input");
});

test("1b-2: after a token change the next snapshot read uses the new client; removing the fleet closes the view", async () => {
  const { fleet } = sampleFleet();
  const tokens = [];
  const fetchImpl = async (url, init) => {
    const path = new URL(url).pathname;
    if (path.endsWith("/fleet")) return json(200, fleet);
    if (path.startsWith("/api/v1/events")) return streamResponse().response;
    tokens.push(init.headers.Authorization);
    return json(200, activitySnapshot({ seq: 9 }));
  };
  const entry = (token) => ({ id: "f1", endpoint: "https://fleet.example.com", token, origin: "local" });
  const manager = createFleetManager({
    entries: [entry("old")],
    createClient: (fleetEntry) => createApiClient({ endpoint: fleetEntry.endpoint, token: fleetEntry.token, fetch: fetchImpl }),
    createLive: (options) => createLiveSource({ ...options, setTimer: () => 0, clearTimer: () => {} }),
    setTimer: () => 0,
    clearTimer: () => {},
  });
  manager.activate("f1");
  await flush();
  const ended = [];
  const handle = manager.watchActivity("f1", "example-captain", { onReset: () => {}, onEnded: () => ended.push(true) });
  await handle.client.activity("example-captain");
  manager.update([entry("new")]);
  await handle.client.activity("example-captain");
  assert.deepEqual(tokens, ["Bearer old", "Bearer new"], "the view's client follows the rebuilt fleet");
  assert.equal(ended.length, 0);
  manager.update([]);
  assert.equal(ended.length, 1, "the fleet went: the view closes");
  await assert.rejects(handle.client.activity("example-captain"), /no longer has a fleet/);
  assert.equal(tokens.length, 2, "nothing is read after removal");
  manager.stop();
});

test("1b-4: a loaded item is not re-read as its seq rises; Load the latest reads it once", async () => {
  const items = { i2: toolItem("i2", 9, { content: [{ type: "text", text: "a".repeat(70_000) }] }) };
  const client = fakeActivityClient([pagedSnapshot(["i2"])]);
  const held = [];
  client.activityItemPage = (agent, turnId, item, page) => {
    const answer = pagesOf(items[item], { id: item })[page];
    return page === 0 ? new Promise((resolve) => held.push(() => resolve(answer))) : Promise.resolve(answer);
  };
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const first = watcher.loadItem("turn-b", "i2");
  await settle();
  assert.equal(held.length, 1, "the first read is in flight");
  const placeholder = (seq) => ({ id: "i2", seq, startedAt: T0 + 70, updatedAt: T0 + 80, kind: "tool", status: "running", callId: "call_i2", name: "read", paged: { pages: 2, bytes: 70_000 } });
  for (let seq = 10; seq < 20; seq += 1) watcher.event(activityEvent(seq, { type: "item", item: placeholder(seq) }));
  held.shift()();
  await first;
  await settle();
  assert.equal(held.length, 0, "ten seq rises during the read start no further read");
  const entry = watcher.view.full("turn-b", "i2");
  assert.equal(entry.state, "ready");
  const item = watcher.view.turns[0].items.find((each) => each.id === "i2");
  assert.equal(item.seq, 19);
  const shown = mount(itemTree("turn-b", item, entry), fakeDoc);
  assert.ok(shown.textContent.includes("A newer version is on the gateway."));
  assert.equal(buttons(shown, "Load the latest").length, 1, "the newer version is offered, not read");
  watcher.event(activityEvent(20, { type: "item", item: placeholder(20) }));
  await settle();
  assert.equal(held.length, 0, "still no read");
  const latest = watcher.loadItem("turn-b", "i2");
  await settle();
  assert.equal(held.length, 1, "Load the latest reads it once");
  held.shift()();
  await latest;
  watcher.stop();
});

test("1b-6: the event buffer overflows by bytes or count with exactly one refetch", async () => {
  for (const flood of [
    (send) => {
      const chunk = "q".repeat(Math.ceil(MAX_BUFFER_BYTES / 3));
      for (let i = 0; i < 4; i += 1) send(textItem("i3", 10 + i, chunk));
    },
    (send) => {
      for (let i = 0; i <= MAX_BUFFER * 2; i += 1) send(textItem("i3", 10 + i, "x"));
    },
  ]) {
    const client = fakeActivityClient(["hold", activitySnapshot({ seq: 2000 }), activitySnapshot({ seq: 2001 })]);
    const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
    watcher.start();
    let seq = 10;
    flood((item) => watcher.event(activityEvent(seq++, { type: "item", item })));
    client.pending[0](activitySnapshot({ seq: 9 }));
    await flush();
    assert.equal(client.calls.length, 2, "one refetch after the overflow");
    assert.equal(watcher.view.cursor.seq, 2000);
    watcher.stop();
  }
});

test("1b-7: an item event that fails its checks shows as unreadable, never dropped", async () => {
  const client = fakeActivityClient([activitySnapshot({ seq: 9 })]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  watcher.event(activityEvent(10, { type: "item", item: { id: "odd", kind: "tool", note: "EVENT-SENTINEL" } }));
  const item = watcher.view.turns[0].items.find((each) => each.id === "odd");
  assert.equal(item?.kind, "unreadable");
  assert.equal(item.seq, 10, "it takes the event's seq");
  assert.ok(mount(itemTree("turn-b", item), fakeDoc).textContent.includes("EVENT-SENTINEL"));
  assert.equal(client.calls.length, 1, "no refetch");
  watcher.stop();
});

test("1b-8: a turn event for a turn the view does not hold goes by its start, and an evicted one stays out", () => {
  const held = Array.from({ length: MAX_TURNS }, (_, i) => turn(`t${i}`, 100 + i, { startedAt: T0 + 1000 * (i + 1), items: [] }));
  const full = decodeActivity({ ...activitySnapshot(), turns: held }).turns;
  const evicted = applyChange(full, activityEvent(200, { type: "turn-ended", turn: turn("t-old", 50, { startedAt: T0, items: undefined }) }, { turnId: "t-old" }));
  assert.deepEqual(evicted.map((t) => t.id), full.map((t) => t.id), "a late end for an evicted turn is not shown as the newest");
  const few = full.slice(-3);
  const placed = applyChange(few, activityEvent(201, { type: "turn-ended", turn: turn("t-mid", 60, { startedAt: few[1].startedAt - 1, items: undefined }) }, { turnId: "t-mid" }));
  assert.deepEqual(placed.map((t) => t.id), [few[0].id, "t-mid", few[1].id, few[2].id], "placed by startedAt");
  const started = applyChange(few, activityEvent(202, { type: "turn-started", turn: turn("t-new", 202, { startedAt: T0 + 999_999, status: "running", items: undefined }) }, { turnId: "t-new" }));
  assert.equal(started.at(-1).id, "t-new", "a new turn still goes on top");
});
