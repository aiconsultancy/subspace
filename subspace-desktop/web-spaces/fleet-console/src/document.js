// An item's text as the Activity view shows it, made as a stream of small
// pieces so it can be measured and drawn a window at a time without ever
// building the whole text (SUB decision 0c13b784 §1–§3). What is measured
// is what shows: the UTF-8 bytes of the text after each hidden character
// becomes its `⟨U+XXXX⟩` marker. A window holds at most WINDOW_BYTES of
// those bytes and WINDOW_LINES lines, and splits only between characters,
// so a marker is never split.
//
// The pieces are those of `json()` (indented JSON in which hidden
// characters are the characters themselves) and of an item's document
// (each field labelled, strings as text, the rest as that JSON). Long
// strings are passed through as slices, never copied whole.

/** The most one mounted window holds: UTF-8 bytes as shown, and lines. */
export const WINDOW_BYTES = 64 * 1024;
export const WINDOW_LINES = 1000;

/**
 * Control characters other than newline and tab, DEL and C1 controls, the
 * bidi marks, embeddings, overrides and isolates, the line and paragraph
 * separators and the byte order mark. Each shows as a marker.
 */
export const HIDDEN = /[\u0000-\u0008\u000B-\u001F\u007F-\u009F\u061C\u200E\u200F\u2028-\u202E\u2066-\u2069\uFEFF]+/g;

/** Whether the UTF-16 unit `u` is one HIDDEN marks (all are single units). */
export const isHidden = (u) => u <= 0x08 || (u >= 0x0b && u <= 0x1f) || (u >= 0x7f && u <= 0x9f) || u === 0x61c || u === 0x200e || u === 0x200f || (u >= 0x2028 && u <= 0x202e) || (u >= 0x2066 && u <= 0x2069) || u === 0xfeff;

export const marker = (char) => `⟨U+${char.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}⟩`;

/** A marker's UTF-8 length: ⟨ and ⟩ are three bytes each, `U+` and four hex digits one each. */
export const MARKER_BYTES = 12;

const hex4 = (u) => `\\u${u.toString(16).padStart(4, "0")}`;
const COMPACT_SHORT = { 0x08: "\\b", 0x09: "\\t", 0x0a: "\\n", 0x0c: "\\f", 0x0d: "\\r", 0x22: '\\"', 0x5c: "\\\\" };
const SHOWN_SHORT = { 0x09: "\\t", 0x0a: "\\n", 0x22: '\\"', 0x5c: "\\\\" };

/**
 * A string's JSON body (without quotes) in pieces. `shown`: as `json()`
 * shows it, where a control other than newline and tab is the character
 * itself; otherwise as JSON.stringify writes it. A lone surrogate is
 * escaped either way; a pair is kept whole.
 */
function* stringPieces(text, shown) {
  const short = shown ? SHOWN_SHORT : COMPACT_SHORT;
  let run = 0;
  for (let i = 0; i < text.length; i += 1) {
    const u = text.charCodeAt(i);
    let escape = null;
    if (u < 0x20 || u === 0x22 || u === 0x5c) escape = short[u] ?? (shown ? null : hex4(u));
    else if (u >= 0xd800 && u <= 0xdfff) {
      if (u <= 0xdbff && i + 1 < text.length && (text.charCodeAt(i + 1) & 0xfc00) === 0xdc00) {
        i += 1;
        continue;
      }
      escape = hex4(u);
    }
    if (escape === null) continue;
    if (i > run) yield text.slice(run, i);
    yield escape;
    run = i + 1;
  }
  if (run < text.length) yield run === 0 ? text : text.slice(run);
}

const skipped = (value) => value === undefined || typeof value === "function" || typeof value === "symbol";

/** JSON of `value` in pieces: indented by two (`indent`) or compact, and `shown` as above. */
function* jsonPieces(value, shown, indent, depth) {
  if (value !== null && typeof value === "object" && typeof value.toJSON === "function") value = value.toJSON();
  if (typeof value === "string") {
    yield '"';
    yield* stringPieces(value, shown);
    yield '"';
    return;
  }
  if (value === null || typeof value !== "object") {
    yield JSON.stringify(value) ?? "null";
    return;
  }
  const pad = indent ? `\n${"  ".repeat(depth + 1)}` : "";
  const close = indent ? `\n${"  ".repeat(depth)}` : "";
  if (Array.isArray(value)) {
    if (!value.length) {
      yield "[]";
      return;
    }
    for (let i = 0; i < value.length; i += 1) {
      yield `${i ? "," : "["}${pad}`;
      yield* jsonPieces(skipped(value[i]) ? null : value[i], shown, indent, depth + 1);
    }
    yield `${close}]`;
    return;
  }
  let first = true;
  for (const key of Object.keys(value)) {
    if (skipped(value[key])) continue;
    yield `${first ? "{" : ","}${pad}"`;
    first = false;
    yield* stringPieces(key, shown);
    yield indent ? '": ' : '":';
    yield* jsonPieces(value[key], shown, indent, depth + 1);
  }
  yield first ? "{}" : `${close}}`;
}

