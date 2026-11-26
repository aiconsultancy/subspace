import assert from "node:assert/strict";
import { test } from "node:test";
import { createApiClient, HttpError, MAX_DIFF_BODY_BYTES, MAX_DIFF_FILE_BODY_BYTES } from "../src/api.js";
import {
  BINARY_PAGE_BYTES,
  binarySides,
  canViewDiff,
  createDiffWatcher,
  DIFF_MESSAGES,
  diffsEnabled,
  hexRows,
  HEX_WINDOW_BYTES,
  imageType,
  isDiffFile,
  validateContent,
} from "../src/diff.js";
import { createDiffPanel, patchLines } from "../src/diff-view.js";
import { crewPanel } from "../src/inspector.js";
import { isGit } from "../src/contract.js";
import { createFleetManager, createSampleFleetManager } from "../src/fleet-manager.js";
import { createSampleDiffClient } from "../src/sample-diff.js";
import { createSampleSource } from "../src/sample-source.js";
import { sampleResolve } from "../src/sample.js";
import { loadedState, NOW } from "./fixtures.js";
import { fakeDoc, fakeTimers } from "./activity-helpers.js";

const flush = async () => {
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
};

const TARGET = { ship: "galactica", member: "starbuck", project: "subspace" };
const FP = "fp-1";

function file(path, extra = {}) {
  return { path, previousPath: null, status: "modified", additions: 1, deletions: 1, patch: `@@ -1,1 +1,1 @@\n-old ${path}\n+new ${path}\n`, ...extra };
}

function page(files, extra = {}) {
  return { ship: TARGET.ship, member: TARGET.member, project: TARGET.project, mode: "branch", branch: "feat/x", base: { ref: "origin/main", commit: "42b07e3" }, head: "a3f91c2", fingerprint: FP, computedAt: NOW, total: files.length, additions: 1, deletions: 1, files, ...extra };
}

const b64 = (bytes) => Buffer.from(bytes).toString("base64");

/**
 * A diff client answering from `routes`: `list(query, n)` and
 * `file(query, n)` return a value, throw, or return a promise. Every call is
 * recorded, with its signal.
 */
function fakeClient({ list = () => page([file("a.txt")]), file: fileRoute = () => Promise.reject(new HttpError(404)) } = {}) {
  const calls = [];
  return {
    calls,
    lists: () => calls.filter((call) => call.kind === "list"),
    files: () => calls.filter((call) => call.kind === "file"),
    client: {
      diff: async (ship, member, query, signal) => {
        const n = calls.filter((call) => call.kind === "list").length;
        calls.push({ kind: "list", ship, member, query, signal });
        return list(query, n);
      },
      diffFile: async (ship, member, query, signal) => {
        const n = calls.filter((call) => call.kind === "file").length;
        calls.push({ kind: "file", ship, member, query, signal });
        return fileRoute(query, n);
      },
    },
  };
}

function watch(routes, options = {}) {
  const fake = fakeClient(routes);
  const timers = fakeTimers();
  let changes = 0;
  const watcher = createDiffWatcher({ client: fake.client, target: TARGET, onChange: () => (changes += 1), ...timers.options, ...options });
  return { ...fake, timers, watcher, changes: () => changes };
}

// ---------- gating ----------

function model(state, fleet = state.fleet) {
  const answers = new Map(sampleResolve(["FLT-212"]).map((answer) => [answer.identifier, answer]));
  return { fleet, ships: state.ships, now: NOW, unavailableSince: null, issues: { enabled: false, idsFor: () => [], get: (id) => answers.get(id) ?? null, canOpen: false } };
}

const withFeature = (fleet, enabled) => ({
  ...fleet,
  gateway: { ...fleet.gateway, features: [...fleet.gateway.features.filter((f) => f.key !== "api.crewDiffs"), ...(enabled === undefined ? [] : [{ key: "api.crewDiffs", label: "Crew diffs", enabled, default: true }])] },
});

