/* oxlint-disable no-control-regex -- File validation rejects disallowed control bytes. */
const zip = "application/zip";
const word = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const excel = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const slides = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
const pdf = "application/pdf";

export const imageTypes = ["image/png", "image/jpeg", "image/gif", "image/webp"] as const;
export const fileTypes = ["text/plain", "text/markdown", pdf, word, excel, slides] as const;
export const chatTypes = [
  ...imageTypes,
  "text/plain",
  "text/markdown",
  pdf,
  zip,
  word,
  excel,
  slides,
] as const;

// Channels shares web source as plain UTF-8 so readers can inspect it as text.
const chatTextExtensions = new Set([
  "html",
  "htm",
  "css",
  "js",
  "mjs",
  "cjs",
  "ts",
  "jsx",
  "tsx",
  "json",
  "jsonl",
  "xml",
  "svg",
  "csv",
  "tsv",
  "yaml",
  "yml",
  "toml",
  "log",
  "diff",
  "patch",
]);

const extensions: Readonly<Record<string, readonly string[]>> = {
  "image/png": ["png"],
  "image/jpeg": ["jpg", "jpeg"],
  "image/gif": ["gif"],
  "image/webp": ["webp"],
  "text/plain": ["txt", "text"],
  "text/markdown": ["md", "markdown"],
  [zip]: ["zip"],
  [pdf]: ["pdf"],
  [word]: ["docx"],
  [excel]: ["xlsx"],
  [slides]: ["pptx"],
};

export function validName(name: string) {
  return (
    name.length > 0 &&
    name.length <= 200 &&
    name.trim() === name &&
    !/[\u0000-\u001f\u007f/\\]/u.test(name) &&
    name !== "." &&
    name !== ".."
  );
}

function matches(buffer: Buffer, offset: number, text: string) {
  return buffer.subarray(offset, offset + text.length).equals(Buffer.from(text, "latin1"));
}

function zipNames(bytes: Buffer) {
  // Inspect the central directory without extracting controlled data, which
  // also avoids writing archive filenames into the filesystem.
  const stop = Math.max(0, bytes.length - 65_557);
  let end = bytes.length - 22;
  while (end >= stop && !matches(bytes, end, "PK\x05\x06")) end--;
  if (end < stop || bytes.readUInt16LE(end + 20) + end + 22 !== bytes.length) return null;
  if (bytes.readUInt16LE(end + 4) || bytes.readUInt16LE(end + 6)) return null;
  const count = bytes.readUInt16LE(end + 10);
  if (count !== bytes.readUInt16LE(end + 8) || count > 10_000) return null;
  let at = bytes.readUInt32LE(end + 16);
  if (at + bytes.readUInt32LE(end + 12) !== end) return null;
  let expanded = 0;
  const names = new Set<string>();
  for (let i = 0; i < count; i++) {
    if (at + 46 > end || !matches(bytes, at, "PK\x01\x02")) return null;
    const method = bytes.readUInt16LE(at + 10);
    const flags = bytes.readUInt16LE(at + 8);
    const compressed = bytes.readUInt32LE(at + 20);
    expanded += bytes.readUInt32LE(at + 24);
    const nameLength = bytes.readUInt16LE(at + 28);
    const next = at + 46 + nameLength + bytes.readUInt16LE(at + 30) + bytes.readUInt16LE(at + 32);
    const local = bytes.readUInt32LE(at + 42);
    if (
      next > end ||
      local + 30 > at ||
      expanded > 250_000_000 ||
      flags & 1 ||
      (method !== 0 && method !== 8) ||
      !matches(bytes, local, "PK\x03\x04")
    )
      return null;
    const name = bytes.subarray(at + 46, at + 46 + nameLength).toString("utf8");
    if (!name || /^[/\\]|[\u0000\\:]|(^|\/)\.\.(\/|$)/u.test(name) || names.has(name)) return null;
    if (
      local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28) + compressed >
      at
    )
      return null;
    names.add(name);
    at = next;
  }
  return at === end ? names : null;
}

export function validContent(name: string, type: string, bytes: Buffer, chat = false) {
  const extension = name.split(".").at(-1)?.toLowerCase() ?? "";
  const matchingExtension =
    extensions[type]?.includes(extension) ||
    (chat && type === "text/plain" && chatTextExtensions.has(extension));
  if (!validName(name) || !bytes.length || !Object.hasOwn(extensions, type) || !matchingExtension)
    return false;
  switch (type) {
    case "image/png":
      return (
        bytes.length >= 45 &&
        matches(bytes, 0, "\u0089PNG\r\n\x1a\n") &&
        matches(bytes, 12, "IHDR") &&
        matches(bytes, bytes.length - 8, "IEND")
      );
    case "image/jpeg":
      return (
        bytes.length >= 4 &&
        matches(bytes, 0, "\xff\xd8\xff") &&
        matches(bytes, bytes.length - 2, "\xff\xd9")
      );
    case "image/gif":
      return (
        bytes.length >= 14 &&
        (matches(bytes, 0, "GIF87a") || matches(bytes, 0, "GIF89a")) &&
        bytes.at(-1) === 0x3b
      );
    case "image/webp":
      return (
        bytes.length >= 16 &&
        matches(bytes, 0, "RIFF") &&
        matches(bytes, 8, "WEBP") &&
        bytes.readUInt32LE(4) + 8 === bytes.length
      );
    case "text/plain":
    case "text/markdown": {
      try {
        return !/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/u.test(
          new TextDecoder("utf-8", { fatal: true }).decode(bytes),
        );
      } catch {
        return false;
      }
    }
    case pdf:
      return (
        /^%PDF-[12]\.\d/u.test(bytes.subarray(0, 12).toString("ascii")) &&
        /%%EOF\s*$/u.test(bytes.subarray(-1024).toString("ascii"))
      );
    case zip:
      return zipNames(bytes) !== null;
    case word:
    case excel:
    case slides: {
      const root =
        type === word
          ? "word/document.xml"
          : type === excel
            ? "xl/workbook.xml"
            : "ppt/presentation.xml";
      const names = zipNames(bytes);
      return Boolean(
        names?.has("[Content_Types].xml") &&
        names.has("_rels/.rels") &&
        names.has(root) &&
        ![...names].some((entry) => /vbaProject\.bin$|(^|\/)embeddings\//iu.test(entry)),
      );
    }
    default:
      return false;
  }
}
