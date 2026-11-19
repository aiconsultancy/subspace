// A draw list: the primitives one tier is made of, in painter's order, in
// the tier's own CSS-pixel space. Colours are [tone, alpha] pairs resolved by
// the renderer, so a theme change needs no rebuild. Both renderers (WebGL2 and
// Canvas 2D) and the text overlay read the same list.

import { ellipsePoints, quadPoints, roundedRectPoints } from "../geometry.js";

/**
 * Kinds: disk (circle, ring or arc), hex, diamond, arrow, rrect, sweep,
 * line, packet (the GPU layers); text, run and pill (the text overlay).
 */
export function createList() {
  const items = [];
  const hits = [];
  const add = (item) => {
    items.push(item);
    return item;
  };
  return {
    items,
    hits,
    hit(region) {
      hits.push(region);
    },
    /** A filled and/or stroked circle; `a0`/`a1` make the stroke an arc. */
    disk(x, y, r, o = {}) {
      return add({
        k: "disk",
        x,
        y,
        r: Math.max(0, r),
        fill: o.fill ?? null,
        stroke: o.stroke ?? null,
        w: o.w ?? 1,
        a0: o.a0 ?? null,
        a1: o.a1 ?? null,
        dash: o.dash ?? null,
        dashOffset: o.dashOffset ?? 0,
        cap: o.cap ?? "butt",
        glow: o.glow ?? null,
      });
    },
    ring(x, y, r, stroke, w = 1, o = {}) {
      return this.disk(x, y, r, { ...o, stroke, w });
    },
    arc(x, y, r, a0, a1, stroke, w = 1, o = {}) {
      return this.disk(x, y, r, { ...o, stroke, w, a0, a1 });
    },
    hex(x, y, R, o = {}) {
      return add({ k: "hex", x, y, R, fill: o.fill ?? null, stroke: o.stroke ?? null, w: o.w ?? 1, glow: o.glow ?? null });
    },
    diamond(x, y, hw, hh, fill) {
      return add({ k: "diamond", x, y, hw, hh, fill });
    },
    /** The ship glyph: an arrow pointing along `rot`, nine units long at size 1. */
    arrow(x, y, size, rot, fill) {
      return add({ k: "arrow", x, y, size, rot, fill });
    },
    rrect(x, y, w, h, r, o = {}) {
      return add({ k: "rrect", x, y, w, h, r, fill: o.fill ?? null, stroke: o.stroke ?? null, sw: o.sw ?? 1, glow: o.glow ?? null });
    },
    /** The radar sweep: a fading wedge on an ellipse, leading edge at `angle`. */
    sweep(x, y, rx, ry, angle, color, line, trail = 0.72) {
      return add({ k: "sweep", x, y, rx, ry, angle, trail, color, line });
    },
    line(pts, color, w = 1, o = {}) {
      return add({ k: "line", pts, closed: Boolean(o.closed), color, w, dash: o.dash ?? null, dashOffset: o.dashOffset ?? 0, glow: o.glow ?? null });
    },
    segment(x1, y1, x2, y2, color, w = 1, o = {}) {
      return this.line([x1, y1, x2, y2], color, w, o);
    },
    ellipse(x, y, rx, ry, color, w = 1, o = {}) {
      return this.line(ellipsePoints(x, y, rx, ry), color, w, { ...o, closed: true });
    },
    quad(q, color, w = 1, o = {}) {
      return this.line(quadPoints(q), color, w, o);
    },
    rrectOutline(x, y, w, h, r, color, sw = 1, o = {}) {
      return this.line(roundedRectPoints(x, y, w, h, r), color, sw, { ...o, closed: true });
    },
    /** A dispatch or report packet: a glowing head at (x, y) with a tail behind it along -(ux, uy). */
    packet(x, y, ux, uy, tone) {
      return add({ k: "packet", x, y, ux, uy, tone, len: 24, r: 3.2 });
    },
    text(s, x, y, o = {}) {
      return add({
        k: "text",
        s: String(s ?? ""),
        x,
        y,
        size: o.size ?? 12,
        weight: o.weight ?? 500,
        mono: Boolean(o.mono),
        color: o.color ?? ["fg", 1],
        align: o.align ?? "left",
        base: o.base ?? "alphabetic",
        max: o.max ?? null,
      });
    },
    /** Text parts drawn one after another on a baseline, each with its own style. */
    run(parts, x, y, o = {}) {
      return add({ k: "run", parts, x, y, gap: o.gap ?? 8, max: o.max ?? null });
    },
    pill(s, x, y, o = {}) {
      return add({
        k: "pill",
        s: String(s ?? ""),
        x,
        y,
        size: o.size ?? 10.5,
        h: o.h ?? 18,
        mono: o.mono ?? true,
        color: o.color ?? ["accent", 1],
        bg: o.bg ?? ["select", 1],
        max: o.max ?? 200,
        align: o.align ?? "center",
        dot: o.dot ?? null,
      });
    },
  };
}

/** Item kinds drawn by the shape, line and particle passes. */
export const SHAPE_KINDS = new Set(["disk", "hex", "diamond", "arrow", "rrect", "sweep"]);
export const TEXT_KINDS = new Set(["text", "run", "pill"]);