test("diff gating: View diff only when api.crewDiffs is on and the project's Git state has no error", async () => {
  const state = await loadedState();
  assert.equal(diffsEnabled(withFeature(state.fleet, true)), true);
  assert.equal(diffsEnabled(withFeature(state.fleet, false)), false, "listed but off");
  assert.equal(diffsEnabled(withFeature(state.fleet, undefined)), false, "an older gateway lists no such feature");
  const git = { branch: "main", head: "a", base: null, ahead: null, behind: null, changedFiles: 0, repository: null, checkedAt: NOW };
  assert.equal(canViewDiff(true, { git }), true);
  assert.equal(canViewDiff(true, { git: null }), false, "no worktree yet");
  assert.equal(canViewDiff(true, { git: { ...git, error: "not a git repository" } }), false, "Git state could not be read");
  assert.equal(canViewDiff(false, { git }), false);

  const on = crewPanel(model(state, withFeature(state.fleet, true)), "galactica", "starbuck");
  const buttons = [...on.matchAll(/data-diff-project="([^"]*)"/g)].map((match) => match[1]);
  assert.ok(buttons.length >= 1, "View diff on the crew card's projects");
  assert.match(on, /data-diff-ship="galactica" data-diff-member="starbuck"/);
  assert.match(on, />View diff</);
  const off = crewPanel(model(state, withFeature(state.fleet, undefined)), "galactica", "starbuck");
  assert.doesNotMatch(off, /data-diff-member/, "no View diff without the feature");
  const broken = structuredClone(state.ships);
  for (const context of broken.galactica.crewMembers.find((m) => m.id === "starbuck").contexts) if (context.git) context.git.error = "fatal: bad object";
  const errored = crewPanel({ ...model(state, withFeature(state.fleet, true)), ships: broken }, "galactica", "starbuck");
  assert.doesNotMatch(errored, /data-diff-member/, "no View diff where Git failed");
});

test("diff contract: a ship event's Git state may carry a fingerprint, or none", () => {
  const git = { branch: "main", head: "a", base: null, ahead: null, behind: null, changedFiles: 0, repository: null, checkedAt: NOW };
  assert.equal(isGit(git), true);
  assert.equal(isGit({ ...git, fingerprint: "abc" }), true);
  assert.equal(isGit({ ...git, fingerprint: null }), true);
  assert.equal(isGit({ ...git, fingerprint: 7 }), false);
});

// ---------- the client ----------

test("diff api: list and file routes carry only the query given, and error bodies are never read", async () => {
  const seen = [];
  const client = createApiClient({
    endpoint: "https://fleet.example.com",
    token: "t",
    fetch: async (url, init) => {
      seen.push({ url, init });
      if (url.includes("/file")) return { ok: false, status: 409, headers: { get: () => null }, json: async () => assert.fail("an error body is not read") };
      return { ok: true, status: 200, headers: { get: () => null }, body: null, json: async () => ({ ok: 1 }), text: async () => '{"ok":1}' };
    },
  });
  await client.diff("ga lactica", "star/buck", { project: null, mode: "branch" }).catch(() => {});
  assert.equal(seen[0].url, "https://fleet.example.com/api/v1/ships/ga%20lactica/crew/star%2Fbuck/diff?mode=branch");
  await client.diff("g", "s", { project: "subspace", mode: "working", cursor: "c 2" }).catch(() => {});
  assert.equal(seen[1].url, "https://fleet.example.com/api/v1/ships/g/crew/s/diff?mode=working&project=subspace&cursor=c+2");
  const error = await client.diffFile("g", "s", { project: "p", mode: "branch", path: "a b/c.png", page: "2", at: "fp", side: "new" }).catch((e) => e);
  assert.equal(seen[2].url, "https://fleet.example.com/api/v1/ships/g/crew/s/diff/file?mode=branch&project=p&path=a+b%2Fc.png&page=2&at=fp&side=new");
  assert.ok(error instanceof HttpError && error.status === 409);
  assert.equal(seen[2].init.credentials, "omit");
  assert.ok(MAX_DIFF_BODY_BYTES >= 400 * 1024 * 1024 && MAX_DIFF_FILE_BODY_BYTES >= 7 * 1024 * 1024);
});

// ---------- status codes ----------

test("diff status: each code of the first page says what it means", async () => {
  const cases = [
    [400, "failed", DIFF_MESSAGES.malformed],
    [404, "unavailable", DIFF_MESSAGES.unavailable],
    [409, "failed", DIFF_MESSAGES.stale],
    [500, "failed", DIFF_MESSAGES.failed],
  ];
  for (const [code, status, message] of cases) {
    const { watcher, lists } = watch({ list: () => Promise.reject(new HttpError(code)) });
    watcher.start();
    await flush();
    assert.equal(watcher.view.status, status, String(code));
    assert.equal(watcher.view.error.message, message, String(code));
    assert.equal(lists().length, 1, `${code} is not retried`);
  }
});

