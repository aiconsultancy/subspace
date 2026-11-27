import assert from "node:assert/strict";
import { test } from "node:test";
import { setFlagsFromString } from "node:v8";
import { runInNewContext } from "node:vm";
import { createApiClient, HttpError, MAX_DIFF_BODY_BYTES, MAX_DIFF_FILE_BODY_BYTES } from "../src/api.js";
import {
  BINARY_PAGE_BYTES,
  BINARY_PART_BYTES,
  binarySides,
  canViewDiff,
  createDiffWatcher,
  DIFF_MESSAGES,
  diffPathAllowed,
  diffsEnabled,
  hexRows,
  HEX_ROW_BYTES,
  HEX_WINDOW_BYTES,
  hexShownBytes,
  imageType,
  isDiffFile,
  validateContent,
} from "../src/diff.js";
import { createDiffController } from "../src/diff-controller.js";
import { createDiffPanel, partWindows } from "../src/diff-view.js";
import { WINDOW_BYTES } from "../src/document.js";
import { base64Length, bytesOf, PATCH_START, patchLines } from "../src/diff-parts.js";
import { crewPanel } from "../src/inspector.js";
import { isGit } from "../src/contract.js";
import { createFleetManager, createSampleFleetManager } from "../src/fleet-manager.js";
import { createSampleDiffClient } from "../src/sample-diff.js";
import { createSampleSource } from "../src/sample-source.js";
import { sampleResolve } from "../src/sample.js";
import { loadedState, NOW } from "./fixtures.js";
import { clickOn, fakeActivityClient, fakeDoc, fakeTimers, officer, pagedWith, settle } from "./activity-helpers.js";
import { activityEvent, activitySnapshot, textItem } from "./activity-fixtures.js";
import { createActivityPanel } from "../src/activity-view.js";
import { createActivityWatcher, INLINE_KEY } from "../src/activity.js";
import { createBudget } from "../src/budget.js";

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
 * recorded, with its signal. As the gateway, a patch's first file page
 * (no `page`, no `side`) is the patch the list gave for that path, with its
 * `patchNextPage`, at the list's fingerprint; `first` replaces that answer,
 * and `file` hears only the rest (`n` counts what it heard).
 */
