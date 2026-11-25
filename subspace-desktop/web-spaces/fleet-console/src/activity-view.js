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
//
// Nothing is cut, and what is mounted is bounded (SUB decision a17870f4): a
// closed turn mounts no items, an open one at most ITEM_WINDOW of them, and
// an item whose text is larger than one window (WINDOW_BYTES or
// WINDOW_LINES) shows one window at a time with First, Previous, Next and
// Last. An item larger than the view's budget is read a part at a time
// (`activity.js`), and each part shows in windows the same way. A run of
// hidden characters is one marker element, each character still its own
// `⟨U+XXXX⟩`, so a copy is exactly what shows.

import { ACTIVITY_MESSAGES, isWellFormedItem } from "./activity.js";
import { formatBytes, formatClock, formatDuration } from "./format.js";
import { splitBounded } from "./pager.js";
import { utf8Length } from "./utf8.js";

/** The most one mounted window of an item's text holds: UTF-8 bytes and lines, split between code points. */
export const WINDOW_BYTES = 64 * 1024;
export const WINDOW_LINES = 1000;
/** The most items an open turn mounts at once; Earlier items and Later items move among the rest. */
export const ITEM_WINDOW = 20;

/**
 * Control characters other than newline and tab, DEL and C1 controls, the
 * bidi marks, embeddings, overrides and isolates, the line and paragraph
 * separators and the byte order mark.
 */
const HIDDEN = /[\u0000-\u0008\u000B-\u001F\u007F-\u009F\u061C\u200E\u200F\u2028-\u202E\u2066-\u2069\uFEFF]+/g;

export const marker = (char) => `⟨U+${char.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}⟩`;

