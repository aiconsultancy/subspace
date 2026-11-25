// The Activity view in the inspector: an officer's turns, newest first, with
// steps, tool calls, reply and reasoning text. Unlike the rest of the
// inspector it builds DOM nodes, never HTML: every value from the API is set
// as text (`createTextNode`), and control and bidi formatting characters show
// as visible `⟨U+XXXX⟩` markers so content cannot disguise itself. Content is
// shown in full; long text wraps or scrolls and stays selectable.
//
// The tree is described by plain objects (`h`), so tests can read it, then
// mounted. Live updates rebuild only the turns and items whose `seq` changed,
// so a selection in unchanged content survives.

import { ACTIVITY_MESSAGES } from "./activity.js";
import { formatBytes, formatClock, formatDuration } from "./format.js";

/**
 * Control characters other than newline and tab, DEL and C1 controls, the
 * bidi marks, embeddings, overrides and isolates, the line and paragraph
 * separators and the byte order mark.
 */
const HIDDEN = /[\u0000-\u0008\u000B-\u001F\u007F-\u009F\u061C\u200E\u200F\u2028-\u202E\u2066-\u2069\uFEFF]/g;

export const marker = (char) => `⟨U+${char.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}⟩`;

/** Split text into plain strings and `{ctl}` markers. */
export function visibleSegments(text) {
  const value = String(text ?? "");
  const out = [];
  let last = 0;
  for (const match of value.matchAll(HIDDEN)) {
    if (match.index > last) out.push(value.slice(last, match.index));
    out.push({ ctl: marker(match[0]) });
    last = match.index + match[0].length;
  }
  if (last < value.length) out.push(value.slice(last));
  return out;
}

/** The text a person sees for `text`, markers included. */
export const visibleText = (text) => visibleSegments(text).map((part) => (typeof part === "string" ? part : part.ctl)).join("");

/** A tree node: `children` are nodes or strings (always set as text). */
export function h(tag, props = {}, ...children) {
  return { tag, props, children: children.flat(Infinity).filter((child) => child !== null && child !== undefined && child !== false && child !== "") };
}

/** Text with its hidden characters as marker spans. */
function txt(value) {
  return visibleSegments(value).map((part) => (typeof part === "string" ? part : h("span", { class: "ctl", title: "Hidden character" }, part.ctl)));
}

const pre = (value, cls = "") => h("pre", { class: `act-pre${cls ? ` ${cls}` : ""}` }, txt(value));
const json = (value) => JSON.stringify(value, null, 2) ?? String(value);

/** A value in full: strings as text, anything else as indented JSON. */
function valueBlock(value) {
  return typeof value === "string" ? pre(value) : pre(json(value), "json");
}

/** An object's fields in full, each labelled, strings shown as text so their lines read naturally. */
function fields(object) {
  const entries = Object.entries(object ?? {});
  if (!entries.length) return [h("p", { class: "act-note" }, "(empty)")];
  return entries.map(([key, value]) => h("div", { class: "act-field" }, h("span", { class: "act-key mono" }, txt(key)), valueBlock(value)));
}

const label = (title, ...rest) => h("h4", { class: "act-label" }, title, ...rest);

const TOOL_STATUS = { input: "Receiving input", running: "Running", completed: "Completed", failed: "Failed" };
const TURN_STATUS = { running: "Running", completed: "Completed", failed: "Failed", cancelled: "Cancelled" };
const STEP_STATUS = { running: "Running", completed: "Completed", failed: "Failed" };
const TEXT_STATUS = { streaming: "Streaming", completed: "Completed" };
const tone = (status) => ({ running: "s-running", input: "s-running", streaming: "s-running", completed: "s-completed", failed: "s-failed", cancelled: "s-cancelled" })[status] ?? "s-idle";
const chip = (status, words) => h("span", { class: `chip ${tone(status)}` }, txt(words[status] ?? status ?? "Unknown"));

export const formatCost = (usd) => (Number.isFinite(usd) ? `$${usd < 0.01 && usd > 0 ? usd.toFixed(4) : usd.toFixed(2)}` : "—");
const count = (n) => (Number.isFinite(n) ? n.toLocaleString("en-GB") : "—");

export function tokensText(tokens) {
  if (!tokens) return "";
  return `${count(tokens.input)} in · ${count(tokens.output)} out · ${count(tokens.reasoning)} reasoning · ${count(tokens.cacheRead)} cache read · ${count(tokens.cacheWrite)} cache write`;
}

