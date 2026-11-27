import assert from "node:assert/strict";
import { test } from "node:test";
import { createBudget } from "../src/budget.js";
import { createPager, PagerError, splitBounded, stepPart } from "../src/pager.js";
import { splitUtf8, utf8Length } from "../src/utf8.js";

const settle = () => new Promise((resolve) => setTimeout(resolve, 1));

/**
 * A source paged by cursor, as a crew diff's patch is: each read returns up
 * to `size()` bytes from the cursor (a UTF-16 offset) and the next cursor.
 * Every reader stops itself after `max` reads, so a pager that loops fails
 * the test instead of growing.
 */
function cursorSource(text, { size = () => 100, max = 200, hold = null } = {}) {
  const reads = [];
  const read = async ({ part, cursor, signal }) => {
    reads.push({ part, cursor });
    if (reads.length > max) throw new Error("runaway read");
    const from = cursor ?? 0;
    const [data] = splitUtf8(text.slice(from), size());
    const next = from + data.length < text.length ? from + data.length : null;
    const reply = { data, next };
    if (hold?.(part, reads.length)) return new Promise((resolve) => hold.release.push(() => resolve(reply)));
    void signal;
    return reply;
  };
  return { read, reads };
}

/** Read every part in order with Next, joining them. */
async function readAll(pager, limit = 100) {
  await pager.open("first");
  const out = [pager.view.data];
  for (let i = 0; pager.view.hasNext; i += 1) {
    assert.ok(i < limit, "Next ends");
    await pager.open("next");
    out.push(pager.view.data);
  }
  return out.join("");
}

test("pager: stepPart stays within the parts", () => {
  assert.equal(stepPart("first", 3, 5), 0);
  assert.equal(stepPart("previous", 0, 5), 0);
  assert.equal(stepPart("next", 4, 5), 4);
  assert.equal(stepPart("last", 1, 5), 4);
  assert.equal(stepPart(9, 1, 5), 4);
  assert.equal(stepPart("next", 0, 1), 0);
  assert.equal(stepPart("sideways", 2, 5), 2);
});

test("pager: splitBounded keeps every window within its bytes and lines, and joins exactly", () => {
  const text = `${"ünï ✓ 🛰\n".repeat(500)}${"x".repeat(3000)}\n\n\n`;
  const windows = splitBounded(text, { maxBytes: 1000, maxLines: 40 });
  assert.equal(windows.join(""), text);
  assert.ok(windows.every((window) => utf8Length(window) <= 1000));
  assert.ok(windows.every((window) => window.split("\n").length - (window.endsWith("\n") ? 1 : 0) <= 40));
  assert.ok(windows.every((window) => !/^[\uDC00-\uDFFF]/.test(window) && !/[\uD800-\uDBFF]$/.test(window)), "split between code points");
  // A line longer than the byte bound is split too: the bound is never exceeded.
  assert.ok(splitBounded("y".repeat(5000), { maxBytes: 1000, maxLines: 1 }).every((window) => window.length === 1000));
  assert.deepEqual(splitBounded("", { maxBytes: 10 }), [""]);
});

test("pager: a source paged by cursor reaches every part in order, holding one part within the budget", async () => {
  const text = "ab✓🛰\n".repeat(400);
  const budget = createBudget(1000);
  const { read } = cursorSource(text);
  const pager = createPager({ budget, key: "a", read, partBytes: 100 });
  assert.equal(pager.view.hasLast, false, "the last part is not known yet");
  const joined = await readAll(pager);
  assert.equal(joined, text, "every byte, nothing repeated");
  assert.equal(pager.view.count, pager.view.part + 1, "the end is known once reached");
  assert.ok(budget.used <= 200, "only the part on show and its drawn copy are held");
  // Back to the start and to the end, all known now.
  await pager.open("first");
  assert.equal(pager.view.part, 0);
  await pager.open("previous");
  assert.equal(pager.view.part, 0);
  await pager.open("last");
  assert.equal(pager.view.part, pager.view.count - 1);
  assert.ok(text.endsWith(pager.view.data));
});