function fakeClient({ list = () => page([file("a.txt")]), file: fileRoute = () => Promise.reject(new HttpError(404)), first = null } = {}) {
  const calls = [];
  const listed = new Map();
  let at = null;
  let heard = 0;
  const firstPage = (query) => {
    const row = listed.get(query.path);
    if (!row || query.at !== at) return Promise.reject(new HttpError(row ? 409 : 404));
    return { fingerprint: at, file: row };
  };
  return {
    calls,
    lists: () => calls.filter((call) => call.kind === "list"),
    files: () => calls.filter((call) => call.kind === "file"),
    client: {
      diff: async (ship, member, query, signal) => {
        const n = calls.filter((call) => call.kind === "list").length;
        calls.push({ kind: "list", ship, member, query, signal });
        const answer = await list(query, n);
        if (answer?.files) {
          if (answer.fingerprint !== at) listed.clear();
          at = answer.fingerprint;
          for (const row of answer.files) if (!listed.has(row.path)) listed.set(row.path, row);
        }
        return answer;
      },
      diffFile: async (ship, member, query, signal) => {
        calls.push({ kind: "file", ship, member, query, signal });
        if (query.page === undefined && query.side === undefined) return (first ?? firstPage)(query);
        heard += 1;
        return fileRoute(query, heard - 1);
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
  await client.diff("g", "s", { project: "subspace", mode: "working", page: "c 2" }).catch(() => {});
  assert.equal(seen[1].url, "https://fleet.example.com/api/v1/ships/g/crew/s/diff?mode=working&project=subspace&page=c+2");
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
  assert.ok(lists().every((call) => call.query.page === undefined), "the identical request");
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

test("diff list: reads every page by its nextPage token until the total is listed", async () => {
  const all = [file("a"), file("b"), file("c"), file("d"), file("e")];
  const { watcher, lists } = watch({
    list: (query) => {
      const start = query.page ? Number(query.page) : 0;
      const next = start + 2 < all.length ? String(start + 2) : undefined;
      return page(all.slice(start, start + 2), { total: all.length, ...(next ? { nextPage: next } : {}) });
    },
  });
  watcher.start();
  await flush();
  assert.equal(watcher.view.status, "ready");
  assert.deepEqual(watcher.view.files.map((entry) => entry.file.path), ["a", "b", "c", "d", "e"]);
  assert.deepEqual(lists().map((call) => call.query.page), [undefined, "2", "4"]);
  assert.ok(lists().every((call) => call.query.project === "subspace" && call.query.mode === "branch"));
});

test("diff list: the real client follows an opaque nextPage token with page= (STAR-5), never cursor=", async () => {
  const seen = [];
  const client = createApiClient({
    endpoint: "https://fleet.example.com",
    token: "fixture",
    fetch: async (url) => {
      const query = new URL(url).searchParams;
      seen.push(query.get("page"));
      if (query.has("cursor")) return new Response(null, { status: 400 });
      const second = query.get("page") === "opaque+/=";
      return Response.json(second ? page([file("b.txt")], { total: 2 }) : page([file("a.txt")], { total: 2, nextPage: "opaque+/=" }));
    },
  });
  const watcher = createDiffWatcher({ client, target: TARGET, onChange() {} });
  watcher.start();
  await settle();
  assert.equal(watcher.view.status, "ready", watcher.view.error?.message);
  assert.deepEqual(watcher.view.files.map(({ file }) => file.path), ["a.txt", "b.txt"]);
  assert.deepEqual(seen, [null, "opaque+/="], "the token goes back byte for byte, encoded once");
});

test("diff list: a later page with another fingerprint, or a 404, means the checkout moved on", async () => {
  for (const later of [() => page([file("b")], { total: 2, fingerprint: "fp-2" }), () => Promise.reject(new HttpError(404))]) {
    const { watcher } = watch({ list: (query) => (query.page ? later() : page([file("a")], { total: 2, nextPage: "1" })) });
    watcher.start();
    await flush();
    assert.equal(watcher.view.status, "failed");
    assert.equal(watcher.view.error.kind, "stale");
    assert.equal(watcher.view.error.message, DIFF_MESSAGES.stale);
  }
});

test("diff list: a repeated page token, a page past the total, an empty later page or a short list is refused", async () => {
  const bad = [
    (query) => page([file(query.page ?? "a")], { total: 3, nextPage: "same" }),
    (query) => (query.page ? page([file("b")], { total: 1 }) : page([file("a")], { total: 1, nextPage: "1" })),
    (query) => (query.page ? page([], { total: 2 }) : page([file("a")], { total: 2, nextPage: "1" })),
    () => page([file("a")], { total: 2 }),
    (query) => (query.page ? page([file("b")], { total: 2, member: "apollo" }) : page([file("a")], { total: 2, nextPage: "1" })),
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

// A garbage collection on demand, for checks that a value is no longer reachable.
setFlagsFromString("--expose-gc");
const gc = runInNewContext("gc");
/** Collect garbage until weak references to unreachable values clear: each turn lets go of the last one's. */
const collect = async () => {
  for (let i = 0; i < 4; i += 1) {
    await new Promise((resolve) => setImmediate(resolve));
    gc();
  }
};

test("diff list: while a later page is pending, neither the first page's raw answer nor any raw row of it is reachable", async () => {
  let firstRow = null;
  let firstAnswer = null;
  let asked;
  const reached = new Promise((resolve) => (asked = resolve));
  let release;
  const pending = new Promise((resolve) => (release = resolve));
  const client = {
    diff: async (ship, member, query) => {
      if (query.page !== undefined) {
        asked();
        return pending;
      }
      // A gateway-sized first page: 64 rows, each with a 64 KiB inline patch of its own.
      const files = Array.from({ length: 64 }, (_, i) => file(`a${i}.txt`, { patch: `+${String(i).padStart(4, "0")}${"x".repeat(64 * 1024)}\n` }));
      const answer = page(files, { total: 65, nextPage: "more" });
      firstRow = new WeakRef(files[0]);
      firstAnswer = new WeakRef(answer);
      return answer;
    },
    diffFile: async () => Promise.reject(new HttpError(404)),
  };
  const watcher = createDiffWatcher({ client, target: TARGET, onChange: () => {}, ...fakeTimers().options });
  watcher.start();
  await reached;
  await collect();
  assert.equal(watcher.view.status, "loading");
  assert.equal(firstRow.deref(), undefined, "the first page's raw row was let go before the second page was asked for");
  assert.equal(firstAnswer.deref(), undefined, "nor the answer that held it");
  release(page([file("last.txt")], { total: 65 }));
  await flush();
  assert.equal(watcher.view.status, "ready");
  assert.equal(watcher.view.files.length, 65);
  assert.equal(watcher.view.files[0].file.path, "a0.txt", "its metadata is kept");
  assert.equal(watcher.view.files[0].file.patch, undefined);
  watcher.stop();
});

test("diff list: an unknown status or kind is kept and shown, never dropped", () => {
  assert.equal(isDiffFile(file("x", { status: "copied", kind: "worktree" })), true);
  assert.equal(isDiffFile({ path: 1 }), false);
});

// ---------- patch parts (SUB decision 75c3ed93 §1, §4) ----------

/** Pages of a patch by page token: `p2`, `p3`, ... each a hunk, the last naming no next page. */
const PATCH_PAGES = {
  p2: { patch: "@@ -10,1 +10,1 @@\n-x\n+y\n", next: "p3" },
  p3: { patch: "@@ -20,1 +20,1 @@\n-z\n+w\n" },
};

const patchPage = (path, at = FP) => (query) => ({ fingerprint: at, file: file(path, { patch: PATCH_PAGES[query.page].patch, ...(PATCH_PAGES[query.page].next ? { patchNextPage: PATCH_PAGES[query.page].next } : {}) }) });
const buttonsIn = (root, words) => root.findAll((node) => node.tag === "button" && node.textContent === words);
const added = (panel) => panel.root.findAll((node) => node.attributes.class === "dl add").map((node) => node.children[1].textContent);

test("diff patch: one part at a time, each a gateway page pinned to the fingerprint, with First, Previous and Next; Last once the count is known", async () => {
  const { watcher, files } = watch({ list: () => page([file("big.ts", { patchNextPage: "p2" })]), file: patchPage("big.ts") });
  watcher.start();
  await flush();
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  const patch = () => watcher.view.patch(0);
  assert.equal(patch().part, 0);
  assert.deepEqual(files().map((call) => call.query), [{ project: "subspace", mode: "branch", path: "big.ts", page: undefined, at: FP }], "the first part is the file route's first page at the snapshot, not the list's inline copy");
  assert.match(panel.root.textContent, /Part 1 of 2\+/);
  assert.deepEqual(buttonsIn(panel.root, "Last part"), [], "no Last while the count is unknown");
  panel.root.listeners.click({ target: buttonsIn(panel.root, "Next part")[0] });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.deepEqual(files()[1].query, { project: "subspace", mode: "branch", path: "big.ts", page: "p2", at: FP });
  assert.equal(patch().data, PATCH_PAGES.p2.patch, "the part replaces the one before: nothing appends");
  assert.deepEqual(added(panel), ["10"], "line numbers from the part's own hunk");
  await watcher.movePatch(0, "next");
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(patch().count, 3, "the last page names no next one");
  assert.match(panel.root.textContent, /Part 3 of 3/);
  await watcher.movePatch(0, "first");
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(buttonsIn(panel.root, "Last part").length, 1, "Last, now that the count is known");
  const reads = files().length;
  panel.root.listeners.click({ target: buttonsIn(panel.root, "Last part")[0] });
  await flush();
  assert.equal(patch().part, 2);
  assert.equal(files().length, reads + 1, "Last reads only the last page");
  assert.equal(files().at(-1).query.page, "p3");
  assert.equal(watcher.view.budget.used, 2 * PATCH_PAGES.p3.patch.length, "only the part on show and its drawn window are charged");
});

test("diff patch: a patch with no next page is one part, with no part controls", async () => {
  const { watcher } = watch({ list: () => page([file("a.txt")]) });
  watcher.start();
  await flush();
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(watcher.view.patch(0).count, 1);
  assert.deepEqual(buttonsIn(panel.root, "Next part"), []);
  assert.deepEqual(added(panel), ["1"]);
});

test("diff patch: a part draws in windows of at most 1,000 lines, numbered across windows and parts", async () => {
  const long = `@@ -1,1500 +1,1500 @@\n${Array.from({ length: 1500 }, (_, i) => `+line ${i}`).join("\n")}\n`;
  const { watcher } = watch({ list: () => page([file("long.ts", { patch: long })]) });
  watcher.start();
  await flush();
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /Section 1 of 2/);
  assert.ok(added(panel).length < 1000);
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.attributes["data-diff-lines"] === "next")[0] });
  const second = added(panel);
  assert.equal(second.at(-1), "1500", "the second window numbers on from the first");
  assert.deepEqual(patchLines("+a\n+b", { old: 4, new: 7, open: null }).end, { old: 4, new: 9, open: "add", head: null }, "a split line carries its kind on");
  assert.deepEqual(patchLines("c\n+d\n", { old: 4, new: 9, open: "add" }).lines.map((line) => [line.kind, line.new, line.more ?? false]), [["add", null, true], ["add", 9, false]]);
});

/** Each line of a patch read in stretches, chained, as [kind, old, new] of the whole lines: a split line is numbered where it is counted. */
function chainedLines(stretches) {
  const whole = [];
  let start = PATCH_START;
  for (const stretch of stretches) {
    const { lines, end } = patchLines(stretch, start);
    for (const line of lines) {
      if (line.more) Object.assign(whole[whole.length - 1], { text: whole[whole.length - 1].text + line.text, kind: line.kind, old: whole[whole.length - 1].old ?? line.old, new: whole[whole.length - 1].new ?? line.new });
      else whole.push({ ...line });
    }
    start = end;
  }
  return whole.map((line) => [line.kind, line.text, line.old, line.new]);
}

const SPLIT_PATCH = "@@ -1,1 +1,2 @@\n ctx\n+a\n@@ -50,2 +60,1 @@ fn x\n-old\n ctx2\n\\ No newline at end of file\n@@ -90 +80 @@\n+new\n";

test("diff patch: a hunk header split anywhere by a part or window boundary numbers as the whole patch does, never counted as context", () => {
  const whole = chainedLines([SPLIT_PATCH]);
  assert.deepEqual(whole, [
    ["hunk", "@@ -1,1 +1,2 @@", null, null],
    ["ctx", " ctx", 1, 1],
    ["add", "+a", null, 2],
    ["hunk", "@@ -50,2 +60,1 @@ fn x", null, null],
    ["del", "-old", 50, null],
    ["ctx", " ctx2", 51, 60],
    ["note", "\\ No newline at end of file", null, null],
    ["hunk", "@@ -90 +80 @@", null, null],
    ["add", "+new", null, 80],
  ]);
  for (let i = 1; i < SPLIT_PATCH.length; i += 1) {
    assert.deepEqual(chainedLines([SPLIT_PATCH.slice(0, i), SPLIT_PATCH.slice(i)]), whole, `split at ${i}: ${JSON.stringify(SPLIT_PATCH.slice(0, i).split("\n").at(-1))}`);
    for (let j = i + 1; j < SPLIT_PATCH.length; j += 1) {
      assert.deepEqual(chainedLines([SPLIT_PATCH.slice(0, i), SPLIT_PATCH.slice(i, j), SPLIT_PATCH.slice(j)]), whole, `split at ${i} and ${j}`);
    }
  }
  const cut = patchLines("+a\n@").end;
  assert.deepEqual(cut, { old: 0, new: 1, open: "hunk", head: "@" }, "cut before the second @: carried, counted as nothing");
  assert.deepEqual(patchLines("@ x").lines.map((line) => [line.kind, line.old]), [["ctx", 0]], "not a header start: counted as its kind");
  assert.equal(patchLines(`@@ -${"9".repeat(80)}`).end.head, null, "the carried start is bounded");
});

test("diff patch: a hunk header split by a 64 KiB gateway page keeps its real line numbers in the next part", async () => {
  const opening = "@@ -1,0 +1,2 @@\n+start\n+";
  const partialHeader = "@@ -50,1 +";
  const prefix = `${opening}${"x".repeat(65_536 - opening.length - partialHeader.length - 1)}\n`;
  const { watcher } = watch({ list: () => page([file("a.txt", { patch: `${prefix}${partialHeader}`, patchNextPage: "p2" })]), file: () => ({ fingerprint: FP, file: file("a.txt", { patch: "60,1 @@\n-old\n+new\n" }) }) });
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  const first = patchLines(watcher.view.patch(0).data, watcher.view.patch(0).start).lines.at(-1);
  assert.deepEqual([first.kind, first.old, first.new], ["hunk", null, null], "the cut header is not context");
  await watcher.movePatch(0, "next");
  const { data, start } = watcher.view.patch(0);
  const lines = patchLines(data, start).lines;
  assert.equal(lines.find(({ text }) => text === "-old").old, 50);
  assert.equal(lines.find(({ text }) => text === "+new").new, 60);
});

test("diff patch: a hunk header split by a 64 KiB window boundary numbers the next window from it", () => {
  const header = "@@ -50,2 +60,1 @@ fn x\n";
  for (let k = 1; k < header.length; k += 1) {
    const filler = `+${"x".repeat(WINDOW_BYTES - k - 2)}\n`;
    const windows = partWindows(`${filler}${header}-old\n ctx2\n`, { old: 1, new: 1, open: null, head: null });
    assert.equal(windows.count, 2, `split ${k} into the header`);
    const cut = windows.lines(0).at(-1);
    assert.equal(cut.text, header.slice(0, k));
    assert.deepEqual([cut.kind, cut.old, cut.new], ["hunk", null, null], `split ${k}: not context`);
    const next = windows.lines(1);
    assert.deepEqual(next.slice(1).map((line) => [line.text, line.old, line.new]), [["-old", 50, null], [" ctx2", 51, 60]], `split ${k}`);
  }
});

test("diff patch: a failed page keeps the part on show and offers Retry page, which reads the same page again", async () => {
  const good = patchPage("big.ts");
  const { watcher, files } = watch({ list: () => page([file("big.ts", { patchNextPage: "p2" })]), file: (query, n) => (n === 0 ? Promise.reject(new HttpError(500)) : good(query)) });
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  await watcher.movePatch(0, "next");
  const patch = watcher.view.patch(0);
  assert.equal(patch.state, "failed");
  assert.equal(patch.part, 0, "the part on show stays");
  assert.equal(patch.data, file("big.ts").patch);
  assert.ok(watcher.view.budget.used > 0, "and stays charged");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, new RegExp(DIFF_MESSAGES.failed.replace(/[()]/g, "\\$&")));
  panel.root.listeners.click({ target: buttonsIn(panel.root, "Retry page")[0] });
  await flush();
  assert.deepEqual(files().map((call) => call.query.page), [undefined, "p2", "p2"]);
  assert.equal(watcher.view.patch(0).part, 1);
  assert.equal(watcher.view.patch(0).state, "ready");
});

test("diff patch: busy and gone pages say what to do", async () => {
  const busy = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(503, 60_000)) });
  busy.watcher.start();
  await flush();
  await busy.watcher.openPatch(0);
  await busy.watcher.movePatch(0, "next");
  assert.equal(busy.watcher.view.busy, "Stargate is busy. Wait 60 seconds, then select Retry page.");
  const gone = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(404)) });
  gone.watcher.start();
  await flush();
  await gone.watcher.openPatch(0);
  await gone.watcher.movePatch(0, "next");
  const panel = createDiffPanel({ doc: fakeDoc, watcher: gone.watcher });
  panel.update(gone.watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, new RegExp(DIFF_MESSAGES.missing));
});

