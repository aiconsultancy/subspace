// Composes one frame for the renderers: the background, the star field, one
// or two tier layers with their camera transforms and alpha, and the gate
// ring during a warp. The renderers only draw what this returns.

import { bracketOffset, introFrame, warpFrame } from "../camera.js";
import { focusFor } from "../layout.js";
import { createList } from "./list.js";
import { buildFleetTier } from "./fleet.js";
import { buildShipTier } from "./ship.js";
import { buildCrewTier } from "./crew.js";
import { buildLandscapeTier } from "./landscape.js";
import { drawFrame } from "./common.js";

/** Draw one tier into a fresh list. `extra` overrides context fields (bracket, focus, hover). */
export function buildTier(selection, ctx, extra = {}) {
  const list = createList();
  if (selection.level === -1) {
    buildLandscapeTier(list, { ...ctx, ...extra });
    return list;
  }
  // Each tier draws its own fleet's data; only the streamed fleet has packets.
  const data = ctx.stateFor ? ctx.stateFor(selection) : null;
  const tierCtx = data
    ? { ...ctx, ...extra, fleet: data.fleet, ships: data.ships, unavailableSince: data.unavailableSince, packets: data.active ? ctx.packets : [], title: data.name, emptyTitle: data.name, emptyMessage: data.emptyMessage ?? ctx.emptyMessage, emptyDetail: data.emptyDetail ?? ctx.emptyDetail }
    : { ...ctx, ...extra };
  if (!tierCtx.fleet) {
    drawFrame(list, tierCtx, tierCtx.emptyTitle ?? "Fleet Console", "[fleet]", tierCtx.emptyDetail ?? "");
    list.text(tierCtx.emptyMessage ?? "Connecting to Stargate…", ctx.W / 2, ctx.H / 2, { size: 13, weight: 500, color: ["muted", 1], align: "center", max: ctx.W - 48 });
  } else if (selection.level === 0) buildFleetTier(list, tierCtx);
  else if (selection.level === 1) buildShipTier(list, tierCtx, selection);
  else buildCrewTier(list, tierCtx, selection);
  return list;
}

/** Node order of the parent tier, which decides where the focus node sits. */
function parentIds(parent, ctx) {
  if (parent.level === -1) return (ctx.fleets ?? []).map((fleet) => fleet.id);
  const data = ctx.stateFor ? ctx.stateFor(parent) : ctx;
  if (!data?.fleet) return [];
  if (parent.level === 0) return data.fleet.ships.map((ship) => ship.id);
  const detail = data.ships[parent.ship];
  return detail && data.fleet.ships.find((ship) => ship.id === parent.ship)?.available ? detail.crewMembers.map((member) => member.id) : [];
}

/**
 * @param {object} nav  the navigator's state
 * @param {object} ctx  scene context: W, H, now, clock, anim, reduced, dark, fleet, ships, packets, hover…
 * @param {number} t    animation time (performance.now)
 * @param {number} dt   seconds since the last frame
 */
export function composeFrame(nav, ctx, t, dt) {
  const C = { x: ctx.W / 2, y: ctx.H / 2 };
  const frame = { layers: [], gate: null, vp: C, tunnel: 0, velocity: 0, interactive: false, hits: [] };
  const warp = nav.warp;
  if (warp) {
    const F = focusFor(warp.parent, warp.child, ctx.W, ctx.H, parentIds(warp.parent, ctx), ctx.avoid ?? []);
    const wf = warpFrame(warp, t, F, ctx.W, ctx.H, ctx.reduced);
    frame.velocity = warp.lnPrev === null || ctx.reduced ? 0 : (wf.lnS - warp.lnPrev) / Math.max(dt, 0.001);
    warp.lnPrev = wf.lnS;
    frame.vp = wf.vp;
    frame.tunnel = wf.tunnel;
    frame.gate = wf.gate;
    if (wf.parent.alpha > 0.01) {
      const list = buildTier(warp.parent, ctx, { focus: warp.child, bracket: 0, hover: null });
      // Zoomed labels grow huge; let them go before the shapes do.
      const textAlpha = ctx.reduced ? wf.parent.alpha : wf.parent.alpha ** 3;
      frame.layers.push({ transform: wf.parent, alpha: wf.parent.alpha, textAlpha, items: list.items });
    }
    if (wf.child.alpha > 0.01) {
      const list = buildTier(warp.child, ctx, { bracket: ctx.reduced ? 0 : 24, hover: null });
      frame.layers.push({ transform: wf.child, alpha: wf.child.alpha, items: list.items });
    }
    return frame;
  }
  if (nav.intro !== null && !ctx.reduced) {
    const intro = introFrame(nav.intro, t, ctx.W, ctx.H);
    frame.velocity = intro.velocity;
    const list = buildTier(nav.sel, ctx, { bracket: intro.bracket, hover: null });
    frame.layers.push({ transform: intro.layer, alpha: intro.layer.alpha, items: list.items });
    return frame;
  }
  const list = buildTier(nav.sel, ctx, { bracket: bracketOffset(nav.landedAt, t, ctx.reduced) });
  frame.layers.push({ transform: { s: 1, tx: 0, ty: 0 }, alpha: 1, items: list.items });
  frame.interactive = true;
  frame.hits = list.hits;
  return frame;
}