test("pager: a source paged by number reads any part directly with cursorAt, including the last", async () => {
  const pages = splitUtf8("0123456789".repeat(50), 64);
  const asked = [];
  const pager = createPager({
    budget: createBudget(10_000),
    key: "n",
    partBytes: 64,
    cursorAt: (part) => part,
    read: async ({ part, cursor }) => {
      asked.push(part);
      if (asked.length > 20) throw new Error("runaway read");
      const at = cursor ?? 0;
      return { data: pages[at], next: at + 1 < pages.length ? at + 1 : null, count: pages.length, version: "v1" };
    },
  });
  await pager.open("first");
  assert.equal(pager.view.hasLast, true);
  await pager.open("last");
  assert.equal(pager.view.part, pages.length - 1);
  assert.equal(pager.view.data, pages.at(-1));
  await pager.open("previous");
  assert.equal(pager.view.data, pages.at(-2));
  assert.deepEqual(asked, [0, pages.length - 1, pages.length - 2], "no part read on the way");
});

test("pager: a late reply after an eviction is dropped, and the evicted source reads in full, in order, from the start (A3)", async () => {
  const textA = "A-line ✓\n".repeat(60);
  const textB = "B".repeat(150);
  // Room for one part beside the one on show, not for two pagers.
  const budget = createBudget(250);
  const release = [];
  const hold = Object.assign((part) => holding && part === 2, { release });
  let holding = true;
  // After its eviction, A's pages come back in another size: boundaries differ.
  let sizeA = 100;
  const a = cursorSource(textA, { size: () => sizeA, hold });
  const b = cursorSource(textB);
  const changes = { a: 0 };
  const pagerA = createPager({ budget, key: "a", read: a.read, partBytes: 100, onEvict: () => (changes.a += 1) });
  const pagerB = createPager({ budget, key: "b", read: b.read, partBytes: 100 });
  // Each open is the budget's latest request, as the activity view makes it.
  const open = (pager, key, to) => {
    budget.request(key);
    return pager.open(to);
  };
  await open(pagerA, "a", "first");
  await open(pagerA, "a", "next");
  // Page 2 of A is held.
  const late = open(pagerA, "a", "next");
  await settle();
  assert.equal(release.length, 1);
  assert.equal(pagerA.view.state, "loading");
  // Opening B, the latest request, makes room by evicting A.
  await open(pagerB, "b", "first");
  assert.equal(changes.a, 1);
  assert.equal(pagerA.view.state, "evicted");
  assert.equal(pagerA.view.data, null);
  assert.equal(pagerA.view.hasNext, false, "no Next that leads nowhere");
  // The old reply arrives: it neither shows nor moves A's cursor.
  holding = false;
  release[0]();
  await late;
  assert.equal(pagerA.view.state, "evicted");
  assert.equal(pagerA.view.data, null);
  assert.equal(pagerA.view.known, 1);
  // A again: read from its start, in its new page size, in full and in order.
  sizeA = 70;
  const before = a.reads.length;
  budget.request("a");
  assert.equal(await readAll(pagerA), textA);
  assert.equal(a.reads[before].cursor, null, "re-read from the start, with no cursor from before");
  assert.equal(pagerB.view.state, "evicted", "B made room for A in turn");
  assert.ok(budget.used <= budget.limit);
  // A → B → A: B's Next is never dead after it comes back.
  await open(pagerB, "b", "next");
  assert.equal(pagerB.view.part, 0, "an evicted pager starts from its first part");
  assert.equal(pagerB.view.hasNext, true);
  await open(pagerB, "b", "next");
  assert.equal(pagerB.view.part, 1);
});

test("pager: a newer version restarts at its first part and says so; restart forgets it", async () => {
  let version = 1;
  let reads = 0;
  const pager = createPager({
    budget: createBudget(1000),
    key: "v",
    partBytes: 10,
    read: async ({ part }) => {
      reads += 1;
      if (reads > 10) throw new Error("runaway read");
      if (part === 1 && version === 1) {
        version = 2;
        return { data: "v2-part-0", next: 1, version: 2, newer: true };
      }
      return { data: `v${version}-part-${part}`, next: part === 0 ? 1 : null, version };
    },
  });
  await pager.open("first");
  await pager.open("next");
  assert.equal(pager.view.part, 0);
  assert.equal(pager.view.newer, true);
  assert.equal(pager.view.data, "v2-part-0");
  assert.equal(pager.view.version, 2);
  await pager.open("next");
  assert.equal(pager.view.data, "v2-part-1");
  await pager.restart();
  assert.equal(pager.view.newer, false);
  assert.equal(pager.view.part, 0);
});