test("diff status: 503 retries the same request after Retry-After, then offers Refresh", async () => {
  const { watcher, lists, timers } = watch({ list: () => Promise.reject(new HttpError(503, 2000)) });
  watcher.start();
  await flush();
  assert.equal(lists().length, 1);
  assert.deepEqual(timers.delays(), [2000]);
  timers.fire();
  await flush();
  assert.equal(lists().length, 2);
  timers.fire();
  await flush();
  assert.equal(lists().length, 3, "three attempts in all");
  assert.equal(timers.pending(), 0);
  assert.equal(watcher.view.status, "failed");
  assert.equal(watcher.view.error.message, "Stargate is busy. Wait 2 seconds, then select Refresh.");
  assert.ok(lists().every((call) => call.query.cursor === undefined), "the identical request");
});

test("diff status: 503 with no Retry-After waits a second; over ten seconds stops at once", async () => {
  const short = watch({ list: (_, n) => (n === 0 ? Promise.reject(new HttpError(503)) : page([file("a.txt")])) });
  short.watcher.start();
  await flush();
  assert.deepEqual(short.timers.delays(), [1000]);
  short.timers.fire();
  await flush();
  assert.equal(short.watcher.view.status, "ready");
  const long = watch({ list: () => Promise.reject(new HttpError(503, 30_000)) });
  long.watcher.start();
  await flush();
  assert.equal(long.timers.pending(), 0, "no wait over ten seconds");
  assert.equal(long.lists().length, 1);
  assert.equal(long.watcher.view.error.message, "Stargate is busy. Wait 30 seconds, then select Refresh.");
});

test("diff status: an empty snapshot is no changes, not an error", async () => {
  const { watcher } = watch({ list: () => page([]) });
  watcher.start();
  await flush();
  assert.equal(watcher.view.status, "ready");
  assert.deepEqual(watcher.view.files, []);
});

// ---------- list paging ----------

test("diff list: reads every page by cursor until the total is listed", async () => {
  const all = [file("a"), file("b"), file("c"), file("d"), file("e")];
  const { watcher, lists } = watch({
    list: (query) => {
      const start = query.cursor ? Number(query.cursor) : 0;
      const next = start + 2 < all.length ? String(start + 2) : undefined;
      return page(all.slice(start, start + 2), { total: all.length, ...(next ? { nextCursor: next } : {}) });
    },
  });
  watcher.start();
  await flush();
  assert.equal(watcher.view.status, "ready");
  assert.deepEqual(watcher.view.files.map((entry) => entry.file.path), ["a", "b", "c", "d", "e"]);
  assert.deepEqual(lists().map((call) => call.query.cursor), [undefined, "2", "4"]);
  assert.ok(lists().every((call) => call.query.project === "subspace" && call.query.mode === "branch"));
});

test("diff list: a later page with another fingerprint, or a 404, means the checkout moved on", async () => {
  for (const later of [() => page([file("b")], { total: 2, fingerprint: "fp-2" }), () => Promise.reject(new HttpError(404))]) {
    const { watcher } = watch({ list: (query) => (query.cursor ? later() : page([file("a")], { total: 2, nextCursor: "1" })) });
    watcher.start();
    await flush();
    assert.equal(watcher.view.status, "failed");
    assert.equal(watcher.view.error.kind, "stale");
    assert.equal(watcher.view.error.message, DIFF_MESSAGES.stale);
  }
});

test("diff list: a repeated cursor, a page past the total, an empty later page or a short list is refused", async () => {
  const bad = [
    (query) => page([file(query.cursor ?? "a")], { total: 3, nextCursor: "same" }),
    (query) => (query.cursor ? page([file("b")], { total: 1 }) : page([file("a")], { total: 1, nextCursor: "1" })),
    (query) => (query.cursor ? page([], { total: 2 }) : page([file("a")], { total: 2, nextCursor: "1" })),
    () => page([file("a")], { total: 2 }),
    (query) => (query.cursor ? page([file("b")], { total: 2, member: "apollo" }) : page([file("a")], { total: 2, nextCursor: "1" })),
  ];
  for (const list of bad) {
    const { watcher, lists } = watch({ list });
    watcher.start();
    await flush();
    assert.equal(watcher.view.status, "failed");
    assert.equal(watcher.view.error.kind, "invalid", JSON.stringify(watcher.view.error));
    assert.ok(lists().length <= 3, "no endless paging");
  }
});

