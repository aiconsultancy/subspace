// Pieces every tier shares: the C4-style boundary with lock-on brackets, the
// crew dial, packets and issue badges.

import { formatDuration, initials } from "../format.js";
import { clamp } from "../camera.js";
import { toneOf } from "../model.js";

export const HOUR_MS = 3600e3;
export const DAY_MS = 24 * HOUR_MS;

/** Alpha that reads the same on dark and light stages. */
export const da = (ctx, dark, light) => (ctx.dark ? dark : light);

export const dur = (ctx, since) => formatDuration(ctx.now - (since ?? ctx.now));

/** The tier boundary: a dashed rounded frame, corner brackets offset by `bracket`, and the title block. */
export function drawFrame(list, ctx, title, tag, desc) {
  const m = 14;
  const x = m;
  const y = m;
  const w = ctx.W - 2 * m;
  const h = ctx.H - 2 * m;
  list.rrectOutline(x, y, w, h, 14, ["line", da(ctx, 0.95, 1)], 1, { dash: [3, 6] });
  const o = ctx.bracket;
  const L = 18;
  for (const [cx, cy, sx, sy] of [[x, y, 1, 1], [x + w, y, -1, 1], [x, y + h, 1, -1], [x + w, y + h, -1, -1]]) {
    const px = cx + sx * o;
    const py = cy + sy * o;
    list.line([px, py + sy * L, px, py, px + sx * L, py], ["accent", 0.8], 1.5, { glow: ["accent", 0.25] });
  }
  list.run(
    [
      { s: title, size: 13, weight: 600, color: ["strong", 1] },
      { s: tag, size: 10.5, weight: 400, mono: true, color: ["muted", 1] },
    ],
    x + 18,
    y + 27,
    { max: w * 0.62 },
  );
  if (desc) list.text(desc, x + 18, y + 45, { size: 11.5, weight: 400, color: ["muted", 1], max: w * 0.58 });
}

/** A crew member's dial: running arcs show the hour, idle arcs four hours, queued dashes, uncertain blinks. */
export function drawDial(list, ctx, view, x, y, r, hot) {
  const state = view.state;
  const k = toneOf(state);
  const elapsed = Math.max(0, ctx.now - (view.since ?? ctx.now));
  const lw = r > 30 ? 4 : 3;
  const glow = hot || state === "running" ? [k, hot ? 0.8 : 0.45] : null;
  list.disk(x, y, r, { fill: ["pane", 1], stroke: ["line", da(ctx, 0.9, 1)], w: lw, glow });
  if (state === "running") {
    const f = (elapsed % HOUR_MS) / HOUR_MS;
    const a1 = -Math.PI / 2 + Math.max(0.05, f) * Math.PI * 2;
    list.arc(x, y, r, -Math.PI / 2, a1, [k, 1], lw, { cap: "round", glow: [k, 0.7] });
    list.disk(x + Math.cos(a1) * r, y + Math.sin(a1) * r, lw, { fill: ["strong", 1] });
  } else if (state === "idle") {
    const f = clamp(elapsed / (4 * HOUR_MS));
    list.arc(x, y, r, -Math.PI / 2, -Math.PI / 2 + Math.max(0.03, f) * Math.PI * 2, ["muted", 0.8], lw, { cap: "round" });
  } else if (state === "queued") {
    list.ring(x, y, r, [k, 0.95], 2, { dash: [3, 5], dashOffset: ctx.reduced ? 0 : -ctx.anim * 0.01 });
  } else if (state === "uncertain") {
    list.ring(x, y, r, [k, ctx.reduced ? 1 : 0.4 + 0.6 * Math.abs(Math.sin(ctx.anim / 420))], lw);
  } else if (state === "unprovisioned") {
    list.ring(x, y, r, ["muted", 0.6], 1.5, { dash: [1, 4] });
  }
  if (hot) list.ring(x, y, r + 8, ["accent", 0.55], 1);
  list.text(initials(view.name), x, y + 1, { size: r > 30 ? 16 : 13, weight: 600, color: [state === "idle" ? "fg" : "strong", 1], align: "center", base: "middle" });
  const dots = Math.min(5, view.queuedCount);
  for (let i = 0; i < dots; i += 1) {
    const a = -Math.PI / 4 + i * 0.3;
    list.disk(x + Math.cos(a) * (r + 9), y + Math.sin(a) * (r + 9), 2.4, { fill: ["yellow", 1] });
  }
}

/** A small issue badge beside a dial: the state's tone as a dot, then the identifier. */
export function drawIssueBadge(list, badge, x, y) {
  if (!badge) return;
  list.pill(badge.identifier, x, y, { size: 9.5, h: 16, color: ["fg", 1], bg: ["select", 1], dot: badge.tone, align: "left", max: 110 });
}

/** Where a packet is along a straight link at `u` in 0..1, heading toward the far end. */
export function packetOnLine(list, link, u, kind) {
  const dir = kind === "dispatch" ? 1 : -1;
  const t = dir > 0 ? u : 1 - u;
  const dx = (link.x2 - link.x1) * dir;
  const dy = (link.y2 - link.y1) * dir;
  const d = Math.hypot(dx, dy) || 1;
  list.packet(link.x1 + (link.x2 - link.x1) * t, link.y1 + (link.y2 - link.y1) * t, dx / d, dy / d, kind === "dispatch" ? "accent" : "green");
}