test("pager: a reply that does not advance, is empty before the end or is too large fails, and nothing loops", async () => {
  const cases = [
    { reply: (cursor) => ({ data: "x", next: cursor }), why: /does not advance/ },
    { reply: () => ({ data: "", next: 5 }), why: /does not advance/ },
    { reply: () => ({ data: "x".repeat(11), next: null }), why: /more than 10/ },
    { reply: () => ({ next: null }), why: /no content/ },
  ];
  for (const { reply, why } of cases) {
    let reads = 0;
    const budget = createBudget(1000);
    const pager = createPager({
      budget,
      key: "k",
      partBytes: 10,
      read: async ({ cursor }) => {
        reads += 1;
        if (reads > 5) throw new Error("runaway read");
        return reads === 1 ? { data: "first", next: 1 } : reply(cursor);
      },
    });
    await pager.open("first");
    await pager.open("next");
    assert.equal(pager.view.state, "failed");
    assert.ok(pager.view.error instanceof PagerError);
    assert.match(pager.view.error.message, why);
    assert.equal(pager.view.data, "first", "the part on show stays");
    assert.equal(budget.bytes("k"), 10, "charged for what it shows: the part and its drawn copy");
    assert.equal(reads, 2);
  }
});

test("pager: room is made before the read starts, and drop discards a reply in flight", async () => {
  const budget = createBudget(300);
  let evicted = 0;
  budget.reserve("other", 250, () => (evicted += 1));
  const release = [];
  const pager = createPager({ budget, key: "p", partBytes: 100, read: () => new Promise((resolve) => release.push(() => resolve({ data: "late", next: null }))) });
  const done = pager.open("first");
  assert.equal(evicted, 1, "the other entry went before the read was sent");
  assert.equal(budget.bytes("p"), 100, "the part to read; its drawn copy is charged once it arrives");
  pager.drop();
  assert.equal(budget.bytes("p"), 0);
  release[0]();
  await done;
  assert.equal(pager.view.state, "dropped");
  assert.equal(pager.view.data, null);
  assert.equal(budget.used, 0);
});

test("budget: least recently opened first out; the latest request is the floor; evictions run after the reservation", () => {
  const budget = createBudget(100);
  const out = [];
  budget.reserve("a", 40, (key) => out.push([key, budget.has("c")]));
  budget.reserve("b", 40, (key) => out.push([key, budget.has("c")]));
  budget.touch("a");
  budget.reserve("c", 40, null);
  assert.deepEqual(out, [["b", true]], "b, the least recently opened, went; c was in place when it was told");
  assert.equal(budget.reserve("d", 200, null), false);
  assert.equal(budget.has("d"), false);
  assert.equal(budget.used, 80, "nothing freed for a reservation that cannot fit");
  budget.request("d");
  assert.equal(budget.reserve("d", 200, null), true);
  assert.equal(budget.used, 200, "the latest request is the floor, held alone");
  assert.deepEqual(out.map(([key]) => key), ["b", "a"]);
  assert.equal(budget.fits("e", 1), false, "nothing fits beside it");
  assert.equal(budget.reserve("e", 1, null), false, "and the request is never freed for another");
  budget.request("e");
  assert.equal(budget.reserve("e", 10, null), true, "a new request ends the last one's claim");
  assert.equal(budget.has("d"), false);
});

