// Sample mode's crew diffs: a fictional checkout answered through the same
// client interface as the live routes, so the viewer's paging, binary pages
// and markers can be tried without a fleet. Every file and line is made up.

import { HttpError } from "./api.js";
import { BINARY_PAGE_BYTES } from "./diff.js";

/** Files per list page here, small so the list's paging shows. */
export const SAMPLE_LIST_PAGE = 4;

// A 1×1 PNG.
const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

function bytesOf(base64) {
  const raw = atob(base64);
  return Uint8Array.from(raw, (char) => char.charCodeAt(0));
}

function pattern(size) {
  const bytes = new Uint8Array(size);
  for (let i = 0; i < size; i += 1) bytes[i] = (i * 31 + (i >> 8)) & 0xff;
  return bytes;
}

function base64Of(bytes) {
  let raw = "";
  for (let i = 0; i < bytes.length; i += 0x8000) raw += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(raw);
}

const hunk = (start, lines) => `@@ -${start},${lines.filter((l) => !l.startsWith("+")).length} +${start},${lines.filter((l) => !l.startsWith("-")).length} @@\n${lines.join("\n")}\n`;

function routePatch(page) {
  const lines = [];
  for (let i = 0; i < 40; i += 1) lines.push(i % 7 === 3 ? `-  const leg${page}_${i} = plot(origin, ${i});` : i % 7 === 4 ? `+  const leg${page}_${i} = plot(origin, ${i}, { warp: true });` : `   step(${page * 40 + i});`);
  return hunk(page * 60 + 1, lines);
}

/** The sample checkout's files, by mode, with their binary sides. */
function sampleFiles() {
  const long = `+${"Long line without spaces: ".repeat(2)}${"0123456789abcdef".repeat(40)}`;
  const files = [
    { mode: "committed", path: "src/warp/route.ts", previousPath: null, status: "modified", additions: 18, deletions: 18, patch: routePatch(0), patchNextPage: "route-1" },
    { mode: "committed", path: "src/warp/legacy.ts", previousPath: null, status: "removed", additions: 0, deletions: 3, patch: hunk(1, ["-export function legacyJump() {", "-  return null;", "-}"]) },
    { mode: "committed", path: "src/warp/plot.ts", previousPath: "src/warp/chart.ts", status: "renamed", additions: 0, deletions: 0, patch: null },
    { mode: "working", path: "docs/notes.md", previousPath: null, status: "added", additions: 4, deletions: 0, untracked: true, patch: hunk(1, ["+# Notes", "+Bidi override here: \u202Eevil\u202C and a bell \u0007.", "+\tTabbed line.", long]) },
    { mode: "working", path: "assets/badge.png", previousPath: null, status: "added", additions: null, deletions: null, untracked: true, binary: true, size: bytesOf(PNG).length, patch: null, sides: { new: bytesOf(PNG) } },
    { mode: "working", path: "assets/chart.bin", previousPath: null, status: "modified", additions: null, deletions: null, binary: true, size: BINARY_PAGE_BYTES + 4096, patch: null, sides: { old: new Uint8Array(0), new: pattern(BINARY_PAGE_BYTES + 4096) } },
    { mode: "working", path: "vendor/charts", previousPath: null, status: "modified", additions: 1, deletions: 1, kind: "submodule", patch: hunk(1, ["-Subproject commit 1d0c3aa0", "+Subproject commit 5be20d70"]) },
    { mode: "working", path: "current", previousPath: null, status: "added", additions: 1, deletions: 0, kind: "symlink", untracked: true, patch: hunk(1, ["+src/warp"]) },
  ];
  return files;
}

const ROUTE_PAGES = { "route-1": { patch: routePatch(1), next: "route-2" }, "route-2": { patch: routePatch(2), next: undefined } };

/**
 * A sample file as the gateway sends it: without the sample's own `mode` and `sides`.
 *
 * @param {{mode?: string, sides?: object, [field: string]: unknown}} entry
 */
const wire = ({ mode: _, sides: __, ...file }) => file;

/**
 * A diff client for sample mode. `fingerprintFor(ship, member, project)` is
 * the checkout's current fingerprint (the one the sample `ship` events carry),
 * or null when there is no such checkout.
 */
export function createSampleDiffClient({ fingerprintFor, now = Date.now }) {
  const all = sampleFiles();
  const inMode = (mode) => all.filter((file) => mode === "branch" || file.mode === mode);
  const missing = () => Promise.reject(new HttpError(404));
  return {
    diff(ship, member, { project = null, mode, page: token } = {}) {
      const fingerprint = fingerprintFor(ship, member, project);
      if (!fingerprint) return missing();
      const files = inMode(mode);
      const start = token ? Number(token.slice(5)) : 0;
      if (!Number.isInteger(start) || start < 0 || start >= Math.max(1, files.length)) return Promise.reject(new HttpError(400));
      const page = files.slice(start, start + SAMPLE_LIST_PAGE);
      const next = start + SAMPLE_LIST_PAGE < files.length ? `from-${start + SAMPLE_LIST_PAGE}` : undefined;
      return Promise.resolve({
        ship,
        member,
        project,
        mode,
        branch: "feat/flt-212-tier-warp",
        base: mode === "working" ? null : { ref: "origin/main", commit: "42b07e3" },
        head: mode === "committed" || mode === "branch" ? "a3f91c2" : null,
        fingerprint,
        computedAt: now(),
        total: files.length,
        additions: files.reduce((sum, file) => sum + (file.additions ?? 0), 0),
        deletions: files.reduce((sum, file) => sum + (file.deletions ?? 0), 0),
        files: page.map(wire),
        ...(next ? { nextPage: next } : {}),
      });
    },
    diffFile(ship, member, { project = null, mode, path, page, at, side } = {}) {
      const fingerprint = fingerprintFor(ship, member, project);
      if (!fingerprint) return missing();
      if (at !== fingerprint) return Promise.reject(new HttpError(409));
      const file = inMode(mode).find((entry) => entry.path === path);
      if (!file) return missing();
      if (side) {
        const bytes = file.sides?.[side];
        if (!bytes) return missing();
        const offset = page ? Number(page.slice(3)) : 0;
        const end = Math.min(bytes.length, offset + BINARY_PAGE_BYTES);
        const content = { side, encoding: "base64", offset, size: bytes.length, ...(end > offset ? { data: base64Of(bytes.subarray(offset, end)) } : {}), ...(end < bytes.length ? { nextPage: `at-${end}` } : {}) };
        return Promise.resolve({ fingerprint, file: wire({ ...file, patch: null }), content });
      }
      // The first page is the patch from its start; only route.ts goes on.
      if (page === undefined) return Promise.resolve({ fingerprint, file: wire(file) });
      const next = ROUTE_PAGES[page];
      if (file.path !== "src/warp/route.ts" || !next) return Promise.reject(new HttpError(400));
      return Promise.resolve({ fingerprint, file: wire({ ...file, patch: next.patch, patchNextPage: next.next }) });
    },
  };
}
