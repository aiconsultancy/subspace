// Tier 3, a crew member (C4 components): a 24-hour dial of outcomes around
// the member, one card per project context, and orbiting subagents.

import { crewLayout } from "../layout.js";
import { crewView, labelOf, taskTook, toneOf } from "../model.js";
import { DAY_MS, da, dur, drawDial, drawFrame, drawIssueBadge } from "./common.js";

function drawClock(list, ctx, L, view) {
  const { C, R } = L;
  list.ring(C.x, C.y, R, ["line", da(ctx, 0.75, 0.95)], 1);
  for (let i = 0; i < 24; i += 1) {
    const a = -Math.PI / 2 + (i * Math.PI) / 12;
    const r2 = R + (i % 6 === 0 ? 6 : 3);
    list.segment(C.x + Math.cos(a) * (R - 3), C.y + Math.sin(a) * (R - 3), C.x + Math.cos(a) * r2, C.y + Math.sin(a) * r2, ["line", da(ctx, 0.85, 1)], 1);
  }
  list.text("now", C.x, C.y - R - 11, { size: 9.5, weight: 500, mono: true, color: ["accent", 1], align: "center" });
  list.text("−12h", C.x, C.y + R + 18, { size: 9.5, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  view.recent.forEach((task, i) => {
    const end = task.finishedAt ?? task.updatedAt;
    if (!Number.isFinite(end)) return;
    const age = ctx.now - end;
    if (age < 0 || age > DAY_MS) return;
    const took = taskTook(task) ?? 0;
    const a1 = -Math.PI / 2 - (age / DAY_MS) * Math.PI * 2;
    const a0 = a1 - Math.max(0.04, (took / DAY_MS) * Math.PI * 2);
    list.arc(C.x, C.y, R, a0, a1, [toneOf(task.status), 1], 5);
    const mid = (a0 + a1) / 2;
    list.hit({ id: `rec:${i}`, kind: "recent", ref: task, x: C.x + Math.cos(mid) * R, y: C.y + Math.sin(mid) * R, r: 9 });
  });
  if (view.state === "running" || view.state === "uncertain") {
    const k = toneOf(view.state);
    const a1 = -Math.PI / 2;
    const a0 = a1 - Math.max(0.04, ((ctx.now - (view.since ?? ctx.now)) / DAY_MS) * Math.PI * 2);
    list.arc(C.x, C.y, R, a0, a1, [k, 1], 5, { glow: [k, 0.7] });
  }
}

function cardLinks(list, ctx, L, cards) {
  const { C, R } = L;
  for (const card of cards) {
    const state = card.ctx.activity;
    const k = toneOf(state);
    const dx = card.cx - C.x;
    const dy = card.cy - C.y;
    const d = Math.hypot(dx, dy) || 1;
    const ux = dx / d;
    const uy = dy / d;
    const t = Math.min(Math.abs(dx) > 0.01 ? card.w / 2 / Math.abs(dx) : 1e9, Math.abs(dy) > 0.01 ? card.h / 2 / Math.abs(dy) : 1e9);
    const x2 = card.cx - dx * t;
    const y2 = card.cy - dy * t;
    const x1 = C.x + ux * (R + 10);
    const y1 = C.y + uy * (R + 10);
    card.anchor = { x: x2 - ux * 30, y: y2 - uy * 30 };
    // Stacked cards on a narrow stage sit under the labels; a link would cross them.
    if (L.narrow) {
      card.anchor = { x: card.x + card.w - 22, y: card.y - 14 };
      continue;
    }
    if (state === "running") list.segment(x1, y1, x2, y2, [k, 0.85], 1.5, { dash: [3, 7], dashOffset: ctx.reduced ? 0 : -ctx.anim * 0.03, glow: [k, 0.35] });
    else if (state === "idle") list.segment(x1, y1, x2, y2, ["line", da(ctx, 0.9, 1)], 1);
    else list.segment(x1, y1, x2, y2, [k, 0.6], 1, { dash: [2, 5] });
  }
}

function drawSubagents(list, ctx, cards, view) {
  const running = cards.find((card) => card.ctx.activity === "running");
  if (!running || !view.subagents.length) return;
  view.subagents.forEach((subagent, i) => {
    const a = (ctx.reduced ? 0 : ctx.anim * 0.0012) + (i * Math.PI * 2) / view.subagents.length;
    const sx = running.anchor.x + Math.cos(a) * 16;
    const sy = running.anchor.y + Math.sin(a) * 16;
    list.segment(running.anchor.x, running.anchor.y, sx, sy, ["magenta", 0.4], 1);
    list.disk(sx, sy, 5, { fill: ["pane", 1], stroke: ["magenta", 1], w: 1.5, glow: ["magenta", 0.6] });
    list.hit({ id: `sub:${i}`, kind: "sub", name: subagent.title, x: sx, y: sy, r: 9 });
  });
}

function drawCard(list, ctx, L, card) {
  const { x, y, w, h, ctx: context } = card;
  const state = context.activity;
  const k = toneOf(state);
  const id = `ctx:${card.index}`;
  const hot = ctx.hover === id;
  const stroke = state === "running" ? [k, hot ? 1 : 0.7] : [hot ? "accent" : "line", da(ctx, 0.95, 1)];
  if (state === "queued") {
    list.rrect(x, y, w, h, 10, { fill: ["pane", 1] });
    list.rrectOutline(x, y, w, h, 10, stroke, 1, { dash: [3, 4] });
  } else {
    list.rrect(x, y, w, h, 10, { fill: ["pane", 1], stroke, sw: hot ? 1.5 : 1, glow: state === "running" ? [k, 0.4] : null });
  }
  list.disk(x + 15, y + 17, 3.5, { fill: [k, 1] });
  list.text(context.project ?? "default project", x + 26, y + 21, { size: 12.5, weight: 600, color: ["strong", 1], max: w - 112 });
  list.text(labelOf(state), x + w - 12, y + 21, { size: 10.5, weight: 500, color: [k, 1], align: "right" });
  const branch = context.git?.branch ?? null;
  list.text(branch ?? "no worktree yet", x + 14, y + 39, { size: 11, weight: 500, mono: true, color: [branch ? "accent" : "muted", 1], max: w - 28 });
  const taskText = context.current?.summary ?? (context.queuedCount ? `${context.queuedCount} queued` : "No task");
  list.text(taskText, x + 14, y + (L.narrow ? 54 : 58), { size: 11.5, weight: 400, color: ["muted", 1], max: w - 28 });
  list.hit({ id, kind: "ctx", ref: context, rect: [x, y, w, h] });
}

export function buildCrewTier(list, ctx, selection) {
  const ship = ctx.fleet.ships.find((entry) => entry.id === selection.ship);
  const member = ship ? ctx.ships[ship.id]?.crewMembers.find((entry) => entry.id === selection.crew) : null;
  if (!ship || !member) return;
  const view = crewView(member);
  const L = crewLayout(ctx.W, ctx.H, view.contexts.length);
  const cards = L.cards.map((card) => ({ ...card, ctx: view.contexts[card.index] }));
  drawFrame(list, ctx, view.name, `[crew · ${view.backend} · ${view.model ?? "default model"}]`, `${view.role ?? "Crew member"} · ${ship.name}`);
  drawClock(list, ctx, L, view);
  cardLinks(list, ctx, L, cards);
  drawSubagents(list, ctx, cards, view);
  for (const card of cards) drawCard(list, ctx, L, card);
  const { C } = L;
  drawDial(list, ctx, view, C.x, C.y, L.r, false);
  drawIssueBadge(list, ctx.issueBadge?.(ship.id, view.id), C.x + 22, C.y - L.R - 15);
  const ly = C.y + L.R + 38;
  list.text(view.name, C.x, ly, { size: 14, weight: 600, color: ["strong", 1], align: "center" });
  list.text(`${labelOf(view.state)} · ${dur(ctx, view.since)}${view.task ? ` · ${view.task.phase}` : ""}`, C.x, ly + 17, { size: 11, weight: 500, mono: true, color: [toneOf(view.state), 1], align: "center", max: ctx.W * 0.6 });
  if (view.queued.length && !L.narrow) {
    list.text(`${view.queuedCount} queued · next: ${view.queued[0].summary}`, C.x, ly + 34, { size: 11, weight: 400, color: ["yellow", 1], align: "center", max: Math.min(320, ctx.W * 0.5) });
  }
  list.hit({ id: "self", kind: "self", x: C.x, y: C.y, r: L.r });
}