test("diff list: an unknown status or kind is kept and shown, never dropped", () => {
  assert.equal(isDiffFile(file("x", { status: "copied", kind: "worktree" })), true);
  assert.equal(isDiffFile({ path: 1 }), false);
});

// ---------- patch paging ----------

test("diff patch: Load next page appends pages pinned to the fingerprint until there are none", async () => {
  const pages = { p2: { patch: "@@ -10,1 +10,1 @@\n-x\n+y\n", next: "p3" }, p3: { patch: "@@ -20,1 +20,1 @@\n-z\n+w\n" } };
  const { watcher, files } = watch({
    list: () => page([file("big.ts", { patchNextPage: "p2" })]),
    file: (query) => ({ fingerprint: FP, file: file("big.ts", { patch: pages[query.page].patch, ...(pages[query.page].next ? { patchNextPage: pages[query.page].next } : {}) }) }),
  });
  watcher.start();
  await flush();
  assert.equal(watcher.view.files[0].next, "p2");
  watcher.loadPage(0);
  await flush();
  assert.deepEqual(files()[0].query, { project: "subspace", mode: "branch", path: "big.ts", page: "p2", at: FP });
  watcher.loadPage(0);
  await flush();
  const entry = watcher.view.files[0];
  assert.equal(entry.pages.length, 3);
  assert.equal(entry.next, null, "the last page names no next one");
  watcher.loadPage(0);
  await flush();
  assert.equal(files().length, 2, "nothing more to read");
  const lines = patchLines(entry.pages.join(""));
  assert.deepEqual(lines.filter((line) => line.kind === "add").map((line) => line.new), [1, 10, 20]);
});

test("diff patch: a failed page offers Retry page, which reads the same page again", async () => {
  const { watcher, files } = watch({
    list: () => page([file("big.ts", { patchNextPage: "p2" })]),
    file: (_, n) => (n === 0 ? Promise.reject(new HttpError(500)) : { fingerprint: FP, file: file("big.ts", { patch: "+more\n" }) }),
  });
  watcher.start();
  await flush();
  watcher.loadPage(0);
  await flush();
  assert.equal(watcher.view.files[0].page.state, "failed");
  assert.equal(watcher.view.files[0].page.error.message, DIFF_MESSAGES.failed);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  const retry = panel.root.findAll((node) => node.tag === "button" && node.textContent === "Retry page");
  assert.equal(retry.length, 1);
  panel.root.listeners.click({ target: retry[0] });
  await flush();
  assert.deepEqual(files().map((call) => call.query.page), ["p2", "p2"]);
  assert.equal(watcher.view.files[0].pages.length, 2);
  assert.equal(watcher.view.files[0].page, null);
});

test("diff patch: busy and gone pages say what to do", async () => {
  const busy = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(503, 60_000)) });
  busy.watcher.start();
  await flush();
  busy.watcher.loadPage(0);
  await flush();
  assert.equal(busy.watcher.view.files[0].page.error.message, "Stargate is busy. Wait 60 seconds, then select Retry page.");
  const gone = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(404)) });
  gone.watcher.start();
  await flush();
  gone.watcher.loadPage(0);
  await flush();
  assert.equal(gone.watcher.view.files[0].page.error.message, DIFF_MESSAGES.missing);
});

test("diff patch: a page that points back at itself or is for another path is refused", async () => {
  for (const answer of [{ fingerprint: FP, file: file("a", { patch: "+x\n", patchNextPage: "p2" }) }, { fingerprint: FP, file: file("b", { patch: "+x\n" }) }]) {
    const { watcher } = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => answer });
    watcher.start();
    await flush();
    watcher.loadPage(0);
    await flush();
    assert.equal(watcher.view.files[0].page.error.kind, "invalid");
  }
});