test("budget: wouldFree names what a reservation would free, in the order it would, and frees nothing", () => {
  const budget = createBudget(100);
  const out = [];
  budget.reserve("pin", 20, null, { pinned: true });
  budget.reserve("a", 30, (key) => out.push(key));
  budget.reserve("b", 30, (key) => out.push(key));
  budget.request("c");
  budget.reserve("c", 10, (key) => out.push(key));
  budget.touch("a");
  assert.deepEqual(budget.wouldFree("pin", 20), [], "it fits");
  assert.deepEqual(budget.wouldFree("pin", 40), ["b"], "the least recently used first; the latest request is never freed");
  assert.deepEqual(budget.wouldFree("pin", 80), ["b", "a"]);
  assert.deepEqual(budget.wouldFree("a", 60), ["b"], "what the key holds is replaced, not freed");
  assert.equal(budget.used, 90);
  assert.deepEqual(out, []);
  budget.reserve("pin", 40, null, { pinned: true });
  assert.deepEqual(out, ["b"], "reserve frees what it named");
});

test("budget: a plan weighs changes against one order and answers as wouldFree does, also leaving one change out", () => {
  const budget = createBudget(200);
  budget.reserve("pin", 30, null, { pinned: true });
  for (const [key, bytes] of [["a", 25], ["b", 40], ["c", 15], ["d", 30], ["e", 20]]) budget.reserve(key, bytes, null);
  budget.request("r");
  budget.reserve("r", 35, null);
  budget.touch("b");
  const without = (as, key) => new Map([...as].filter(([other]) => other !== key));
  // The pinned growth, and an ordinary reservation whose floor includes the latest request, `r`, itself resized: at 140
  // bytes it is refused until `r` holds less, and refused again when `r` is left out.
  assert.deepEqual(budget.wouldFree("x", 140), []);
  assert.deepEqual(budget.wouldFree("x", 140, { as: new Map([["r", 10]]) }), ["a", "c", "d", "e", "b"]);
  for (const [key, bytes, options] of [["pin", 120, { pinned: true }], ["x", 60, {}], ["x", 140, {}]]) {
    const plan = budget.plan(key, bytes, options);
    const as = new Map();
    const agree = (label) => {
      assert.deepEqual(plan.freed(), budget.wouldFree(key, bytes, { ...options, as }), `${key} ${bytes} ${label}`);
      for (const other of as.keys()) assert.deepEqual(plan.freed(other), budget.wouldFree(key, bytes, { ...options, as: without(as, other) }), `${key} ${bytes} ${label} without ${other}`);
      assert.deepEqual(plan.keys(), [...as.keys()]);
    };
    agree("as held");
    for (const [other, size] of [["d", 5], ["r", 10], ["a", 2], ["e", 1]]) {
      plan.set(other, size);
      as.set(other, size);
      agree(`with ${other}`);
    }
    plan.set("d", 25);
    as.set("d", 25);
    agree("d set again, in its place");
    plan.delete("a");
    as.delete("a");
    agree("without a");
  }
  assert.deepEqual({ used: budget.used, d: budget.bytes("d"), r: budget.bytes("r") }, { used: 195, d: 30, r: 35 }, "weighing changes nothing");
});

