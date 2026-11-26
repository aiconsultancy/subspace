// The diff view in the inspector: a crew member's changed files for one
// project, one file's unified patch, and a binary file's bytes as hex and
// ASCII. Like the Activity view it builds DOM nodes, never HTML: every value
// from the API is set as text, and control and bidi formatting characters
// show as `⟨U+XXXX⟩` markers, so a patch cannot disguise what it changes.
//
// Everything is reachable and nothing is cut (SUB-70). Long lines wrap.
// Large content is shown a window at a time, so the page stays small: the
// file list 200 files at a time; a patch one part (a gateway page) at a
// time, each part in the Activity view's windows of at most 64 KiB as shown
// and 1,000 lines (SUB decision 75c3ed93 §1); and the hex view 4 KiB at a
// time. Each has First, Previous and Next, and Last where it is known.

import { h, mount, txt } from "./activity-view.js";
import { binarySides, DIFF_MODES, diffError, hexRows, HEX_WINDOW_BYTES, knownKind, knownStatus, PREVIEW_MAX_PIXELS, previewable } from "./diff.js";
import { bytesOf, patchLines } from "./diff-parts.js";
import { measurePieces, WINDOW_BYTES, WINDOW_LINES } from "./document.js";
import { formatBytes, formatClock } from "./format.js";

export const FILES_WINDOW = 200;

const STATUS = { added: ["A", "Added"], removed: ["D", "Removed"], modified: ["M", "Modified"], renamed: ["R", "Renamed"] };
const SIDE = { old: "Old", new: "New" };
const count = (n) => n.toLocaleString("en-GB");

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

/**
 * The windows of one patch part, as the Activity view draws a part: at most
 * WINDOW_BYTES as shown and WINDOW_LINES each, never splitting a marker.
 * `starts[w]` is where window `w` starts in the patch's numbering, worked
 * out as far as a window shown.
 */
export function partWindows(data, start) {
  const { bounds } = measurePieces([data], { maxBytes: WINDOW_BYTES, maxLines: WINDOW_LINES });
  const starts = [start];
  return {
    count: bounds.length - 1,
    /** Window `w`'s lines, numbered from where it starts. */
    lines(w) {
      for (let k = starts.length; k <= w; k += 1) starts[k] = patchLines(data.slice(bounds[k - 1], bounds[k]), starts[k - 1]).end;
      return patchLines(data.slice(bounds[w], bounds[w + 1]), starts[w]).lines;
    },
  };
}

/** Where a part read for the patch stands, in words, or null when its window says it all. */
function partNotes(entry, patch, view) {
  const notes = [];
  const button = (attribute, words) => h("button", { class: "btn", type: "button", [attribute]: String(entry.index) }, words);
  if (patch.evicted) notes.push(h("p", { class: "act-note" }, "This part was let go to make room for a newer read. It is read again, at the same snapshot, when shown."), button("data-diff-reopen", "Show it again"));
  else if (patch.state === "loading") notes.push(h("p", { class: "act-note" }, patch.data === null ? "Reading the patch…" : `Reading part ${patch.want + 1}; this part stays on show until it arrives…`));
  else if (patch.state === "failed") {
    const problem = diffError(patch.error, { file: true, action: "Retry page" });
    if (view.changed || problem.kind === "cancelled" || problem.kind === "stale") notes.push(h("p", { class: "act-note" }, "The checkout changed. Refresh to read this part."));
    else notes.push(h("p", { class: "act-warn" }, txt(problem.message)), button("data-diff-retry", "Retry page"));
  } else if (view.changed && patch.hasNext) notes.push(h("p", { class: "act-note" }, "The checkout changed. Refresh to read the rest of this patch."));
  return notes.length ? h("div", { class: "act-messages" }, notes) : null;
}

