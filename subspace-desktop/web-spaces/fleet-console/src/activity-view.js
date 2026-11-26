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
//
// Sizes are what shows (SUB decision 0c13b784 §1): the UTF-8 bytes of the
// text after each hidden character becomes its 12-byte marker. A body is
// measured once per version, as numbers only (`document.js`), and only the
// window drawn is ever built as text (§2).
//
// What draws is planned before anything is built and charged to the view's
// budget beside the inline snapshot and the items read (`planBodies`, SUB
// decisions ed71a299, 6f5e67dd, 0c13b784 §3–4). A body with no room shows
// its size and Show; no turn is closed to make room. The budget alone
// decides what may pass its limit: the body last asked for with Show is the
// floor only while it is the budget's latest request (SUB decision
// 1c19f413), so a Load or another part asked for after it sends it back to
// its caption when there is no room.

import { ACTIVITY_MESSAGES, isWellFormedItem } from "./activity.js";
import { documentPieces, HIDDEN, isRecord, marker, measurePieces, partBlocks, restOf, shownJsonPieces, sliceOfPieces, sourcePieces, stepBlocks, WINDOW_BYTES, WINDOW_LINES } from "./document.js";
import { formatBytes, formatClock, formatDuration } from "./format.js";

export { marker, WINDOW_BYTES, WINDOW_LINES };
/** The most items an open turn mounts at once; Earlier items and Later items move among the rest. */
export const ITEM_WINDOW = 20;

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

/**
 * Indented JSON in which a hidden character in any string or key stays the
 * character itself, so the view marks it `⟨U+XXXX⟩` as everywhere else; a
 * typed backslash stays escaped (`\\u0000`) and so stays distinguishable.
 * Newlines and tabs keep their escapes, which JSON needs to read as one
 * value. The document's formatter (`shownJsonPieces`), so the short view
 * and the windows show JSON alike.
 */
export const json = (value) => [...shownJsonPieces(value)].join("");

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

/** The fields a friendly view left out of `object`, labelled: nothing an item carries is hidden (SUB-70). */
function otherFields(object, keys, title = "Other fields") {
  const rest = restOf(object, keys);
  return rest ? [label(title), ...fields(rest)] : null;
}

const ERROR_FIELDS = ["type", "message", "status", "retryAfterMs", "responseBody"];

function errorBlock(error) {
  if (!error) return null;
  if (typeof error !== "object" || Array.isArray(error)) return [label("Error"), valueBlock(error)];
  return h(
    "div",
    { class: "act-error" },
    label("Error"),
    pre(`${error.type ?? "Error"}: ${error.message ?? ""}`),
    Number.isFinite(error.status) ? h("p", { class: "act-note" }, `HTTP status ${error.status}`) : null,
    Number.isFinite(error.retryAfterMs) ? h("p", { class: "act-note" }, `Retry after ${formatDuration(error.retryAfterMs)}`) : null,
    error.responseBody !== null && error.responseBody !== undefined ? [label("Response body"), typeof error.responseBody === "string" ? pre(error.responseBody) : valueBlock(error.responseBody)] : null,
    otherFields(error, ERROR_FIELDS, "Other error fields"),
  );
}

function contentBlock(content) {
  if (content === null || content === undefined) return null;
  if (!Array.isArray(content)) return [label("Output"), valueBlock(content)];
  if (!content.length) return [label("Output"), h("p", { class: "act-note" }, "(no output)")];
  // The blocks the item's document has (`partBlocks`): a text part's text after its other fields; any other part as JSON.
  return [label("Output"), content.map((part, index) => partBlocks(part, index).map((block) => (block.text !== undefined ? pre(block.text) : valueBlock(block.json))))];
}

/** The fields every item has, shown in its row rather than its body. */
const ROW_FIELDS = ["id", "seq", "kind", "startedAt", "updatedAt"];
/** The fields each kind's friendly view shows; the rest show as Other fields. */
const SHOWN_FIELDS = {
  step: [...ROW_FIELDS, "status", "model", "endedAt", "finish", "tokens", "cost", "error"],
  text: [...ROW_FIELDS, "status", "text"],
  reasoning: [...ROW_FIELDS, "status", "text"],
  tool: [...ROW_FIELDS, "status", "name", "callId", "executed", "input", "rawInput", "metadata", "content", "error"],
};

