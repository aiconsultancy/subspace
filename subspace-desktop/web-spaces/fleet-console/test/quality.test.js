import assert from "node:assert/strict";
import { test } from "node:test";
import { ambientFps, COOLDOWN_MS, createGovernor, judgeWindow, LEVELS, sceneScale, SETTLE_MS, snapRefresh, UP_AFTER_WINDOWS, WINDOW_MS } from "../src/quality.js";

const REFRESH = 1000 / 60;

/** Feed `seconds` of frames drawn every `interval` ms aiming at `target`, with `work` ms of CPU. */
function run(governor, { from, seconds, interval, target = REFRESH, work = 2, gpu = null }) {
  let now = from;
  let changes = 0;
  const end = from + seconds * 1000;
  while (now < end) {
    now += interval;
    if (governor.sample({ interval, target, work, gpu }, now)) changes += 1;
  }
  return { now, changes };
}

test("levels give up resolution first, then one bloom level, then bloom, then ambient rate", () => {
  assert.equal(LEVELS[0].scale, 2);
  assert.equal(LEVELS[0].bloom, 2);
  assert.ok(LEVELS[0].horizon);
  for (let i = 1; i < LEVELS.length; i += 1) {
    const [a, b] = [LEVELS[i - 1], LEVELS[i]];
    assert.ok(b.scale <= a.scale && b.bloom <= a.bloom && b.ambientFps <= a.ambientFps, `level ${i} gives something up`);
    assert.ok(b.scale < a.scale || b.bloom < a.bloom || b.ambientFps < a.ambientFps || b.horizon !== a.horizon);
  }
  const firstBloomDrop = LEVELS.findIndex((level) => level.bloom < 2);
  const lastScaleDrop = LEVELS.findLastIndex((level, i) => i > 0 && level.scale < LEVELS[i - 1].scale);
  assert.ok(lastScaleDrop < firstBloomDrop, "resolution goes before bloom");
  assert.ok(LEVELS.findIndex((level) => level.ambientFps < 30) > LEVELS.findIndex((level) => level.bloom === 0), "ambient rate goes last");
});

test("scene scale never exceeds the display, nor drops below 1", () => {
  assert.equal(sceneScale(0, 2), 2);
  assert.equal(sceneScale(1, 2), 1.5);
  assert.equal(sceneScale(1, 1), 1);
  assert.equal(sceneScale(5, 2), 1);
  assert.equal(ambientFps(0, true), 30);
  assert.equal(ambientFps(0, false), 20, "slower while the page lacks focus");
  assert.equal(ambientFps(LEVELS.length - 1, true), 20);
});

test("display rates snap to standard intervals despite jitter", () => {
  assert.equal(snapRefresh(13), 1000 / 75);
  assert.equal(snapRefresh(16.9), 1000 / 60);
  assert.equal(snapRefresh(8.1), 1000 / 120);
  assert.equal(snapRefresh(34), 1000 / 30);
  const governor = createGovernor();
  for (let i = 0; i < 120; i += 1) governor.observeCallback(i % 10 === 0 ? 9 : 16.7 + (i % 3) * 0.3);
  assert.equal(governor.refresh, 1000 / 60, "occasional short callbacks do not fool it");
});

test("windows: slow when frames miss their target or the CPU is nearly out of budget, headroom when well inside", () => {
  const frames = (n, interval, target, work) => Array.from({ length: n }, () => ({ interval, target, work, gpu: null }));
  assert.equal(judgeWindow(frames(3, 50, REFRESH, 1)), "unknown");
  assert.equal(judgeWindow(frames(30, 33.4, REFRESH, 2)), "slow", "half rate during a warp");
  assert.equal(judgeWindow(frames(30, 33.4, 33.3, 2)), "headroom", "30 fps is on target for ambient motion");
  assert.equal(judgeWindow(frames(30, REFRESH, REFRESH, 15)), "slow", "CPU near the frame budget");
  assert.equal(judgeWindow(frames(30, REFRESH, REFRESH, 9)), "ok");
  assert.equal(judgeWindow([...frames(30, REFRESH, REFRESH, 2)].map((f) => ({ ...f, gpu: 15 }))), "slow", "GPU time counts where measurable");
});

test("the governor steps down after about a second of slow frames, one level at a time", () => {
  const governor = createGovernor();
  const first = run(governor, { from: 0, seconds: 1.05, interval: 40 });
  assert.equal(governor.level, 1);
  assert.equal(first.changes, 1);
  // Frames just after a change are ignored while the new level settles.
  assert.equal(governor.sample({ interval: 40, target: REFRESH, work: 2 }, first.now + SETTLE_MS / 2), false);
  run(governor, { from: first.now + SETTLE_MS, seconds: 5, interval: 40 });
  assert.ok(governor.level >= 4, `kept stepping down: ${governor.level}`);
  run(governor, { from: 100_000, seconds: 30, interval: 40 });
  assert.equal(governor.level, LEVELS.length - 1, "and stops at the last level");
});