test("diff patch: a 409 for a stale `at`, or a page at another fingerprint, leads to Refresh", async () => {
  for (const answer of [() => Promise.reject(new HttpError(409)), () => ({ fingerprint: "fp-2", file: file("a", { patch: "+x\n" }) })]) {
    const { watcher, lists } = watch({ list: (_, n) => page([file("a", { patchNextPage: "p2" })], { fingerprint: n === 0 ? FP : "fp-2" }), file: answer });
    watcher.start();
    await flush();
    watcher.loadPage(0);
    await flush();
    assert.equal(watcher.view.banner, DIFF_MESSAGES.changed, "Changed since loaded");
    assert.equal(watcher.view.files[0].pages.length, 1, "nothing from the stale page is shown");
    const panel = createDiffPanel({ doc: fakeDoc, watcher });
    panel.update(watcher.view, { name: "Starbuck" });
    const banner = panel.root.findAll((node) => node.attributes.class === "act-warn diff-banner")[0];
    assert.match(banner.textContent, /^Changed since loaded · Refresh$/);
    panel.root.listeners.click({ target: banner.findAll((node) => node.tag === "button")[0] });
    await flush();
    assert.equal(lists().length, 2, "Refresh reads the snapshot again");
    assert.equal(watcher.view.banner, null);
    assert.equal(watcher.view.diff.fingerprint, "fp-2");
  }
});

// ---------- binary pages ----------

function binaryRoutes(sides, { pageBytes = BINARY_PAGE_BYTES, extra = () => ({}) } = {}) {
  return {
    list: () => page([file("img.bin", { binary: true, size: sides.new?.length ?? 0, additions: null, deletions: null, patch: null })]),
    file: (query) => {
      const bytes = sides[query.side];
      const offset = query.page ? Number(query.page) : 0;
      const end = Math.min(bytes.length, offset + pageBytes);
      const content = { side: query.side, encoding: "base64", offset, size: bytes.length, ...(end > offset ? { data: b64(bytes.subarray(offset, end)) } : {}), ...(end < bytes.length ? { nextPage: String(end) } : {}), ...extra(offset, end, bytes.length) };
      return { fingerprint: FP, file: file("img.bin", { binary: true, patch: null }), content };
    },
  };
}

test("diff binary: an empty file is one page of size 0", async () => {
  const { watcher, files } = watch(binaryRoutes({ old: new Uint8Array(0), new: new Uint8Array(3) }));
  watcher.start();
  await flush();
  watcher.openBinary(0, "old");
  await flush();
  assert.equal(watcher.view.binary.state, "ready");
  assert.equal(watcher.view.binary.page.size, 0);
  assert.equal(watcher.view.binary.page.nextPage, null);
  assert.equal(files().length, 1);
  assert.deepEqual(files()[0].query, { project: "subspace", mode: "branch", path: "img.bin", page: undefined, at: FP, side: "old" });
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /Old side: empty file \(0 bytes\)\./);
});

test("diff binary: a page ending exactly at the size is the last, and pages follow by offset", async () => {
  const bytes = Uint8Array.from({ length: 2 * BINARY_PAGE_BYTES }, (_, i) => i & 0xff);
  const { watcher, files } = watch(binaryRoutes({ old: bytes, new: bytes }));
  watcher.start();
  await flush();
  watcher.openBinary(0, "new");
  await flush();
  const view = () => watcher.view.binary;
  assert.equal(view().page.offset, 0);
  assert.equal(view().page.nextPage, String(BINARY_PAGE_BYTES));
  for (let i = 0; i < BINARY_PAGE_BYTES / HEX_WINDOW_BYTES - 1; i += 1) watcher.moveBinary("next");
  assert.equal(files().length, 1, "windows within a page read nothing");
  watcher.moveBinary("next");
  await flush();
  assert.equal(files()[1].query.page, String(BINARY_PAGE_BYTES));
  assert.equal(view().page.offset, BINARY_PAGE_BYTES);
  assert.equal(view().page.offset + view().page.bytes.length, view().page.size, "ends exactly at the size");
  assert.equal(view().page.nextPage, null, "so it has no next page");
  watcher.moveBinary("previous");
  await flush();
  assert.equal(files()[2].query.page, undefined, "Previous from a page's first window reads the page before");
  assert.equal(view().window, BINARY_PAGE_BYTES / HEX_WINDOW_BYTES - 1, "and lands on its last window");
});

