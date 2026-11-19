// Navigation between tiers and the warp camera. A selection is
// {level, fleet, ship, crew}: level -1 is the Fleets landscape (shown as L0,
// only when there are two or more fleets), 0 a fleet, 1 a ship, 2 a crew
// member. Moving between two selections passes through every tier in
// between, one warp each. Pure: times are passed in.

export const TOP = Object.freeze({ level: 0, fleet: null, ship: null, crew: null });
export const LANDSCAPE = Object.freeze({ level: -1, fleet: null, ship: null, crew: null });

/** The fleet tier of fleet `id`. */
export const fleetTier = (id) => ({ level: 0, fleet: id ?? null, ship: null, crew: null });

export const WARP_IN_MS = 1300;
export const WARP_OUT_MS = 1050;
export const WARP_CHAINED_MS = 780;
export const CROSSFADE_MS = 260;
export const INTRO_MS = 1100;
export const LANDING_MS = 520;
export const BRACKET_PX = 24;

export const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
export const smoothstep = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};
export const easeInOut = (t) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);
export const easeOut = (t) => 1 - (1 - t) ** 3;

/** The selections from the top (the landscape when there is one) down to `selection`. */
export function chain(selection, landscape = false) {
  const fleet = selection.fleet ?? null;
  const steps = landscape ? [LANDSCAPE] : [];
  if (selection.level >= 0) steps.push({ level: 0, fleet, ship: null, crew: null });
  if (selection.level >= 1) steps.push({ level: 1, fleet, ship: selection.ship, crew: null });
  if (selection.level >= 2) steps.push({ level: 2, fleet, ship: selection.ship, crew: selection.crew });
  return steps.length ? steps : [TOP];
}

export const sameSelection = (a, b) =>
  a.level === b.level && (a.fleet ?? null) === (b.fleet ?? null) && (a.ship ?? null) === (b.ship ?? null) && (a.crew ?? null) === (b.crew ?? null);

/** The tiers to pass through, in order, to get from `from` to `to`. */
export function planSteps(from, to, landscape = false) {
  const a = chain(from, landscape);
  const b = chain(to, landscape);
  let shared = 0;
  while (shared < a.length && shared < b.length && sameSelection(a[shared], b[shared])) shared += 1;
  const steps = [];
  for (let i = a.length - 2; i >= shared - 1 && i >= 0; i -= 1) steps.push(a[i]);
  for (let i = shared; i < b.length; i += 1) steps.push(b[i]);
  return steps;
}

/** The tier above `selection`: the landscape above a fleet only when there is one. */
export function parentOf(selection, landscape = false) {
  const steps = chain(selection, landscape);
  return steps.length > 1 ? steps[steps.length - 2] : selection;
}

export function warpDuration(dir, chained, reduced) {
  if (reduced) return CROSSFADE_MS;
  if (chained) return WARP_CHAINED_MS;
  return dir > 0 ? WARP_IN_MS : WARP_OUT_MS;
}

/**
 * Tracks the current selection, the warp in flight and queued steps.
 * `reduced()` says whether motion is reduced right now; `landscape()` whether
 * the Fleets tier exists (two or more fleets).
 */
export function createNavigator({ reduced = () => false, start = 0, landscape = () => false } = {}) {
  const state = {
    sel: TOP,
    warp: null,
    queue: [],
    after: null,
    landedAt: start,
    intro: reduced() ? null : start,
  };

  function startNext(now, chained) {
    const to = state.queue.shift();
    const from = state.sel;
    const dir = to.level > from.level ? 1 : -1;
    state.warp = {
      dir,
      parent: dir > 0 ? from : to,
      child: dir > 0 ? to : from,
      t0: now,
      dur: warpDuration(dir, chained, reduced()),
      lnPrev: null,
    };
    state.sel = to;
  }

  return {
    state,
    /** Where navigation is heading: the queued target, else the current tier. */
    target() {
      return state.after ?? (state.queue.length ? state.queue[state.queue.length - 1] : state.sel);
    },
    go(target, now) {
      if (state.warp) {
        state.after = target;
        return false;
      }
      const steps = planSteps(state.sel, target, landscape());
      if (steps.length === 0) return false;
      state.queue = steps;
      state.intro = null;
      startNext(now, steps.length > 1);
      return true;
    },
    /** Jump without a warp, for a selection that vanished from the data. */
    jump(target, now) {
      state.sel = target;
      state.warp = null;
      state.queue = [];
      state.after = null;
      state.landedAt = now;
    },
    /** Advance time; returns true when a warp landed this tick. */
    tick(now) {
      if (state.intro !== null && now - state.intro >= INTRO_MS) {
        state.intro = null;
        state.landedAt = now;
      }
      const warp = state.warp;
      if (!warp || now - warp.t0 < warp.dur) return false;
      state.warp = null;
      state.landedAt = now;
      if (state.queue.length) startNext(now, true);
      else if (state.after) {
        const target = state.after;
        state.after = null;
        this.go(target, now);
      }
      return true;
    },
    busy() {
      return Boolean(state.warp || state.intro !== null);
    },
  };
}

/**
 * Camera for a warp at time `now`: the parent tier zooms exponentially about
 * the focus node `F` while the child tier grows out of it to fill the stage.
 */
export function warpFrame(warp, now, F, W, H, reduced) {
  const raw = clamp((now - warp.t0) / warp.dur);
  const p = warp.dir > 0 ? raw : 1 - raw;
  const C = { x: W / 2, y: H / 2 };
  if (reduced) {
    const identity = { s: 1, tx: 0, ty: 0 };
    return { raw, p, parent: { ...identity, alpha: 1 - p }, child: { ...identity, alpha: p }, gate: null, vp: C, lnS: 0, tunnel: 0 };
  }
  const e = easeInOut(p);
  const Rf = Math.min(W, H) * 0.46;
  const Smax = Math.max(2, Rf / Math.max(1, F.r));
  const lnS = Math.log(Smax) * e;
  const S = Math.exp(lnS);
  const ox = F.x + (C.x - F.x) * e;
  const oy = F.y + (C.y - F.y) * e;
  const k = S / Smax;
  const amt = Math.sin(Math.PI * raw);
  return {
    raw,
    p,
    lnS,
    parent: { s: S, tx: ox - S * F.x, ty: oy - S * F.y, alpha: 1 - smoothstep(0.28, 0.72, p) },
    child: { s: k, tx: ox - k * C.x, ty: oy - k * C.y, alpha: smoothstep(0.1, 0.68, p) },
    gate: { x: ox, y: oy, r: F.r * S, p, amp: Math.sin(Math.PI * p) },
    vp: { x: C.x + (ox - C.x) * amt, y: C.y + (oy - C.y) * amt },
    tunnel: amt,
  };
}

/** The opening zoom: the fleet tier settles in from 55% scale. */
export function introFrame(start, now, W, H) {
  const raw = clamp((now - start) / INTRO_MS);
  const e = easeOut(raw);
  const s = 0.55 + 0.45 * e;
  return { raw, layer: { s, tx: (W / 2) * (1 - s), ty: (H / 2) * (1 - s), alpha: 0.35 + 0.65 * e }, bracket: BRACKET_PX * (1 - e), velocity: 2.6 * (1 - raw) };
}

/** Lock-on bracket offset after landing: closes from 24 px onto the frame corners. */
export function bracketOffset(landedAt, now, reduced) {
  if (reduced) return 0;
  return BRACKET_PX * (1 - easeOut(clamp((now - landedAt) / LANDING_MS)));
}