test("it steps up slowly, after sustained headroom and a cooldown since the last step down", () => {
  const governor = createGovernor();
  const slow = run(governor, { from: 0, seconds: 1.05, interval: 40 });
  assert.equal(governor.level, 1);
  const quick = run(governor, { from: slow.now + SETTLE_MS, seconds: (UP_AFTER_WINDOWS - 1) * (WINDOW_MS / 1000), interval: REFRESH, work: 1 });
  assert.equal(governor.level, 1, "not after a few good windows");
  run(governor, { from: quick.now, seconds: (COOLDOWN_MS / 1000) + UP_AFTER_WINDOWS + 1, interval: REFRESH, work: 1 });
  assert.equal(governor.level, 0);
});

test("ordinary frames neither step down nor up; pinned levels never move", () => {
  const steady = createGovernor({ start: 2 });
  run(steady, { from: 0, seconds: 20, interval: REFRESH, work: 9 });
  assert.equal(steady.level, 2, "ok windows reset the headroom count");
  const pinned = createGovernor({ pinned: 3 });
  run(pinned, { from: 0, seconds: 10, interval: 60 });
  assert.equal(pinned.level, 3);
  assert.equal(pinned.pinned, true);
});

test("impossible intervals are ignored and reset forgets a partial window", () => {
  const governor = createGovernor();
  assert.equal(governor.sample({ interval: 900, target: REFRESH, work: 1 }, 10), false);
  assert.equal(governor.sample({ interval: -1, target: REFRESH, work: 1 }, 20), false);
  run(governor, { from: 0, seconds: 0.9, interval: 40 });
  governor.reset();
  run(governor, { from: 5000, seconds: 0.5, interval: 40 });
  assert.equal(governor.level, 0, "half a window after reset is not enough");
  assert.ok(governor.history.length === 0);
});

/**
 * Drive the governor as main.js does: one animation-frame callback every
 * `cadence(level, warping)` ms, warps of `warpMs` every `everyMs` drawn on
 * every callback and judged against the learned refresh, and ambient frames
 * drawn at the ambient rate otherwise. Returns when the level changed.
 */
function simulate(governor, { seconds, cadence, warpMs = 1500, everyMs = 10_000, work = 2 }) {
  let now = 0;
  let lastDraw = 0;
  const changes = [];
  while (now < seconds * 1000) {
    const warping = now % everyMs < warpMs;
    const interval = cadence(governor.level, warping);
    now += interval;
    governor.observeCallback(interval);
    const target = warping ? governor.refresh : 1000 / ambientFps(governor.level, true);
    if (!warping && now - lastDraw < target - 4) continue;
    const drawn = now - lastDraw;
    lastDraw = now;
    if (governor.sample({ interval: drawn, target, work }, now)) changes.push({ at: now, level: governor.level });
  }
  return changes;
}

test("on a display that really runs at 30 Hz, the governor learns it and settles at full quality", () => {
  const governor = createGovernor();
  const changes = simulate(governor, { seconds: 180, cadence: () => 1000 / 30 });
  assert.ok(Math.abs(governor.refresh - 1000 / 30) < 0.01, `learned ${governor.refresh}`);
  assert.equal(governor.level, 0);
  assert.ok(changes.length <= 6, `settles instead of cycling: ${changes.length} changes`);
  assert.deepEqual(changes.filter((change) => change.at > 90_000), [], "no changes in the last 90 s");
});

test("when lowering quality speeds frames up, the governor keeps the display rate and the lower level", () => {
  const governor = createGovernor();
  // GPU-bound at full quality: frames take 30 ms at level 0, 22 ms at level 1.
  simulate(governor, { seconds: 120, cadence: (level) => (level === 0 ? 30 : level === 1 ? 22 : 1000 / 60) });
  assert.ok(Math.abs(governor.refresh - 1000 / 60) < 0.01, `kept 60 Hz: ${governor.refresh}`);
  assert.ok(governor.level >= 1);
});

test("a level that keeps failing soon after it is retried waits longer each time", () => {
  const governor = createGovernor();
  // Full quality is fine at rest but too slow while warping, every 10 s.
  const changes = simulate(governor, { seconds: 600, cadence: (level, warping) => (warping && level === 0 ? 26 : 1000 / 60) });
  const ups = changes.filter((change, i) => i > 0 && change.level < changes[i - 1].level).length + (changes[0]?.level === 0 ? 1 : 0);
  assert.ok(ups <= 8, `retries back off: ${ups} step-ups in 10 minutes`);
  const late = changes.filter((change) => change.at > 300_000).length;
  assert.ok(late <= 4, `rarely in the second five minutes: ${late}`);
});