/** Split text into plain strings and `{ctl}` markers, one for each run of hidden characters. */
export function visibleSegments(text) {
  const value = String(text ?? "");
  const out = [];
  let last = 0;
  for (const match of value.matchAll(HIDDEN)) {
    if (match.index > last) out.push(value.slice(last, match.index));
    out.push({ ctl: Array.from(match[0], marker).join("") });
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

/** Text with each run of hidden characters as one marker span. */
function txt(value) {
  return visibleSegments(value).map((part) => (typeof part === "string" ? part : h("span", { class: "ctl", title: "Hidden character" }, part.ctl)));
}

const pre = (value, cls = "") => h("pre", { class: `act-pre${cls ? ` ${cls}` : ""}` }, txt(value));
const JSON_ESCAPE = /\\(u[0-9a-fA-F]{4}|.)/g;
const SHORT_CONTROLS = { b: "\b", f: "\f", r: "\r" };

/**
 * Indented JSON in which a hidden character in any string or key stays the
 * character itself, so the view marks it `⟨U+XXXX⟩` as everywhere else.
 * JSON.stringify writes C0 controls as escapes (`\u0000`, `\r`), which would
 * read like text typed that way; those become characters again here, while
 * a typed backslash stays escaped (`\\u0000`) and so stays distinguishable.
 * Newlines and tabs keep their escapes, which JSON needs to read as one value.
 */
export function json(value) {
  const text = JSON.stringify(value, null, 2) ?? String(value);
  return text.replace(JSON_ESCAPE, (whole, code) => {
    if (code in SHORT_CONTROLS) return SHORT_CONTROLS[code];
    if (code.length === 5) {
      const unit = Number.parseInt(code.slice(1), 16);
      // Only controls; an escaped lone surrogate stays as written.
      if (unit < 0x20) return String.fromCharCode(unit);
    }
    return whole;
  });
}

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
const TURN_STATUS = { running: "Running", completed: "Completed", failed: "Failed", cancelled: "Cancelled", unreadable: "Unreadable" };
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

const present = (value) => value !== null && value !== undefined;
const objectFields = (value) => (typeof value === "object" && !Array.isArray(value) ? fields(value) : [valueBlock(value)]);

/** Parsed and streamed input are both shown when both came: they can differ. */
function toolInput(item) {
  const parsed = present(item.input);
  const streamed = present(item.rawInput);
  if (!parsed && !streamed) return [label("Input"), h("p", { class: "act-note" }, "(none)")];
  return [
    parsed ? [label("Input", streamed ? h("span", { class: "act-sub" }, " (parsed)") : null), ...objectFields(item.input)] : null,
    streamed ? [label("Input", h("span", { class: "act-sub" }, " (as streamed)")), pre(item.rawInput)] : null,
  ];
}

function toolItem(item) {
  const input = toolInput(item);
  return [
    h("div", { class: "act-head" }, h("b", { class: "mono" }, txt(item.name ?? "Tool")), item.callId ? h("span", { class: "act-sub mono" }, txt(item.callId)) : null, chip(item.status, TOOL_STATUS)),
    item.executed === false ? h("p", { class: "act-note" }, "The tool did not run.") : null,
    input,
    present(item.metadata) ? [label("Progress"), ...objectFields(item.metadata)] : null,
    contentBlock(item.content),
    errorBlock(item.error),
  ];
}

const KIND_NAMES = { step: "Step", text: "Reply", reasoning: "Reasoning", tool: "Tool call" };

/** An item shown as the JSON it came as, with why. */
function rawItem(title, note, value) {
  return [h("div", { class: "act-head" }, h("b", {}, txt(title))), h("p", { class: "act-note" }, note), valueBlock(value)];
}

function itemBody(item) {
  if (item.kind === "unreadable") {
    const note = item.wholeTurn ? "This turn doesn't have the fields this console reads. Shown as sent." : "This item doesn't have the fields this console reads. Shown as sent.";
    return rawItem(item.wholeTurn ? "Turn" : "Item", note, item.raw);
  }
  if (!KIND_NAMES[item.kind]) return rawItem(item.kind ?? "Item", "A kind of item this console doesn't know yet. Shown as sent.", item);
  if (!isWellFormedItem(item)) return rawItem(KIND_NAMES[item.kind], "This item doesn't have the fields this console reads. Shown as sent.", item);
  if (item.kind === "step") return stepItem(item);
  if (item.kind === "text" || item.kind === "reasoning") return textItem(item);
  return toolItem(item);
}

/**
 * An item's whole content as one text, for showing in windows: each field
 * labelled, strings as text and anything else as JSON, nothing left out.
 */
export function itemDocument(item) {
  const block = (value) => (typeof value === "string" ? value : json(value));
  const out = [];
  for (const [key, value] of Object.entries(item ?? {})) {
    if (key === "content" && Array.isArray(value)) {
      value.forEach((part, index) => out.push(`content[${index}]${part?.type === "text" ? " (text)" : ""}\n${part?.type === "text" && typeof part.text === "string" ? part.text : json(part)}`));
    } else out.push(`${key}\n${block(value)}`);
  }
  return out.join("\n\n");
}

/** Content split into windows that each fit WINDOW_BYTES and WINDOW_LINES. */
export const windowsOf = (text) => splitBounded(text, { maxBytes: WINDOW_BYTES, maxLines: WINDOW_LINES });

/**
 * How an item shows: field by field when its text fits one window, or else
 * as its text in windows, one mounted at a time. `windows` is null in the
 * first case; `bytes` is its text's UTF-8 length either way.
 */
export function formatItem(item) {
  const text = item.kind === "unreadable" ? json(item.raw) : itemDocument(item);
  const windows = windowsOf(text);
  return { windows: windows.length > 1 ? windows : null, bytes: utf8Length(text) };
}

/** A part read of an item too large to hold, in windows. */
export function formatPart(data) {
  return { windows: windowsOf(data), bytes: utf8Length(data) };
}

const clampIndex = (index, count) => Math.min(Math.max(0, index), Math.max(0, count - 1));

/** One window of `windows`, with First, Previous, Next and Last among them when there are several. */
function windowed(turnId, itemId, windows, index) {
  const at = clampIndex(index, windows.length);
  if (windows.length === 1) return [pre(windows[0], "act-part")];
  const go = (to, words, disabled) => h("button", { class: "btn", type: "button", "data-activity-window": to, "data-activity-turn": turnId, "data-activity-item": itemId, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
  const nav = h(
    "div",
    { class: "act-parts", role: "group", "aria-label": "Sections" },
    go("first", "First", at === 0),
    go("previous", "Previous", at === 0),
    h("span", { class: "act-sub" }, `Section ${at + 1} of ${windows.length}`),
    go("next", "Next", at === windows.length - 1),
    go("last", "Last", at === windows.length - 1),
  );
  return [nav, pre(windows[at], "act-part")];
}

/** An item's body from its format: field by field, or one window of its text. */
function formattedBody(turnId, item, format, window) {
  return format.windows ? [h("div", { class: "act-head" }, h("b", { class: item.kind === "tool" ? "mono" : "" }, txt(item.kind === "tool" ? (item.name ?? "Tool") : (KIND_NAMES[item.kind] ?? item.kind ?? "Item")))), windowed(turnId, item.id, format.windows, window)] : itemBody(item);
}

/** An item read part by part: the part's navigation, then one window of the part. */
function partsBody(turnId, item, full, format, window) {
  const go = (to, words, disabled) => h("button", { class: "btn", type: "button", "data-activity-part": to, "data-activity-turn": turnId, "data-activity-item": item.id, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
  const of = full.count === null ? `${full.known}+` : String(full.count);
  const nav = h(
    "div",
    { class: "act-parts", role: "group", "aria-label": "Parts" },
    go("first", "First part", !full.hasPrevious),
    go("previous", "Previous part", !full.hasPrevious),
    h("span", { class: "act-sub" }, `Part ${full.part + 1} of ${of}`),
    go("next", "Next part", !full.hasNext),
    go("last", "Last part", !full.hasLast),
  );
  return [nav, format ? windowed(turnId, item.id, format.windows, window) : null];
}

/**
 * A paged item: read on request, then shown whole (field by field or in
 * windows) or, when larger than the view's budget, a part at a time.
 * `full` is the watcher's entry; `format` its format, made by the caller.
 */
function pagedItem(turnId, item, full, format, window) {
  const what = item.kind === "tool" ? (item.name ?? "Tool") : (KIND_NAMES[item.kind] ?? item.kind);
  const pages = `${item.paged.pages} ${item.paged.pages === 1 ? "page" : "pages"}, ${formatBytes(item.paged.bytes)}`;
  const head = h("div", { class: "act-head" }, h("b", { class: item.kind === "tool" ? "mono" : "" }, txt(what)), item.callId ? h("span", { class: "act-sub mono" }, txt(item.callId)) : null, chip(item.status, { ...TOOL_STATUS, ...TEXT_STATUS }));
  const load = (words) => h("button", { class: "btn", type: "button", "data-activity-load": item.id, "data-activity-turn": turnId }, words);
  if (full?.mode === "parts") {
    const latest = full.state === "ready" && full.seq !== null && full.seq < item.seq;
    const note =
      full.state === "loading"
        ? full.data === null
          ? `Large item (${pages}), more than this view holds at once. Reading its first part…`
          : `Reading part ${full.want + 1}…`
        : full.newer
          ? "The item changed while it was read: this is the newer version, from its first part."
          : `Large item (${pages}), more than this view holds at once, so it shows a part at a time.`;
    return [
      head,
      h("p", { class: "act-note" }, note),
      full.data !== null ? partsBody(turnId, item, full, format, window) : null,
      latest ? [h("p", { class: "act-note" }, "A newer version is on the gateway."), load("Load the latest")] : null,
      full.state === "failed" ? [h("p", { class: "act-warn" }, "Could not read the part: ", txt(full.error)), load("Try again")] : null,
    ];
  }
  const shown = full?.item ?? null;
  if (!shown) {
    if (full?.state === "loading" || full?.state === "queued") return [head, h("p", { class: "act-note" }, `Large item (${pages}). Reading every page…`)];
    return [head, h("p", { class: "act-note" }, `Large item (${pages}), read on request and then shown in full.`), full?.state === "failed" ? h("p", { class: "act-warn" }, "Could not read it: ", txt(full.error)) : null, load(full?.state === "failed" ? "Try again" : "Load in full")];
  }
  const note =
    full.state === "loading" ? "Reading the newer version…" : full.state === "failed" ? null : full.seq < item.seq ? "A newer version is on the gateway." : `Read in full (${pages}).`;
  // A newer version is read only on request: a running tool changes often.
  const latest = full.state === "ready" && full.seq < item.seq;
  return [
    formattedBody(turnId, shown, format, window),
    note ? h("p", { class: "act-note" }, note) : null,
    latest ? load("Load the latest") : null,
    full.state === "failed" ? [h("p", { class: "act-warn" }, "Could not read the newer version: ", txt(full.error)), load("Try again")] : null,
  ];
}

/** What an item's body is made from: its own fields, the item read whole, or the part shown. */
function itemSource(item, full) {
  if (!item.paged) return { kind: "inline", source: item };
  if (full?.mode === "parts") return full.data === null ? null : { kind: "part", source: full.data };
  return full?.item ? { kind: "whole", source: full.item } : null;
}

/**
 * One item of a turn. `full` is the item's read when it is paged. `format`
 * makes (or recalls) the format of a source; `window` is the window shown.
 */
export function itemTree(turnId, item, full = null, { window = 0, format = null } = {}) {
  const from = itemSource(item, full);
  const made = from ? (format ?? ((at) => (at.kind === "part" ? formatPart(at.source) : formatItem(at.source))))(from) : null;
  return h("li", { class: `act-item k-${item.kind}` }, item.paged ? pagedItem(turnId, item, full, made, window) : formattedBody(turnId, item, made, window));
}

/** A turn's summary line. */
export function turnSummary(turn) {
  const usage = turn.usage ? ` · ${turn.usage.steps} ${turn.usage.steps === 1 ? "step" : "steps"} · ${tokensText(turn.usage.tokens)} · ${formatCost(turn.usage.cost)}` : "";
  const started = Number.isFinite(turn.startedAt);
  const ended = started && Number.isFinite(turn.endedAt) ? ` · took ${formatDuration(turn.endedAt - turn.startedAt)}` : "";
  return [
    h("span", { class: "act-turn-head" }, chip(turn.status, TURN_STATUS), h("span", { class: "mono" }, started ? `Started ${formatClock(turn.startedAt)}` : "Start not known"), started && turn.status === "running" ? h("span", { class: "act-sub" }, "for ", h("span", { "data-since": String(Math.round(turn.startedAt)) }, "")) : null),
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
  const tryAgain = () => (view.refreshing ? h("p", { class: "act-note" }, "Reading activity again…") : [view.retrying ? h("p", { class: "act-note" }, "Trying again shortly.") : null, h("button", { class: "btn", type: "button", "data-activity-retry": "" }, "Try again")]);
  if (view.status === "failed") messages.push(h("p", { class: "act-warn" }, "Could not read activity: ", txt(view.error)), tryAgain());
  if (view.status === "ready") {
    if (view.agent && !view.agent.collected) messages.push(h("p", { class: "act-warn" }, ACTIVITY_MESSAGES.notCollected));
    else if (!view.turns.length) messages.push(h("p", { class: "act-note" }, "No turns yet. A turn appears when the officer's input is submitted."));
    if (view.stale) messages.push(h("p", { class: "act-warn" }, "Could not refresh: ", txt(view.error), ". Showing the last activity read; newer changes appear once it is read again."), tryAgain());
    if (view.polling) messages.push(h("p", { class: "act-note" }, ACTIVITY_MESSAGES.polling));
    else if (!officer.live) messages.push(h("p", { class: "act-note" }, "Not live: the event stream is not connected, so this may be out of date."));
  }
  return h(
    "header",
    { class: "ihead act-ihead" },
    h("button", { class: "btn act-back", type: "button", "data-activity-close": "" }, "‹ Back"),
    h("p", { class: "eyebrow" }, `${role} · Activity`),
    h("h2", { tabindex: "-1" }, txt(officer.name)),
    h("p", { class: "meta mono" }, txt(view.agentId), view.agent?.backend ? [" · ", txt(view.agent.backend)] : null),
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

const turnVersion = (turn) => `${turn.seq}|${turn.status}|${turn.reconnects}|${turn.endedAt}|${turn.items.length === 0}`;

/** Put `element` at `index` among `parent`'s children, moving it only if needed (moving drops a selection). */
function place(parent, element, index) {
  const current = parent.children[index];
  if (current !== element) parent.insertBefore(element, current ?? null);
}

const isOpen = (details) => details.getAttribute("open") !== null;

/**
 * The live panel. `update(view, officer)` redraws what changed.
 *
 * What it mounts is bounded: a closed turn mounts no items; an open one
 * mounts at most ITEM_WINDOW items, each at most one window (WINDOW_BYTES
 * and WINDOW_LINES). The formats it keeps and the content it mounts are
 * charged to the view's budget (`view.budget`) beside the items read, so
 * the least recently opened goes first: a format is made again when next
 * drawn, and an open turn is closed.
 *
 * @param {{doc: Document, onClose?: () => void, onLoad?: (turn: string, item: string) => void, onPart?: (turn: string, item: string, to: string) => void, onRetry?: () => void}} options
 */
export function createActivityPanel({ doc, onClose, onLoad, onPart, onRetry }) {
  const root = doc.createElement("div");
  root.setAttribute("class", "activity");
  const header = doc.createElement("div");
  const list = doc.createElement("div");
  list.setAttribute("class", "act-turns");
  root.appendChild(header);
  root.appendChild(list);
  /** turn ID -> {element, summary, notes, nav, items, version, navKey, start, kids: Map(item ID -> {element, version, bytes})} */
  const turns = new Map();
  let headerKey = "";
  /** Formats made, by item: key -> {source, format}. Inline items' and parts' are charged to the budget; a whole read's is in its read's charge. */
  const formats = new Map();
  /** The window shown of each item, with the source it was chosen for: key -> {source, index}. */
  const windows = new Map();
  let last = null;
  const itemKey = (turn, item) => `${turn}\u0000${item}`;
  const budgetOf = () => last?.view.budget ?? null;

  function forgetFormat(key) {
    formats.delete(key);
    budgetOf()?.release(`fmt\u0000${key}`);
  }

  /** The format of an item's source, made once per source and charged to the budget. */
  const formatFor = (key) => (from) => {
    const held = formats.get(key);
    if (held?.source === from.source) {
      budgetOf()?.touch(`fmt\u0000${key}`);
      return held.format;
    }
    const format = from.kind === "part" ? formatPart(from.source) : formatItem(from.source);
    formats.set(key, { source: from.source, format });
    const budget = budgetOf();
    if (budget && from.kind !== "whole" && format.windows) {
      budget.reserve(`fmt\u0000${key}`, format.bytes, () => formats.delete(key), { force: true });
    } else budget?.release(`fmt\u0000${key}`);
    return format;
  };

  function windowAt(key, source) {
    const at = windows.get(key);
    return at?.source === source ? at.index : 0;
  }

  /** Unmount a turn's items and give back their charge. */
  function closeTurn(id, entry) {
    entry.items.replaceChildren();
    entry.kids.clear();
    budgetOf()?.release(`mnt\u0000${id}`);
  }

  function moveWindow(button) {
    const { activityTurn: turn, activityItem: item, activityWindow: to } = button.dataset;
    const key = itemKey(turn, item);
    const held = formats.get(key);
    if (!held?.format.windows) return;
    const count = held.format.windows.length;
    const at = windowAt(key, held.source);
    const next = { first: 0, previous: Math.max(0, at - 1), next: Math.min(count - 1, at + 1), last: count - 1 }[to] ?? at;
    if (next === at) return;
    windows.set(key, { source: held.source, index: next });
    redraw();
    const escape = globalThis.CSS?.escape ?? ((value) => value.replace(/["\\]/g, "\\$&"));
    root.querySelector?.(`[data-activity-window="${to}"][data-activity-item="${escape(item)}"]`)?.focus?.();
  }

  function moveItems(button) {
    const entry = turns.get(button.dataset.activityTurn);
    const turn = last?.view.turns.find((each) => each.id === button.dataset.activityTurn);
    if (!entry || !turn) return;
    const end = Math.max(0, turn.items.length - ITEM_WINDOW);
    const start = entry.start ?? end;
    const to = button.dataset.activityItems === "earlier" ? Math.max(0, start - ITEM_WINDOW) : Math.min(end, start + ITEM_WINDOW);
    // At the end, the window follows new items.
    entry.start = to >= end ? null : to;
    redraw();
  }

  const redraw = () => last && update(last.view, last.officer);

  root.addEventListener("click", (event) => {
    const target = event.target;
    const on = (name) => target.closest(`[${name}]`);
    if (on("data-activity-close")) onClose?.();
    else if (on("data-activity-retry")) onRetry?.();
    else if (on("data-activity-window")) moveWindow(on("data-activity-window"));
    else if (on("data-activity-items")) moveItems(on("data-activity-items"));
    else if (on("data-activity-part")) {
      const { activityTurn: turn, activityItem: item, activityPart: to } = on("data-activity-part").dataset;
      onPart?.(turn, item, to);
    } else {
      const load = on("data-activity-load");
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
    const nav = doc.createElement("div");
    const items = doc.createElement("ol");
    items.setAttribute("class", "act-items");
    element.appendChild(summary);
    element.appendChild(notes);
    element.appendChild(nav);
    element.appendChild(items);
    entry = { element, summary, notes, nav, items, version: "", navKey: "", start: null, kids: new Map() };
    // Opening a turn mounts its items; closing it unmounts them.
    element.addEventListener("toggle", () => redraw());
    turns.set(turn.id, entry);
    return entry;
  }

  /** The items window of a turn, and its Earlier and Later items buttons. */
  function itemsWindow(turn, entry) {
    const end = Math.max(0, turn.items.length - ITEM_WINDOW);
    const start = Math.min(entry.start ?? end, end);
    const navKey = `${start}|${turn.items.length}`;
    if (navKey !== entry.navKey) {
      const go = (to, words) => h("button", { class: "btn", type: "button", "data-activity-items": to, "data-activity-turn": turn.id }, words);
      const shown = turn.items.length > ITEM_WINDOW ? h("div", { class: "act-parts", role: "group", "aria-label": "Items" }, start > 0 ? go("earlier", "Earlier items") : null, h("span", { class: "act-sub" }, `Items ${start + 1}–${Math.min(turn.items.length, start + ITEM_WINDOW)} of ${turn.items.length}`), start < end ? go("later", "Later items") : null) : null;
      entry.nav.replaceChildren(...(shown ? [mount(shown, doc)] : []));
      entry.navKey = navKey;
    }
    return turn.items.slice(start, start + ITEM_WINDOW);
  }

  function drawItems(turn, entry, view) {
    const shown = itemsWindow(turn, entry);
    const ids = new Set(shown.map((item) => item.id));
    for (const [id, kid] of entry.kids) {
      if (ids.has(id)) continue;
      kid.element.remove();
      entry.kids.delete(id);
    }
    let mounted = 0;
    shown.forEach((item, at) => {
      const full = item.paged ? view.full(turn.id, item.id) : null;
      const key = itemKey(turn.id, item.id);
      const from = itemSource(item, full);
      const window = from ? windowAt(key, from.source) : 0;
      const version = `${item.seq}|${full ? `${full.state}|${full.seq}|${full.error ?? ""}|${full.part ?? ""}|${full.want ?? ""}|${full.newer ?? ""}|${full.data === null}` : ""}|${window}`;
      let kid = entry.kids.get(item.id);
      if (!kid || kid.version !== version) {
        let format = null;
        const tree = itemTree(turn.id, item, full, { window, format: (source) => (format = formatFor(key)(source)) });
        const element = mount(tree, doc);
        if (kid) kid.element.replaceWith(element);
        const bytes = format ? (format.windows ? utf8Length(format.windows[clampIndex(window, format.windows.length)]) : format.bytes) : 0;
        kid = { element, version, bytes };
        entry.kids.set(item.id, kid);
      }
      mounted += kid.bytes;
      place(entry.items, kid.element, at);
    });
    return mounted;
  }

  function update(view, officer) {
    last = { view, officer };
    const key = JSON.stringify([view.status, view.waiting, view.stale, view.refreshing, view.retrying, view.polling, view.error, view.agent, view.agentId, view.turns.length === 0, officer.name, officer.role, officer.live]);
    if (key !== headerKey) {
      header.replaceChildren(mount(headerTree(view, officer), doc));
      headerKey = key;
    }
    const shown = view.status === "unavailable" || (view.status === "failed" && !view.turns.length) ? [] : view.turns;
    const keep = new Set(shown.map((turn) => turn.id));
    for (const [id, entry] of turns) {
      if (keep.has(id)) continue;
      closeTurn(id, entry);
      entry.element.remove();
      turns.delete(id);
    }
    // Forget the formats and windows of items no longer shown.
    const items = new Set(shown.flatMap((turn) => turn.items.map((item) => itemKey(turn.id, item.id))));
    for (const held of [...formats.keys()]) if (!items.has(held)) forgetFormat(held);
    for (const held of [...windows.keys()]) if (!items.has(held)) windows.delete(held);
    shown.forEach((turn, index) => {
      const entry = turnEntry(turn);
      const version = turnVersion(turn);
      if (version !== entry.version) {
        entry.summary.replaceChildren(...turnSummary(turn).map((node) => mount(node, doc)));
        entry.notes.replaceChildren(...turnNotes(turn).filter(Boolean).map((node) => mount(node, doc)));
        entry.version = version;
      }
      if (!isOpen(entry.element)) {
        // A closed turn builds no item bodies.
        if (entry.kids.size) closeTurn(turn.id, entry);
        entry.nav.replaceChildren();
        entry.navKey = "";
      } else {
        const mounted = drawItems(turn, entry, view);
        // Charged as mounted; freed by closing the turn when least recently opened.
        view.budget?.reserve(`mnt\u0000${turn.id}`, mounted, () => {
          const evicted = turns.get(turn.id);
          if (!evicted) return;
          evicted.element.removeAttribute("open");
          closeTurn(turn.id, evicted);
          evicted.nav.replaceChildren();
          evicted.navKey = "";
        }, { force: true });
      }
      place(list, entry.element, index);
    });
  }

  return { root, update };
}
