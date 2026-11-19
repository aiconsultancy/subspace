// Tier 1, the fleet (C4 context): the rear admiral at the centre of a radar
// grid, ships on the ring with their crew orbiting them.

import { fleetLayout } from "../layout.js";
import { captainLine, shipCounts, shipState, toneOf } from "../model.js";
import { clamp } from "../camera.js";
import { da, dur, drawFrame, HOUR_MS, packetOnLine } from "./common.js";

const ORBIT_SPEED = { running: 0.00055, queued: 0.00018 };

function hasPeerCaptains(ship) {
  return ship.features?.some((feature) => /(^|\.)peerCaptains$/i.test(feature.key) && feature.enabled);
}

function drawGrid(list, ctx, L) {
  const { C, rx, ry } = L;
  for (const f of [0.36, 0.68, 1, 1.32]) {
    list.ellipse(C.x, C.y, rx * f, ry * f, ["line", f === 1 ? da(ctx, 0.75, 1) : da(ctx, 0.35, 0.55)], 1);
  }
  for (let i = 0; i < 12; i += 1) {
    const a = (i * Math.PI) / 6;
    list.segment(C.x + Math.cos(a) * rx * 0.36, C.y + Math.sin(a) * ry * 0.36, C.x + Math.cos(a) * rx * 1.32, C.y + Math.sin(a) * ry * 1.32, ["line", da(ctx, 0.22, 0.4)], 1);
  }
  if (!ctx.reduced) {
    const angle = ((ctx.anim / 5200) % 1) * Math.PI * 2;
    list.sweep(C.x, C.y, rx * 1.32, ry * 1.32, angle, ["accent", da(ctx, 0.11, 0.07)], ["accent", da(ctx, 0.45, 0.35)]);
  }
}

function drawLinks(list, ctx, L, nodes) {
  const { C } = L;
  for (const node of nodes) {
    const dx = node.x - C.x;
    const dy = node.y - C.y;
    const d = Math.hypot(dx, dy) || 1;
    const ux = dx / d;
    const uy = dy / d;
    node.link = { x1: C.x + ux * 44, y1: C.y + uy * 44, x2: node.x - ux * (node.orbit + 5), y2: node.y - uy * (node.orbit + 5) };
    const { x1, y1, x2, y2 } = node.link;
    if (node.state === "unreachable") list.segment(x1, y1, x2, y2, ["red", 0.55], 1, { dash: [2, 5] });
    else if (node.state === "running") list.segment(x1, y1, x2, y2, ["accent", da(ctx, 0.5, 0.6)], 1, { glow: ["accent", 0.3] });
    else list.segment(x1, y1, x2, y2, ["line", da(ctx, 0.95, 1)], 1);
  }
}

function drawPeers(list, ctx, L, nodes) {
  const peers = nodes.filter((node) => hasPeerCaptains(node.ship) && node.ship.available);
  if (peers.length < 2) return;
  for (let i = 0; i < peers.length; i += 1) {
    if (peers.length === 2 && i === 1) break;
    const a = peers[i];
    const b = peers[(i + 1) % peers.length];
    const mx = (a.x + b.x) / 2;
    const my = (a.y + b.y) / 2;
    const q = { x1: a.x, y1: a.y - a.orbit, mx: mx + (mx - L.C.x) * 0.45, my: my + (my - L.C.y) * 0.45, x2: b.x + b.orbit * 0.7, y2: b.y - b.orbit * 0.7 };
    list.quad(q, ["magenta", 0.6], 1, { dash: [2, 6], dashOffset: ctx.reduced ? 0 : -ctx.anim * 0.01 });
    if (!L.narrow && i === 0) list.text("peer captains", q.mx - 4, q.my + 4, { size: 10, weight: 400, mono: true, color: ["magenta", 0.9], align: "center" });
  }
}

function drawPackets(list, ctx, nodes) {
  for (const packet of ctx.packets) {
    const node = nodes.find((entry) => entry.id === packet.ship);
    if (!node?.link) continue;
    const elapsed = ctx.clock - packet.t0;
    const u = packet.kind === "dispatch" ? elapsed / 1300 : (elapsed - 900) / 1300;
    if (u >= 0 && u <= 1) packetOnLine(list, node.link, u, packet.kind);
  }
}

function drawOrbitCrew(list, ctx, node) {
  const members = ctx.ships[node.id]?.crewMembers;
  if (!node.ship.available || !members?.length) return;
  members.forEach((member, i) => {
    const state = member.activity;
    const k = toneOf(state);
    const speed = ORBIT_SPEED[state] ?? 0.00005;
    const a = (i * 2 * Math.PI) / members.length + (ctx.reduced ? 0 : ctx.anim * speed);
    const sx = node.x + Math.cos(a) * node.orbit;
    const sy = node.y + Math.sin(a) * node.orbit;
    if (state === "running" && !ctx.reduced) {
      for (let j = 10; j >= 1; j -= 1) {
        const aj = a - j * 0.07;
        list.disk(node.x + Math.cos(aj) * node.orbit, node.y + Math.sin(aj) * node.orbit, 2.5 * (1 - j / 12), { fill: [k, 0.5 * (1 - j / 11)] });
      }
    }
    if (state === "idle" || state === "unprovisioned") {
      const since = member.idleSince ?? member.since ?? ctx.now;
      const fade = 1 - clamp((ctx.now - since) / (4 * HOUR_MS)) * 0.6;
      list.ring(sx, sy, 2.7, ["muted", fade], 1.3);
    } else {
      const blink = state === "uncertain" && !ctx.reduced ? 0.4 + 0.6 * Math.abs(Math.sin(ctx.anim / 380)) : 1;
      list.disk(sx, sy, 3.3, { fill: [k, blink], glow: state === "running" ? [k, 0.8] : null });
    }
  });
}