test("budget: a plan's fixed orders: a pinned growth past everything that can be freed never names a resized pinned entry or the latest request, and a floor is weighed with each change left out", () => {
  const budget = createBudget(200);
  budget.reserve("pin", 30, null, { pinned: true });
  budget.reserve("pin2", 20, null, { pinned: true });
  for (const [key, bytes] of [["a", 25], ["b", 40], ["c", 15], ["d", 30], ["e", 20]]) budget.reserve(key, bytes, null);
  budget.request("r");
  budget.reserve("r", 15, null);
  budget.touch("b");
  // By use: a, c, d, e, b. The growth resizes d, then pin2 and r (both kept), then a: they go after the others, in that
  // order, and the kept two never go. Held beside pin: 165, then 97 once resized.
  const plan = budget.plan("pin", 190, { pinned: true });
  for (const [key, size] of [["d", 5], ["pin2", 10], ["r", 5], ["a", 2]]) plan.set(key, size);
  assert.deepEqual(plan.freed(), ["c", "e", "b", "d", "a"], "past everything that can be freed, and still short");
  assert.deepEqual(plan.freed("d"), ["c", "d", "e", "b", "a"], "d as it holds now, in its own place");
  assert.deepEqual(plan.freed("pin2"), ["c", "e", "b", "d", "a"]);
  assert.deepEqual(plan.freed("r"), ["c", "e", "b", "d", "a"]);
  assert.deepEqual(plan.freed("a"), ["a", "c", "e", "b", "d"]);
  const shorter = budget.plan("pin", 150, { pinned: true });
  for (const [key, size] of [["d", 5], ["pin2", 10], ["r", 5], ["a", 2]]) shorter.set(key, size);
  assert.deepEqual(shorter.freed(), ["c", "e", "b"], "it stops once the growth fits");
  // pin2 and r grown instead of shrunk: the walk passes everything, and the kept two still never go.
  const grown = budget.plan("pin", 190, { pinned: true });
  for (const [key, size] of [["r", 60], ["pin2", 60], ["d", 5]]) grown.set(key, size);
  assert.deepEqual(grown.freed(), ["a", "c", "e", "b", "d"]);
  assert.deepEqual(grown.freed("r"), ["a", "c", "e", "b", "d"]);
  // An ordinary reservation's floor holds pin, pin2 and r (65 bytes): 150 more are refused until r holds nothing, and
  // refused again with r weighed as it holds now; d left out goes back to its own place.
  const floored = budget.plan("x", 150);
  assert.deepEqual(floored.freed(), [], "refused beside the floor");
  floored.set("r", 0);
  floored.set("d", 5);
  assert.deepEqual(floored.freed(), ["a", "c", "e", "b", "d"]);
  assert.deepEqual(floored.freed("r"), [], "r as it holds now: refused again");
  assert.deepEqual(floored.freed("d"), ["a", "c", "d", "e", "b"]);
  // The same walk as reserve makes it: pin's growth frees all five and leaves pin2 and r held.
  const out = [];
  for (const key of ["a", "b", "c", "d", "e"]) budget.reserve(key, budget.bytes(key), (k) => out.push(k));
  budget.reserve("pin", 190, null, { pinned: true });
  assert.deepEqual(out, ["a", "b", "c", "d", "e"]);
  assert.deepEqual([budget.has("pin2"), budget.has("r")], [true, true]);
});

/**
 * Round 11's budget order, from `freeOrder` and `admits` at 58d0eb77, over a
 * copy of the entries kept beside a budget: what a plan must name, without
 * going through the budget's own walk.
 */
function referenceBudget(limit) {
  const entries = new Map();
  let clock = 0;
  let requested = null;
  const kept = (key) => key === requested || entries.get(key)?.pinned === true;
  const sizeOf = (key, sizes) => (sizes.has(key) ? sizes.get(key) : (entries.get(key)?.bytes ?? 0));
  function wouldFree(key, bytes, pinned, sizes = new Map()) {
    const fixed = [...entries.keys()].filter((other) => other !== key && kept(other)).reduce((sum, other) => sum + sizeOf(other, sizes), 0);
    if (!pinned && key !== requested && fixed + bytes > limit) return [];
    const resized = [...sizes.keys()].filter((other) => other !== key && entries.has(other));
    let held = [...entries].reduce((sum, [other, entry]) => sum + (other === key ? 0 : entry.bytes), 0);
    for (const other of resized) held += sizes.get(other) - entries.get(other).bytes;
    const used = (other) => (sizes.has(other) ? clock + 1 + resized.indexOf(other) : entries.get(other).used);
    const order = [];
    for (const other of [...entries.keys()].filter((each) => each !== key && !kept(each)).sort((a, b) => used(a) - used(b))) {
      if (held + bytes <= limit) break;
      order.push(other);
      held -= sizeOf(other, sizes);
    }
    return order;
  }
  return {
    wouldFree,
    reserve(key, bytes, pinned, free) {
      const fixed = [...entries.keys()].filter((other) => other !== key && kept(other)).reduce((sum, other) => sum + entries.get(other).bytes, 0);
      if (!pinned && key !== requested && fixed + bytes > limit) return null;
      entries.delete(key);
      const freed = free ? wouldFree(key, bytes, true) : [];
      for (const other of freed) entries.delete(other);
      entries.set(key, { bytes, used: ++clock, pinned });
      return freed;
    },
    request(key) {
      requested = key;
      if (entries.has(key)) entries.get(key).used = ++clock;
    },
    touch(key) {
      if (entries.has(key)) entries.get(key).used = ++clock;
    },
    shrink(key, bytes) {
      if (entries.has(key)) entries.get(key).bytes = Math.min(bytes, entries.get(key).bytes);
    },
    release: (key) => entries.delete(key),
    used: () => [...entries.values()].reduce((sum, entry) => sum + entry.bytes, 0),
  };
}