function errorBlock(error) {
  if (!error) return null;
  return h(
    "div",
    { class: "act-error" },
    label("Error"),
    pre(`${error.type ?? "Error"}: ${error.message ?? ""}`),
    Number.isFinite(error.status) ? h("p", { class: "act-note" }, `HTTP status ${error.status}`) : null,
    Number.isFinite(error.retryAfterMs) ? h("p", { class: "act-note" }, `Retry after ${formatDuration(error.retryAfterMs)}`) : null,
    error.responseBody !== null && error.responseBody !== undefined ? [label("Response body"), pre(error.responseBody)] : null,
  );
}

function contentBlock(content) {
  if (content === null || content === undefined) return null;
  if (!Array.isArray(content)) return [label("Output"), valueBlock(content)];
  if (!content.length) return [label("Output"), h("p", { class: "act-note" }, "(no output)")];
  return [
    label("Output"),
    content.map((part) => {
      if (part?.type === "text") return pre(part.text);
      if (part?.type === "file") return h("div", { class: "act-file" }, h("p", { class: "act-note" }, "File ", txt(part.name ?? "(unnamed)"), " · ", txt(part.mime ?? "")), pre(part.uri ?? ""));
      return valueBlock(part);
    }),
  ];
}

function stepItem(item) {
  const model = item.model ? `${item.model.provider}/${item.model.id}${item.model.variant ? `#${item.model.variant}` : ""}` : "Model not reported";
  const took = Number.isFinite(item.endedAt) ? ` · ${formatDuration(item.endedAt - item.startedAt)}` : "";
  return [
    h("div", { class: "act-head" }, h("b", {}, "Step"), h("span", { class: "act-sub mono" }, txt(model)), chip(item.status, STEP_STATUS)),
    h("p", { class: "act-note" }, `Started ${formatClock(item.startedAt)}${took}${item.finish ? ` · finish: ` : ""}`, item.finish ? txt(item.finish) : null),
    item.tokens ? h("p", { class: "act-note mono" }, `${tokensText(item.tokens)} · ${formatCost(item.cost)}`) : h("p", { class: "act-note" }, item.status === "running" ? "Tokens settle when the step ends." : "No token use reported."),
    errorBlock(item.error),
  ];
}

function textItem(item) {
  return [
    h("div", { class: "act-head" }, h("b", {}, item.kind === "reasoning" ? "Reasoning" : "Reply"), chip(item.status, TEXT_STATUS)),
    pre(item.text ?? "", item.kind === "reasoning" ? "reasoning" : "reply"),
  ];
}

function toolItem(item) {
  const input = item.input !== null && item.input !== undefined ? [label("Input"), ...(typeof item.input === "object" && !Array.isArray(item.input) ? fields(item.input) : [valueBlock(item.input)])] : [label("Input", h("span", { class: "act-sub" }, " (as streamed)")), pre(item.rawInput ?? "")];
  return [
    h("div", { class: "act-head" }, h("b", { class: "mono" }, txt(item.name ?? "Tool")), item.callId ? h("span", { class: "act-sub mono" }, txt(item.callId)) : null, chip(item.status, TOOL_STATUS)),
    item.executed === false ? h("p", { class: "act-note" }, "The tool did not run.") : null,
    input,
    item.metadata !== null && item.metadata !== undefined ? [label("Progress"), ...(typeof item.metadata === "object" && !Array.isArray(item.metadata) ? fields(item.metadata) : [valueBlock(item.metadata)])] : null,
    contentBlock(item.content),
    errorBlock(item.error),
  ];
}

const KIND_NAMES = { step: "Step", text: "Reply", reasoning: "Reasoning", tool: "Tool call" };

function pagedItem(turnId, item, full) {
  const what = item.kind === "tool" ? (item.name ?? "Tool") : (KIND_NAMES[item.kind] ?? item.kind);
  const pages = `${item.paged.pages} ${item.paged.pages === 1 ? "page" : "pages"}, ${formatBytes(item.paged.bytes)}`;
  const shown = full?.item ?? null;
  const head = h("div", { class: "act-head" }, h("b", { class: item.kind === "tool" ? "mono" : "" }, txt(what)), item.callId ? h("span", { class: "act-sub mono" }, txt(item.callId)) : null, chip(item.status, { ...TOOL_STATUS, ...TEXT_STATUS }));
  const load = (words) => h("button", { class: "btn", type: "button", "data-activity-load": item.id, "data-activity-turn": turnId }, words);
  if (!shown) {
    if (full?.state === "loading") return [head, h("p", { class: "act-note" }, `Large item (${pages}). Reading every page…`)];
    return [head, h("p", { class: "act-note" }, `Large item (${pages}), read on request and then shown in full.`), full?.state === "failed" ? h("p", { class: "act-warn" }, "Could not read it: ", txt(full.error)) : null, load(full?.state === "failed" ? "Try again" : "Load in full")];
  }
  const note =
    full.state === "loading" ? "Reading the newer version…" : full.state === "failed" ? null : full.seq < item.seq ? "A newer version is on the gateway." : `Read in full (${pages}).`;
  return [
    itemBody(shown),
    note ? h("p", { class: "act-note" }, note) : null,
    full.state === "failed" ? [h("p", { class: "act-warn" }, "Could not read the newer version: ", txt(full.error)), load("Try again")] : null,
  ];
}

