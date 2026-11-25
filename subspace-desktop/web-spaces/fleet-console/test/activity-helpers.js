// Shared fakes for the activity tests: a small DOM, timers fired by hand,
// a fake activity client and the pages a gateway would send. Every fake
// page reader in the tests stops itself after a few reads.

import { HttpError } from "../src/api.js";
import { splitUtf8, utf8Length } from "../src/utf8.js";
import { activitySnapshot, GENERATION, T0 } from "./activity-fixtures.js";

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
