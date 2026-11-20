// An incremental server-sent events parser that follows the HTML event-stream
// rules. It takes decoded text chunks in any split, so a fetch body can be fed
// as it streams. EventSource cannot send an Authorization header, which the
// Fleet API needs, so the page reads the stream with fetch and parses it here.

const MAX_LINE = 1 << 20;
/** The most data one event may gather before its closing blank line. */
const MAX_EVENT = 4 << 20;

/**
 * Memory stays bounded whatever the server sends: every line is checked
 * against `maxLine` (complete or not), and an event's data against
 * `maxEvent`. Going over either throws, and the caller drops the stream.
 *
 * @param {(event: {type: string, data: string, id: string}) => void} onEvent
 * @param {{onComment?: (text: string) => void, onRetry?: (ms: number) => void, maxLine?: number, maxEvent?: number}} [options]
 */
export function createSseParser(onEvent, options = {}) {
  const maxLine = options.maxLine ?? MAX_LINE;
  const maxEvent = options.maxEvent ?? MAX_EVENT;
  let buffer = "";
  let data = [];
  let size = 0;
  let type = "";
  let lastId = "";
  let pendingCR = false;
  let started = false;

  function dispatch() {
    if (data.length === 0) {
      type = "";
      return;
    }
    const event = { type: type || "message", data: data.join("\n"), id: lastId };
    data = [];
    size = 0;
    type = "";
    onEvent(event);
  }

  function field(name, value) {
    if (name === "event") type = value;
    else if (name === "data") {
      size += value.length + 1;
      if (size > maxEvent) {
        end();
        throw new Error("The event stream sent an event larger than the parser accepts.");
      }
      data.push(value);
    }
    else if (name === "id") {
      if (!value.includes("\0")) lastId = value;
    } else if (name === "retry") {
      if (/^\d+$/.test(value)) options.onRetry?.(Number(value));
    }
  }

  function line(text) {
    if (text === "") {
      dispatch();
      return;
    }
    if (text.charCodeAt(0) === 58 /* : */) {
      options.onComment?.(text.slice(1).replace(/^ /, ""));
      return;
    }
    const colon = text.indexOf(":");
    if (colon === -1) {
      field(text, "");
      return;
    }
    let value = text.slice(colon + 1);
    if (value.charCodeAt(0) === 32) value = value.slice(1);
    field(text.slice(0, colon), value);
  }

  function push(chunk) {
    let text = String(chunk ?? "");
    if (!started && text.length > 0) {
      started = true;
      if (text.charCodeAt(0) === 0xfeff) text = text.slice(1);
    }
    if (text.length === 0) return;
    let start = 0;
    if (pendingCR) {
      pendingCR = false;
      if (text.charCodeAt(0) === 10) start = 1;
    }
    for (let i = start; i < text.length; i += 1) {
      const code = text.charCodeAt(i);
      if (code !== 10 && code !== 13) continue;
      const whole = buffer + text.slice(start, i);
      buffer = "";
      if (whole.length > maxLine) tooLong();
      if (code === 13) {
        if (i + 1 >= text.length) pendingCR = true;
        else if (text.charCodeAt(i + 1) === 10) i += 1;
      }
      start = i + 1;
      line(whole);
    }
    buffer += text.slice(start);
    if (buffer.length > maxLine) tooLong();
  }

  function tooLong() {
    end();
    throw new Error("The event stream sent a line longer than the parser accepts.");
  }

  /** The stream ended: an event without its closing blank line is discarded, as the spec says. */
  function end() {
    buffer = "";
    data = [];
    size = 0;
    type = "";
  }

  return {
    push,
    end,
    get lastEventId() {
      return lastId;
    },
  };
}