test("diff patch: a page that points back at itself or is for another path is refused", async () => {
  for (const answer of [{ fingerprint: FP, file: file("a", { patch: "+x\n", patchNextPage: "p2" }) }, { fingerprint: FP, file: file("b", { patch: "+x\n" }) }]) {
    const { watcher } = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => answer });
    watcher.start();
    await flush();
    await watcher.openPatch(0);
    await watcher.movePatch(0, "next");
    assert.equal(watcher.view.patch(0).state, "failed");
    assert.equal(watcher.view.patch(0).part, 0);
  }
});

test("diff patch: a 409 for a stale `at`, or a page at another fingerprint, leads to Refresh", async () => {
  for (const answer of [() => Promise.reject(new HttpError(409)), () => ({ fingerprint: "fp-2", file: file("a", { patch: "+x\n" }) })]) {
    const { watcher, lists } = watch({ list: (_, n) => page([file("a", { patchNextPage: "p2" })], { fingerprint: n === 0 ? FP : "fp-2" }), file: answer });
    watcher.start();
    await flush();
    await watcher.openPatch(0);
    await watcher.movePatch(0, "next");
    assert.equal(watcher.view.banner, DIFF_MESSAGES.changed, "Changed since loaded");
    assert.equal(watcher.view.patch(0).part, 0, "nothing from the stale page is shown");
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

/** Two files whose patches are a page each beyond the inline one, in a budget with room for one part's read beside the other's part. */
/**
 * Two rows with one display path (two byte paths Git shows the same), one
 * on each list page; the second's patch runs past its inline part. `at`
 * gives the fingerprint of each list read, by its number.
 */
function sharedRows({ binary = false, at = () => FP } = {}) {
  const path = "name\uFFFD.txt";
  const row = (patch, extra = {}) => file(path, binary ? { binary: true, size: 3, additions: null, deletions: null, patch: null } : { patch, ...extra });
  return watch({
    list: (query, n) => {
      const fingerprint = at(n);
      if (query.page === undefined) return page([row("+first\n"), file("other.txt")], { total: 3, nextPage: "L2", fingerprint });
      return page([row("+second\n", { patchNextPage: "c2" })], { total: 3, fingerprint });
    },
  });
}

const selectRow = async (panel, watcher, index) => {
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.attributes["data-diff-select"] === String(index))[0] });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
};

test("diff patch: rows sharing a display path each show their own patch, read again from their own list page, never another file's (STAR-5)", async () => {
  const { watcher, files, lists } = sharedRows();
  watcher.start();
  await flush();
  assert.equal(lists().length, 2);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(watcher.view.patch(0).data, "+first\n");
  await selectRow(panel, watcher, 2);
  assert.equal(watcher.view.patch(2).data, "+second\n", "the second row's own patch, not the first's under the same path");
  assert.equal(watcher.view.patch(2).count, 1, "one part: its continuation cannot be addressed");
  assert.deepEqual(lists().slice(2).map((call) => call.query.page), [undefined, "L2"], "each read again from the list page that listed it");
  assert.deepEqual(files(), [], "no read by a path the gateway would answer with another file");
  assert.ok(panel.root.textContent.includes(DIFF_MESSAGES.sharedPatch), "says plainly the rest cannot be read here");
  assert.deepEqual(buttonsIn(panel.root, "Next part"), []);
  await selectRow(panel, watcher, 1);
  assert.equal(files().length, 1, "a path only one row has is read from the file route");
  assert.equal(watcher.view.patch(1).data, file("other.txt").patch);
  assert.ok(!panel.root.textContent.includes(DIFF_MESSAGES.sharedPatch));
});

