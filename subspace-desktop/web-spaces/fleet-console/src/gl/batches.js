// Turns a frame's draw lists into instance data and ordered batches for the
// WebGL2 renderer. No GL calls: buffers are plain growable arrays, so this is
// testable in Node.
//
// Two cost controls live here. A large stroked ring is drawn as tight pieces
// hugging the stroke instead of one big quad that runs the SDF shader over
// its empty middle; each piece owns a hard angular slice, so the stroke stays
// exact and seamless. The radar sweep's quad covers only the wedge.

export const SHAPE_FLOATS = 26;
export const LINE_FLOATS = 17;
export const PACKET_FLOATS = 9;

const TAU = Math.PI * 2;
const SHAPE_TYPE = { disk: 0, hex: 1, rrect: 2, diamond: 3, arrow: 4, sweep: 6, ringPiece: 7 };
const KIND = { disk: "shape", hex: "shape", rrect: "shape", diamond: "shape", arrow: "shape", sweep: "shape", line: "line", packet: "packet" };
/** A stroke-only ring with a screen radius above this draws as tight pieces. */
export const RING_SPLIT_PX = 18;

/** A growable Float32Array of fixed-size instances. */
export function createInstances(floatsPerInstance) {
  return {
    data: new Float32Array(floatsPerInstance * 256),
    stride: floatsPerInstance,
    count: 0,
    reserve(extra) {
      const need = (this.count + extra) * this.stride;
      if (need <= this.data.length) return;
      let size = this.data.length;
      while (size < need) size *= 2;
      const next = new Float32Array(size);
      next.set(this.data.subarray(0, this.count * this.stride));
      this.data = next;
    },
  };
}

/** True when a disk item is a plain ring that is cheaper split into pieces at `scale`. */
export function ringSplit(item, scale) {
  return item.k === "disk" && !item.fill && Boolean(item.stroke) && item.a0 === null && item.r * scale > RING_SPLIT_PX;
}

/**
 * The pieces of a ring: for each, the quad's centre, rotation (the slice's
 * middle angle) and half extents, and how far the quad centre sits from the
 * ring centre along that angle.
 */
export function ringPieces(item, scale) {
  const count = Math.max(8, Math.min(64, Math.round((item.r * scale) / 24)));
  const span = TAU / count;
  const pad = 1.5;
  const inner = item.r * Math.cos(span / 2) - item.w / 2 - pad;
  const outer = item.r + item.w / 2 + pad;
  const offset = (inner + outer) / 2;
  const pieces = [];
  for (let i = 0; i < count; i += 1) {
    const mid = (i + 0.5) * span;
    pieces.push({ x: item.x + Math.cos(mid) * offset, y: item.y + Math.sin(mid) * offset, rot: mid, hw: (outer - inner) / 2, hh: item.r * Math.sin(span / 2) + pad, offset, span });
  }
  return pieces;
}

/** The axis-aligned box around the sweep's wedge, padded by `pad`. */
export function sweepBounds(item, pad = 3) {
  let minX = item.x;
  let maxX = item.x;
  let minY = item.y;
  let maxY = item.y;
  const steps = 10;
  for (let i = 0; i <= steps; i += 1) {
    const a = item.angle - (item.trail * i) / steps;
    const x = item.x + Math.cos(a) * item.rx;
    const y = item.y + Math.sin(a) * item.ry;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x);
    minY = Math.min(minY, y);
    maxY = Math.max(maxY, y);
  }
  return { x: (minX + maxX) / 2, y: (minY + maxY) / 2, hw: (maxX - minX) / 2 + pad, hh: (maxY - minY) / 2 + pad };
}

/**
 * @param {{shape, line, packet}} instances  buffers from createInstances
 * @param {(spec) => number[]} rgba          [tone, alpha] to [r, g, b, a]
 * @param {(spec) => number[]} glowOf        [tone, strength] to [r, g, b, strength]
 * @param {(tone) => number[]} rgb           tone to [r, g, b]
 */
