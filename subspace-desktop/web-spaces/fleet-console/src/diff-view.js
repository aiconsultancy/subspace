// The diff view in the inspector: a crew member's changed files for one
// project, one file's unified patch, and a binary file's bytes as hex and
// ASCII. Like the Activity view it builds DOM nodes, never HTML: every value
// from the API is set as text, and control and bidi formatting characters
// show as `⟨U+XXXX⟩` markers, so a patch cannot disguise what it changes.
//
// Everything is reachable and nothing is cut (SUB-70). Long lines wrap.
// Large content is shown a window at a time, so the page stays small: the
// file list 200 files at a time, a patch 1,000 lines at a time and the hex
// view 4 KiB at a time, each with First, Previous, Next and Last.

import { h, mount, txt } from "./activity-view.js";
import { binarySides, DIFF_MODES, hexRows, HEX_WINDOW_BYTES, knownKind, knownStatus, PREVIEW_MAX_PIXELS, previewable } from "./diff.js";
import { formatBytes, formatClock } from "./format.js";

export const FILES_WINDOW = 200;
export const LINES_WINDOW = 1000;

const STATUS = { added: ["A", "Added"], removed: ["D", "Removed"], modified: ["M", "Modified"], renamed: ["R", "Renamed"] };
const SIDE = { old: "Old", new: "New" };
const count = (n) => n.toLocaleString("en-GB");

/** Lines of a patch with their old and new line numbers, from the hunk headers. */
export function patchLines(text) {
  const lines = text.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  let oldLine = 0;
  let newLine = 0;
  return lines.map((line) => {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (hunk) {
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      return { kind: "hunk", text: line, old: null, new: null };
    }
    if (line.startsWith("+")) return { kind: "add", text: line, old: null, new: newLine++ };
    if (line.startsWith("-")) return { kind: "del", text: line, old: oldLine++, new: null };
    if (line.startsWith("\\")) return { kind: "note", text: line, old: null, new: null };
    return { kind: "ctx", text: line, old: oldLine++, new: newLine++ };
  });
}