/** What a step's short view leaves out of its model and tokens (`stepBlocks`), each labelled. */
const stepRest = (item) => stepBlocks(item).map((block) => [label(block.label), ...(isRecord(block.json) ? fields(block.json) : [valueBlock(block.json)])]);

function stepItem(item) {
  const model = isRecord(item.model) ? `${item.model.provider}/${item.model.id}${item.model.variant ? `#${item.model.variant}` : ""}` : "Model not reported";
  const took = Number.isFinite(item.endedAt) ? ` · ${formatDuration(item.endedAt - item.startedAt)}` : "";
  return [
    h("div", { class: "act-head" }, h("b", {}, "Step"), h("span", { class: "act-sub mono" }, txt(model)), chip(item.status, STEP_STATUS)),
    h("p", { class: "act-note" }, `Started ${formatClock(item.startedAt)}${took}${item.finish ? ` · finish: ` : ""}`, item.finish ? txt(item.finish) : null),
    item.tokens ? h("p", { class: "act-note mono" }, `${tokensText(item.tokens)} · ${formatCost(item.cost)}`) : h("p", { class: "act-note" }, item.status === "running" ? "Tokens settle when the step ends." : "No token use reported."),
    stepRest(item),
    errorBlock(item.error),
    otherFields(item, SHOWN_FIELDS.step),
  ];
}