test("diff binary: a page that ends at the size but names a next page is still the last", () => {
  const content = validateContent(
    { fingerprint: FP, file: { path: "a" }, content: { side: "new", encoding: "base64", offset: 0, size: 3, data: b64([1, 2, 3]), nextPage: "x" } },
    { path: "a", at: FP, side: "new", page: null, offset: 0, size: null },
  );
  assert.equal(content.nextPage, null);
});

test("diff binary: wrong side, offset, encoding, size or a short page without a next page is refused", () => {
  const ask = { path: "a", at: FP, side: "new", page: null, offset: 0, size: null };
  const good = { side: "new", encoding: "base64", offset: 0, size: 4, data: b64([1, 2]), nextPage: "2" };
  const answer = (content) => ({ fingerprint: FP, file: { path: "a" }, content: { ...good, ...content } });
  assert.equal(validateContent(answer({}), ask).nextPage, "2");
  for (const bad of [{ side: "old" }, { encoding: "hex" }, { offset: 1 }, { size: 1 }, { nextPage: undefined }, { data: "!!!!" }, { data: undefined }]) {
    assert.throws(() => validateContent(answer(bad), ask), /binary page|base64/i, JSON.stringify(bad));
  }
  assert.throws(() => validateContent(answer({}), { ...ask, size: 9 }), /another size/);
  assert.throws(() => validateContent(answer({ data: b64(new Uint8Array(BINARY_PAGE_BYTES + 3)), size: BINARY_PAGE_BYTES + 9 }), ask), /at most 1 MiB/);
  assert.throws(() => validateContent({ ...answer({}), fingerprint: "fp-2" }, ask), /changed while loading/);
});

test("diff binary: a failed page offers Retry page for that page", async () => {
  const routes = binaryRoutes({ new: Uint8Array.from([1, 2, 3]) });
  let fail = true;
  const { watcher, files } = watch({ ...routes, file: (query, n) => (fail && n === 0 ? Promise.reject(new HttpError(500)) : routes.file(query)) });
  watcher.start();
  await flush();
  watcher.openBinary(0, "new");
  await flush();
  assert.equal(watcher.view.binary.state, "failed");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  const retry = panel.root.findAll((node) => node.tag === "button" && node.textContent === "Retry page");
  assert.equal(retry.length, 1);
  fail = false;
  panel.root.listeners.click({ target: retry[0] });
  await flush();
  assert.equal(files().length, 2);
  assert.equal(watcher.view.binary.state, "ready");
  assert.deepEqual([...watcher.view.binary.page.bytes], [1, 2, 3]);
});

test("diff binary: sides, hex rows and image signatures", () => {
  assert.deepEqual(binarySides({ status: "added" }), ["new"]);
  assert.deepEqual(binarySides({ status: "removed" }), ["old"]);
  assert.deepEqual(binarySides({ status: "modified" }), ["old", "new"]);
  assert.deepEqual(hexRows(Uint8Array.from([0x41, 0x00, 0x7f, 0x20, 0x7e, 1, 2, 3, 0x42]), 16), ["0000000000000010  41 00 7f 20 7e 01 02 03  A.. ~...", "0000000000000018  42                       B"]);
  assert.equal(imageType(Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])), "image/png");
  assert.equal(imageType(Uint8Array.from([1, 2, 3])), null);
});

// ---------- the fingerprint banner ----------

test("diff banner: the ship event's fingerprint moving shows Changed since loaded; a repeat or none shows nothing", async () => {
  const { watcher } = watch({ list: () => page([file("a")]) });
  watcher.observe(FP);
  watcher.start();
  await flush();
  assert.equal(watcher.view.banner, null);
  watcher.observe(FP);
  watcher.observe(undefined);
  watcher.observe(null);
  assert.equal(watcher.view.banner, null, "the same fingerprint, or none, proves nothing");
  watcher.observe("fp-2");
  assert.equal(watcher.view.banner, DIFF_MESSAGES.changed);
  watcher.observe(FP);
  assert.equal(watcher.view.banner, null, "back to the loaded fingerprint");
});

test("diff banner: a fingerprint that moved while the snapshot loaded shows once it lands", async () => {
  let release;
  const { watcher } = watch({ list: () => new Promise((resolve) => (release = () => resolve(page([file("a")])))) });
  watcher.observe("fp-0");
  watcher.start();
  await flush();
  watcher.observe("fp-9");
  release();
  await flush();
  assert.equal(watcher.view.banner, DIFF_MESSAGES.changed);
  const quiet = watch({ list: () => page([file("a")]) });
  quiet.watcher.observe("fp-0");
  quiet.watcher.start();
  await flush();
  assert.equal(quiet.watcher.view.banner, null, "a live value seen before the load started is not a change");
});

