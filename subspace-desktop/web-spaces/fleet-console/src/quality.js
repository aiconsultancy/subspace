// Quality levels and the adaptive governor. The governor watches frame
// timings and steps quality down when frames run slow for about a second,
// and back up slowly when there is headroom. Nothing is persisted. Pure:
// times are passed in.

/**
 * Level 0 is the full look. Each step gives up the least visible thing first:
 * internal resolution, then the near bloom level, then bloom, then ambient rate.
 * `scale` caps the scene's device-pixel ratio; text always draws at full DPR.
 */
export const LEVELS = Object.freeze([
  Object.freeze({ scale: 2, bloom: 2, horizon: true, ambientFps: 30, label: "full" }),
  Object.freeze({ scale: 1.5, bloom: 2, horizon: true, ambientFps: 30, label: "1.5× scene" }),
  Object.freeze({ scale: 1.25, bloom: 2, horizon: false, ambientFps: 30, label: "1.25× scene" }),
  Object.freeze({ scale: 1, bloom: 2, horizon: false, ambientFps: 30, label: "1× scene" }),
  Object.freeze({ scale: 1, bloom: 1, horizon: false, ambientFps: 30, label: "one bloom level" }),
  Object.freeze({ scale: 1, bloom: 0, horizon: false, ambientFps: 30, label: "no bloom" }),
  Object.freeze({ scale: 1, bloom: 0, horizon: false, ambientFps: 20, label: "no bloom, 20 fps ambient" }),
]);

/** The scene's internal pixel ratio for a display ratio at a level. */
export function sceneScale(level, dpr) {
  return Math.max(1, Math.min(dpr, LEVELS[level].scale));
}

/** Frames per second for ambient motion: lower while the page lacks focus. */
export function ambientFps(level, focused) {
  const fps = LEVELS[level].ambientFps;
  return focused ? fps : Math.min(fps, 20);
}

export const WINDOW_MS = 1000;
const DISPLAY_RATES = [240, 165, 144, 120, 100, 90, 75, 60, 50, 48, 30];

/** The standard display interval nearest to a measured one. */
export function snapRefresh(interval) {
  let best = 1000 / 60;
  let distance = Infinity;
  for (const rate of DISPLAY_RATES) {
    const candidate = 1000 / rate;
    const d = Math.abs(Math.log(interval / candidate));
    if (d < distance) {
      distance = d;
      best = candidate;
    }
  }
  return best;
}
export const UP_AFTER_WINDOWS = 5;
export const COOLDOWN_MS = 8000;
export const SETTLE_MS = 500;

const percentile = (values, p) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))];
};

/**
 * Judge one window of drawn frames. Each sample has `interval` (time since
 * the previous drawn frame), `target` (the interval it aimed for: the display
 * rate during a warp, the ambient rate otherwise), `work` (CPU time) and,
 * where measurable, `gpu`.
 */
export function judgeWindow(samples) {
  return summarizeWindow(samples).verdict;
}

/** The verdict with the numbers behind it, for the readout. */
export function summarizeWindow(samples) {
  if (samples.length < 6) return { verdict: "unknown", ratio50: null, ratio90: null, busy90: null, n: samples.length };
  const ratios = samples.map((s) => s.interval / s.target);
  const ratio50 = percentile(ratios, 0.5);
  const ratio90 = percentile(ratios, 0.9);
  const busy90 = percentile(samples.map((s) => Math.max(s.work, Number.isFinite(s.gpu) ? s.gpu : 0) / s.target), 0.9);
  const verdict = ratio50 > 1.4 || busy90 > 0.85 ? "slow" : ratio90 < 1.2 && busy90 < 0.4 ? "headroom" : "ok";
  return { verdict, ratio50, ratio90, busy90, n: samples.length };
}

/**
 * @param {{start?: number, min?: number, max?: number, pinned?: number|null}} options
 * `pinned` fixes the level (for `?quality=`) and turns the governor off.
 */
export function createGovernor({ start = 0, min = 0, max = LEVELS.length - 1, pinned = null } = {}) {
  let level = pinned ?? start;
  let samples = [];
  let windowStart = null;
  let headroomWindows = 0;
  let lastDown = -Infinity;
  let settleUntil = 0;
  let refresh = 1000 / 60;
  let cadence = [];
  let lastVerdict = "unknown";
  const history = [];

  function step(direction, now) {
    const next = Math.max(min, Math.min(max, level + direction));
    if (next === level) return false;
    level = next;
    settleUntil = now + SETTLE_MS;
    headroomWindows = 0;
    samples = [];
    windowStart = null;
    if (direction > 0) lastDown = now;
    return true;
  }

  return {
    get level() {
      return level;
    },
    get settings() {
      return LEVELS[level];
    },
    /** The display's frame interval: the best cadence seen, between 240 Hz and 30 Hz. */
    get refresh() {
      return refresh;
    },
    get verdict() {
      return lastVerdict;
    },
    get pinned() {
      return pinned !== null;
    },
    /** The last windows' summaries, newest last. */
    get history() {
      return history.slice();
    },
    /**
     * Every animation-frame callback's interval, drawn or not, to learn the
     * display rate: the median of each 60, snapped to a standard rate, since
     * callbacks jitter. It never drifts up, so a slow GPU cannot redefine slow.
     */
    observeCallback(interval) {
      if (!Number.isFinite(interval) || interval <= 0 || interval > 250) return;
      cadence.push(interval);
      if (cadence.length < 60) return;
      refresh = Math.min(refresh, snapRefresh(percentile(cadence, 0.5)));
      cadence = [];
    },
    /**
     * Record one drawn frame of continuous animation (never an on-demand
     * frame). Returns true when the level changed.
     */
    sample({ interval, target, work, gpu = null }, now) {
      if (pinned !== null) return false;
      if (now < settleUntil || !Number.isFinite(interval) || interval <= 0 || interval > 250 || !(target > 0)) return false;
      if (windowStart === null) windowStart = now;
      samples.push({ interval, target, work, gpu });
      if (now - windowStart < WINDOW_MS) return false;
      const summary = summarizeWindow(samples);
      lastVerdict = summary.verdict;
      history.push({ ...summary, level, refresh });
      if (history.length > 20) history.shift();
      samples = [];
      windowStart = now;
      if (lastVerdict === "slow") return step(1, now);
      if (lastVerdict === "headroom") {
        headroomWindows += 1;
        if (headroomWindows >= UP_AFTER_WINDOWS && now - lastDown >= COOLDOWN_MS) return step(-1, now);
        return false;
      }
      headroomWindows = 0;
      return false;
    },
    /** Forget a partial window, after a pause such as the page being hidden. */
    reset() {
      samples = [];
      windowStart = null;
      headroomWindows = 0;
    },
  };
}
