// Tier 2, a ship (C4 containers): the captain in the middle, crew dials on
// an ellipse with dispatch links, branch pills and issue badges.

import { shipLayout } from "../layout.js";
import { captainLine, crewView, labelOf, shipCounts, toneOf } from "../model.js";
import { clamp } from "../camera.js";
import { initials } from "../format.js";
import { quadPoint } from "../geometry.js";
import { da, dur, drawDial, drawFrame, drawIssueBadge } from "./common.js";

function drawCaptain(list, ctx, L, ship, busy) {
  const { x, y, r } = L.captain;
  const captain = ship.captain;
  const state = ship.available ? captain.activity : "unreachable";
  const k = toneOf(state);
  if (state === "running" && !ctx.reduced) list.ring(x, y, r + 9, [k, 0.55], 1, { dash: [2, 6], dashOffset: -ctx.anim * 0.015 });
  list.disk(x, y, r, { fill: ["pane", 1], stroke: [k, 1], w: 1.5, dash: state === "unreachable" ? [3, 3] : null, glow: [k, state === "idle" ? 0.2 : 0.55] });
  list.text(initials(captain.name), x, y + 1, { size: 15, weight: 600, color: ["strong", 1], align: "center", base: "middle" });
  const top = y - r - 44;
  list.text(captain.name, x, top, { size: 13, weight: 600, color: ["strong", 1], align: "center" });
  list.text("[captain]", x, top + 14, { size: 10, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  const line = state === "unreachable" ? "No answer" : `${captainLine(captain, busy)} · ${dur(ctx, captain.since)}`;
  list.text(line, x, top + 28, { size: 10.5, weight: 500, mono: true, color: [state === "running" ? "accent" : state === "unreachable" ? "red" : "muted", 1], align: "center", max: 260 });
  list.hit({ id: "captain", kind: "captain", ship: ship.id, x, y, r: r + 4 });
}

function drawCrewLinks(list, ctx, L, nodes) {
  const { C } = L;
  for (const node of nodes) {
    const { view } = node;
    const k = toneOf(view.state);
    const dx = node.x - C.x;
    const dy = node.y - C.y;
    const d = Math.hypot(dx, dy) || 1;
    const ux = dx / d;
    const uy = dy / d;
    const x1 = C.x + ux * (L.captain.r + 6);
    const y1 = C.y + uy * (L.captain.r + 6);
    const x2 = node.x - ux * (node.r + 8);
    const y2 = node.y - uy * (node.r + 8);
    node.curve = { x1, y1, mx: (x1 + x2) / 2 - uy * 18, my: (y1 + y2) / 2 + ux * 18, x2, y2 };
    if (view.state === "running") list.quad(node.curve, [k, 0.85], 1.5, { dash: [3, 7], dashOffset: ctx.reduced ? 0 : -ctx.anim * 0.03, glow: [k, 0.35] });
    else if (view.state === "queued" || view.state === "uncertain") list.quad(node.curve, [k, 0.6], 1, { dash: [2, 5] });
    else list.quad(node.curve, ["line", da(ctx, 0.85, 1)], 1);
  }
}

function drawCrewPackets(list, ctx, ship, nodes) {
  for (const packet of ctx.packets) {
    if (packet.ship !== ship.id || !packet.crew) continue;
    const node = nodes.find((entry) => entry.id === packet.crew);
    if (!node?.curve) continue;
    const elapsed = ctx.clock - packet.t0;
    let u = packet.kind === "dispatch" ? (elapsed - 1100) / 1100 : elapsed / 1100;
    if (u < 0 || u > 1) continue;
    if (packet.kind === "report") u = 1 - u;
    const back = clamp(u + (packet.kind === "dispatch" ? -0.06 : 0.06));
    const at = quadPoint(node.curve, u);
    const behind = quadPoint(node.curve, back);
    const dx = at.x - behind.x;
    const dy = at.y - behind.y;
    const d = Math.hypot(dx, dy) || 1;
    list.packet(at.x, at.y, dx / d, dy / d, packet.kind === "dispatch" ? "accent" : "green");
  }
}

function drawCrewNode(list, ctx, ship, node, pillMax) {
  const { view } = node;
  const id = `crew:${ship.id}:${view.id}`;
  const hot = ctx.hover === id || (ctx.focus?.level === 2 && ctx.focus.crew === view.id);
  drawDial(list, ctx, view, node.x, node.y, node.r, hot);
  if (view.contexts.length > 1) list.pill(`${view.contexts.length} repos`, node.x - node.r - 14, node.y - node.r + 2, { size: 9.5, h: 15, color: ["fg", 1] });
  drawIssueBadge(list, ctx.issueBadge?.(ship.id, view.id), node.x + node.r * 0.72, node.y - node.r - 4);
  const above = Math.sin(node.angle) < -0.4;
  const ly = above ? node.y - node.r - 50 : node.y + node.r + 18;
  list.text(view.name, node.x, ly, { size: 12.5, weight: 600, color: ["strong", 1], align: "center" });
  if (view.branch) list.pill(view.branch, node.x, ly + 15, { max: pillMax });
  else list.text("no branch yet", node.x, ly + 19, { size: 10.5, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  list.text(`${labelOf(view.state)} · ${dur(ctx, view.since)}`, node.x, ly + 38, { size: 10.5, weight: 500, mono: true, color: [toneOf(view.state), 1], align: "center" });
  list.hit({ id, kind: "crew", ship: ship.id, crew: view.id, x: node.x, y: node.y, r: node.r + 10 });
}

export function buildShipTier(list, ctx, selection) {
  const ship = ctx.fleet.ships.find((entry) => entry.id === selection.ship);
  if (!ship) return;
  const detail = ctx.ships[ship.id];
  const members = ship.available && detail ? detail.crewMembers : [];
  const L = shipLayout(ctx.W, ctx.H, members.map((member) => member.id));
  const nodes = L.crew.map((node, i) => ({ ...node, view: crewView(members[i]) }));
  drawFrame(list, ctx, ship.name, `[ship · ${ship.captain.transport} · ${ship.captain.backend}]`, `${ship.repository ?? "No GitHub repository"} · captain ${ship.captain.name}`);
  for (const f of [0.5, 1]) list.ellipse(L.C.x, L.C.y, L.rx * f, L.ry * f, ["line", f === 1 ? da(ctx, 0.6, 0.85) : da(ctx, 0.3, 0.5)], 1);
  if (ship.available) {
    drawCrewLinks(list, ctx, L, nodes);
    drawCrewPackets(list, ctx, ship, nodes);
  }
  drawCaptain(list, ctx, L, ship, shipCounts(ship, detail).busy);
  if (!ship.available) {
    list.text("Crew status unavailable", L.C.x, L.C.y + 70, { size: 13, weight: 600, color: ["red", 1], align: "center" });
    const since = ctx.unavailableSince[ship.id];
    const reason = ship.error ?? "Stargate could not read this ship's crew.";
    list.text(`${reason}${since ? ` Unavailable for ${dur(ctx, since)}.` : ""} Nothing is guessed.`, L.C.x, L.C.y + 90, { size: 11.5, weight: 400, color: ["muted", 1], align: "center", max: ctx.W - 60 });
    return;
  }
  if (!detail) {
    list.text("Reading crew…", L.C.x, L.C.y + 80, { size: 12, weight: 500, color: ["muted", 1], align: "center" });
    return;
  }
  if (nodes.length === 0) list.text("No crew on this ship", L.C.x, L.C.y + 80, { size: 12, weight: 500, color: ["muted", 1], align: "center" });
  const pillMax = Math.min(190, ctx.W * 0.3);
  for (const node of nodes) drawCrewNode(list, ctx, ship, node, pillMax);
}