export function createBatcher(instances, { rgba, glowOf, rgb }) {
  function writeShape(item) {
    const buffer = instances.shape;
    buffer.reserve(1);
    const d = buffer.data;
    let o = buffer.count * SHAPE_FLOATS;
    const put = (...values) => {
      for (const value of values) d[o++] = value;
    };
    const fill = rgba(item.fill);
    const stroke = rgba(item.stroke ?? item.line);
    const glow = glowOf(item.glow);
    switch (item.k) {
      case "disk": {
        let span = item.a0 === null || item.a1 === null ? 0 : item.a1 - item.a0;
        if (span >= TAU) span = 0;
        else if (span < 0) span = ((span % TAU) + TAU) % TAU;
        const half = item.r + item.w * (item.cap === "round" ? 1 : 0.5) + 1;
        const dash = item.dash ?? [0, 0];
        put(item.x, item.y, half, half, 0, SHAPE_TYPE.disk, ...fill, ...stroke, item.r, item.stroke ? item.w : 0, item.a0 ?? 0, span, dash[0], dash[1], item.dashOffset, item.cap === "round" ? 1 : 0, ...glow);
        break;
      }
      case "hex": {
        const half = item.R + item.w + 1;
        put(item.x, item.y, half, half, 0, SHAPE_TYPE.hex, ...fill, ...stroke, item.R, item.stroke ? item.w : 0, 0, 0, 0, 0, 0, 0, ...glow);
        break;
      }
      case "rrect":
        put(item.x + item.w / 2, item.y + item.h / 2, item.w / 2 + item.sw + 1, item.h / 2 + item.sw + 1, 0, SHAPE_TYPE.rrect, ...fill, ...stroke, item.w / 2, item.h / 2, item.r, item.stroke ? item.sw : 0, 0, 0, 0, 0, ...glow);
        break;
      case "diamond":
        put(item.x, item.y, item.hw + 1, item.hh + 1, 0, SHAPE_TYPE.diamond, ...fill, 0, 0, 0, 0, item.hw, item.hh, 0, 0, 0, 0, 0, 0, ...glow);
        break;
      case "arrow": {
        const half = 9 * item.size + 1;
        put(item.x, item.y, half, half, item.rot, SHAPE_TYPE.arrow, ...fill, 0, 0, 0, 0, item.size, 0, 0, 0, 0, 0, 0, 0, ...glow);
        break;
      }
      case "sweep": {
        const box = sweepBounds(item);
        put(box.x, box.y, box.hw, box.hh, 0, SHAPE_TYPE.sweep, ...rgba(item.color), ...stroke, item.angle, item.trail, item.x - box.x, item.y - box.y, item.rx, item.ry, 0, 0, ...glowOf([item.color[0], 0.6]));
        break;
      }
      default:
        return 0;
    }
    buffer.count += 1;
    return 1;
  }

  function writeRingPieces(item, scale) {
    const pieces = ringPieces(item, scale);
    const buffer = instances.shape;
    buffer.reserve(pieces.length);
    const stroke = rgba(item.stroke);
    const glow = glowOf(item.glow);
    const dash = item.dash ?? [0, 0];
    for (const piece of pieces) {
      buffer.data.set([piece.x, piece.y, piece.hw, piece.hh, piece.rot, SHAPE_TYPE.ringPiece, 0, 0, 0, 0, ...stroke, item.r, item.w, piece.rot, piece.span, dash[0], dash[1], item.dashOffset, piece.offset, ...glow], buffer.count * SHAPE_FLOATS);
      buffer.count += 1;
    }
    return pieces.length;
  }

  function writeLine(item, pts = item.pts, closed = item.closed, width = item.w, color = item.color) {
    const n = pts.length / 2;
    const segments = closed ? n : n - 1;
    if (segments < 1) return 0;
    const buffer = instances.line;
    buffer.reserve(segments);
    const c = rgba(color);
    const glow = glowOf(item.glow);
    const dash = item.dash ?? [0, 0];
    const d = buffer.data;
    let distance = 0;
    for (let i = 0; i < segments; i += 1) {
      const j = (i + 1) % n;
      const x0 = pts[i * 2];
      const y0 = pts[i * 2 + 1];
      const x1 = pts[j * 2];
      const y1 = pts[j * 2 + 1];
      const o = buffer.count * LINE_FLOATS;
      d[o] = x0;
      d[o + 1] = y0;
      d[o + 2] = x1;
      d[o + 3] = y1;
      d[o + 4] = c[0];
      d[o + 5] = c[1];
      d[o + 6] = c[2];
      d[o + 7] = c[3];
      d[o + 8] = width;
      d[o + 9] = dash[0];
      d[o + 10] = dash[1];
      d[o + 11] = item.dashOffset ?? 0;
      d[o + 12] = distance;
      d[o + 13] = glow[0];
      d[o + 14] = glow[1];
      d[o + 15] = glow[2];
      d[o + 16] = glow[3];
      buffer.count += 1;
      distance += Math.hypot(x1 - x0, y1 - y0);
    }
    return segments;
  }

  function writePacket(item) {
    const buffer = instances.packet;
    buffer.reserve(1);
    const c = rgb(item.tone);
    buffer.data.set([item.x, item.y, item.ux, item.uy, c[0], c[1], c[2], item.len, item.r], buffer.count * PACKET_FLOATS);
    buffer.count += 1;
    return 1;
  }

  /** Instance data for every layer; returns batches in painter's order. */
  return function build(layers) {
    instances.shape.count = 0;
    instances.line.count = 0;
    instances.packet.count = 0;
    const batches = [];
    layers.forEach((layer, index) => {
      const scale = layer.transform.s;
      let current = null;
      for (const item of layer.items) {
        const split = ringSplit(item, scale);
        const kind = KIND[item.k];
        if (!kind) continue;
        const start = instances[kind].count;
        let written;
        if (split) written = writeRingPieces(item, scale);
        else if (kind === "shape") written = writeShape(item);
        else if (kind === "line") written = writeLine(item);
        else written = writePacket(item);
        if (!written) continue;
        const glows = Boolean(item.glow) || kind === "packet";
        if (current && current.kind === kind) {
          current.count += written;
          current.glow = current.glow || glows;
        } else {
          current = { kind, layer: index, start, count: written, glow: glows };
          batches.push(current);
        }
      }
    });
    return batches;
  };
}