test("diff banner: unstable shows Changed while loading", async () => {
  const { watcher } = watch({ list: () => page([file("a")], { unstable: true }) });
  watcher.start();
  await flush();
  assert.equal(watcher.view.banner, DIFF_MESSAGES.unstable);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /Changed while loading · Refresh/);
});

// ---------- cancellation ----------

test("diff cancel: a page read waiting in backoff is cancelled when the fingerprint goes stale", async () => {
  const { watcher, files, timers } = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(503, 2000)) });
  watcher.start();
  await flush();
  watcher.loadPage(0);
  await flush();
  assert.equal(timers.pending(), 1, "waiting to retry");
  watcher.observe("fp-2");
  assert.equal(timers.pending(), 0, "the wait is cancelled");
  timers.fire();
  await flush();
  assert.equal(files().length, 1, "no read at the stale fingerprint");
  assert.equal(watcher.view.files[0].page, null);
});

test("diff cancel: closing the view cancels reads in backoff and in flight", async () => {
  const { watcher, files, lists, timers, changes } = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(503, 2000)) });
  watcher.start();
  await flush();
  watcher.loadPage(0);
  await flush();
  const before = changes();
  watcher.stop();
  assert.equal(timers.pending(), 0);
  timers.fire();
  await flush();
  assert.equal(files().length, 1);
  assert.equal(changes(), before, "nothing reaches a closed view");
  // A list read in flight when the view closes: its signal aborts and its answer is dropped.
  let release;
  const held = watch({ list: () => new Promise((resolve) => (release = () => resolve(page([file("a")])))) });
  held.watcher.start();
  await flush();
  held.watcher.stop();
  assert.equal(held.lists()[0].signal.aborted, true);
  release();
  await flush();
  assert.deepEqual(held.watcher.view.files, []);
  assert.equal(lists().length, 1);
});

test("diff cancel: Refresh and a mode change drop the old snapshot's reads", async () => {
  let release;
  const { watcher, lists } = watch({
    list: (query, n) => (n === 0 ? new Promise((resolve) => (release = () => resolve(page([file("old")])))) : page([file("new")], { mode: query.mode })),
  });
  watcher.start();
  await flush();
  watcher.setMode("working");
  await flush();
  release();
  await flush();
  assert.deepEqual(watcher.view.files.map((entry) => entry.file.path), ["new"], "the old answer never lands");
  assert.equal(lists()[0].signal.aborted, true);
  assert.equal(lists()[1].query.mode, "working");
  assert.equal(watcher.view.mode, "working");
});

test("diff cancel: a binary page in backoff is cancelled by Hide bytes", async () => {
  const routes = binaryRoutes({ new: Uint8Array.from([1]) });
  const { watcher, files, timers } = watch({ ...routes, file: () => Promise.reject(new HttpError(503, 1000)) });
  watcher.start();
  await flush();
  watcher.openBinary(0, "new");
  await flush();
  assert.equal(timers.pending(), 1);
  watcher.closeBinary();
  assert.equal(timers.pending(), 0);
  assert.equal(watcher.view.binary, null);
  assert.equal(files().length, 1);
});

test("diff cancel: a page answer still in flight when the fingerprint goes stale is dropped", async () => {
  let release;
  const { watcher } = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => new Promise((resolve) => (release = () => resolve({ fingerprint: FP, file: file("a", { patch: "+late\n" }) }))) });
  watcher.start();
  await flush();
  watcher.loadPage(0);
  await flush();
  watcher.observe("fp-2");
  release();
  await flush();
  assert.equal(watcher.view.files[0].pages.length, 1, "the late page is not shown");
  assert.equal(watcher.view.files[0].page, null);
  assert.equal(watcher.view.banner, DIFF_MESSAGES.changed);
});

// ---------- source change ----------