test("diff patch: a shared row's list page at another fingerprint is stale: the Refresh banner, and no bytes shown", async () => {
  const { watcher, files } = sharedRows({ at: (n) => (n < 2 ? FP : "fp-2") });
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  assert.equal(watcher.view.patch(0).data, null);
  assert.equal(watcher.view.patch(0).state, "failed");
  assert.equal(watcher.view.banner, DIFF_MESSAGES.changed);
  assert.deepEqual(files(), []);
});

test("diff binary: rows sharing a display path offer no bytes or preview, and say why; nothing is read by that path", async () => {
  const { watcher, files } = sharedRows({ binary: true });
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
  await watcher.openPreview(2, "new");
  assert.equal(watcher.view.binary, null);
  assert.equal(watcher.view.preview, null);
  assert.deepEqual(files(), []);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.ok(panel.root.textContent.includes(DIFF_MESSAGES.sharedBytes));
  assert.deepEqual(buttonsIn(panel.root, "New bytes"), []);
});

test("diffPathAllowed mirrors STAR-5's file route path rule: no empty, . or .. component, NUL, backslash or control character, at most 4096 code points", () => {
  for (const path of ["src/a.ts", "a", "name\uFFFD.txt", ".hidden/x", "a..b", "\u{1F600}".repeat(4096), "é".repeat(4096)]) assert.equal(diffPathAllowed(path), true, JSON.stringify(path.slice(0, 20)));
  for (const path of ["", "/a", "a/", "a//b", "./a", "a/.", "../a", "a/../b", "a\u0000b", "a\\b", "only\tfile.txt", "a\nb", "a\u007fb", "a\u0085b", "\u{1F600}".repeat(4097), "a".repeat(4097)])
    assert.equal(diffPathAllowed(path), false, JSON.stringify(path.slice(0, 20)));
});

/**
 * One row whose unique path the file route refuses (a tab, as Git allows),
 * beside one it accepts, as a gateway that answers such a path 400 would.
 * `extra` adds to the refused row.
 */
function refusedPath(extra = {}) {
  const path = "only\tfile.txt";
  const patch = "@@ -1 +1 @@\n-old\n+new\n";
  return {
    path,
    patch,
    ...watch({
      list: () => page([file(path, { patch, ...extra }), file("other.txt")]),
      first: (query) => (diffPathAllowed(query.path) ? { fingerprint: FP, file: file(query.path) } : Promise.reject(new HttpError(400))),
    }),
  };
}

test("diff patch: a unique path the file route refuses (a tab) shows its intact inline patch, read again from its list page, with no /diff/file request", async () => {
  const { watcher, files, lists, path, patch } = refusedPath();
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  assert.equal(watcher.view.patch(0).state, "ready");
  assert.equal(watcher.view.patch(0).data, patch, "the whole patch the list gave");
  assert.equal(watcher.view.patch(0).count, 1);
  assert.deepEqual(files(), [], "no read by a path the file route refuses");
  assert.deepEqual(lists().map((call) => call.query.page), [undefined, undefined], "read again from the list page that listed it");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.ok(panel.root.textContent.includes("+new"));
  assert.ok(!panel.root.textContent.includes(DIFF_MESSAGES.pathPatch), "nothing is missing, so nothing to explain");
  assert.deepEqual(buttonsIn(panel.root, "Retry page"), []);
  await selectRow(panel, watcher, 1);
  assert.deepEqual(files().map((call) => call.query.path), ["other.txt"], "a path the route accepts is read from it");
  assert.equal(entryPath(watcher, 0), path);
});

const entryPath = (watcher, index) => watcher.view.files[index].file.path;

test("diff patch: a refused path whose patch runs past its inline part shows that part and says why the rest cannot be read, never a retry that always fails", async () => {
  const { watcher, files, patch } = refusedPath({ patchNextPage: "c2" });
  watcher.start();
  await flush();
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(watcher.view.patch(0).data, patch);
  assert.equal(watcher.view.patch(0).count, 1);
  assert.deepEqual(files(), []);
  assert.ok(panel.root.textContent.includes(DIFF_MESSAGES.pathPatch));
  assert.ok(!panel.root.textContent.includes(DIFF_MESSAGES.sharedPatch), "its own reason, not another's");
  assert.deepEqual(buttonsIn(panel.root, "Next part"), []);
  assert.deepEqual(buttonsIn(panel.root, "Retry page"), []);
});

test("diff binary: a refused path offers no bytes or preview and says why; nothing is read by that path", async () => {
  const { watcher, files } = refusedPath({ binary: true, size: 3, additions: null, deletions: null, patch: null });
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
  await watcher.openPreview(0, "new");
  assert.equal(watcher.view.binary, null);
  assert.equal(watcher.view.preview, null);
  assert.deepEqual(files(), []);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.ok(panel.root.textContent.includes(DIFF_MESSAGES.pathBytes));
  assert.deepEqual(buttonsIn(panel.root, "New bytes"), []);
  assert.deepEqual(buttonsIn(panel.root, "Retry page"), []);
});

function twoPatches(fileRoute) {
  const pad = (path) => `@@ -1,1 +1,1 @@\n-${"o".repeat(200)}\n+${path}\n`;
  const budget = createBudget(1024 * 1024 + 100);
  const routes = {
    list: () => page([file("a.ts", { patch: pad("a"), patchNextPage: "p2" }), file("b.ts", { patch: pad("b") })]),
    file: fileRoute,
  };
  return { ...watch(routes, { budget }), budget };
}

test("diff patch: a part the budget let go is read again at the same `at` when next opened, never from the start once the count is known", async () => {
  const pages = patchPage("a.ts");
  const { watcher, files } = twoPatches((query) => ({ ...pages({ page: "p3" }), file: file("a.ts", { patch: `@@ -20,1 +20,1 @@\n-${"z".repeat(300)}\n+w\n` }) }));
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  await watcher.movePatch(0, "next");
  assert.equal(watcher.view.patch(0).count, 2, "the second page is the last");
  assert.equal(files().length, 2, "a.ts's first page, then p2");
  await watcher.openPatch(1);
  assert.equal(watcher.view.patch(0).evicted, true, "opening b.ts's patch let a.ts's part go");
  assert.equal(watcher.view.patch(1).state, "ready", "the latest request is held");
  await watcher.openPatch(0);
  assert.equal(files().length, 4, "b.ts's first page, then only a.ts's p2 again");
  assert.deepEqual(files()[3].query, { project: "subspace", mode: "branch", path: "a.ts", page: "p2", at: FP }, "the same page at the same `at`");
  assert.equal(watcher.view.patch(0).part, 1, "the part shown before, not the first");
  assert.equal(watcher.view.patch(0).state, "ready");
  assert.equal(watcher.view.patch(0).hasLast, false, "on the last part");
  assert.equal(watcher.view.patch(0).hasPrevious, true);
});

