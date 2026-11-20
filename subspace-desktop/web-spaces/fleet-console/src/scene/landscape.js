// Tier L0, the Fleets landscape (C4 system landscape): this Subspace at the
// hub, one node per fleet around it. A link's style says how the fleet is
// reached (a live stream, a periodic read, or not at all); a node shows its
// ships in orbit, small CPU and memory arcs, and its state. Fleets that cannot
// be read are drawn as plainly as unavailable ships.

import { landscapeLayout } from "../layout.js";
import { shipCounts, shipState, toneOf } from "../model.js";
import { da, drawFrame } from "./common.js";
import { tokenProblem } from "../connection.js";

const BAD = new Set(["signed-out", "forbidden", "unreachable"]);

/** What a fleet node shows: its tone, status line and whether it failed. */
export function fleetStatus(fleet) {
  const status = fleet.connection.status;
  if (fleet.connection.reason === "no-token") return { tone: "red", bad: true, text: tokenProblem(fleet.entry?.problem).short };
  if (BAD.has(status)) return { tone: "red", bad: true, text: { "signed-out": "Signed out · token refused", forbidden: "Forbidden network", unreachable: "Unreachable" }[status] };
  const data = fleet.state.fleet;
  if (!data) return { tone: "muted", bad: false, text: status === "reconnecting" ? "Reconnecting" : "Connecting…" };
  const online = data.ships.filter((ship) => ship.available).length;
  const counts = data.ships.map((ship) => shipCounts(ship, fleet.state.ships[ship.id]));
  const busy = counts.reduce((sum, count) => sum + count.busy, 0);
  const people = counts.reduce((sum, count) => sum + count.people, 0);
  const text = `${online}/${data.ships.length} ships · ${busy}/${people} busy`;
  if (status === "reconnecting") return { tone: "yellow", bad: false, text: `Reconnecting · ${text}` };
  return { tone: busy > 0 ? "accent" : "muted", bad: false, text, busy, online };
}

const gaugeTone = (fraction, warn, alarm) => (fraction > alarm ? "red" : fraction > warn ? "yellow" : "accent");

function drawGauges(list, ctx, node, host) {
  const { x, y, r } = node;
  const usage = host.cpu.usage ?? 0;
  const memory = host.memory.totalBytes ? 1 - host.memory.availableBytes / host.memory.totalBytes : 0;
  const top = -Math.PI / 2;
  for (const [radius, fraction, tone] of [
    [r - 7, usage, gaugeTone(usage, 0.65, 0.85)],
    [r - 13, memory, gaugeTone(memory, 0.75, 0.9)],
  ]) {
    list.ring(x, y, radius, ["line", da(ctx, 0.5, 0.7)], 3);
    if (fraction > 0.005) list.arc(x, y, radius, top, top + Math.min(1, fraction) * Math.PI * 2, [tone, 0.95], 3, { cap: "round" });
  }
}

function drawOrbitShips(list, ctx, node, fleet, bad) {
  const ships = fleet.state.fleet?.ships ?? [];
  ships.forEach((ship, i) => {
    const a = (i * 2 * Math.PI) / Math.max(1, ships.length) + (ctx.reduced ? 0 : ctx.anim * 0.00008);
    const sx = node.x + Math.cos(a) * node.orbit;
    const sy = node.y + Math.sin(a) * node.orbit;
    const state = shipState(ship, fleet.state.ships[ship.id]);
    const tone = bad ? "muted" : toneOf(state);
    if (!ship.available) list.ring(sx, sy, 3, ["red", 0.9], 1.3, { dash: [1.5, 1.5] });
    else list.disk(sx, sy, 3.2, { fill: [tone, bad ? 0.5 : 1], glow: state === "running" && !bad ? [tone, 0.7] : null });
  });
}

