// Point lists for the line renderer: ellipses, quadratic curves and rounded
// rectangles as flat [x0, y0, x1, y1, ...] arrays. Pure.

// Layouts repeat frame after frame, so recent point lists are reused. They are
// read only by the renderers, never changed.
const cache = new Map();
function remember(key, make) {
  let value = cache.get(key);
  if (!value) {
    value = make();
    if (cache.size > 256) cache.clear();
    cache.set(key, value);
  }
  return value;
}

export function ellipsePoints(cx, cy, rx, ry, segments = 96) {
  return remember(`e${cx},${cy},${rx},${ry},${segments}`, () => buildEllipse(cx, cy, rx, ry, segments));
}

function buildEllipse(cx, cy, rx, ry, segments) {
  const points = [];
  for (let i = 0; i < segments; i += 1) {
    const a = (i / segments) * Math.PI * 2;
    points.push(cx + Math.cos(a) * rx, cy + Math.sin(a) * ry);
  }
  return points;
}

export function quadPoint(q, u) {
  const v = 1 - u;
  return { x: v * v * q.x1 + 2 * v * u * q.mx + u * u * q.x2, y: v * v * q.y1 + 2 * v * u * q.my + u * u * q.y2 };
}

export function quadPoints(q, segments = 28) {
  const points = [];
  for (let i = 0; i <= segments; i += 1) {
    const p = quadPoint(q, i / segments);
    points.push(p.x, p.y);
  }
  return points;
}

export function roundedRectPoints(x, y, w, h, r, perCorner = 6) {
  return remember(`r${x},${y},${w},${h},${r},${perCorner}`, () => buildRoundedRect(x, y, w, h, r, perCorner));
}

function buildRoundedRect(x, y, w, h, r, perCorner) {
  const radius = Math.max(0, Math.min(r, w / 2, h / 2));
  const corners = [
    [x + w - radius, y + radius, -Math.PI / 2],
    [x + w - radius, y + h - radius, 0],
    [x + radius, y + h - radius, Math.PI / 2],
    [x + radius, y + radius, Math.PI],
  ];
  const points = [];
  for (const [cx, cy, start] of corners) {
    for (let i = 0; i <= perCorner; i += 1) {
      const a = start + (i / perCorner) * (Math.PI / 2);
      points.push(cx + Math.cos(a) * radius, cy + Math.sin(a) * radius);
    }
  }
  return points;
}

/** Length of a polyline, closed or open. */
export function polylineLength(points, closed) {
  let total = 0;
  const n = points.length / 2;
  const last = closed ? n : n - 1;
  for (let i = 0; i < last; i += 1) {
    const j = (i + 1) % n;
    total += Math.hypot(points[j * 2] - points[i * 2], points[j * 2 + 1] - points[i * 2 + 1]);
  }
  return total;
}