test("diff patch: a part the budget let go before the count is known is read again by its own cursor at the same `at`, with no Last", async () => {
  // Every continuation names another, so the count is never learned.
  const { watcher, files } = twoPatches(() => ({ fingerprint: FP, file: file("a.ts", { patch: `@@ -20,1 +20,1 @@\n-${"z".repeat(300)}\n+w\n`, patchNextPage: "p3" }) }));
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  await watcher.movePatch(0, "next");
  assert.equal(watcher.view.patch(0).part, 1);
  assert.equal(watcher.view.patch(0).count, null, "the count is not known yet");
  await watcher.openPatch(1);
  assert.equal(watcher.view.patch(0).evicted, true);
  await watcher.openPatch(0);
  assert.deepEqual(files().map((call) => [call.query.path, call.query.page]), [["a.ts", undefined], ["a.ts", "p2"], ["b.ts", undefined], ["a.ts", "p2"]], "only the part shown before, by its cursor, never the first again");
  assert.equal(files()[3].query.at, FP, "at the same `at`");
  assert.equal(watcher.view.patch(0).part, 1, "the part shown before");
  assert.equal(watcher.view.patch(0).state, "ready");
  assert.equal(watcher.view.patch(0).hasPrevious, true);
  assert.equal(watcher.view.patch(0).hasLast, false, "Last still needs the count");
  await watcher.movePatch(0, "previous");
  assert.deepEqual(files().at(-1).query, { project: "subspace", mode: "branch", path: "a.ts", page: undefined, at: FP }, "Previous reads the first page");
  assert.equal(watcher.view.patch(0).part, 0);
});

test("diff patch: a 409 on the read after an eviction shows the Refresh banner", async () => {
  const later = `@@ -20,1 +20,1 @@\n-${"z".repeat(300)}\n+w\n`;
  const { watcher, files } = twoPatches((_, n) => (n === 0 ? { fingerprint: FP, file: file("a.ts", { patch: later }) } : Promise.reject(new HttpError(409))));
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  await watcher.movePatch(0, "next");
  await watcher.openPatch(1);
  assert.equal(watcher.view.patch(0).evicted, true);
  assert.equal(watcher.view.banner, null);
  await watcher.openPatch(0);
  assert.equal(files()[1].query.at, FP);
  assert.equal(watcher.view.banner, DIFF_MESSAGES.changed, "the snapshot can no longer be read: Refresh");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /Changed since loaded · Refresh/);
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

test("diff binary: once the snapshot moved, the bytes on show still move window by window, but nothing more is read until Refresh", async () => {
  const bytes = new Uint8Array(HEX_WINDOW_BYTES * 6).map((_, i) => i % 251);
  const { watcher, files } = watch(binaryRoutes({ new: bytes }, { pageBytes: HEX_WINDOW_BYTES * 2 }));
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
  assert.equal(watcher.view.binary.windows, 2);
  watcher.observe("fp-2");
  watcher.observe(FP);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  const disabled = (to) => panel.root.findAll((node) => node.attributes["data-diff-bytes"] === to)[0].attributes["aria-disabled"] === "true";
  assert.equal(disabled("next"), false, "Next within the part reads nothing");
  watcher.moveBinary("next");
  assert.equal(watcher.view.binary.window, 1);
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(disabled("next"), true, "Next into the next part is a read");
  assert.equal(disabled("previous"), false);
  await watcher.moveBinary("next");
  await watcher.retryBinary();
  assert.equal(files().length, 1, "nothing more is read");
  assert.equal(watcher.view.binary.part.part, 0);
  assert.ok(panel.root.textContent.includes("New bytes 4,096–8,192"), "the bytes read stay on show");
  watcher.refresh();
  await flush();
  await watcher.openBinary(0, "new");
  watcher.moveBinary("next");
  await watcher.moveBinary("next");
  assert.equal(watcher.view.binary.part.part, 1, "after Refresh the next part is read");
});

test("diff binary: an empty file is one page of size 0", async () => {
  const { watcher, files } = watch(binaryRoutes({ old: new Uint8Array(0), new: new Uint8Array(3) }));
  watcher.start();
  await flush();
  await watcher.openBinary(0, "old");
  assert.equal(watcher.view.binary.state, "ready");
  assert.equal(watcher.view.binary.size, 0);
  assert.equal(watcher.view.binary.part.count, 1, "one part, no next page");
  assert.equal(files().length, 1);
  assert.deepEqual(files()[0].query, { project: "subspace", mode: "branch", path: "img.bin", page: undefined, side: "old", at: FP });
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /Old side: empty file \(0 bytes\)\./);
});

test("diff binary: hex windows page across a part boundary; Previous lands on the part before's last window, offsets run on", async () => {
  const bytes = Uint8Array.from({ length: 2 * BINARY_PAGE_BYTES }, (_, i) => i & 0xff);
  const { watcher, files } = watch(binaryRoutes({ old: bytes, new: bytes }));
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
  const view = () => watcher.view.binary;
  const perPart = BINARY_PAGE_BYTES / HEX_WINDOW_BYTES;
  assert.equal(view().windows, perPart);
  assert.equal(view().part.hasNext, true);
  assert.equal(watcher.view.budget.used, BINARY_PART_BYTES + hexShownBytes(HEX_WINDOW_BYTES), "the part, as base64, and one drawn hex window");
  for (let i = 0; i < perPart - 1; i += 1) watcher.moveBinary("next");
  assert.equal(files().length, 1, "windows within a part read nothing");
  await watcher.moveBinary("next");
  assert.equal(files()[1].query.page, String(BINARY_PAGE_BYTES), "the gateway's page token");
  assert.equal(view().offset, BINARY_PAGE_BYTES);
  assert.equal(view().window, 0);
  assert.equal(view().part.count, 2, "the page ending exactly at the size is the last");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  const rows = () => panel.root.findAll((node) => node.attributes.class === "act-pre diff-hex")[0].textContent.split("\n");
  assert.equal(rows()[0].slice(0, 16), BINARY_PAGE_BYTES.toString(16).padStart(16, "0"), "each row shows its offset in the file");
  assert.match(panel.root.textContent, /New bytes 1,048,576–1,052,672 of 2,097,152/);
  await watcher.moveBinary("previous");
  assert.equal(files()[2].query.page, undefined, "Previous from a part's first window reads the part before");
  assert.equal(view().window, perPart - 1, "and lands on its last window");
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(rows().at(-1).slice(0, 16), (BINARY_PAGE_BYTES - HEX_ROW_BYTES).toString(16).padStart(16, "0"));
  assert.equal(panel.root.findAll((node) => node.attributes["data-diff-bytes"] === "last").length, 1, "Last, as the last page's token is known");
  await watcher.moveBinary("last");
  assert.equal(view().part.part, 1);
  assert.equal(view().window, perPart - 1, "Last lands on the last window");
  assert.equal(files().length, 4);
});

test("diff binary: no Last while reaching the last page means reading every page between", async () => {
  const bytes = new Uint8Array(3 * 1024);
  const { watcher } = watch(binaryRoutes({ old: bytes, new: bytes }, { pageBytes: 1024 }));
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(panel.root.findAll((node) => node.attributes["data-diff-bytes"] === "last").length, 0);
  watcher.moveBinary("last");
  assert.equal(watcher.view.binary.part.part, 0, "Last does nothing yet");
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

test("diff binary: bytes the budget let go are read again at the same `at`, at the part shown before", async () => {
  const budget = createBudget(64 * 1024);
  const bytes = Uint8Array.from({ length: 2048 }, (_, i) => i & 0xff);
  const { watcher, files } = watch(binaryRoutes({ old: bytes, new: bytes }, { pageBytes: 1024 }), { budget: budget.scope("diff\u0000") });
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
  await watcher.moveBinary("next");
  assert.equal(watcher.view.binary.part.count, 2);
  const activity = budget.scope("activity\u0000");
  activity.request("item");
  activity.reserve("item", 64 * 1024 - 100, null);
  assert.equal(watcher.view.binary.part.evicted, true, "the Activity view's latest read let the bytes go");
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.tag === "button" && node.textContent === "Show them again")[0] });
  await flush();
  assert.deepEqual(files().at(-1).query, { project: "subspace", mode: "branch", path: "img.bin", page: "1024", side: "new", at: FP });
  assert.equal(watcher.view.binary.part.part, 1);
  assert.equal(watcher.view.binary.offset, 1024);
});