function textItem(item) {
  return [
    h("div", { class: "act-head" }, h("b", {}, item.kind === "reasoning" ? "Reasoning" : "Reply"), chip(item.status, TEXT_STATUS)),
    pre(item.text ?? "", item.kind === "reasoning" ? "reasoning" : "reply"),
    otherFields(item, SHOWN_FIELDS[item.kind]),
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
    otherFields(item, SHOWN_FIELDS.tool),
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
 * An item's whole content as one text: each field labelled, strings as text
 * and anything else as JSON, nothing left out. The panel never builds it;
 * it measures and draws its pieces a window at a time.
 */
export const itemDocument = (item) => [...documentPieces(item)].join("");

/**
 * How a body measures: `bounds` are its windows' starts (UTF-16 units) with
 * its end last, `sizes` each window's bytes as shown, `bytes` the total as
 * shown. Made from pieces, so the body's text is never built whole.
 *
 * @param {{kind: "inline"|"whole", item: object}|{kind: "part", data: string}} source
 */
export const measureSource = (source) => measurePieces(sourcePieces(source));

/** The number of windows a measure splits a body into. */
const windowCount = (measure) => measure.bounds.length - 1;

/** Window `index` of a body, built alone. */
const windowText = (source, measure, index) => sliceOfPieces(sourcePieces(source), measure.bounds[index], measure.bounds[index + 1]);

/**
 * How an item shows: field by field when it fits one window, or else in
 * windows, one drawn at a time. `windows` (null in the first case) builds
 * every window, so it is for tests and tools: the panel draws one window
 * and builds no other. `bytes` is its size as shown.
 */
export function formatItem(item) {
  const source = { kind: "inline", item };
  const measure = measureSource(source);
  const count = windowCount(measure);
  return { windows: count > 1 ? Array.from({ length: count }, (_, index) => windowText(source, measure, index)) : null, bytes: measure.bytes };
}

const clampIndex = (index, count) => Math.min(Math.max(0, index), Math.max(0, count - 1));

/**
 * A body held back to keep the view within its budget: a caption with its
 * size as shown, and Show, which asks for it (SUB decision ed71a299).
 */
function heldBack(turnId, itemId, measure) {
  return [
    h("p", { class: "act-note" }, `Not drawn, to stay within the view's memory budget (${count(measure.bytes)} bytes).`),
    h("button", { class: "btn", type: "button", "data-activity-show": itemId, "data-activity-turn": turnId }, "Show"),
  ];
}

/**
 * One window of a body, with First, Previous, Next and Last among them when
 * there are several; or its caption when it is held back.
 * `body` is {source, measure, window, held}.
 */
function windowed(turnId, itemId, body) {
  const { source, measure } = body;
  if (body.held) return heldBack(turnId, itemId, measure);
  const count = windowCount(measure);
  const at = clampIndex(body.window, count);
  const text = windowText(source, measure, at);
  if (count === 1) return [pre(text, "act-part")];
  const go = (to, words, disabled) => h("button", { class: "btn", type: "button", "data-activity-window": to, "data-activity-turn": turnId, "data-activity-item": itemId, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
  const nav = h(
    "div",
    { class: "act-parts", role: "group", "aria-label": "Sections" },
    go("first", "First", at === 0),
    go("previous", "Previous", at === 0),
    h("span", { class: "act-sub" }, `Section ${at + 1} of ${count}`),
    go("next", "Next", at === count - 1),
    go("last", "Last", at === count - 1),
  );
  return [nav, pre(text, "act-part")];
}

/** An item's body from its measure: field by field when it fits one window, or else one window of its text; its caption when held back. */
function formattedBody(turnId, item, body) {
  if (windowCount(body.measure) < 2 && !body.held) return itemBody(item);
  return [h("div", { class: "act-head" }, h("b", { class: item.kind === "tool" ? "mono" : "" }, txt(item.kind === "tool" ? (item.name ?? "Tool") : (KIND_NAMES[item.kind] ?? item.kind ?? "Item")))), windowed(turnId, item.id, body)];
}

/** An item read part by part: the part's navigation, then one window of the part. */
function partsBody(turnId, item, full, body) {
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
  return [nav, body ? windowed(turnId, item.id, body) : null];
}

/**
 * A paged item: read on request, then shown whole (field by field or in
 * windows) or, when larger than the view's budget, a part at a time.
 * `full` is the watcher's entry; `body` what its body is drawn from, or
 * null when there is none. Built in three segments, each a function: its
 * head and status, its body, and its notes and actions. The panel keys the
 * body by the read's revision and the others by the placeholder's, so a
 * new status, size or seq redraws them and leaves the body (and any text
 * selected in it) alone (SUB decision 1c19f413 §3).
 */
function pagedSegments(turnId, item, full, body) {
  const what = item.kind === "tool" ? (item.name ?? "Tool") : (KIND_NAMES[item.kind] ?? item.kind);
  const pages = `${item.paged.pages} ${item.paged.pages === 1 ? "page" : "pages"}, ${formatBytes(item.paged.bytes)}`;
  const head = () => h("div", { class: "act-head" }, h("b", { class: item.kind === "tool" ? "mono" : "" }, txt(what)), item.callId ? h("span", { class: "act-sub mono" }, txt(item.callId)) : null, chip(item.status, { ...TOOL_STATUS, ...TEXT_STATUS }));
  const load = (words) => h("button", { class: "btn", type: "button", "data-activity-load": item.id, "data-activity-turn": turnId }, words);
  if (full?.mode === "parts") {
    const latest = full.state === "ready" && full.seq !== null && full.seq < item.seq;
    const note =
      full.state === "loading"
        ? full.data === null
          ? full.replacing
            ? `Large item (${pages}), no longer held whole beside the rest, so it shows a part at a time. Reading part ${full.want + 1}; the window below stays on show until it arrives…`
            : `Large item (${pages}), more than this view holds at once. Reading its first part…`
          : full.reloading
            ? "Reading the latest version. This part stays on show until it arrives…"
            : `Reading part ${full.want + 1}…`
        : full.newer
          ? "The item changed while it was read: this is the newer version, from its first part."
          : `Large item (${pages}), more than this view holds at once, so it shows a part at a time.`;
    return {
      head: () => [head(), h("p", { class: "act-note" }, note)],
      body: () => (full.data !== null ? partsBody(turnId, item, full, body) : null),
      foot: () => [
        latest ? [h("p", { class: "act-note" }, "A newer version is on the gateway."), load("Load the latest")] : null,
        full.state === "failed" ? [h("p", { class: "act-warn" }, "Could not read the part: ", txt(full.error)), load("Try again")] : null,
      ],
    };
  }
  const shown = full?.item ?? null;
  if (!shown) {
    const reading = full?.state === "loading" || full?.state === "queued";
    return {
      head: () => [head(), h("p", { class: "act-note" }, reading ? `Large item (${pages}). Reading every page…` : `Large item (${pages}), read on request and then shown in full.`)],
      body: () => null,
      foot: () => (reading ? null : [full?.state === "failed" ? h("p", { class: "act-warn" }, "Could not read it: ", txt(full.error)) : null, load(full?.state === "failed" ? "Try again" : "Load in full")]),
    };
  }
  // The head is the placeholder's, live: its status and size, and what the
  // body below is, the earlier version once the item changed (SUB decision
  // 533c2b29). It is keyed apart from the body, which stays as read.
  const earlier = full.seq < item.seq;
  const note = earlier ? `A newer version is on the gateway. Below is the earlier version, as read; the item is now ${pages}.` : `Read in full (${pages}).`;
  // A newer version is read only on request: a running tool changes often.
  const latest = full.state === "ready" && earlier;
  return {
    head: () => [head(), h("p", { class: "act-note" }, note)],
    body: () => formattedBody(turnId, shown, body),
    foot: () => [
      full.state === "loading" ? h("p", { class: "act-note" }, "Reading the newer version…") : null,
      latest ? load("Load the latest") : null,
      full.state === "failed" ? [h("p", { class: "act-warn" }, "Could not read the newer version: ", txt(full.error)), load("Try again")] : null,
    ],
  };
}

/**
 * An item's three segments (`pagedSegments`): an item shown from its own
 * fields is all body.
 */
export function itemSegments(turnId, item, full, body) {
  if (item.paged) return pagedSegments(turnId, item, full, body);
  return { head: () => null, body: () => formattedBody(turnId, item, body), foot: () => null };
}

/**
 * What each of an item's segments is drawn from, as keys: the head and
 * foot from the placeholder's revision (its seq, status and size) and the
 * read's state; the body from the version of its source, the window shown,
 * whether it is held back and the part's place among the parts.
 */
export function segmentKeys(item, full, bodyVersion, window, held) {
  const read = full ? [full.mode, full.state, full.seq, full.error ?? "", full.want ?? "", full.newer ?? "", full.reloading ?? "", full.data === null, full.item ? 1 : 0] : [];
  const chrome = JSON.stringify([item.seq, item.status, item.kind, item.name ?? "", item.callId ?? "", item.paged?.pages ?? "", item.paged?.bytes ?? "", ...read]);
  const parts = full?.mode === "parts" ? [full.part, full.count, full.known, full.hasPrevious, full.hasNext, full.hasLast] : [];
  return { head: chrome, foot: chrome, body: JSON.stringify([bodyVersion, window, held, ...parts]) };
}

/** What an item's body is made from: its own fields, the item read whole, or the part shown; null when there is nothing to draw yet. */
export function itemSource(item, full) {
  if (!item.paged) return { kind: "inline", item };
  if (full?.mode === "parts") return full.data === null ? null : { kind: "part", data: full.data };
  return full?.item ? { kind: "whole", item: full.item } : null;
}

/**
 * One item of a turn. `full` is the item's read when it is paged. `measure`
 * is the measure of its source (`measureSource`), which the panel keeps per
 * version; `window` is the window shown; `held` draws the body's caption
 * and Show instead of the body.
 */
export function itemTree(turnId, item, full = null, { window = 0, measure = null, held = false } = {}) {
  const source = itemSource(item, full);
  const body = source ? { source, measure: measure ?? measureSource(source), window, held } : null;
  const segments = itemSegments(turnId, item, full, body);
  return h("li", { class: `act-item k-${item.kind}` }, segments.head(), segments.body(), segments.foot());
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
 * The version of a body's source, which keys its measure and window: the
 * snapshot's generation with the item's seq, the read's seq and length, or
 * the part's place in its version. Never the source itself, so nothing kept
 * holds a body that was evicted or replaced.
 */
export function sourceVersion(source, item, full, view) {
  const generation = view.cursor?.generation ?? "";
  if (source.kind === "inline") return `i|${generation}|${item.seq}`;
  if (source.kind === "whole") return `w|${generation}|${full.seq}|${full.bytes}`;
  return `p|${generation}|${full.seq}|${full.version?.bytes}|${full.version?.pages}|${full.part}|${full.newer}`;
}

/** The bytes a body draws, as shown: the window drawn, or the whole body when it fits one. */
const drawnBytes = (measure, window) => measure.sizes[clampIndex(window, measure.sizes.length)];

/** What a measure holds between frames: its numbers (§2). */
const measureHeld = (measure) => 8 * (measure.bounds.length + measure.sizes.length);

/**
 * What a read already reserved for its body's drawn copy. An item read
 * whole holds its length twice, the held copy and the drawn one (SUB
 * decision 6f5e67dd). A part's read reserved its drawn window in shown
 * bytes, markers at 12 bytes (SUB decisions 8357ec55, 1c19f413 §2): the
 * window it draws is never larger, however short the part or however many
 * markers it shows, so it costs nothing more. An inline body reserved none.
 */
function allowanceOf(source, full) {
  if (source.kind === "whole") return full.bytes ?? 0;
  if (source.kind === "part") return Infinity;
  return 0;
}

/** What drawing a body costs the budget beyond its read's allowance: the bytes it draws as shown, and its measure. */
const drawCost = (measure, window, allowance) => Math.max(0, drawnBytes(measure, window) + measureHeld(measure) - allowance);

/**
 * Which bodies draw (SUB decisions ed71a299, 6f5e67dd, 0c13b784): those
 * asked for with Show first, the latest first; then those of turns opened
 * by hand, the turn opened last first; then those of turns open by
 * default (the running turn), newest first; within a turn, top to bottom.
 * Each draws while its cost fits what is left of `room`; the rest are held
 * back, to show a caption and Show. A body whose read reserved its drawn
 * window (an item read whole, a part) costs nothing more and always draws.
 * The floor body (the budget's latest request) is not planned here: it is
 * charged on its own, under its key, and `room` is what it leaves.
 *
 * @param {Array<{key: string, cost: number, asked: number, group: number, turnRank: number, index: number}>} bodies
 * @param {number} room
 * @returns {{drawn: Set<string>, cost: number}}
 */
export function planBodies(bodies, room) {
  const ranked = [...bodies].sort((a, b) => b.asked - a.asked || a.group - b.group || a.turnRank - b.turnRank || a.index - b.index);
  const drawn = new Set();
  let left = room;
  let cost = 0;
  for (const body of ranked) {
    // A body its read already reserved for costs nothing more, and draws.
    if (body.cost > 0 && body.cost > left) continue;
    drawn.add(body.key);
    left -= body.cost;
    cost += body.cost;
  }
  return { drawn, cost };
}

/** The panel's key in the view's budget: what it draws, fitted to the room each frame. */
export const DRAW_KEY = "draw";

/**
 * The live panel. `update(view, officer)` redraws what changed.
 *
 * What it mounts is bounded: a closed turn builds no bodies; an open one
 * mounts at most ITEM_WINDOW items, each at most one window (WINDOW_BYTES
 * and WINDOW_LINES as shown). Every body is measured before it is built,
 * and what the panel draws is charged to the view's budget (`view.budget`)
 * beside the inline snapshot and the items read, fitted each frame to the
 * room they leave (`planBodies`). A body that does not fit is not drawn:
 * it shows its size and Show, never closing its turn. Show asks for it:
 * it becomes the budget's latest request, charged under its own key, which
 * frees items read, the least recently opened first, then the other bodies
 * drawn; it draws, a window at a time, whatever room is left. A later
 * request (another Show, a Load, another part) ends its claim. A read may
 * free the panel's charge; the next frame fits it again.
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
  /** turn ID -> {element, summary, notes, nav, items, version, navKey, start, defaultOpen, kids: Map(item ID -> {element, kind, head, body, foot})}, each segment {element, key} */
  const turns = new Map();
  let headerKey = "";
  /**
   * Bodies measured, by item: key -> {version, bounds, sizes, bytes}. Only
   * numbers, never the body's text or a reference to it, made once per
   * version (`sourceVersion`) and dropped as soon as the version shown
   * changes: a new seq, a read evicted or replaced, another part (§2).
   */
  const measures = new Map();
  /** The window shown of each item, for the version it was chosen in: key -> {version, index}. */
  const windows = new Map();
  /** Bodies asked for with Show: key -> when, on the `opened` clock. */
  const asked = new Map();
  /** Turns opened by hand: turn ID -> when, on the `opened` clock. */
  const turnOpened = new Map();
  /** Ticks on each Show and each turn opened by hand: later is larger. */
  let opened = 0;
  /** The key the floor body is charged under in the budget (the latest request, shown), or null. */
  let floorKey = null;
  let last = null;
  /** Set once the view closed: nothing draws again. */
  let disposed = false;
  const itemKey = (turn, item) => `${turn}\u0000${item}`;
  const budgetOf = () => last?.view.budget ?? null;
  /** How many bodies have been measured: a body is measured once per version, never on each draw. */
  let measured = 0;

  /** The measure of a body's version, made once. */
  function measureFor(key, version, source) {
    const held = measures.get(key);
    if (held?.version === version) return held;
    measured += 1;
    const made = { version, ...measureSource(source) };
    measures.set(key, made);
    return made;
  }

  function windowAt(key, version) {
    const at = windows.get(key);
    return at?.version === version ? at.index : 0;
  }

  /** Unmount a turn's items. */
  function closeTurn(entry) {
    entry.items.replaceChildren();
    entry.kids.clear();
    entry.nav.replaceChildren();
    entry.navKey = "";
  }

  function moveWindow(button) {
    const { activityTurn: turn, activityItem: item, activityWindow: to } = button.dataset;
    const key = itemKey(turn, item);
    const held = measures.get(key);
    if (!held || windowCount(held) < 2) return;
    const count = windowCount(held);
    const at = windowAt(key, held.version);
    const next = { first: 0, previous: Math.max(0, at - 1), next: Math.min(count - 1, at + 1), last: count - 1 }[to] ?? at;
    if (next === at) return;
    windows.set(key, { version: held.version, index: next });
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

  /** Show: ask for a body held back. It is the budget's latest request, so the next frame charges it as the floor. */
  function ask(turnId, itemId) {
    const key = itemKey(turnId, itemId);
    opened += 1;
    asked.set(key, opened);
    budgetOf()?.request(key);
    redraw();
  }

  /**
   * Charge the floor body: the body asked for with Show, while it is still
   * the budget's latest request. Charged under its own key, which the budget
   * holds even past its limit, freeing items read and the other bodies drawn
   * first (SUB decisions 0c13b784 §3, 1c19f413). A body that is no longer the
   * latest request gives up that charge and is planned with the rest.
   */
  function chargeFloor(budget, bodies) {
    const key = budget?.requested ?? null;
    const floor = key === null ? null : (bodies.find((body) => body.key === key && body.source?.kind === "inline") ?? null);
    if (floorKey !== null && floorKey !== floor?.key) budget?.release(floorKey);
    floorKey = floor ? floor.key : null;
    if (floor) budget.reserve(floor.key, floor.cost, null);
    return floor;
  }

  const redraw = () => last && update(last.view, last.officer);

  /**
   * Forget the measures, windows and asks of bodies whose version shown
   * changed or that are gone: an item no longer shown, a read evicted or
   * replaced, another part. Nothing kept outlives the version it was made for.
   */
  function forgetStale(shown, view) {
    const live = new Map();
    for (const turn of shown) {
      for (const item of turn.items) {
        const full = item.paged ? view.full(turn.id, item.id) : null;
        const source = itemSource(item, full);
        if (source) live.set(itemKey(turn.id, item.id), sourceVersion(source, item, full, view));
      }
    }
    for (const [key, held] of [...measures]) if (live.get(key) !== held.version) measures.delete(key);
    for (const [key, held] of [...windows]) if (live.get(key) !== held.version) windows.delete(key);
    for (const key of [...asked.keys()]) if (!live.has(key)) asked.delete(key);
  }

  root.addEventListener("click", (event) => {
    const target = event.target;
    const on = (name) => target.closest(`[${name}]`);
    if (on("data-activity-close")) onClose?.();
    else if (on("data-activity-retry")) onRetry?.();
    else if (on("data-activity-window")) moveWindow(on("data-activity-window"));
    else if (on("data-activity-items")) moveItems(on("data-activity-items"));
    else if (on("data-activity-show")) {
      const { activityTurn: turn, activityShow: item } = on("data-activity-show").dataset;
      ask(turn, item);
    } else if (on("data-activity-part")) {
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
    // The running turn opens by default; ended ones open on request.
    const defaultOpen = turn.status === "running";
    if (defaultOpen) element.setAttribute("open", "");
    const summary = doc.createElement("summary");
    const notes = doc.createElement("div");
    const nav = doc.createElement("div");
    const items = doc.createElement("ol");
    items.setAttribute("class", "act-items");
    element.appendChild(summary);
    element.appendChild(notes);
    element.appendChild(nav);
    element.appendChild(items);
    entry = { element, summary, notes, nav, items, version: "", navKey: "", start: null, defaultOpen, kids: new Map() };
    // Opening a turn draws its items; closing it unmounts them. Opening by
    // hand is timed, as its bodies draw before those of turns open by
    // default; the toggle the default opening fires is not opening by hand.
    element.addEventListener("toggle", () => {
      if (isOpen(element)) {
        if (!entry.defaultOpen && !turnOpened.has(turn.id)) turnOpened.set(turn.id, ++opened);
      } else {
        entry.defaultOpen = false;
        turnOpened.delete(turn.id);
        for (const key of [...asked.keys()]) if (key.startsWith(`${turn.id}\u0000`)) asked.delete(key);
      }
      redraw();
    });
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

  /** An open turn's bodies, each measured (once per version) and costed, before anything is built. */
  function bodiesOf(turn, entry, view) {
    return itemsWindow(turn, entry).map((item, index) => {
      const full = item.paged ? view.full(turn.id, item.id) : null;
      const key = itemKey(turn.id, item.id);
      const source = itemSource(item, full);
      if (!source) return { item, full, key, index, source: null, version: "", window: 0, measure: null, cost: 0 };
      const version = sourceVersion(source, item, full, view);
      const measure = measureFor(key, version, source);
      const window = clampIndex(windowAt(key, version), windowCount(measure));
      return { item, full, key, index, source, version, window, measure, cost: drawCost(measure, window, allowanceOf(source, full)) };
    });
  }

  /** Mount an open turn's items as planned: a body drawn, or held back with its caption. */
  function drawItems(turn, entry, bodies, drawn) {
    const ids = new Set(bodies.map((body) => body.item.id));
    for (const [id, kid] of entry.kids) {
      if (ids.has(id)) continue;
      kid.element.remove();
      entry.kids.delete(id);
    }
    for (const { item, full, key, index, source, measure, version: bodyVersion, window } of bodies) {
      const held = measure !== null && !drawn.has(key);
      const keys = segmentKeys(item, full, bodyVersion, window, held);
      let kid = entry.kids.get(item.id);
      if (!kid) {
        const element = doc.createElement("li");
        const segment = () => {
          const each = doc.createElement("div");
          each.setAttribute("class", "act-seg");
          element.appendChild(each);
          return { element: each, key: null };
        };
        kid = { element, kind: "", head: segment(), body: segment(), foot: segment() };
        entry.kids.set(item.id, kid);
      }
      if (kid.kind !== item.kind) {
        kid.element.setAttribute("class", `act-item k-${item.kind}`);
        kid.kind = item.kind;
      }
      // Each segment is rebuilt only when what it is drawn from changed. An
      // item read whole that now reads in parts keeps its window on show
      // until the part arrives (SUB decision 533c2b29).
      const keepBody = full?.replacing === true && kid.body.key !== null;
      let segments = null;
      for (const name of ["head", "body", "foot"]) {
        const segment = kid[name];
        if (segment.key === keys[name] || (name === "body" && keepBody)) continue;
        segments ??= itemSegments(turn.id, item, full, source ? { source, measure, window, held } : null);
        const tree = segments[name]();
        segment.element.replaceChildren(...h("div", {}, tree).children.map((node) => mount(node, doc)));
        segment.key = keys[name];
      }
      place(entry.items, kid.element, index);
    }
  }

  function update(view, officer) {
    if (disposed) return;
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
      closeTurn(entry);
      entry.element.remove();
      turns.delete(id);
      turnOpened.delete(id);
    }
    forgetStale(shown, view);
    // Plan before anything is built: measure and cost each body of each open turn.
    const open = [];
    shown.forEach((turn, index) => {
      const entry = turnEntry(turn);
      const version = turnVersion(turn);
      if (version !== entry.version) {
        entry.summary.replaceChildren(...turnSummary(turn).map((node) => mount(node, doc)));
        entry.notes.replaceChildren(...turnNotes(turn).filter(Boolean).map((node) => mount(node, doc)));
        entry.version = version;
      }
      // A closed turn builds no item bodies.
      if (!isOpen(entry.element)) {
        if (entry.kids.size || entry.navKey) closeTurn(entry);
      } else open.push({ turn, entry, index, bodies: bodiesOf(turn, entry, view) });
      place(list, entry.element, index);
    });
    const budget = view.budget ?? null;
    // The floor body first, under its own key: the rest are fitted to the room it leaves.
    budget?.release(DRAW_KEY);
    const floor = chargeFloor(
      budget,
      open.flatMap(({ bodies }) => bodies.filter((body) => body.measure)),
    );
    // Charging it may have freed items read: plan from what is left.
    if (floor) {
      forgetStale(shown, view);
      for (const each of open) each.bodies = bodiesOf(each.turn, each.entry, view);
    }
    const ranked = [];
    for (const { turn, index, bodies } of open) {
      const byHand = turnOpened.get(turn.id);
      for (const body of bodies) {
        if (body.measure && body.key !== floor?.key) ranked.push({ key: body.key, cost: body.cost, asked: asked.get(body.key) ?? 0, group: byHand === undefined ? 1 : 0, turnRank: byHand === undefined ? index : -byHand, index: body.index });
      }
    }
    const room = budget ? budget.limit - budget.used : Infinity;
    const plan = planBodies(ranked, room);
    if (floor) plan.drawn.add(floor.key);
    for (const { turn, entry, bodies } of open) {
      drawItems(turn, entry, bodies, plan.drawn);
      // Where each item read whole shows, so it reads in parts from there should it no longer fit.
      for (const { item, source, measure, window, key } of bodies) {
        if (source?.kind === "whole") view.place?.(turn.id, item.id, window / windowCount(measure), plan.drawn.has(key) ? drawnBytes(measure, window) : 0);
      }
    }
    // Fitted to the room above, so it frees nothing and always fits.
    if (plan.cost > 0) budget?.reserve(DRAW_KEY, plan.cost, null, { makeRoom: false });
  }

  /** The view closed: unmount what it drew and forget what it kept; it never draws again. */
  function dispose() {
    disposed = true;
    for (const entry of turns.values()) closeTurn(entry);
    turns.clear();
    list.replaceChildren();
    measures.clear();
    windows.clear();
    asked.clear();
    turnOpened.clear();
    last = null;
  }

  return {
    root,
    update,
    dispose,
    /** What the panel keeps between frames, for tests: the versions measured, the windows chosen and how many measures were made. */
    held: () => ({ measures: [...measures].map(([key, held]) => [key, held.version]), windows: [...windows].map(([key, held]) => [key, held.version, held.index]), measured }),
  };
}