test("budget: a plan and reserve free what round 11's order freed, over seeded random budgets", () => {
  let seed = 115;
  // Multiplied within 32 bits: in doubles the product passes 2^53 and loses bits, and the stream falls into a
  // short cycle that repeats most budgets.
  const random = () => (seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff) / 2147483648;
  const int = (n) => Math.floor(random() * n);
  const keys = ["a", "b", "c", "d", "e", "f", "pin", "pin2", "x"];
  const pick = () => keys[int(keys.length)];
  let checks = 0;
  const runs = new Set();
  for (let n = 0; n < 400; n += 1) {
    const limit = 50 + int(300);
    const budget = createBudget(limit);
    const reference = referenceBudget(limit);
    const steps = [];
    // Each entry keeps the eviction call it was reserved with: one list for them all.
    const out = [];
    const evict = (key) => out.push(key);
    for (let step = 0; step < 4 + int(10); step += 1) {
      const key = pick();
      const op = random();
      if (op < 0.5) {
        const bytes = int(limit);
        const pinned = key.startsWith("pin");
        const free = random() < 0.9;
        out.length = 0;
        const held = budget.reserve(key, bytes, evict, { pinned, makeRoom: free });
        const want = reference.reserve(key, bytes, pinned, free);
        steps.push(`reserve ${key} ${bytes}${pinned ? " pinned" : ""}${free ? "" : " in place"}`);
        assert.deepEqual(held ? out : null, want, steps.join("; "));
        assert.equal(budget.used, reference.used(), steps.join("; "));
      } else if (op < 0.65) {
        budget.request(key);
        reference.request(key);
        steps.push(`request ${key}`);
      } else if (op < 0.8) {
        budget.touch(key);
        reference.touch(key);
        steps.push(`touch ${key}`);
      } else if (op < 0.9) {
        const bytes = int(limit);
        budget.shrink(key, bytes);
        reference.shrink(key, bytes);
        steps.push(`shrink ${key} ${bytes}`);
      } else {
        budget.release(key);
        reference.release(key);
        steps.push(`release ${key}`);
      }
      // A plan with up to four entries resized (pinned ones, the latest request, the key itself or absent keys
      // included), one set again and one deleted, each weighed in full and with every change left out in turn.
      const key2 = pick();
      const bytes = int(limit * 1.5);
      const pinned = random() < 0.3;
      const plan = budget.plan(key2, bytes, { pinned });
      const sizes = new Map();
      for (let i = int(5); i > 0; i -= 1) {
        const [other, size] = [pick(), int(limit)];
        plan.set(other, size);
        sizes.set(other, size);
      }
      const resized = [...sizes.keys()];
      if (resized.length > 0 && random() < 0.5) {
        const [other, size] = [resized[int(resized.length)], int(limit)];
        plan.set(other, size);
        sizes.set(other, size);
      }
      if (resized.length > 0 && random() < 0.5) {
        const other = resized[int(resized.length)];
        plan.delete(other);
        sizes.delete(other);
      }
      const at = `${steps.join("; ")}; plan ${key2} ${bytes}${pinned ? " pinned" : ""} as ${JSON.stringify([...sizes])}`;
      assert.deepEqual(plan.freed(), reference.wouldFree(key2, bytes, pinned, sizes), at);
      for (const other of [...sizes.keys(), pick()]) {
        const without = new Map([...sizes].filter(([each]) => each !== other));
        assert.deepEqual(plan.freed(other), reference.wouldFree(key2, bytes, pinned, without), `${at} without ${other}`);
        checks += 1;
      }
    }
    runs.add(`${limit}: ${steps.join("; ")}`);
  }
  // Every budget is a different one: a generator that cycles would repeat earlier runs.
  assert.equal(runs.size, 400, `${runs.size} of 400 budgets differ`);
  assert.ok(checks > 3000, `${checks} plans weighed`);
});
