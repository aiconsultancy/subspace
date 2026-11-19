// Tier layouts: where each node sits for a stage of W x H CSS pixels. Pure, so
// the renderer, hit testing and the warp all agree on the same positions.

export const NARROW_STAGE = 560;
export const NARROW_CREW = 600;

/** Where a landscape node's labels sit: below it, or above it near the top. */
export function landscapeLabel(node, narrow, W) {
  const lines = narrow ? 2 : 4;
  // A little short of -0.5, so nodes level with each other (30° above the middle) agree.
  const below = Math.sin(node.angle) > -0.49;
  const ly = below ? node.y + node.orbit + 20 : node.y - node.orbit - 12 - (lines - 1) * 14;
  const half = narrow ? W * 0.21 : 110;
  return { below, ly, box: { x0: node.x - half, y0: ly - 12, x1: node.x + half, y1: ly + (lines - 1) * 14 + 4 } };
}

const overlap = (a, b) => Math.max(0, Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0)) * Math.max(0, Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0));

// Ways to rearrange the landscape when labels would collide, least change
// first: turn half a step, flatten the ellipse, lift it, move it right, and
// last, draw the fleets smaller.
const VARIANTS = [];
for (const turn of [0, 1]) for (const flat of [0, 1, 2]) for (const lift of [0, 1, 2, 3]) for (const shift of [0, 1, 2]) for (const shrink of [0, 1]) VARIANTS.push({ turn, flat, lift, shift, shrink, change: turn + flat + lift + shift * 1.2 + shrink * 2.5 });
VARIANTS.sort((a, b) => a.change - b.change);

/**
 * Fleets tier (the landscape): this Subspace in the middle, one node per
 * fleet around it. Two fleets sit left and right, an even count starts half
 * a step round, so the arrangement stays level. `avoid` lists rectangles the
 * labels must keep clear of (the tier stack); the frame title, this
 * Subspace's own label and the stage's edges are always kept clear. The first
 * arrangement that clears them all wins, else the one that overlaps least.
 */
export function landscapeLayout(W, H, fleetIds, avoid = []) {
  const portrait = H > W;
  const n = Math.max(1, fleetIds.length);
  const narrow = W < NARROW_STAGE;
  const rx = Math.min(W * 0.33, H * 0.62);
  const ry0 = portrait ? Math.min(H * 0.26, rx * 1.1) : Math.min(H * 0.27, rx * 0.72);
  const scale = Math.max(0.6, Math.min(1, 5 / n, W / 760));
  const start0 = n === 1 ? 0 : n % 2 === 0 ? -Math.PI / 2 + Math.PI / n : -Math.PI / 2;
  const keepClear = [{ x0: 0, y0: 0, x1: Math.min(W * 0.4, 300), y1: 64 }, ...avoid];
  let best = null;
  for (const variant of VARIANTS) {
    if (variant.turn && n < 3) continue;
    const C = { x: W / 2 + variant.shift * W * 0.05, y: H / 2 + (portrait ? 30 : 8) - variant.lift * H * 0.035 };
    const ry = ry0 * (1 - variant.flat * 0.14);
    const start = start0 + (variant.turn ? Math.PI / n : 0);
    // This Subspace's own node and its two lines of text.
    const hub = { x0: C.x - 70, y0: C.y - 24, x1: C.x + 70, y1: C.y + 54 };
    let cost = 0;
    const nodes = fleetIds.map((id, i) => {
      const angle = start + (i * 2 * Math.PI) / n;
      const size = scale * (1 - variant.shrink * 0.18);
      const r = 44 * size;
      const node = { id, angle, x: C.x + Math.cos(angle) * rx, y: C.y + Math.sin(angle) * ry, r, orbit: r + 16 * size };
      node.label = landscapeLabel(node, narrow, W);
      const { box } = node.label;
      for (const rect of [...keepClear, hub]) cost += overlap(box, rect);
      cost += overlap({ x0: node.x - node.orbit, y0: node.y - node.orbit, x1: node.x + node.orbit, y1: node.y + node.orbit }, hub);
      const wide = box.x1 - box.x0;
      cost += (Math.max(0, box.y1 - (H - 24) - 0.5) + Math.max(0, 24 - box.y0)) * wide + (Math.max(0, 8 - box.x0) + Math.max(0, box.x1 - (W - 8))) * 60;
      if (node.x - node.orbit < 0 || node.x + node.orbit > W || node.y - node.orbit < 0 || node.y + node.orbit > H) cost += 1e6;
      return node;
    });
    if (!best || cost < best.cost) best = { cost, C, ry, nodes };
    if (cost === 0) break;
  }
  return { C: best.C, rx, ry: best.ry, nodes: best.nodes, hub: { x: best.C.x, y: best.C.y, r: 20 }, narrow };
}