function itemBody(item) {
  if (item.kind === "step") return stepItem(item);
  if (item.kind === "text" || item.kind === "reasoning") return textItem(item);
  if (item.kind === "tool") return toolItem(item);
  return [h("div", { class: "act-head" }, h("b", {}, txt(item.kind ?? "Item"))), valueBlock(item)];
}

/** One item of a turn. `full` is the item read in full when it is paged. */
export function itemTree(turnId, item, full = null) {
  return h("li", { class: `act-item k-${item.kind}` }, item.paged ? pagedItem(turnId, item, full) : itemBody(item));
}

/** A turn's summary line. */
export function turnSummary(turn) {
  const usage = turn.usage ? ` · ${turn.usage.steps} ${turn.usage.steps === 1 ? "step" : "steps"} · ${tokensText(turn.usage.tokens)} · ${formatCost(turn.usage.cost)}` : "";
  const ended = Number.isFinite(turn.endedAt) ? ` · took ${formatDuration(turn.endedAt - turn.startedAt)}` : "";
  return [
    h("span", { class: "act-turn-head" }, chip(turn.status, TURN_STATUS), h("span", { class: "mono" }, `Started ${formatClock(turn.startedAt)}`), turn.status === "running" ? h("span", { class: "act-sub" }, "for ", h("span", { "data-since": String(Math.round(turn.startedAt)) }, "")) : null),
    h("span", { class: "act-sub mono act-turn-id" }, txt(turn.id), ended, usage),
  ];
}

/** The notes above a turn's items. */
export function turnNotes(turn) {
  return [turn.reconnects > 0 ? h("p", { class: "act-warn" }, ACTIVITY_MESSAGES.reconnected) : null, turn.items.length ? null : h("p", { class: "act-note" }, "Nothing observed in this turn yet.")];
}

const ROLE_NAMES = { rear_admiral: "Rear admiral", captain: "Captain" };

/**
 * The view's header and messages.
 * @param {object} view    createActivityWatcher's view
 * @param {{name: string, role: string, live: boolean}} officer
 */
export function headerTree(view, officer) {
  const role = ROLE_NAMES[view.agent?.role ?? officer.role] ?? "Officer";
  const messages = [];
  if (view.status === "loading") messages.push(h("p", { class: "act-note" }, view.waiting ? "Waiting for the event stream before reading activity…" : "Reading activity…"));
  if (view.status === "unavailable") messages.push(h("p", { class: "act-warn" }, ACTIVITY_MESSAGES.unavailable));
  if (view.status === "failed") messages.push(h("p", { class: "act-warn" }, "Could not read activity: ", txt(view.error)), h("button", { class: "btn", type: "button", "data-activity-retry": "" }, "Try again"));
  if (view.status === "ready") {
    if (view.agent && !view.agent.collected) messages.push(h("p", { class: "act-warn" }, ACTIVITY_MESSAGES.notCollected));
    else if (!view.turns.length) messages.push(h("p", { class: "act-note" }, "No turns yet. A turn appears when the officer's input is submitted."));
    if (view.error) messages.push(h("p", { class: "act-warn" }, "Could not refresh: ", txt(view.error), ". Showing the last activity read."));
    if (!officer.live) messages.push(h("p", { class: "act-note" }, "Not live: the event stream is not connected, so this may be out of date."));
  }
  return h(
    "header",
    { class: "ihead act-ihead" },
    h("button", { class: "btn act-back", type: "button", "data-activity-close": "" }, "‹ Back"),
    h("p", { class: "eyebrow" }, `${role} · Activity`),
    h("h2", { tabindex: "-1" }, txt(officer.name)),
    h("p", { class: "meta mono" }, txt(view.agentId), view.agent?.backend ? ` · ${view.agent.backend}` : ""),
    messages.length ? h("div", { class: "act-messages", role: "status" }, messages) : null,
  );
}

const SAFE_ATTRIBUTE = /^(class|title|type|tabindex|role|data-[a-z-]+|aria-[a-z]+)$/;