/** An 8 KiB PNG, as far as the signature check goes. */
const PNG = Uint8Array.from({ length: 8 * 1024 }, (_, i) => [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a][i] ?? 0);

test("diff preview: an image grows only into free room, charged its size, and gives it back before anything else", async () => {
  const budget = createBudget(64 * 1024);
  const activity = budget.scope("activity\u0000");
  activity.reserve("older", 20 * 1024, null);
  const { watcher, files } = watch(binaryRoutes({ new: PNG }), { budget: budget.scope("diff\u0000") });
  watcher.start();
  await flush();
  await watcher.openPreview(0, "new");
  assert.equal(watcher.view.preview.state, "ready");
  assert.equal(watcher.view.preview.type, "image/png");
  assert.equal(budget.used, 20 * 1024 + PNG.length, "charged its size, freeing nothing");
  assert.equal(files().length, 1);
  activity.request("newer");
  activity.reserve("newer", 40 * 1024, null);
  assert.equal(watcher.view.preview.state, "given-back", "the image's room went first");
  assert.equal(watcher.view.preview.blob, null);
  assert.ok(activity.has("older"), "though the Activity view's entry is older");
  assert.equal(budget.used, 60 * 1024);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /gave its room back to a newer read/);
  assert.equal(panel.root.findAll((node) => node.tag === "button" && node.textContent === "Show image again").length, 1);
  watcher.closeBinary();
  assert.equal(budget.used, 60 * 1024);
});

test("diff preview: the image is held once, as the Blob the panel shows, never gathered into a second whole copy", async () => {
  const budget = createBudget(64 * 1024);
  const { watcher, files } = watch(binaryRoutes({ new: PNG }, { pageBytes: 3000 }), { budget: budget.scope("diff\u0000") });
  watcher.start();
  await flush();
  await watcher.openPreview(0, "new");
  const { preview } = watcher.view;
  assert.equal(preview.state, "ready");
  assert.equal(files().length, 3);
  assert.ok(preview.blob instanceof Blob);
  assert.equal(preview.blob.type, "image/png");
  assert.deepEqual(new Uint8Array(await preview.blob.arrayBuffer()), PNG, "every page, in order");
  assert.equal(Object.values(preview).filter((value) => value instanceof Uint8Array).length, 0, "no assembled copy beside it");
  assert.equal(budget.used, PNG.length, "the one copy is what is charged");
  const shown = [];
  const urls = { createObjectURL: (blob) => (shown.push(blob), "blob:preview"), revokeObjectURL: () => {} };
  const querySelector = function (selector) {
    const name = selector.slice(1, -1);
    return this.findAll((node) => name in node.attributes)[0] ?? null;
  };
  const doc = { ...fakeDoc, createElement: (tag) => Object.assign(fakeDoc.createElement(tag), { querySelector }) };
  const panel = createDiffPanel({ doc, watcher, urls });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(shown.length, 1);
  assert.equal(shown[0], preview.blob, "the panel shows the watcher's Blob itself, not a copy");
  assert.equal(panel.root.findAll((node) => node.tag === "img")[0].attributes.src, "blob:preview");
});

test("diff preview: more than 16 pages is too large however small each is; its room goes back and Show bytes still works", async () => {
  const budget = createBudget(64 * 1024);
  const { watcher, files } = watch(binaryRoutes({ new: PNG }, { pageBytes: 256 }), { budget: budget.scope("diff\u0000") });
  watcher.start();
  await flush();
  await watcher.openPreview(0, "new");
  assert.equal(watcher.view.preview.state, "too-large");
  assert.equal(files().length, 16, "the 16 reads it may make, no more");
  assert.equal(watcher.view.preview.blob, null);
  assert.equal(budget.used, 0, "its room went back");
  await watcher.openBinary(0, "new");
  assert.equal(watcher.view.binary.state, "ready", "Show bytes pages through it instead");
});

test("diff preview: an image that cannot fit in the free room says so with its size, and keeps Show bytes", async () => {
  const budget = createBudget(64 * 1024);
  const activity = budget.scope("activity\u0000");
  activity.request("item");
  activity.reserve("item", 60 * 1024, null);
  const { watcher, files } = watch(binaryRoutes({ new: PNG }), { budget: budget.scope("diff\u0000") });
  watcher.start();
  await flush();
  await watcher.openPreview(0, "new");
  assert.equal(watcher.view.preview.state, "no-room");
  assert.equal(files().length, 1, "no more pages are read");
  assert.equal(budget.used, 60 * 1024, "nothing is charged or freed");
  assert.ok(activity.has("item"));
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  assert.match(panel.root.textContent, /This image is 8 KiB, more than the 4 KiB the console has free for a preview\./);
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.tag === "button" && node.textContent === "Show bytes")[0] });
  await flush();
  assert.equal(watcher.view.binary.state, "ready", "Show bytes pages through it instead");
});

test("diff binary: a failed page offers Retry page for that page", async () => {
  const routes = binaryRoutes({ new: Uint8Array.from([1, 2, 3]) });
  let fail = true;
  const { watcher, files } = watch({ ...routes, file: (query, n) => (fail && n === 0 ? Promise.reject(new HttpError(500)) : routes.file(query)) });
  watcher.start();
  await flush();
  await watcher.openBinary(0, "new");
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
  assert.deepEqual([...bytesOf(watcher.view.binary.part.data, 0, 3)], [1, 2, 3]);
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
});

test("diff banner: once the snapshot moved, the banner and the network controls stay stale until Refresh, even when the fingerprint comes back, and what was read stays on show", async () => {
  const { watcher, files } = twoPatches(patchPage("a.ts"));
  watcher.start();
  await flush();
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  panel.update(watcher.view, { name: "Starbuck" });
  await flush();
  panel.update(watcher.view, { name: "Starbuck" });
  assert.equal(watcher.view.patch(0).state, "ready");
  const reads = files().length;
  watcher.observe("fp-2");
  watcher.observe(FP);
  assert.equal(watcher.view.banner, DIFF_MESSAGES.changed, "A → B → A: the cancelled reads are not restored, so the banner stays");
  assert.equal(watcher.view.changed, true);
  panel.update(watcher.view, { name: "Starbuck" });
  assert.ok(panel.root.textContent.includes("+a"), "the part already read stays on show");
  const next = panel.root.findAll((node) => node.attributes["data-diff-part"] === "next")[0];
  assert.equal(next.attributes["aria-disabled"], "true", "Next part is a read: off until Refresh");
  await watcher.movePatch(0, "next");
  await watcher.openPatch(1);
  await flush();
  assert.equal(files().length, reads, "nothing more is read");
  assert.equal(watcher.view.patch(0).part, 0);
  assert.equal(watcher.view.patch(1), null, "b.ts's patch was never read");
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.attributes["data-diff-select"] === "1")[0] });
  assert.match(panel.root.textContent, /The checkout changed before this patch was read\. Refresh to read it\./);
  watcher.refresh();
  await flush();
  assert.equal(watcher.view.banner, null, "Refresh clears it");
  await watcher.openPatch(0);
  await watcher.movePatch(0, "next");
  assert.equal(watcher.view.patch(0).state, "ready");
  assert.equal(watcher.view.patch(0).part, 1);
});