/** `json(value)` in pieces: indented, with hidden characters as themselves. */
export function* shownJsonPieces(value) {
  if (skipped(value)) {
    yield String(value);
    return;
  }
  yield* jsonPieces(value, true, true, 0);
}

/** `JSON.stringify(value)` in pieces. */
export const compactJsonPieces = (value) => jsonPieces(value, false, false, 0);

/**
 * The fields of `object` other than `keys`, or null when there are none:
 * what a friendly view of it leaves out, shown beside it as a block. A
 * null-prototype record, so every own field stays a field: JSON may carry
 * one named `__proto__`, which `{}` would take as its prototype.
 */
export function restOf(object, keys) {
  if (object === null || typeof object !== "object" || Array.isArray(object)) return null;
  let rest = null;
  for (const [key, value] of Object.entries(object)) {
    if (keys.includes(key) || skipped(value)) continue;
    rest ??= Object.create(null);
    rest[key] = value;
  }
  return rest;
}

/**
 * A tool output's content part as blocks, the one projection the item's
 * document (so its measure and windows) and its short view share (SUB
 * decision 1c19f413 §5, SUB-70). A text part is its friendly text, after
 * its other fields (an `annotation`, say) as JSON, so nothing it carries is
 * left out and they show before a long text's windows. Any other part is
 * its JSON, whole. Each block is `{text}` or `{json}`, with the label it
 * has in the document.
 *
 * @returns {Array<{label: string, text?: string, json?: unknown}>}
 */
export function partBlocks(part, index) {
  const text = part?.type === "text";
  if (!text || typeof part.text !== "string") return [{ label: `content[${index}]${text ? " (text)" : ""}`, json: part }];
  const rest = restOf(part, ["type", "text"]);
  return [...(rest ? [{ label: `content[${index}] (other fields)`, json: rest }] : []), { label: `content[${index}] (text)`, text: part.text }];
}

/** The members of a step's `model` and `tokens` its short view puts in words. */
const STEP_MEMBERS = { model: ["provider", "id", "variant"], tokens: ["input", "output", "reasoning", "cacheRead", "cacheWrite"] };

/** Whether `value` is a record whose members a friendly view can read. */
export const isRecord = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

/**
 * What a step's short view leaves out of its `model` and `tokens`, as
 * blocks in the shape of `partBlocks` (SUB decision 533c2b29): the members
 * it does not put in words, or the whole value when it is not a record.
 * The document holds both fields whole, so these are always inside what
 * was measured.
 *
 * @returns {Array<{label: string, json: unknown}>}
 */
export function stepBlocks(item) {
  const blocks = [];
  for (const [field, members] of Object.entries(STEP_MEMBERS)) {
    const value = item[field];
    if (value === null || value === undefined) continue;
    const rest = isRecord(value) ? restOf(value, members) : value;
    if (rest !== null) blocks.push({ label: isRecord(value) ? `${field} (other fields)` : field, json: rest });
  }
  return blocks;
}

/** An item's document in pieces: each field labelled, strings as text, the rest as shown JSON; content parts as `partBlocks`. */
export function* documentPieces(item) {
  let first = true;
  const label = (words) => {
    const out = `${first ? "" : "\n\n"}${words}\n`;
    first = false;
    return out;
  };
  for (const [key, value] of Object.entries(item ?? {})) {
    if (key === "content" && Array.isArray(value)) {
      for (let index = 0; index < value.length; index += 1) {
        for (const block of partBlocks(value[index], index)) {
          yield label(block.label);
          if (block.text !== undefined) yield block.text;
          else yield* shownJsonPieces(block.json);
        }
      }
    } else {
      yield label(key);
      if (typeof value === "string") yield value;
      else yield* shownJsonPieces(value);
    }
  }
}

/** What an item's body is made from, in pieces: an unreadable item its JSON, any other its document, a part its text. */
export function sourcePieces(source) {
  if (source.kind === "part") return [source.data];
  if (source.item.kind === "unreadable") return shownJsonPieces(source.item.raw);
  return documentPieces(source.item);
}

/** The UTF-8 bytes of compact JSON of `value`, counted without building it. */
export function jsonBytes(value) {
  let bytes = 0;
  for (const piece of compactJsonPieces(value)) bytes += textBytes(piece);
  return bytes;
}