/** First, Previous, Next and Last for a window over `total` things. */
function pager(name, at, windows, label) {
  if (windows <= 1) return null;
  const button = (to, words, disabled) => h("button", { class: "btn", type: "button", [`data-diff-${name}`]: to, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
  return h(
    "div",
    { class: "act-parts", role: "group", "aria-label": label },
    button("first", "First", at === 0),
    button("previous", "Previous", at === 0),
    h("span", { class: "act-sub" }, `${label} ${at + 1} of ${windows}`),
    button("next", "Next", at >= windows - 1),
    button("last", "Last", at >= windows - 1),
  );
}

const projectName = (target) => target.project ?? "default project";

export function headerTree(view, member) {
  const messages = [];
  if (view.status === "loading") messages.push(h("p", { class: "act-note" }, "Reading the diff…"));
  if (view.status === "unavailable" || view.status === "failed") messages.push(h("p", { class: "act-warn" }, txt(view.error?.message ?? "")));
  if (view.banner) messages.push(h("p", { class: "act-warn diff-banner" }, `${view.banner} · `, h("button", { class: "btn", type: "button", "data-diff-refresh": "" }, "Refresh")));
  if (view.busy && view.status === "ready") messages.push(h("p", { class: "act-note" }, view.busy));
  if (view.status === "ready" && view.diff) {
    const d = view.diff;
    const files = `${count(d.total)} file${d.total === 1 ? "" : "s"} changed`;
    messages.push(h("p", { class: "act-note" }, `${files} · +${count(d.additions ?? 0)} −${count(d.deletions ?? 0)}`));
    const from = d.base ? [d.base.ref, d.base.commit?.slice(0, 7)].filter(Boolean).join(" ") : null;
    const to = d.mode === "committed" ? d.head?.slice(0, 7) ?? "HEAD" : "worktree";
    messages.push(h("p", { class: "act-note mono" }, txt(`${from ?? (d.mode === "working" ? "HEAD" : "no base")} → ${to}`), Number.isFinite(d.computedAt) ? ` · read ${formatClock(d.computedAt)}` : ""));
    if (!d.total) messages.push(h("p", { class: "act-note" }, "No changes."));
  }
  return h(
    "header",
    { class: "ihead act-ihead" },
    h("button", { class: "btn act-back", type: "button", "data-diff-close": "" }, "‹ Back"),
    h("p", { class: "eyebrow" }, "Crew · Diff"),
    h("h2", { tabindex: "-1" }, txt(member.name)),
    h("p", { class: "meta mono" }, txt(projectName(view.target)), view.diff?.branch ? [" · ", txt(view.diff.branch)] : null),
    h(
      "div",
      { class: "diff-bar" },
      h(
        "div",
        { class: "diff-modes", role: "group", "aria-label": "Changes to show" },
        DIFF_MODES.map(({ mode, label }) => h("button", { class: "btn", type: "button", "data-diff-mode": mode, "aria-pressed": String(view.mode === mode) }, label)),
      ),
      h("button", { class: "btn", type: "button", "data-diff-refresh": "", "aria-label": "Refresh the diff" }, "Refresh"),
    ),
    messages.length ? h("div", { class: "act-messages", role: "status" }, messages) : null,
  );
}

function fileFacts(file) {
  const facts = [];
  if (file.binary) facts.push(`binary${Number.isInteger(file.size) ? ` · ${formatBytes(file.size)}` : ""}`);
  else if (file.additions !== null || file.deletions !== null) facts.push(`+${count(file.additions ?? 0)} −${count(file.deletions ?? 0)}`);
  if (file.untracked) facts.push("untracked");
  if (file.kind) facts.push(file.kind);
  return facts.join(" · ");
}

function statusChip(file) {
  const [letter, words] = STATUS[file.status] ?? ["?", file.status];
  return h("span", { class: `diff-st st-${knownStatus(file.status) ? file.status : "other"}`, title: words }, letter);
}

export function fileListTree(view, at, selected) {
  if (view.status !== "ready" || !view.files.length) return null;
  const windows = Math.ceil(view.files.length / FILES_WINDOW);
  const start = at * FILES_WINDOW;
  const rows = view.files.slice(start, start + FILES_WINDOW).map((entry) =>
    h(
      "li",
      {},
      h(
        "button",
        { class: "diff-file", type: "button", "data-diff-select": String(entry.index), ...(entry.index === selected ? { "aria-current": "true" } : {}) },
        statusChip(entry.file),
        h("span", { class: "diff-path" }, txt(entry.file.path), entry.file.previousPath ? h("span", { class: "act-sub" }, " ← ", txt(entry.file.previousPath)) : null),
        h("span", { class: "act-sub" }, fileFacts(entry.file)),
      ),
    ),
  );
  return h(
    "section",
    { class: "sec" },
    h("h3", {}, "Files ", h("span", { class: "n" }, count(view.files.length))),
    pager("files", at, windows, "Files page"),
    h("ul", { class: "plain diff-files" }, rows),
  );
}

function patchTree(entry, lines, at, view) {
  const windows = Math.max(1, Math.ceil(lines.length / LINES_WINDOW));
  const shown = lines.slice(at * LINES_WINDOW, (at + 1) * LINES_WINDOW);
  const numbers = (line) => [h("span", { class: "ln" }, line.old === null ? "" : String(line.old)), h("span", { class: "ln" }, line.new === null ? "" : String(line.new))];
  const body = shown.map((line) => h("div", { class: `dl ${line.kind}` }, numbers(line), h("span", { class: "code" }, txt(line.text))));
  const footer = [];
  if (entry.unloaded) footer.push(h("p", { class: "act-note" }, "Later pages of this patch were unloaded to keep memory bounded. Load next page reads them again."));
  if (entry.next) {
    if (view.changed) footer.push(h("p", { class: "act-note" }, "The checkout changed. Refresh to read the rest of this patch."));
    else if (entry.page?.state === "loading") footer.push(h("p", { class: "act-note" }, "Reading the next page…"));
    else if (entry.page?.state === "failed") footer.push(h("p", { class: "act-warn" }, txt(entry.page.error.message)), h("button", { class: "btn", type: "button", "data-diff-load": String(entry.index) }, "Retry page"));
    else footer.push(h("button", { class: "btn", type: "button", "data-diff-load": String(entry.index) }, "Load next page"));
  }
  return [
    pager("lines", at, windows, "Lines page"),
    h("div", { class: "diff-patch", role: "table", "aria-label": `Patch of ${entry.file.path}` }, body),
    footer.length ? h("div", { class: "act-messages" }, footer) : null,
  ];
}

function hexTree(binary) {
  const side = SIDE[binary.side];
  const parts = [];
  if (binary.state === "loading") parts.push(h("p", { class: "act-note" }, `Reading ${side.toLowerCase()} bytes…`));
  if (binary.state === "failed") parts.push(h("p", { class: "act-warn" }, txt(binary.error.message)), h("button", { class: "btn", type: "button", "data-diff-binary-retry": "" }, "Retry page"));
  if (binary.state === "idle" && !binary.page) parts.push(h("p", { class: "act-note" }, "The checkout changed. Refresh to read these bytes."));
  const page = binary.page;
  if (page) {
    if (page.size === 0) parts.push(h("p", { class: "act-note" }, `${side} side: empty file (0 bytes).`));
    else {
      const start = binary.window * HEX_WINDOW_BYTES;
      const slice = page.bytes.subarray(start, start + HEX_WINDOW_BYTES);
      const from = page.offset + start;
      const atStart = page.offset === 0 && binary.window === 0;
      const atEnd = from + slice.length >= page.size;
      const button = (to, words, disabled) => h("button", { class: "btn", type: "button", "data-diff-bytes": to, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
      parts.push(
        h(
          "div",
          { class: "act-parts", role: "group", "aria-label": `${side} bytes` },
          button("first", "First", atStart),
          button("previous", "Previous", atStart),
          h("span", { class: "act-sub" }, `${side} bytes ${count(from)}–${count(from + slice.length)} of ${count(page.size)}`),
          button("next", "Next", atEnd),
        ),
        h("pre", { class: "act-pre diff-hex", "aria-label": `${side} bytes as hex and ASCII` }, hexRows(slice, from).join("\n")),
      );
    }
  }
  return h("div", { class: "diff-binary" }, h("h4", { class: "act-label" }, `${side} bytes`), parts, h("button", { class: "btn", type: "button", "data-diff-binary-close": "" }, "Hide bytes"));
}

const PREVIEW_NOTES = {
  loading: "Reading the image…",
  "too-large": "Too large to preview: over 10 MiB or 16 pages. View its bytes instead.",
  unsupported: "Not a PNG, JPEG, GIF or WebP image. View its bytes instead.",
};

function previewTree(preview) {
  const side = SIDE[preview.side];
  const parts = [];
  if (PREVIEW_NOTES[preview.state]) parts.push(h("p", { class: "act-note" }, PREVIEW_NOTES[preview.state]));
  if (preview.state === "failed") parts.push(h("p", { class: "act-warn" }, txt(preview.error.message)), h("button", { class: "btn", type: "button", "data-diff-preview": preview.side }, "Retry page"));
  if (preview.state === "ready") parts.push(h("div", { class: "diff-image", "data-diff-image": "" }));
  return h(
    "div",
    { class: "diff-binary" },
    h("h4", { class: "act-label" }, `${side} image`),
    parts,
    h("div", { class: "act-parts" }, h("button", { class: "btn", type: "button", "data-diff-binary": preview.side }, "Show bytes"), h("button", { class: "btn", type: "button", "data-diff-binary-close": "" }, "Hide image")),
  );
}

function binaryTree(view, entry) {
  const sides = binarySides(entry.file);
  const actions = sides.map((side) => h("button", { class: "btn", type: "button", "data-diff-binary": side, ...(view.changed ? { "aria-disabled": "true" } : {}) }, `${SIDE[side]} bytes`));
  if (previewable(entry.file.path)) actions.push(...sides.map((side) => h("button", { class: "btn", type: "button", "data-diff-preview": side, ...(view.changed ? { "aria-disabled": "true" } : {}) }, `Preview ${side} image`)));
  const open = view.binary?.index === entry.index ? hexTree(view.binary) : view.preview?.index === entry.index ? previewTree(view.preview) : null;
  return [h("p", { class: "act-note" }, `Binary file${Number.isInteger(entry.file.size) ? `, ${formatBytes(entry.file.size)}` : ""}. Its bytes are read a page at a time.`), h("div", { class: "act-parts" }, actions), open];
}

const KIND_NOTES = {
  submodule: "Submodule: the patch shows the commit the checkout records.",
  symlink: "Symbolic link: the patch shows the target text Git records. It is never followed.",
};

export function fileTree(view, entry, lines, at) {
  if (!entry) return null;
  const file = entry.file;
  const [, words] = STATUS[file.status] ?? [null, file.status];
  const head = h(
    "div",
    { class: "act-head" },
    statusChip(file),
    h("b", { class: "mono" }, txt(file.path)),
    file.previousPath ? h("span", { class: "act-sub" }, "from ", txt(file.previousPath)) : null,
    h("span", { class: "act-sub" }, [words, fileFacts(file)].filter(Boolean).join(" · ")),
  );
  const notes = [];
  if (!knownKind(file.kind)) notes.push(h("p", { class: "act-note" }, `Kind: ${file.kind}.`));
  if (KIND_NOTES[file.kind]) notes.push(h("p", { class: "act-note" }, KIND_NOTES[file.kind]));
  let body;
  if (file.binary) body = binaryTree(view, entry);
  else if (!lines.length && !entry.next) body = h("p", { class: "act-note" }, file.status === "renamed" ? "Renamed with no changes to its content." : "No text changes.");
  else body = patchTree(entry, lines, at, view);
  return h("section", { class: "sec diff-detail" }, head, notes, body);
}

/**
 * The live panel. `update(view, member)` redraws what changed. `watcher` is a
 * createDiffWatcher; `urls` makes and revokes the object URL of an image
 * preview (absent outside a browser).
 */
export function createDiffPanel({ doc, watcher, onClose, urls = globalThis.URL }) {
  const root = doc.createElement("div");
  root.setAttribute("class", "activity diff");
  const header = doc.createElement("div");
  const list = doc.createElement("div");
  const detail = doc.createElement("div");
  for (const node of [header, list, detail]) root.appendChild(node);
  const keys = { header: "", list: "", detail: "" };
  let files = null;
  let selected = null;
  let fileWindow = 0;
  let lineWindow = 0;
  let linesCache = { entry: null, version: -1, count: 0, lines: [] };
  let imageUrl = null;
  let last = null;

  const dropImage = () => {
    if (imageUrl) urls?.revokeObjectURL?.(imageUrl);
    imageUrl = null;
  };

  function linesOf(entry) {
    if (!entry || entry.file.binary) return [];
    if (linesCache.entry !== entry || linesCache.version !== entry.version || linesCache.count !== entry.pages.length) {
      linesCache = { entry, version: entry.version, count: entry.pages.length, lines: patchLines(entry.pages.join("")) };
    }
    return linesCache.lines;
  }

  function select(index) {
    if (!last || !last.view.files[index] || index === selected) return;
    selected = index;
    lineWindow = 0;
    redraw();
    root.querySelector?.("[data-diff-select][aria-current]")?.focus?.();
  }

  function move(kind, to) {
    if (!last) return;
    const total = kind === "files" ? last.view.files.length : linesOf(last.view.files[selected]).length;
    const size = kind === "files" ? FILES_WINDOW : LINES_WINDOW;
    const windows = Math.max(1, Math.ceil(total / size));
    const at = kind === "files" ? fileWindow : lineWindow;
    const next = Math.max(0, Math.min(windows - 1, { first: 0, previous: at - 1, next: at + 1, last: windows - 1 }[to] ?? at));
    if (next === at) return;
    if (kind === "files") fileWindow = next;
    else lineWindow = next;
    redraw();
  }

  root.addEventListener("click", (event) => {
    const target = event.target;
    const hit = (name) => target.closest(`[${name}]`);
    const disabled = (node) => node.getAttribute?.("aria-disabled") === "true";
    let node;
    if (hit("data-diff-close")) onClose?.();
    else if ((node = hit("data-diff-mode"))) watcher.setMode(node.dataset.diffMode);
    else if (hit("data-diff-refresh")) watcher.refresh();
    else if ((node = hit("data-diff-select"))) select(Number(node.dataset.diffSelect));
    else if ((node = hit("data-diff-files")) && !disabled(node)) move("files", node.dataset.diffFiles);
    else if ((node = hit("data-diff-lines")) && !disabled(node)) move("lines", node.dataset.diffLines);
    else if ((node = hit("data-diff-load"))) watcher.loadPage(Number(node.dataset.diffLoad));
    else if (hit("data-diff-binary-retry")) watcher.retryBinary();
    else if (hit("data-diff-binary-close")) watcher.closeBinary();
    else if ((node = hit("data-diff-binary")) && !disabled(node)) watcher.openBinary(selected, node.dataset.diffBinary);
    else if ((node = hit("data-diff-preview")) && !disabled(node)) watcher.openPreview(selected, node.dataset.diffPreview);
    else if ((node = hit("data-diff-bytes")) && !disabled(node)) watcher.moveBinary(node.dataset.diffBytes);
  });

  function showImage(preview) {
    dropImage();
    const holder = detail.querySelector?.("[data-diff-image]");
    if (!holder) return;
    if (!urls?.createObjectURL || typeof Blob !== "function") {
      holder.appendChild(doc.createTextNode("Image previews need the app's web view."));
      return;
    }
    imageUrl = urls.createObjectURL(new Blob([preview.bytes], { type: preview.type }));
    const image = doc.createElement("img");
    image.setAttribute("alt", `${SIDE[preview.side]} side of ${preview.path}`);
    image.addEventListener("load", () => {
      if (image.naturalWidth > PREVIEW_MAX_PIXELS || image.naturalHeight > PREVIEW_MAX_PIXELS) {
        dropImage();
        image.replaceWith(doc.createTextNode(`Too large to preview: ${image.naturalWidth} × ${image.naturalHeight} pixels, over 4,096. View its bytes instead.`));
      }
    });
    image.setAttribute("src", imageUrl);
    holder.appendChild(image);
  }

  function redraw() {
    const { view, member } = last;
    const headerKey = JSON.stringify([view.status, view.mode, view.error?.message, view.banner, view.busy, view.diff?.fingerprint, view.diff?.computedAt, member.name]);
    if (headerKey !== keys.header) {
      header.replaceChildren(mount(headerTree(view, member), doc));
      keys.header = headerKey;
    }
    if (view.files !== files) {
      // A new snapshot: keep the same file selected when it is still there.
      const path = files?.[selected]?.file.path;
      const same = path === undefined ? -1 : view.files.findIndex((entry) => entry.file.path === path);
      files = view.files;
      selected = same >= 0 ? same : view.files.length ? 0 : null;
      fileWindow = selected === null ? 0 : Math.floor(selected / FILES_WINDOW);
      lineWindow = 0;
      keys.list = keys.detail = "";
    }
    const listKey = `${view.status}|${fileWindow}|${selected}`;
    if (listKey !== keys.list) {
      const tree = fileListTree(view, fileWindow, selected);
      list.replaceChildren(...(tree ? [mount(tree, doc)] : []));
      keys.list = listKey;
    }
    const entry = view.status === "ready" ? view.files[selected] : null;
    const lines = linesOf(entry);
    lineWindow = Math.min(lineWindow, Math.max(0, Math.ceil(lines.length / LINES_WINDOW) - 1));
    const open = (part) => (part?.index === selected ? `${part.side}|${part.state}|${part.version}` : "");
    const detailKey = `${view.status}|${selected}|${entry?.version}|${lineWindow}|${view.changed}|${open(view.binary)}|${open(view.preview)}`;
    if (detailKey !== keys.detail) {
      dropImage();
      const tree = fileTree(view, entry, lines, lineWindow);
      detail.replaceChildren(...(tree ? [mount(tree, doc)] : []));
      if (view.preview?.index === selected && view.preview.state === "ready") showImage(view.preview);
      keys.detail = detailKey;
    }
  }

  return {
    root,
    update(view, member) {
      last = { view, member };
      redraw();
    },
    destroy() {
      dropImage();
    },
  };
}