test("diff controller: the fingerprint already cached for the checkout is the baseline before the first list read, so a fresh list is not news", async () => {
  let panelView;
  const { client, files } = fakeClient({ list: () => page([file("a.ts", { patchNextPage: "p2" })]), file: patchPage("a.ts") });
  const controller = createDiffController({
    inspector: { replaceChildren() {}, querySelector: () => null },
    open: () => ({ client, stop() {} }),
    ships: () => ({ [TARGET.ship]: { crewMembers: [{ id: TARGET.member, contexts: [{ project: TARGET.project, git: { fingerprint: "fp-before-start" } }] }] } }),
    toast() {},
    announce() {},
    frame: () => 1,
    cancelFrame() {},
    createPanel: ({ watcher }) => ({ root: {}, update: (view) => (panelView = { view, watcher }), destroy() {} }),
  });
  controller.open(TARGET.ship, TARGET.member, TARGET.project);
  await flush();
  controller.draw();
  assert.equal(panelView.view.diff.fingerprint, FP);
  assert.equal(panelView.view.banner, null, "a value cached before the read is not a change during it");
  await panelView.watcher.openPatch(0);
  await panelView.watcher.movePatch(0, "next");
  assert.equal(panelView.watcher.view.patch(0).state, "ready", "the snapshot's reads were not cancelled");
  assert.equal(panelView.watcher.view.patch(0).part, 1);
  assert.equal(files().length, 2);
  controller.close();
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
  await watcher.openPatch(0);
  watcher.movePatch(0, "next");
  await flush();
  assert.equal(timers.pending(), 1, "waiting to retry");
  watcher.observe("fp-2");
  assert.equal(timers.pending(), 0, "the wait is cancelled");
  timers.fire();
  await flush();
  assert.equal(files().length, 2, "the first page and the waiting read; none at the stale fingerprint");
  assert.equal(watcher.view.patch(0).part, 0, "the part on show stays");
});

test("diff cancel: closing the view cancels reads in backoff and in flight", async () => {
  const { watcher, files, lists, timers, changes } = watch({ list: () => page([file("a", { patchNextPage: "p2" })]), file: () => Promise.reject(new HttpError(503, 2000)) });
  watcher.start();
  await flush();
  await watcher.openPatch(0);
  watcher.movePatch(0, "next");
  await flush();
  const before = changes();
  watcher.stop();
  assert.equal(timers.pending(), 0);
  timers.fire();
  await flush();
  assert.equal(files().length, 2);
  assert.equal(changes(), before, "nothing reaches a closed view");
  assert.equal(watcher.view.budget.used, 0, "and it holds nothing of the budget");
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
  await watcher.openPatch(0);
  watcher.movePatch(0, "next");
  await flush();
  watcher.observe("fp-2");
  release();
  await flush();
  assert.equal(watcher.view.patch(0).part, 0, "the late page is not shown");
  assert.notEqual(watcher.view.patch(0).data, "+late\n");
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
  const route = watcher.view.files.findIndex((e) => e.file.path === "src/warp/route.ts");
  await watcher.openPatch(route);
  await watcher.movePatch(route, "next");
  assert.equal(watcher.view.patch(route).part, 1, "a patch in parts");
  const chart = watcher.view.files.findIndex((e) => e.file.path === "assets/chart.bin");
  await watcher.openBinary(chart, "old");
  assert.equal(watcher.view.binary.size, 0, "an empty old side");
  await watcher.openBinary(chart, "new");
  assert.equal(base64Length(watcher.view.binary.part.data), BINARY_PAGE_BYTES);
  const panel = createDiffPanel({ doc: fakeDoc, watcher });
  const notes = watcher.view.files.findIndex((e) => e.file.path === "docs/notes.md");
  panel.update(watcher.view, { name: member.name });
  panel.root.listeners.click({ target: panel.root.findAll((node) => node.attributes["data-diff-select"] === String(notes))[0] });
  await flush();
  panel.update(watcher.view, { name: member.name });
  assert.match(panel.root.textContent, /⟨U\+202E⟩evil⟨U\+202C⟩/, "bidi controls show as markers");
  assert.match(panel.root.textContent, /⟨U\+0007⟩/);
  watcher.stop();
  manager.stop();
});

// ---------- the console's one budget (SUB decision 75c3ed93 §3) ----------

/**
 * The patch text a view keeps, in UTF-8 bytes: every distinct string of at
 * least 1 KiB reachable from the view's data (its rows and snapshot), and the
 * parts on show of the patches `opened`. Metadata is far shorter.
 */
function retainedText(view, opened = []) {
  const seen = new Set();
  const strings = new Set();
  const walk = (value) => {
    if (typeof value === "string") {
      if (value.length >= 1024) strings.add(value);
    } else if (value && typeof value === "object" && !seen.has(value)) {
      seen.add(value);
      for (const item of Object.values(value)) walk(item);
    }
  };
  walk({ files: view.files, diff: view.diff });
  for (const index of opened) walk(view.patch(index)?.data);
  return [...strings].reduce((sum, text) => sum + Buffer.byteLength(text), 0);
}

test("diff budget: a 17-page list keeps no patch text outside the budget; an opened patch is read at the snapshot and charged", async () => {
  // Stargate's list pages hold at most 4 MiB of inline patches, 64 KiB a file:
  // 17 full pages are 1,088 files and 68 MiB, more than the 64 MiB budget.
  const rows = 17 * 64;
  const inline = (path) => `+${path}${"x".repeat(65_534 - path.length)}\n`;
  const budget = createBudget(64 * 2 ** 20);
  const { watcher, files, lists } = watch(
    {
      list: (query) => {
        const offset = Number(query.page ?? 0);
        const listed = Array.from({ length: 64 }, (_, n) => file(`file-${offset + n}.txt`, { status: "added", patch: inline(`file-${offset + n}.txt`) }));
        return page(listed, { total: rows, ...(offset + 64 < rows ? { nextPage: String(offset + 64) } : {}) });
      },
    },
    { budget },
  );
  watcher.start();
  await settle();
  assert.equal(watcher.view.status, "ready", watcher.view.error?.message ?? watcher.view.status);
  assert.equal(lists().length, 17);
  assert.equal(watcher.view.files.length, rows, "every row is listed");
  assert.equal(retainedText(watcher.view), 0, "no list page's patch text is kept");
  assert.ok(watcher.view.files.every((entry) => !("patch" in entry.file) && !("patchNextPage" in entry.file)));
  assert.equal(files().length, 0, "nothing is read until a patch is opened");
  const last = rows - 1;
  await watcher.openPatch(last);
  assert.deepEqual(files().map((call) => call.query), [{ project: "subspace", mode: "branch", path: `file-${last}.txt`, page: undefined, at: FP }]);
  assert.equal(watcher.view.patch(last).data, inline(`file-${last}.txt`));
  const retained = retainedText(watcher.view, [last]);
  assert.ok(retained > 0 && budget.used >= retained && retained <= budget.limit, `${retained} retained, ${budget.used} charged, ${budget.limit} limit`);
  watcher.stop();
  assert.equal(retainedText(watcher.view), 0, "closed: nothing kept");
  assert.equal(budget.used, 0);
});


test("diff budget: the views share one budget through scopes; each frees the other's oldest, never the latest request", () => {
  const budget = createBudget(100);
  const activity = budget.scope("activity\u0000");
  const diff = budget.scope("diff\u0000");
  const evicted = [];
  activity.reserve("item", 40, (key) => evicted.push(`activity ${key}`));
  diff.request("patch");
  assert.equal(diff.reserve("patch", 50, (key) => evicted.push(`diff ${key}`)), true);
  assert.equal(activity.used, 90, "one total, seen from either view");
  assert.equal(diff.requested, "patch");
  assert.equal(activity.requested, null, "the latest request is the diff view's");
  assert.equal(activity.bytes("patch"), 0, "each view's keys are its own");
  assert.equal(activity.reserve("other", 60, null), false, "the other view's latest request is never freed for it");
  assert.deepEqual(evicted, []);
  activity.request("other");
  assert.equal(activity.reserve("other", 60, null), true);
  assert.deepEqual(evicted, ["activity item", "diff patch"], "least recently used first, across views, each told its own key");
  assert.equal(budget.used, 60);
  diff.reserve("next", 30, null);
  activity.clear();
  assert.equal(budget.used, 30, "clear gives back only that view's keys");
  assert.equal(budget.requested, null, "and its claim to the latest request");
  assert.equal(diff.bytes("next"), 30);
});