/** Build DOM from a tree. Strings become text nodes; nothing is parsed as HTML. */
export function mount(node, doc) {
  if (typeof node === "string") return doc.createTextNode(node);
  const element = doc.createElement(node.tag);
  for (const [name, value] of Object.entries(node.props)) {
    if (!SAFE_ATTRIBUTE.test(name)) throw new Error(`Attribute ${name} is not allowed in the activity view.`);
    element.setAttribute(name, String(value));
  }
  for (const child of node.children) element.appendChild(mount(child, doc));
  return element;
}

const itemVersion = (item, full) => `${item.seq}|${full ? `${full.state}|${full.seq}|${full.error ?? ""}` : ""}`;
const turnVersion = (turn) => `${turn.seq}|${turn.status}|${turn.reconnects}|${turn.endedAt}|${turn.items.length === 0}`;

/** Put `element` at `index` among `parent`'s children, moving it only if needed (moving drops a selection). */
function place(parent, element, index) {
  const current = parent.children[index];
  if (current !== element) parent.insertBefore(element, current ?? null);
}

/**
 * The live panel. `update(view, officer)` redraws what changed.
 * @param {{doc: Document, onClose: () => void, onLoad: (turn: string, item: string) => void, onRetry: () => void}} options
 */
export function createActivityPanel({ doc, onClose, onLoad, onRetry }) {
  const root = doc.createElement("div");
  root.setAttribute("class", "activity");
  const header = doc.createElement("div");
  const list = doc.createElement("div");
  list.setAttribute("class", "act-turns");
  root.appendChild(header);
  root.appendChild(list);
  /** turn ID -> {element, summary, notes, items, version, kids: Map(item ID -> {element, version})} */
  const turns = new Map();
  let headerKey = "";

  root.addEventListener("click", (event) => {
    const target = event.target;
    if (target.closest("[data-activity-close]")) onClose?.();
    else if (target.closest("[data-activity-retry]")) onRetry?.();
    else {
      const load = target.closest("[data-activity-load]");
      if (load) onLoad?.(load.dataset.activityTurn, load.dataset.activityLoad);
    }
  });

  function turnEntry(turn) {
    let entry = turns.get(turn.id);
    if (entry) return entry;
    const element = doc.createElement("details");
    element.setAttribute("class", "act-turn");
    // The running turn opens; ended ones open on request.
    if (turn.status === "running") element.setAttribute("open", "");
    const summary = doc.createElement("summary");
    const notes = doc.createElement("div");
    const items = doc.createElement("ol");
    items.setAttribute("class", "act-items");
    element.appendChild(summary);
    element.appendChild(notes);
    element.appendChild(items);
    entry = { element, summary, notes, items, version: "", kids: new Map() };
    turns.set(turn.id, entry);
    return entry;
  }

  return {
    root,
    update(view, officer) {
      const key = JSON.stringify([view.status, view.waiting, view.error, view.agent, view.agentId, view.turns.length === 0, officer.name, officer.role, officer.live]);
      if (key !== headerKey) {
        header.replaceChildren(mount(headerTree(view, officer), doc));
        headerKey = key;
      }
      const shown = view.status === "unavailable" || (view.status === "failed" && !view.turns.length) ? [] : view.turns;
      const keep = new Set(shown.map((turn) => turn.id));
      for (const [id, entry] of turns) {
        if (keep.has(id)) continue;
        entry.element.remove();
        turns.delete(id);
      }
      shown.forEach((turn, index) => {
        const entry = turnEntry(turn);
        const version = turnVersion(turn);
        if (version !== entry.version) {
          entry.summary.replaceChildren(...turnSummary(turn).map((node) => mount(node, doc)));
          entry.notes.replaceChildren(...turnNotes(turn).filter(Boolean).map((node) => mount(node, doc)));
          entry.version = version;
        }
        const ids = new Set(turn.items.map((item) => item.id));
        for (const [id, kid] of entry.kids) {
          if (ids.has(id)) continue;
          kid.element.remove();
          entry.kids.delete(id);
        }
        turn.items.forEach((item, at) => {
          const full = item.paged ? view.full(turn.id, item.id) : null;
          const itemKey = itemVersion(item, full);
          let kid = entry.kids.get(item.id);
          if (!kid || kid.version !== itemKey) {
            const element = mount(itemTree(turn.id, item, full), doc);
            if (kid) kid.element.replaceWith(element);
            kid = { element, version: itemKey };
            entry.kids.set(item.id, kid);
          }
          place(entry.items, kid.element, at);
        });
        place(list, entry.element, index);
      });
    },
  };
}
