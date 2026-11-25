// UTF-8 lengths and splits of JavaScript strings, without encoding them:
// item pages are measured in UTF-8 bytes, and long text is shown in parts.

/** Bytes the UTF-16 unit at `i` (with its pair) takes in UTF-8, and how many units it spans. */
function unitAt(text, i) {
  const unit = text.charCodeAt(i);
  if (unit < 0x80) return [1, 1];
  if (unit < 0x800) return [2, 1];
  if (unit >= 0xd800 && unit <= 0xdbff && i + 1 < text.length && (text.charCodeAt(i + 1) & 0xfc00) === 0xdc00) return [4, 2];
  // A lone surrogate encodes as U+FFFD, three bytes.
  return [3, 1];
}

/** UTF-8 length of `text`. */
export function utf8Length(text) {
  let bytes = 0;
  for (let i = 0; i < text.length; ) {
    const [size, units] = unitAt(text, i);
    bytes += size;
    i += units;
  }
  return bytes;
}

/**
 * Split `text` into parts of at most `maxBytes` UTF-8 bytes each, only on
 * code point boundaries, so the parts join back into `text` exactly.
 */
export function splitUtf8(text, maxBytes) {
  const parts = [];
  let start = 0;
  let bytes = 0;
  for (let i = 0; i < text.length; ) {
    const [size, units] = unitAt(text, i);
    if (bytes + size > maxBytes && i > start) {
      parts.push(text.slice(start, i));
      start = i;
      bytes = 0;
    }
    bytes += size;
    i += units;
  }
  if (start < text.length || !parts.length) parts.push(text.slice(start));
  return parts;
}