function shipLabel(ctx, node, narrow) {
  const ship = node.ship;
  if (!ship.available) {
    const since = ctx.unavailableSince[ship.id];
    return { text: since ? `Unavailable · ${dur(ctx, since)}` : "Unavailable", tone: "red" };
  }
  const counts = shipCounts(ship, ctx.ships[ship.id]);
  const text = narrow ? `${counts.busy}/${counts.people} busy` : `${counts.busy}/${counts.people} busy · ${counts.queued} queued`;
  return { text, tone: counts.busy ? "accent" : "muted" };
}

function drawShipNode(list, ctx, L, node) {
  const { ship, x, y, r, orbit, state } = node;
  const k = toneOf(state);
  const id = `ship:${ship.id}`;
  const hot = ctx.hover === id || (ctx.focus?.level === 1 && ctx.focus.ship === ship.id);
  list.ring(x, y, orbit, ["line", da(ctx, 0.75, 0.95)], 1, ship.available ? {} : { dash: [2, 4] });
  drawOrbitCrew(list, ctx, node);
  const glow = hot || state === "running" ? [k, hot ? 0.9 : 0.5] : null;
  list.disk(x, y, r, { fill: ["pane", 1], stroke: [k, state === "idle" ? 0.85 : 1], w: hot ? 2 : 1.5, dash: ship.available ? null : [3, 3], glow });
  const glyph = state === "idle" ? "muted" : state === "unreachable" ? "red" : "strong";
  list.arrow(x, y, r / 22, Math.atan2(y - L.C.y, x - L.C.x), [glyph, 0.95]);

  const lines = L.narrow ? 2 : 3;
  const above = Math.sin(node.angle) < -0.5;
  const ly = above ? y - orbit - 12 - (lines - 1) * 14 : y + orbit + 18;
  list.text(ship.name, x, ly, { size: 13, weight: 600, color: ["strong", 1], align: "center" });
  if (!L.narrow) list.text(`[ship · ${ship.captain.transport} · ${ship.captain.backend}]`, x, ly + 14, { size: 10, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  const label = shipLabel(ctx, node, L.narrow);
  list.text(label.text, x, ly + (L.narrow ? 14 : 28), { size: 10.5, weight: 500, mono: true, color: [label.tone, 1], align: "center", max: Math.max(110, ctx.W / Math.max(3, L.ships.length)) });
  list.hit({ id, kind: "ship", ship: ship.id, x, y, r: orbit + 8 });
}

function drawAdmiral(list, ctx, L) {
  const { x, y } = L.admiral;
  const admiral = ctx.fleet.admiral;
  const hot = ctx.hover === "admiral";
  if (!admiral) {
    list.hex(x, y, 24, { fill: ["pane", 1], stroke: ["line", 1], w: 1 });
    list.text("No rear admiral", x, y + 52, { size: 11, weight: 500, color: ["muted", 1], align: "center" });
    return;
  }
  const state = admiral.activity;
  const k = toneOf(state);
  list.ring(x, y, 40, [k, 0.55], 1, { dash: [2, 5], dashOffset: ctx.reduced ? 0 : -ctx.anim * 0.012 });
  if (state === "running") {
    const a0 = ctx.reduced ? 0 : ctx.anim / 900;
    list.arc(x, y, 40, a0, a0 + 1.1, [k, 1], 2, { glow: [k, 0.8] });
  }
  list.hex(x, y, 28, { fill: ["pane", 1], stroke: [k, 1], w: 1.5, glow: [k, hot ? 0.9 : 0.6] });
  list.diamond(x, y, 8, 11, [k, 0.95]);
  list.text(admiral.name, x, y + 60, { size: 13, weight: 600, color: ["strong", 1], align: "center" });
  list.text("[rear admiral]", x, y + 74, { size: 10, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  list.text(`${captainLine(admiral)} · ${dur(ctx, admiral.since)}`, x, y + 88, { size: 10.5, weight: 500, mono: true, color: [state === "running" ? "accent" : "muted", 1], align: "center" });
  list.hit({ id: "admiral", kind: "admiral", x, y, r: 36 });
}

export function buildFleetTier(list, ctx) {
  const fleet = ctx.fleet;
  const ids = fleet.ships.map((ship) => ship.id);
  const L = fleetLayout(ctx.W, ctx.H, ids);
  const nodes = L.ships.map((node, i) => {
    const ship = fleet.ships[i];
    return { ...node, ship, state: shipState(ship, ctx.ships[ship.id]) };
  });
  const gateway = fleet.gateway;
  drawFrame(
    list,
    ctx,
    ctx.title ?? fleet.name,
    `[fleet · ${fleet.host.hostname}]`,
    `Stargate ${gateway.version} · OpenCode ${gateway.opencodeVersion ?? "unknown"} · ${fleet.ships.length} ship${fleet.ships.length === 1 ? "" : "s"}`,
  );
  drawGrid(list, ctx, L);
  drawLinks(list, ctx, L, nodes);
  drawPeers(list, ctx, L, nodes);
  drawPackets(list, ctx, nodes);
  for (const node of nodes) drawShipNode(list, ctx, L, node);
  drawAdmiral(list, ctx, L);
}
