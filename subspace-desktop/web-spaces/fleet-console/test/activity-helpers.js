// Shared fakes for the activity tests: a small DOM, timers fired by hand,
// a fake activity client and the pages a gateway would send. Every fake
// page reader in the tests stops itself after a few reads.

import assert from "node:assert/strict";
import { createActivityWatcher, INLINE_KEY, PART_MAX_BYTES } from "../src/activity.js";
import { createActivityPanel } from "../src/activity-view.js";
import { HttpError } from "../src/api.js";
import { splitUtf8, utf8Length } from "../src/utf8.js";
import { activityEvent, activitySnapshot, GENERATION, T0, textItem, toolItem } from "./activity-fixtures.js";

export const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

/** Timers a test fires by hand. */
export function fakeTimers() {
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
  removeAttribute(name) {
    delete this.attributes[name];
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

export const fakeDoc = { createElement: (tag) => new FakeElement(tag), createTextNode: (text) => new FakeText(text) };

// ---------- decoding ----------

export function fakeActivityClient(snapshots) {
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

export const flush = async () => {
  for (let i = 0; i < 4; i += 1) await settle();
};

/** Pages of `item` as the gateway sends them: at most 64 KiB of UTF-8 each, split between code points. */
export function pagesOf(item, { agent = "example-captain", turn: turnId = "turn-b", id = item.id, seq = item.seq } = {}) {
  const text = JSON.stringify(item);
  const parts = splitUtf8(text, 64 * 1024);
  return parts.map((data, page) => ({ agent, turn: turnId, item: id, seq, page, pages: parts.length, bytes: utf8Length(text), data }));
}

/** A snapshot whose running turn holds paged items with these IDs. */
export function pagedSnapshot(ids, { seq = 9, generation = GENERATION } = {}) {
  const snapshot = activitySnapshot({ seq, generation });
  snapshot.turns[1].items = ids.map((id) => ({ id, seq: 9, startedAt: T0 + 70, updatedAt: T0 + 80, kind: "tool", status: "completed", callId: `call_${id}`, name: "read", paged: { pages: 2, bytes: 70_000 } }));
  return snapshot;
}

export const clickOn = (panel, node) => panel.root.listeners.click({ target: node });
export const buttons = (root, words) => root.findAll((node) => node.tag === "button" && node.textContent === words);

export const officer = { name: "Example", role: "captain", live: true };

/** A watcher whose budget is the snapshot's inline charge plus `extra`, and a panel drawn from it. */
export async function prepare(snapshot, extra) {
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
export function usePaged(snapshot, item, pages) {
  snapshot.turns[1].items = [{ ...snapshot.turns[1].items[0], id: item.id, kind: "tool", name: "read", seq: item.seq, paged: { pages: pages.length, bytes: pages[0].bytes } }];
}

/** A snapshot whose running turn holds one paged item, `text` long, and the pages it is served as. */
export function pagedWith(text, id = "ia") {
  const item = toolItem(id, 9, { content: [{ type: "text", text }] });
  const pages = pagesOf(item);
  const snapshot = pagedSnapshot([id]);
  usePaged(snapshot, item, pages);
  return { item, pages, snapshot };
}

/** The windows of bodies drawn under `root`. */
export const windowsIn = (root) => root.findAll((node) => node.attributes.class === "act-pre act-part");

// ---------- wholes on show in one turn, for the refit tests ----------

const TURN = "turn-b";
const K = 1024;
export const keyOf = (id) => `${TURN}\u0000${id}`;
/** A live item that grows the pinned inline charge by `size` bytes. */
const growth = (size, seq = 10) => activityEvent(seq, { type: "item", item: textItem(`growth-${seq}`, seq, "g".repeat(size)) });

/** Paged items of `sizes` KiB in one turn, named `ids`, with each one's pages. */
function several(ids, sizes) {
  const made = ids.map((id, index) => pagedWith(String.fromCharCode(97 + index).repeat(sizes[index] * K), id));
  for (const other of made.slice(1)) made[0].snapshot.turns[1].items.push(other.snapshot.turns[1].items[0]);
  return { snapshot: made[0].snapshot, pages: Object.fromEntries(ids.map((id, index) => [id, made[index].pages])) };
}

/** `ids` loaded whole in order, each window drawn; every page read is logged in `requested`. */
async function loadedWhole(snapshot, pages, ids, extra) {
  const prepared = await prepare(snapshot, extra);
  const { watcher, client, panel } = prepared;
  const requested = [];
  client.activityItemPage = async (agent, turn, id, page) => {
    requested.push([id, page]);
    return pages[id][page];
  };
  for (const id of ids) {
    await watcher.loadItem(TURN, id);
    panel.update(watcher.view, officer);
  }
  await settle();
  panel.update(watcher.view, officer);
  return { ...prepared, requested };
}

/**
 * Wholes of `sizes` KiB named `ids`, loaded in order (so the first is the
 * least recently used and the last is the latest request), all with their
 * windows on show, in a view `spare` bytes larger than they need. Also
 * returns the snapshot they were read from.
 */
export async function wholesOnShow(ids, sizes, spare) {
  const { snapshot, pages } = several(ids, sizes);
  // What the wholes and their windows hold, measured with room to spare.
  const roomy = await loadedWhole(snapshot, pages, ids, 50 * K * K);
  const held = roomy.watcher.view.budget.used - roomy.inline;
  roomy.watcher.stop();
  const loaded = await loadedWhole(snapshot, pages, ids, held + spare);
  assert.equal(windowsIn(loaded.panel.root).length, ids.length, "every window drawn");
  assert.equal(loaded.watcher.view.budget.requested, keyOf(ids.at(-1)));
  return { ...loaded, snapshot };
}

/** Each of `ids`' mode, or "Load in full" when it was freed. */
export const modesOf = (watcher, ids) => Object.fromEntries(ids.map((id) => [id, watcher.view.full(TURN, id)?.mode ?? "Load in full"]));

/** What a refit of `id` gives back: its charge less its handover (the window on show and an incoming part). */
export const givenBack = (watcher, id) => watcher.view.budget.bytes(keyOf(id)) - (64 * K + PART_MAX_BYTES);

/** Grow the inline charge by `size` and let every read settle. */
export async function grow(watcher, panel, size) {
  watcher.event(growth(size));
  for (let i = 0; i < 4; i += 1) {
    await settle();
    panel.update(watcher.view, officer);
  }
}