/** Fleet tier: the rear admiral in the middle, ships on an ellipse around it. */
export function fleetLayout(W, H, shipIds) {
  const portrait = H > W;
  const C = { x: W / 2, y: H / 2 + (portrait ? 22 : 10) };
  const rx = Math.min(W * 0.36, H * 0.66);
  // Portrait stages (a phone, a narrow window) have room to spread vertically.
  const ry = portrait ? Math.min(H * 0.26, rx * 1.05) : Math.min(H * 0.29, rx * 0.66);
  const n = Math.max(1, shipIds.length);
  const scale = Math.max(0.62, Math.min(1, 6 / n, W / 520));
  const ships = shipIds.map((id, i) => {
    const angle = -Math.PI / 2 + (i * 2 * Math.PI) / n;
    return { id, angle, x: C.x + Math.cos(angle) * rx, y: C.y + Math.sin(angle) * ry, r: 22 * scale, orbit: 36 * scale };
  });
  return { C, rx, ry, ships, admiral: { x: C.x, y: C.y, r: 28 }, narrow: W < NARROW_STAGE };
}

/** Ship tier: the captain in the middle, crew dials on an ellipse, offset half a step. */
export function shipLayout(W, H, crewIds) {
  const C = { x: W / 2, y: H / 2 + 12 };
  const n = Math.max(1, crewIds.length);
  const rx = Math.min(W * 0.34, H * 0.75);
  const ry = Math.min(H * (H > W ? 0.32 : 0.29), rx * (H > W ? 1.15 : 0.72));
  const scale = Math.max(0.7, Math.min(1, 7 / n, W / 480));
  const crew = crewIds.map((id, i) => {
    const angle = -Math.PI / 2 + Math.PI / n + (i * 2 * Math.PI) / n;
    return { id, angle, x: C.x + Math.cos(angle) * rx, y: C.y + Math.sin(angle) * ry, r: 26 * scale };
  });
  return { C, rx, ry, captain: { x: C.x, y: C.y, r: 34 }, crew, narrow: W < NARROW_STAGE };
}

function card(cx, cy, w, h, index) {
  return { index, x: cx - w / 2, y: cy - h / 2, w, h, cx, cy };
}

/** Crew tier: the member's 24-hour dial with one card per project context. */
export function crewLayout(W, H, contextCount) {
  const n = Math.max(0, contextCount);
  if (W < NARROW_CREW) {
    const C = { x: W / 2, y: H * 0.3 };
    const w = Math.min(320, W - 48);
    const h = 62;
    const cards = Array.from({ length: n }, (_, i) => card(W / 2, H * 0.53 + i * (h + 10) + h / 2, w, h, i));
    return { C, R: 50, r: 32, cards, narrow: true };
  }
  const C = { x: n === 1 ? W * 0.4 : W / 2, y: H / 2 + 6 };
  const w = 236;
  const h = 76;
  const rx = Math.min(W * 0.3, 320);
  if (n <= 3) {
    const slots = n === 1 ? [0] : n === 2 ? [0, Math.PI] : [-0.45, Math.PI + 0.45, Math.PI / 2];
    const cards = slots.slice(0, n).map((angle, i) => card(C.x + Math.cos(angle) * rx, C.y + Math.sin(angle) * H * 0.26, w, h, i));
    return { C, R: 68, r: 40, cards, narrow: false };
  }
  // Four or more: two columns either side of the dial.
  const rows = Math.ceil(n / 2);
  const gap = 12;
  const top = C.y - (rows * h + (rows - 1) * gap) / 2 + h / 2;
  const cards = Array.from({ length: n }, (_, i) => {
    const side = i % 2 === 0 ? 1 : -1;
    const row = Math.floor(i / 2);
    return card(C.x + side * rx, top + row * (h + gap), w, h, i);
  });
  return { C, R: 68, r: 40, cards, narrow: false };
}

/**
 * The circle a warp flies through: the child's node in the parent's layout.
 * `ids` gives the parent's node order: fleet IDs for the landscape, ship IDs
 * for a fleet, crew IDs for a ship. `avoid` is the landscape's, as drawn.
 */
export function focusFor(parent, child, W, H, ids, avoid = []) {
  if (parent.level === -1) {
    const node = landscapeLayout(W, H, ids, avoid).nodes.find((entry) => entry.id === child.fleet);
    return node ? { x: node.x, y: node.y, r: node.orbit } : { x: W / 2, y: H / 2, r: 60 };
  }
  if (parent.level === 0) {
    const node = fleetLayout(W, H, ids).ships.find((entry) => entry.id === child.ship);
    return node ? { x: node.x, y: node.y, r: node.orbit } : { x: W / 2, y: H / 2, r: 36 };
  }
  const node = shipLayout(W, H, ids).crew.find((entry) => entry.id === child.crew);
  return node ? { x: node.x, y: node.y, r: node.r } : { x: W / 2, y: H / 2, r: 26 };
}