/** First, Previous, Next and Last part of a patch; Last only once the part count is known. */
function partNav(patch) {
  if (patch.data === null || patch.count === 1) return null;
  const button = (to, words, disabled) => h("button", { class: "btn", type: "button", "data-diff-part": to, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
  const of = patch.count === null ? `${patch.known}+` : String(patch.count);
  return h(
    "div",
    { class: "act-parts", role: "group", "aria-label": "Patch parts" },
    button("first", "First part", !patch.hasPrevious),
    button("previous", "Previous part", !patch.hasPrevious),
    h("span", { class: "act-sub" }, `Part ${patch.part + 1} of ${of}`),
    button("next", "Next part", !patch.hasNext),
    patch.count !== null ? button("last", "Last part", !patch.hasLast) : null,
  );
}

/** One part of a patch: its navigation, one window of it with its line numbers, and where its reads stand. */
function patchTree(entry, shown, view) {
  const { patch, windows, section } = shown;
  const numbers = (line) => [h("span", { class: "ln" }, line.old === null ? "" : String(line.old)), h("span", { class: "ln" }, line.new === null ? "" : String(line.new))];
  const body = windows ? windows.lines(section).map((line) => h("div", { class: `dl ${line.kind}${line.more ? " more" : ""}` }, numbers(line), h("span", { class: "code" }, txt(line.text)))) : [];
  return [
    partNav(patch),
    windows ? pager("lines", section, windows.count, "Section") : null,
    windows ? h("div", { class: "diff-patch", role: "table", "aria-label": `Patch of ${entry.file.path}` }, body) : null,
    partNotes(entry, patch, view),
  ];
}

/**
 * One side's bytes: one 4 KiB window of the part on show (a gateway page),
 * each row with its offset in the file, and First, Previous, Next, and Last
 * where its page can be read directly. A window move past the part's edge
 * reads the next or previous part; the part on show stays until it arrives.
 */
function hexTree(binary, view) {
  const side = SIDE[binary.side];
  const part = binary.part;
  const parts = [];
  if (part?.evicted) parts.push(h("p", { class: "act-note" }, "These bytes were let go to make room for a newer read. They are read again, at the same snapshot, when shown."), h("button", { class: "btn", type: "button", "data-diff-binary": binary.side }, "Show them again"));
  else if (binary.state === "loading") parts.push(h("p", { class: "act-note" }, part?.data === null ? `Reading ${side.toLowerCase()} bytes…` : `Reading more ${side.toLowerCase()} bytes; these stay on show until they arrive…`));
  else if (binary.state === "failed") {
    const problem = diffError(part.error, { file: true, action: "Retry page" });
    if (view.changed || problem.kind === "cancelled" || problem.kind === "stale") parts.push(h("p", { class: "act-note" }, "The checkout changed. Refresh to read these bytes."));
    else parts.push(h("p", { class: "act-warn" }, txt(problem.message)), h("button", { class: "btn", type: "button", "data-diff-binary-retry": "" }, "Retry page"));
  }
  const after = part?.data === null || part?.data === undefined ? null : part;
  if (after) {
    if (binary.size === 0) parts.push(h("p", { class: "act-note" }, `${side} side: empty file (0 bytes).`));
    else {
      const start = binary.window * HEX_WINDOW_BYTES;
      const slice = bytesOf(after.data, start, HEX_WINDOW_BYTES);
      const from = binary.offset + start;
      const atStart = after.part === 0 && binary.window === 0;
      const atEnd = !after.hasNext && binary.window >= binary.windows - 1;
      const button = (to, words, disabled) => h("button", { class: "btn", type: "button", "data-diff-bytes": to, ...(disabled ? { "aria-disabled": "true" } : {}) }, words);
      parts.push(
        h(
          "div",
          { class: "act-parts", role: "group", "aria-label": `${side} bytes` },
          button("first", "First", atStart),
          button("previous", "Previous", atStart),
          h("span", { class: "act-sub" }, `${side} bytes ${count(from)}–${count(from + slice.length)}${binary.size === null ? "" : ` of ${count(binary.size)}`}`),
          button("next", "Next", atEnd),
          after.count !== null && (after.hasLast || after.part === after.count - 1) ? button("last", "Last", atEnd) : null,
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
  const open = view.binary?.index === entry.index ? hexTree(view.binary, view) : view.preview?.index === entry.index ? previewTree(view.preview) : null;
  return [h("p", { class: "act-note" }, `Binary file${Number.isInteger(entry.file.size) ? `, ${formatBytes(entry.file.size)}` : ""}. Its bytes are read a page at a time.`), h("div", { class: "act-parts" }, actions), open];
}

const KIND_NOTES = {
  submodule: "Submodule: the patch shows the commit the checkout records.",
  symlink: "Symbolic link: the patch shows the target text Git records. It is never followed.",
};

/**
 * The selected file. `shown` is its patch as the panel draws it:
 * {patch, windows, section}, the watcher's part view, the windows of the
 * part on show and the one drawn; null before the patch is opened.
 */
export function fileTree(view, entry, shown) {
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
  else if (!shown || (shown.patch.count === 1 && shown.patch.data === "")) body = h("p", { class: "act-note" }, file.status === "renamed" ? "Renamed with no changes to its content." : "No text changes.");
  else body = patchTree(entry, shown, view);
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
  /** The section (window) of the patch part on show, and the windows of that part. */
  let section = 0;
  let windowsOf = { data: null, start: null, windows: null };
  let imageUrl = null;
  let last = null;

  const dropImage = () => {
    if (imageUrl) urls?.revokeObjectURL?.(imageUrl);
    imageUrl = null;
  };

  /** The selected file's patch as the panel draws it, or null: the part on show, its windows and the section drawn. */
  function shownPatch(view) {
    const patch = view.status === "ready" && selected !== null ? view.patch(selected) : null;
    if (!patch) return null;
    if (patch.data === null) return { patch, windows: null, section: 0 };
    if (windowsOf.data !== patch.data || windowsOf.start !== patch.start) {
      windowsOf = { data: patch.data, start: patch.start, windows: partWindows(patch.data, patch.start) };
      section = 0;
    }
    section = Math.min(section, windowsOf.windows.count - 1);
    return { patch, windows: windowsOf.windows, section };
  }

  /** Open the selected file's patch: the part on show before, read again at the same snapshot if it was let go. */
  function openSelected() {
    if (selected !== null && !last.view.files[selected]?.file.binary) watcher.openPatch(selected);
  }

  function select(index) {
    if (!last || !last.view.files[index] || index === selected) return;
    selected = index;
    windowsOf = { data: null, start: null, windows: null };
    section = 0;
    openSelected();
    redraw();
    root.querySelector?.("[data-diff-select][aria-current]")?.focus?.();
  }

  function move(kind, to) {
    if (!last) return;
    const shown = kind === "lines" ? shownPatch(last.view) : null;
    const windows = kind === "files" ? Math.max(1, Math.ceil(last.view.files.length / FILES_WINDOW)) : (shown?.windows?.count ?? 1);
    const at = kind === "files" ? fileWindow : section;
    const next = Math.max(0, Math.min(windows - 1, { first: 0, previous: at - 1, next: at + 1, last: windows - 1 }[to] ?? at));
    if (next === at) return;
    if (kind === "files") fileWindow = next;
    else section = next;
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
    else if ((node = hit("data-diff-part")) && !disabled(node)) watcher.movePatch(selected, node.dataset.diffPart);
    else if (hit("data-diff-retry")) watcher.retryPatch(selected);
    else if (hit("data-diff-reopen")) watcher.openPatch(selected);
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
      windowsOf = { data: null, start: null, windows: null };
      section = 0;
      keys.list = keys.detail = "";
      if (view.status === "ready") openSelected();
    }
    const listKey = `${view.status}|${fileWindow}|${selected}`;
    if (listKey !== keys.list) {
      const tree = fileListTree(view, fileWindow, selected);
      list.replaceChildren(...(tree ? [mount(tree, doc)] : []));
      keys.list = listKey;
    }
    const entry = view.status === "ready" ? view.files[selected] : null;
    const shown = shownPatch(view);
    const open = (part) => (part?.index === selected ? `${part.side}|${part.state}|${part.version}` : "");
    const detailKey = `${view.status}|${selected}|${shown?.patch.changes}|${section}|${view.changed}|${open(view.binary)}|${open(view.preview)}`;
    if (detailKey !== keys.detail) {
      dropImage();
      const tree = fileTree(view, entry, shown);
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