test("diff budget: an entry that yields is freed before any other, however recently used, and never by its own reservation", () => {
  const budget = createBudget(100);
  const evicted = [];
  const evict = (key) => evicted.push(key);
  budget.reserve("old", 30, evict);
  budget.reserve("whole", 30, evict, { makeRoom: false, yields: true });
  budget.reserve("mid", 30, evict);
  budget.touch("whole");
  assert.deepEqual(evicted, []);
  budget.request("new");
  budget.reserve("new", 30, evict);
  assert.deepEqual(evicted, ["whole"]);
  assert.equal(budget.used, 90);
});

test("diff budget: an Activity view on the console's budget gives back only its own keys when it stops", async () => {
  const budget = createBudget(64 * 1024 * 1024);
  budget.scope("diff\u0000").reserve("held", 1000, null);
  const watcher = createActivityWatcher({ client: fakeActivityClient([activitySnapshot()]), agent: "example-captain", awaitStream: false, budget: budget.scope("activity\u0000") });
  watcher.start();
  await settle();
  const inline = watcher.view.budget.bytes(INLINE_KEY);
  assert.ok(inline > 0, "the inline snapshot is charged");
  assert.equal(budget.used, 1000 + inline, "to the shared budget");
  watcher.stop();
  assert.equal(budget.used, 1000, "the diff view's entry stays");
});

test("diff budget: wouldFree on a view's scope names only that view's keys, in the order reserve frees them, one that yields first", () => {
  const budget = createBudget(100);
  const activity = budget.scope("activity\u0000");
  const diff = budget.scope("diff\u0000");
  activity.reserve("old", 20, null);
  diff.reserve("patch", 20, null);
  activity.reserve("mid", 20, null);
  diff.reserve("image", 20, null, { makeRoom: false, yields: true });
  activity.reserve("new", 10, null);
  assert.deepEqual(budget.wouldFree("activity\u0000grow", 60), ["diff\u0000image", "activity\u0000old", "diff\u0000patch"]);
  assert.deepEqual(activity.wouldFree("grow", 60), ["old"], "the diff view's keys are left to the diff view");
  assert.deepEqual(diff.wouldFree("grow", 60), ["image", "patch"]);
  assert.deepEqual(activity.wouldFree("grow", 10), [], "nothing while it fits");
  assert.equal(budget.used, 90, "a question frees nothing");
  activity.request("grow");
  activity.reserve("grow", 60, null);
  assert.deepEqual(
    ["old", "mid", "new"].filter((key) => activity.has(key)),
    ["mid", "new"],
    "reserve frees what wouldFree named, across both views",
  );
  assert.equal(diff.has("image") || diff.has("patch"), false);
});

test("diff budget: wouldFree on a view's scope admits as reserve does, with reserve's pinned option, and names only that view's keys", () => {
  const budget = createBudget(100);
  const activity = budget.scope("activity\u0000");
  const diff = budget.scope("diff\u0000");
  activity.reserve("floor", 70, null, { pinned: true });
  activity.reserve("old", 10, null);
  diff.reserve("patch", 10, null);
  assert.deepEqual(activity.wouldFree("plain", 40), [], "reserve would refuse it, so nothing is named");
  assert.deepEqual(diff.wouldFree("plain", 40), []);
  assert.deepEqual(activity.wouldFree("floor", 100, { pinned: true }), ["old"], "a pinned reservation is admitted: the scope forwards pinned");
  assert.deepEqual(diff.wouldFree("floor", 100, { pinned: true }), ["patch"]);
  assert.equal(diff.reserve("plain", 40, null), false);
  assert.equal(activity.reserve("plain", 40, null), false);
  assert.equal(budget.used, 90, "nothing freed");
  const freed = [];
  activity.reserve("old", 10, (key) => freed.push(`activity:${key}`));
  diff.reserve("patch", 10, (key) => freed.push(`diff:${key}`));
  activity.reserve("floor", 100, null, { pinned: true });
  assert.deepEqual(freed, ["activity:old", "diff:patch"], "reserve frees what the two scopes named");
});

test("diff budget: wouldFree names nothing for makeRoom: false, scoped or not, as reserve then frees nothing", () => {
  for (const scoped of [true, false]) {
    const root = createBudget(10);
    const budget = scoped ? root.scope("diff\u0000") : root;
    const freed = [];
    budget.reserve("part", 8, () => freed.push("part"));
    for (const options of [{ makeRoom: false }, { makeRoom: false, yields: true }, { makeRoom: false, pinned: true }]) {
      assert.deepEqual(budget.wouldFree("preview", 6, options), [], `${scoped ? "scoped" : "unscoped"} ${JSON.stringify(options)}`);
    }
    assert.deepEqual(budget.wouldFree("preview", 6, { yields: true }), ["part"], "one that makes room still names what it frees");
    assert.equal(budget.reserve("preview", 6, null, { makeRoom: false, yields: true }), true);
    assert.deepEqual(freed, [], "and reserve frees nothing");
  }
});

test("diff budget: on the console's budget, inline growth refits the Activity view's displayed whole, which makes the room before the diff view's older part is freed", async () => {
  const a = pagedWith("a".repeat(600 * 1024), "ia");
  const b = pagedWith("b".repeat(70 * 1024), "ib");
  a.snapshot.turns[1].items.push(b.snapshot.turns[1].items[0]);
  const probe = createActivityWatcher({ client: fakeActivityClient([a.snapshot]), agent: "example-captain", awaitStream: false });
  probe.start();
  await settle();
  const inline = probe.view.budget.bytes(INLINE_KEY);
  probe.stop();
  const patchBytes = 50_000;
  const budget = createBudget(inline + 2 * a.pages[0].bytes + 2 * b.pages[0].bytes + 20_000 + patchBytes);
  const evicted = [];
  budget.scope("diff\u0000").reserve("patch", patchBytes, (key) => evicted.push(key));
  const client = fakeActivityClient([a.snapshot]);
  client.activityItemPage = async (agent, turn, id, page) => (id === "ia" ? a.pages : b.pages)[page];
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false, budget: budget.scope("activity\u0000") });
  watcher.start();
  await settle();
  const panel = createActivityPanel({ doc: fakeDoc });
  await watcher.loadItem("turn-b", "ia");
  panel.update(watcher.view, officer);
  const last = panel.root.findAll((node) => node.attributes["data-activity-window"] === "last" && node.attributes["data-activity-item"] === "ia")[0];
  clickOn(panel, last);
  await watcher.loadItem("turn-b", "ib");
  panel.update(watcher.view, officer);
  assert.equal(watcher.view.full("turn-b", "ia").mode, "whole");
  watcher.event(activityEvent(10, { type: "item", item: textItem("grown-10", 10, "g".repeat(100 * 1024)) }));
  await settle();
  await settle();
  const shown = watcher.view.full("turn-b", "ia");
  assert.ok(shown, "the displayed whole is not freed");
  assert.equal(shown.mode, "parts", "it is handed over to parts");
  assert.equal(shown.state, "ready");
  assert.equal(watcher.view.full("turn-b", "ib").mode, "whole", "the latest request stays whole");
  assert.deepEqual(evicted, [], "the refit made the room, so the diff view's older part, first in line, is not freed");
  assert.equal(budget.scope("diff\u0000").bytes("patch"), patchBytes);
  assert.ok(budget.used <= budget.limit, `within the console's budget (${budget.used} of ${budget.limit})`);
  watcher.stop();
});
