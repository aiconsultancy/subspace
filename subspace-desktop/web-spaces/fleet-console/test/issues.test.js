import assert from "node:assert/strict";
import { test } from "node:test";
import { createIssueResolver, extractIdentifiers, issueStateOf, MAX_WANTED, RESOLVE_BATCH } from "../src/issues.js";

const scope = { teamKeys: ["ABC", "ENG"], linearWorkspace: "acme" };
const text = (value) => ({ kind: "text", text: value });
const branch = (value) => ({ kind: "branch", text: value });

test("finds team-key identifiers in prose, in order, without duplicates", () => {
  const ids = extractIdentifiers([text("Fix ABC-1643 then ENG-7; ABC-1643 again"), text("(ABC-12).")], scope);
  assert.deepEqual(ids, ["ABC-1643", "ENG-7", "ABC-12"]);
});

test("prose needs the key as written and whole-word boundaries", () => {
  const ids = extractIdentifiers([text("abc-1 xABC-2 ABC-3x ABC-04 OPS-5 ABC-")], scope);
  assert.deepEqual(ids, []);
});

test("branch names match case-insensitively between separators", () => {
  const ids = extractIdentifiers([branch("feat/abc-1643-activity"), branch("fix/Eng-22_retry"), branch("feat/xabc-99")], scope);
  assert.deepEqual(ids, ["ABC-1643", "ENG-22"]);
});

test("Linear links count only for the board's workspace", () => {
  const ids = extractIdentifiers(
    [text("See https://linear.app/acme/issue/abc-77/some-title and https://linear.app/other/issue/ENG-88/x")],
    scope,
  );
  assert.deepEqual(ids, ["ABC-77"]);
});

test("no team keys, no identifiers; odd keys are ignored safely", () => {
  assert.deepEqual(extractIdentifiers([text("ABC-1")], { teamKeys: [] }), []);
  assert.deepEqual(extractIdentifiers([text("ABC-1")], null), []);
  assert.deepEqual(extractIdentifiers([text("A.B-1 ABC-2")], { teamKeys: ["A.B", "ABC"] }), ["ABC-2"]);
});

test("PR titles and phases are prose sources", () => {
  const ids = extractIdentifiers([text("Fleet API v1 routes (ENG-198)"), text("Waiting on ABC-5")], scope);
  assert.deepEqual(ids, ["ENG-198", "ABC-5"]);
});

function fakeTimers() {
  const queue = [];
  return {
    setTimer: (fn, ms) => {
      queue.push({ fn, ms });
      return queue.length;
    },
    clearTimer: () => {},
    async runAll() {
      while (queue.length) await queue.shift().fn();
    },
    get pending() {
      return queue.length;
    },
  };
}

test("the resolver debounces, batches at most 50, and only asks for new identifiers", async () => {
  const calls = [];
  const timers = fakeTimers();
  let changes = 0;
  const resolver = createIssueResolver({
    resolve: async (ids) => {
      calls.push(ids);
      return ids.map((identifier) => ({ identifier, state: "available", summary: { identifier, title: identifier, url: "", state: { name: "Todo", kind: "unstarted" }, assignee: null, archived: false } }));
    },
    onChange: () => (changes += 1),
    setTimer: timers.setTimer,
    clearTimer: timers.clearTimer,
  });
  const ids = Array.from({ length: 120 }, (_, i) => `ABC-${i + 1}`);
  resolver.want(ids.slice(0, 70));
  resolver.want(ids.slice(60));
  assert.equal(timers.pending, 1, "one debounce timer");
  await timers.runAll();
  assert.deepEqual(calls.map((batch) => batch.length), [50, 50, 20]);
  assert.ok(calls.every((batch) => batch.length <= RESOLVE_BATCH && new Set(batch).size === batch.length));
  assert.equal(changes, 1);
  assert.equal(resolver.get("ABC-5").state, "available");
  resolver.want(["ABC-5"]);
  assert.equal(timers.pending, 0, "known identifiers are not asked again");
  resolver.refresh();
  await timers.runAll();
  assert.equal(calls.length, 6, "refresh re-resolves everything wanted");
  assert.equal(changes, 1, "unchanged answers do not notify");
});