function textBytes(text) {
  let bytes = 0;
  for (let i = 0; i < text.length; i += 1) {
    const u = text.charCodeAt(i);
    if (u < 0x80) bytes += 1;
    else if (u < 0x800) bytes += 2;
    else if (u >= 0xd800 && u <= 0xdbff && i + 1 < text.length && (text.charCodeAt(i + 1) & 0xfc00) === 0xdc00) {
      bytes += 4;
      i += 1;
    } else bytes += 3;
  }
  return bytes;
}

/**
 * Measure pieces as they show, in one pass, keeping nothing of them:
 * `bounds` are the windows' starts in UTF-16 units with the end last,
 * `sizes` each window's bytes as shown, `bytes` the total as shown.
 * A window holds at most `maxBytes` and `maxLines`, and splits only
 * between characters (a hidden character and its marker stay whole).
 *
 * @param {Iterable<string>} pieces
 * @returns {{bounds: number[], sizes: number[], bytes: number}}
 */
export function measurePieces(pieces, { maxBytes = WINDOW_BYTES, maxLines = WINDOW_LINES } = {}) {
  const bounds = [0];
  const sizes = [];
  let pos = 0;
  let start = 0;
  let bytes = 0;
  let lines = 1;
  let total = 0;
  for (const piece of pieces) {
    for (let i = 0; i < piece.length; ) {
      const u = piece.charCodeAt(i);
      let size;
      let units = 1;
      if (u < 0x80) size = isHidden(u) ? MARKER_BYTES : 1;
      else if (u < 0x800) size = isHidden(u) ? MARKER_BYTES : 2;
      else if (u >= 0xd800 && u <= 0xdbff && i + 1 < piece.length && (piece.charCodeAt(i + 1) & 0xfc00) === 0xdc00) {
        size = 4;
        units = 2;
      } else size = isHidden(u) ? MARKER_BYTES : 3;
      if (pos > start && (bytes + size > maxBytes || lines > maxLines)) {
        bounds.push(pos);
        sizes.push(bytes);
        total += bytes;
        start = pos;
        bytes = 0;
        lines = 1;
      }
      bytes += size;
      if (u === 0x0a) lines += 1;
      pos += units;
      i += units;
    }
  }
  bounds.push(pos);
  sizes.push(bytes);
  total += bytes;
  return { bounds, sizes, bytes: total };
}

/** The text of pieces from unit `from` to unit `to`: only that window is built. */
export function sliceOfPieces(pieces, from, to) {
  const out = [];
  let pos = 0;
  for (const piece of pieces) {
    const end = pos + piece.length;
    if (end > from) out.push(piece.slice(Math.max(0, from - pos), Math.min(piece.length, to - pos)));
    pos = end;
    if (pos >= to) break;
  }
  return out.join("");
}

/** Text with each hidden character as its marker: the bytes this is, as shown. */
export const shownBytes = (text) => measurePieces([String(text ?? "")], { maxBytes: Infinity, maxLines: Infinity }).bytes;

/**
 * Where unit `at` of an item's document (`documentPieces`) is in the item as
 * served, its compact JSON (`JSON.stringify`, the item being parsed JSON):
 * the UTF-16 offset there of the character shown at `at`, or of the start of
 * the value a label or punctuation belongs to; the end of the JSON past the
 * document's end. A text part's other fields, shown before its text, map
 * to where each is served in the part (SUB decision cf21d510): the shown
 * order is kept only to count the document's units. One pass over the
 * item, stopping at `at`, building nothing whole.
 */