test("diff source: an address or token change drops what was read and reads from the new client", async () => {
  const tokens = [];
  const held = [];
  const fetchImpl = async (url, init) => {
    tokens.push(init.headers.Authorization);
    if (url.endsWith("/fleet")) return { ok: false, status: 503, headers: { get: () => null } };
    const body = JSON.stringify(page([file(init.headers.Authorization)]));
    const answer = { ok: true, status: 200, headers: { get: () => null }, json: async () => JSON.parse(body), text: async () => body };
    if (init.headers.Authorization === "Bearer old") return new Promise((resolve) => held.push(() => resolve(answer)));
    return answer;
  };
  const entry = (token) => ({ id: "f1", endpoint: "https://fleet.example.com", token, origin: "local" });
  const manager = createFleetManager({
    entries: [entry("old")],
    createClient: (fleetEntry) => createApiClient({ endpoint: fleetEntry.endpoint, token: fleetEntry.token, fetch: fetchImpl }),
    createLive: () => ({ start() {}, stop() {}, setAgents() {} }),
    setTimer: () => 0,
    clearTimer: () => {},
  });
  manager.activate("f1");
  await flush();
  let watcher = null;
  const ended = [];
  const handle = manager.openDiff("f1", { onReset: () => watcher?.reset(), onEnded: () => ended.push(true) });
  watcher = createDiffWatcher({ client: handle.client, target: TARGET, setTimer: () => 0, clearTimer: () => {} });
  watcher.start();
  await flush();
  assert.equal(held.length, 1, "a read held on the old source");
  manager.update([entry("new")]);
  await flush();
  for (const release of held) release();
  await flush();
  assert.equal(watcher.view.status, "ready");
  assert.deepEqual(watcher.view.files.map((e) => e.file.path), ["Bearer new"], "the old answer never lands");
  manager.update([entry("")]);
  assert.equal(ended.length, 1, "the token removed: the view ends");
  await assert.rejects(handle.client.diff("galactica", "starbuck", { mode: "branch" }), /no longer has a fleet/);
  watcher.stop();
  manager.stop();
});

test("diff source: the sample fleet serves a checkout with paging, markers and binary sides", async () => {
  const still = { setTimer: () => 0, clearTimer() {}, every: () => 0, stopEvery() {} };
  const manager = createSampleFleetManager({ createSource: ({ variant, onEvent }) => createSampleSource({ variant, onEvent, ...still }), createDiff: (options) => createSampleDiffClient(options) });
  manager.start();
  const id = manager.list()[0].id;
  const fleet = manager.get(id);
  assert.equal(diffsEnabled(fleet.state.fleet), true, "the sample gateway lists api.crewDiffs");
  const [ship, detail] = Object.entries(fleet.state.ships).find(([, d]) => d.crewMembers.some((m) => m.contexts.some((c) => c.git)));
  const member = detail.crewMembers.find((m) => m.contexts.some((c) => c.git));
  const context = member.contexts.find((c) => c.git);
  assert.equal(typeof context.git.fingerprint, "string");
  const handle = manager.openDiff(id);
  const watcher = createDiffWatcher({ client: handle.client, target: { ship, member: member.id, project: context.project ?? null }, setTimer: () => 0, clearTimer: () => {} });
  watcher.observe(context.git.fingerprint);
  watcher.start();
  await flush();
  assert.equal(watcher.view.status, "ready", JSON.stringify(watcher.view.error));
  assert.equal(watcher.view.files.length, watcher.view.diff.total);
  assert.ok(watcher.view.files.length > 4, "more than one list page");
  const route = watcher.view.files.findIndex((e) => e.next);
  watcher.loadPage(route);
  await flush();
  assert.equal(watcher.view.files[route].pages.length, 2);
  const chart = watcher.view.files.findIndex((e) => e.file.path === "assets/chart.bin");
  watcher.openBinary(chart, "old");
  await flush();
  assert.equal(watcher.view.binary.page.size, 0, "an empty old side");
  watcher.openBinary(chart, "new");
  await flush();
  assert.equal(watcher.view.binary.page.bytes.length, BINARY_PAGE_BYTES);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  const notes = watcher.view.files.findIndex((e) => e.file.path === "docs/notes.md");
  panel.update(watcher.view, { name: member.name });
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.attributes["data-diff-select"] === String(notes))[0] });
  assert.match(panel.root.textContent, /⟨U\+202E⟩evil⟨U\+202C⟩/, "bidi controls show as markers");
  assert.match(panel.root.textContent, /⟨U\+0007⟩/);
  watcher.stop();
  manager.stop();
});