test("the resolver keeps only the most recently wanted identifiers", async () => {
  const timers = fakeTimers();
  const asked = [];
  const resolver = createIssueResolver({
    resolve: async (ids) => {
      asked.push(...ids);
      return ids.map((identifier) => ({ identifier, state: "pending" }));
    },
    setTimer: timers.setTimer,
    clearTimer: timers.clearTimer,
  });
  const ids = Array.from({ length: MAX_WANTED + 100 }, (_, i) => `ABC-${i + 1}`);
  for (const id of ids) resolver.want([id]);
  resolver.want(["ABC-1"]);
  await timers.runAll();
  assert.equal(resolver.get("ABC-50"), null, "old identifiers are forgotten");
  assert.equal(resolver.get("ABC-101"), null, "wanting ABC-1 again pushed out the next oldest");
  assert.equal(resolver.get(ids.at(-1)).state, "pending");
  assert.equal(resolver.get("ABC-1").state, "pending", "wanting one again keeps it");
  asked.length = 0;
  resolver.refresh();
  await timers.runAll();
  assert.equal(asked.length, MAX_WANTED, "refresh asks only for what is kept");
});

test("a failed resolve marks the batch failed instead of throwing", async () => {
  const timers = fakeTimers();
  const resolver = createIssueResolver({ resolve: async () => { throw new Error("no board"); }, setTimer: timers.setTimer, clearTimer: timers.clearTimer });
  resolver.want(["ABC-1"]);
  await timers.runAll();
  assert.equal(resolver.get("ABC-1").state, "failed");
  resolver.clear();
  assert.equal(resolver.get("ABC-1"), null);
});

test("state glyphs follow IRC's mapping", () => {
  assert.deepEqual(issueStateOf("started"), { glyph: "contrast", tone: "yellow" });
  assert.deepEqual(issueStateOf("completed"), { glyph: "circle-check", tone: "accent" });
  assert.deepEqual(issueStateOf("triage"), { glyph: "circle-dot-dashed", tone: "orange" });
  assert.deepEqual(issueStateOf("whatever"), { glyph: "circle", tone: "muted" });
});

test("the issue binding uses sample answers, the bridge, or nothing at all", async () => {
  const { createIssueBinding } = await import("../src/issue-binding.js");
  const { sampleFleet } = await import("./fixtures.js");
  const starbuck = sampleFleet().ships.galactica.crewMembers[0];
  let changes = 0;
  const sample = createIssueBinding({ getBridge: () => null, onChange: () => (changes += 1) });
  await sample.setup(true);
  assert.equal(sample.enabled, true);
  assert.deepEqual(sample.idsFor(starbuck), ["FLT-212"]);
  assert.equal(sample.idsFor(starbuck), sample.idsFor(starbuck), "cached per member object");
  assert.deepEqual(sample.badgeFor(starbuck), { identifier: "FLT-212", tone: "muted" }, "pending reads as muted");

  const none = createIssueBinding({ getBridge: () => ({ issues: { available: () => false } }), onChange: () => {} });
  await none.setup(false);
  assert.equal(none.enabled, false);
  assert.deepEqual(none.idsFor(starbuck), []);
  assert.equal(none.badgeFor(starbuck), null);

  const noBoard = createIssueBinding({ getBridge: () => ({ issues: { available: () => true, scope: async () => null } }), onChange: () => {} });
  await noBoard.setup(false);
  assert.equal(noBoard.enabled, false, "no board answers: quiet");

  const live = createIssueBinding({
    getBridge: () => ({ issues: { available: () => true, canOpen: () => false, scope: async () => ({ teamKeys: ["FLT"], linearWorkspace: null }), resolve: async (ids) => ids.map((identifier) => ({ identifier, state: "unavailable" })) } }),
    onChange: () => {},
  });
  await live.setup(false);
  assert.equal(live.enabled, true);
  assert.equal(live.canOpen, false);
  assert.deepEqual(live.idsFor(starbuck), ["FLT-212"]);
  assert.ok(changes >= 1);
});