function drawFleetNode(list, ctx, L, node, fleet) {
  const status = fleetStatus(fleet);
  const id = `fleet:${fleet.id}`;
  const hot = ctx.hover === id || (ctx.focus?.level === 0 && ctx.focus.fleet === fleet.id);
  const { x, y, r, orbit } = node;
  list.ring(x, y, orbit, [status.bad ? "red" : "line", status.bad ? 0.5 : da(ctx, 0.7, 0.9)], 1, status.bad ? { dash: [2, 4] } : {});
  drawOrbitShips(list, ctx, node, fleet, status.bad);
  const glow = hot || status.busy ? [status.tone, hot ? 0.9 : 0.5] : null;
  list.disk(x, y, r, { fill: ["pane", 1], stroke: [status.tone, 1], w: hot ? 2 : 1.5, dash: status.bad ? [3, 3] : null, glow });
  const host = fleet.state.fleet?.host;
  if (host && !status.bad) drawGauges(list, ctx, node, host);
  list.hex(x, y, Math.min(15, r * 0.34), { fill: ["pane", 1], stroke: [status.tone, 1], w: 1.3 });
  list.diamond(x, y, Math.min(4.5, r * 0.1), Math.min(6.5, r * 0.15), [status.tone, status.bad ? 0.8 : 0.95]);

  // Narrow stages keep the name and status only, so labels clear the frame and each other.
  const { ly } = node.label;
  const width = L.narrow ? ctx.W * 0.42 : 220;
  list.text(fleet.name, x, ly, { size: 14, weight: 600, color: ["strong", 1], align: "center", max: width });
  if (!L.narrow) {
    const where = fleet.state.fleet?.host.hostname ?? hostOf(fleet.entry.endpoint);
    list.text(`[fleet · ${where}]`, x, ly + 15, { size: 10, weight: 400, mono: true, color: ["muted", 1], align: "center", max: width });
  }
  list.text(status.text, x, ly + (L.narrow ? 15 : 30), { size: 10.5, weight: 500, mono: true, color: [status.bad ? "red" : status.tone === "muted" ? "muted" : status.tone, 1], align: "center", max: L.narrow ? width : 240 });
  if (host && !status.bad && !L.narrow) {
    const memory = host.memory.totalBytes ? 1 - host.memory.availableBytes / host.memory.totalBytes : 0;
    const cpu = host.cpu.usage === null ? "–" : `${Math.round(host.cpu.usage * 100)}%`;
    list.text(`CPU ${cpu} · MEM ${Math.round(memory * 100)}%`, x, ly + 44, { size: 10, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  }
  list.hit({ id, kind: "fleet", fleet: fleet.id, x, y, r: orbit + 8 });
}

function hostOf(endpoint) {
  try {
    return new URL(endpoint).host;
  } catch {
    return "unknown";
  }
}

function drawHub(list, ctx, L) {
  const { x, y } = L.hub;
  list.rrect(x - 17, y - 17, 34, 34, 9, { fill: ["pane", 1], stroke: ["accent", 0.9], sw: 1.5, glow: ["accent", 0.35] });
  for (const dx of [-7, 0, 7]) list.disk(x + dx, y, 2.2, { fill: ["accent", 0.9] });
  list.text("Subspace", x, y + 34, { size: 12, weight: 600, color: ["strong", 1], align: "center" });
  list.text("[this desktop]", x, y + 48, { size: 10, weight: 400, mono: true, color: ["muted", 1], align: "center" });
  list.hit({ id: "hub", kind: "hub", x, y, r: 22 });
}

function drawLinks(list, ctx, L, nodes, fleets) {
  const { x: hx, y: hy } = L.hub;
  nodes.forEach((node, i) => {
    const fleet = fleets[i];
    const status = fleetStatus(fleet);
    const dx = node.x - hx;
    const dy = node.y - hy;
    const d = Math.hypot(dx, dy) || 1;
    const ux = dx / d;
    const uy = dy / d;
    const x1 = hx + ux * 28;
    const y1 = hy + uy * 28;
    const x2 = node.x - ux * (node.orbit + 6);
    const y2 = node.y - uy * (node.orbit + 6);
    if (status.bad) list.segment(x1, y1, x2, y2, ["red", 0.55], 1, { dash: [2, 5] });
    else if (fleet.streaming && fleet.state.fleet) list.segment(x1, y1, x2, y2, ["accent", 0.8], 1.5, { dash: [3, 7], dashOffset: ctx.reduced ? 0 : -ctx.anim * 0.03, glow: ["accent", 0.35] });
    else if (fleet.state.fleet) list.segment(x1, y1, x2, y2, ["line", da(ctx, 0.95, 1)], 1);
    else list.segment(x1, y1, x2, y2, ["muted", 0.5], 1, { dash: [3, 4] });
  });
}

/** `ctx.fleets`: [{id, name, entry, state, connection, streaming}]. */
export function buildLandscapeTier(list, ctx) {
  const fleets = ctx.fleets ?? [];
  const L = landscapeLayout(ctx.W, ctx.H, fleets.map((fleet) => fleet.id), ctx.avoid ?? []);
  const reachable = fleets.filter((fleet) => fleet.state.fleet && !fleetStatus(fleet).bad).length;
  drawFrame(list, ctx, "Fleets", `[landscape · ${fleets.length} fleet${fleets.length === 1 ? "" : "s"}]`, ctx.sample ? `Sample data · ${reachable} of ${fleets.length} answering` : `${reachable} of ${fleets.length} answering`);
  for (const f of [0.55, 1]) list.ellipse(L.C.x, L.C.y, L.rx * f, L.ry * f, ["line", f === 1 ? da(ctx, 0.4, 0.6) : da(ctx, 0.22, 0.4)], 1);
  drawLinks(list, ctx, L, L.nodes, fleets);
  drawHub(list, ctx, L);
  L.nodes.forEach((node, i) => drawFleetNode(list, ctx, L, node, fleets[i]));
  if (fleets.length === 0) list.text("No fleets yet", L.C.x, L.C.y + 80, { size: 12, weight: 500, color: ["muted", 1], align: "center" });
}