export function servedUnit(item, at) {
  let doc = 0;
  let raw = 0;
  let found = null;
  /** A stretch shown in `shown` units and served in `served`, with no finer match: a label, punctuation, a number. */
  const span = (shown, served) => {
    if (found === null && at < doc + shown) found = raw;
    doc += shown;
    raw += served;
  };
  /** A string's characters, shown as themselves (`plain`) or as `json()` writes them, and served as JSON.stringify writes them. */
  const chars = (text, plain) => {
    for (let i = 0; i < text.length && found === null; ) {
      const u = text.charCodeAt(i);
      const units = u >= 0xd800 && u <= 0xdbff && i + 1 < text.length && (text.charCodeAt(i + 1) & 0xfc00) === 0xdc00 ? 2 : 1;
      let shown = units;
      let served = units;
      if (units === 1 && (u < 0x20 || u === 0x22 || u === 0x5c || (u >= 0xd800 && u <= 0xdfff))) {
        served = COMPACT_SHORT[u]?.length ?? 6;
        if (!plain) shown = u >= 0xd800 ? 6 : (SHOWN_SHORT[u]?.length ?? 1);
      }
      span(shown, served);
      i += units;
    }
  };
  /** A value as `shownJsonPieces` writes it at `depth`, served compact. */
  const value = (v, depth) => {
    if (found !== null) return;
    if (typeof v === "string") {
      span(1, 1);
      chars(v, false);
      span(1, 1);
    } else if (v === null || typeof v !== "object") {
      const units = (JSON.stringify(v) ?? "null").length;
      span(units, units);
    } else if (Array.isArray(v)) {
      if (!v.length) return span(2, 2);
      // `[` or `,`, a newline and the indent, against `[` or `,`.
      for (let i = 0; i < v.length && found === null; i += 1) {
        span(2 * depth + 4, 1);
        value(skipped(v[i]) ? null : v[i], depth + 1);
      }
      span(2 * depth + 2, 1);
    } else {
      let first = true;
      for (const key of Object.keys(v)) {
        if (found !== null) return;
        if (skipped(v[key])) continue;
        first = false;
        span(2 * depth + 5, 2);
        chars(key, false);
        span(3, 2);
        value(v[key], depth + 1);
      }
      span(first ? 2 : 2 * depth + 2, first ? 2 : 1);
    }
  };
  const compactUnits = (v) => {
    let units = 0;
    for (const piece of compactJsonPieces(v)) units += piece.length;
    return units;
  };
  let first = true;
  const label = (words) => {
    span((first ? 0 : 2) + words.length + 1, 0);
    first = false;
  };
  span(0, 1);
  let fields = 0;
  for (const [key, v] of Object.entries(item ?? {})) {
    if (found !== null) break;
    // `,"key":` as served, before the value.
    span(0, (fields ? 1 : 0) + compactUnits(key) + 1);
    fields += 1;
    if (key === "content" && Array.isArray(v)) {
      span(0, 1);
      for (let index = 0; index < v.length && found === null; index += 1) {
        if (index) span(0, 1);
        const part = v[index];
        if (part?.type !== "text" || typeof part.text !== "string") {
          label(partBlocks(part, index)[0].label);
          value(part, 0);
          continue;
        }
        // The part as served: where its text starts, and the `{` or `,` before each field.
        const start = raw;
        let units = 1;
        let textAt = 0;
        const fieldAt = new Map();
        for (const field of Object.keys(part)) {
          if (skipped(part[field])) continue;
          fieldAt.set(field, units > 1 ? units : 0);
          units += (units > 1 ? 1 : 0) + compactUnits(field) + 1;
          if (field === "text") textAt = units;
          units += compactUnits(part[field]);
        }
        for (const block of partBlocks(part, index)) {
          if (block.text !== undefined) {
            raw = start + textAt;
            label(block.label);
            span(0, 1);
            chars(block.text, true);
            continue;
          }
          // The other fields, shown as one object in their own order, each mapped to where the part serves it.
          const fields = Object.keys(block.json);
          raw = start + fieldAt.get(fields[0]);
          label(block.label);
          for (const field of fields) {
            if (found !== null) break;
            raw = start + fieldAt.get(field);
            // `{` or `,`, a newline, the indent and `"`, against `{` or `,` and `"`; the key; `": ` against `":`.
            span(5, 2);
            chars(field, false);
            span(3, 2);
            value(block.json[field], 1);
          }
          raw = start + units;
          span(2, 1);
        }
        if (found === null) raw = start + units + 1;
      }
      span(0, 1);
    } else {
      label(key);
      if (typeof v === "string") {
        span(0, 1);
        chars(v, true);
        span(0, 1);
      } else value(v, 0);
    }
  }
  span(0, 1);
  return found ?? raw;
}

/**
 * Which part of an item as served holds unit `unit` of its compact JSON,
 * when the gateway pages that JSON at `pageBytes` UTF-8 bytes, splitting
 * only between code points, and a part is `partPages` pages: the part, and
 * the offset of `unit` in the part's text (UTF-16 units). Streams the JSON.
 */
export function servedPart(item, unit, { pageBytes, partPages }) {
  let page = 0;
  let bytes = 0;
  let pos = 0;
  let partStart = 0;
  for (const piece of compactJsonPieces(item)) {
    for (let i = 0; i < piece.length; ) {
      const u = piece.charCodeAt(i);
      const pair = u >= 0xd800 && u <= 0xdbff && i + 1 < piece.length && (piece.charCodeAt(i + 1) & 0xfc00) === 0xdc00;
      const size = u < 0x80 ? 1 : u < 0x800 ? 2 : pair ? 4 : 3;
      if (bytes > 0 && bytes + size > pageBytes) {
        page += 1;
        bytes = 0;
        if (page % partPages === 0) partStart = pos;
      }
      if (pos >= unit) return { part: Math.floor(page / partPages), unit: pos - partStart };
      bytes += size;
      pos += pair ? 2 : 1;
      i += pair ? 2 : 1;
    }
  }
  return { part: Math.floor(page / partPages), unit: pos - partStart };
}
