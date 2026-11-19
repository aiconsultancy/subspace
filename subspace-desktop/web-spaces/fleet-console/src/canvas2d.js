// The Canvas 2D fallback renderer, for web views without WebGL2. It draws the
// same composed frame as the WebGL2 renderer, with canvas shadows for glow, as
// the approved mock-up did. It follows the quality level too: its internal
// resolution, and no shadows once bloom is off.

import { projectStar } from "./stars.js";

const TAU = Math.PI * 2;

export function createCanvasRenderer(canvas) {
  const g = canvas.getContext("2d");
  if (!g) return null;
  let W = 1;
  let H = 1;
  let dpr = 1;
  let displayDpr = 1;
  let quality = { scale: 2, bloom: 2 };
  let palette = null;

  const col = (spec, extra = 1) => {
    if (!spec) return "transparent";
    const c = palette.rgb[spec[0]] ?? palette.rgb.muted;
    return `rgba(${c[0]},${c[1]},${c[2]},${Math.max(0, Math.min(1, spec[1] * extra))})`;
  };
  const glow = (spec) => {
    if (!spec || quality.bloom === 0) return;
    g.shadowColor = col(spec, palette.dark ? 1 : 0.5);
    g.shadowBlur = 8 + spec[1] * 18;
  };
  const noGlow = () => {
    g.shadowBlur = 0;
    g.shadowColor = "transparent";
  };
  const circlePath = (x, y, r) => {
    g.beginPath();
    g.arc(x, y, Math.max(0, r), 0, TAU);
  };
  const dashed = (item) => {
    g.setLineDash(item.dash ?? []);
    g.lineDashOffset = item.dashOffset ?? 0;
  };

  function drawDisk(item) {
    if (item.fill) {
      glow(item.glow);
      circlePath(item.x, item.y, item.r);
      g.fillStyle = col(item.fill);
      g.fill();
      noGlow();
    }
    if (!item.stroke) return;
    g.beginPath();
    if (item.a0 !== null && item.a1 !== null) g.arc(item.x, item.y, item.r, item.a0, item.a1);
    else g.arc(item.x, item.y, item.r, 0, TAU);
    dashed(item);
    g.lineCap = item.cap;
    g.lineWidth = item.w;
    g.strokeStyle = col(item.stroke);
    if (!item.fill) glow(item.glow);
    g.stroke();
    noGlow();
    g.setLineDash([]);
    g.lineCap = "butt";
  }

  function hexPath(x, y, r) {
    g.beginPath();
    for (let i = 0; i < 6; i += 1) {
      const a = -Math.PI / 2 + (i * Math.PI) / 3;
      const px = x + Math.cos(a) * r;
      const py = y + Math.sin(a) * r;
      if (i) g.lineTo(px, py);
      else g.moveTo(px, py);
    }
    g.closePath();
  }

  function drawShapeFillStroke(path, item, strokeWidth) {
    if (item.fill) {
      glow(item.glow);
      path();
      g.fillStyle = col(item.fill);
      g.fill();
      noGlow();
    }
    if (item.stroke) {
      path();
      g.lineWidth = strokeWidth;
      g.strokeStyle = col(item.stroke);
      g.stroke();
    }
  }

  function roundRectPath(x, y, w, h, r) {
    g.beginPath();
    if (g.roundRect) g.roundRect(x, y, w, h, r);
    else g.rect(x, y, w, h);
  }

  function drawSweep(item) {
    g.save();
    g.translate(item.x, item.y);
    g.scale(1, item.ry / item.rx);
    const slices = 18;
    for (let i = 0; i < slices; i += 1) {
      const a = item.angle - (i * item.trail) / slices;
      g.beginPath();
      g.moveTo(0, 0);
      g.arc(0, 0, item.rx, a - item.trail / slices, a);
      g.closePath();
      g.fillStyle = col(item.color, 1 - i / slices);
      g.fill();
    }
    g.beginPath();
    g.moveTo(0, 0);
    g.lineTo(Math.cos(item.angle) * item.rx, Math.sin(item.angle) * item.rx);
    g.restore();
    g.strokeStyle = col(item.line);
    g.lineWidth = 1;
    g.stroke();
  }

  function drawLine(item) {
    const p = item.pts;
    g.beginPath();
    g.moveTo(p[0], p[1]);
    for (let i = 2; i < p.length; i += 2) g.lineTo(p[i], p[i + 1]);
    if (item.closed) g.closePath();
    dashed(item);
    g.lineWidth = item.w;
    g.strokeStyle = col(item.color);
    glow(item.glow ? [item.glow[0], item.glow[1] * 0.6] : null);
    g.stroke();
    noGlow();
    g.setLineDash([]);
  }

  function drawPacket(item) {
    const tail = { x: item.x - item.ux * item.len, y: item.y - item.uy * item.len };
    const gradient = g.createLinearGradient(tail.x, tail.y, item.x, item.y);
    gradient.addColorStop(0, col([item.tone, 0]));
    gradient.addColorStop(1, col([item.tone, 0.95]));
    g.strokeStyle = gradient;
    g.lineWidth = 2;
    g.beginPath();
    g.moveTo(tail.x, tail.y);
    g.lineTo(item.x, item.y);
    g.stroke();
    glow([item.tone, 0.6]);
    circlePath(item.x, item.y, item.r);
    g.fillStyle = col([item.tone, 1]);
    g.fill();
    noGlow();
  }

  function drawItem(item) {
    switch (item.k) {
      case "disk":
        drawDisk(item);
        break;
      case "hex":
        drawShapeFillStroke(() => hexPath(item.x, item.y, item.R), item, item.w);
        break;
      case "rrect":
        drawShapeFillStroke(() => roundRectPath(item.x, item.y, item.w, item.h, item.r), item, item.sw);
        break;
      case "diamond":
        g.beginPath();
        g.moveTo(item.x, item.y - item.hh);
        g.lineTo(item.x + item.hw, item.y);
        g.lineTo(item.x, item.y + item.hh);
        g.lineTo(item.x - item.hw, item.y);
        g.closePath();
        g.fillStyle = col(item.fill);
        g.fill();
        break;
      case "arrow":
        g.save();
        g.translate(item.x, item.y);
        g.rotate(item.rot);
        g.scale(item.size, item.size);
        g.beginPath();
        g.moveTo(9, 0);
        g.lineTo(-6, -6.5);
        g.lineTo(-2.5, 0);
        g.lineTo(-6, 6.5);
        g.closePath();
        g.fillStyle = col(item.fill);
        g.fill();
        g.restore();
        break;
      case "sweep":
        drawSweep(item);
        break;
      case "line":
        drawLine(item);
        break;
      case "packet":
        drawPacket(item);
        break;
      default:
        break;
    }
  }

  function drawBackground(frame) {
    g.fillStyle = col(["stage", 1]);
    g.fillRect(0, 0, W, H);
    const span = Math.max(W, H);
    const vignette = g.createRadialGradient(W / 2, H / 2, 0, W / 2, H / 2, span * 0.7);
    vignette.addColorStop(0, col(["accent", palette.dark ? 0.06 : 0.035]));
    vignette.addColorStop(1, col(["accent", 0]));
    g.fillStyle = vignette;
    g.fillRect(0, 0, W, H);
    if (frame.tunnel > 0.001) {
      const tunnel = g.createRadialGradient(frame.vp.x, frame.vp.y, 0, frame.vp.x, frame.vp.y, span * 0.6);
      tunnel.addColorStop(0, col(["accent", (palette.dark ? 0.16 : 0.09) * frame.tunnel]));
      tunnel.addColorStop(1, col(["accent", 0]));
      g.fillStyle = tunnel;
      g.fillRect(0, 0, W, H);
    }
  }

  function drawStars(frame, stars, motion) {
    const f = Math.max(W, H) * 0.5;
    const streaking = motion.streak > 0.004;
    for (const star of stars) {
      const head = projectStar(star, star.z, frame.vp, f);
      if (head.x < -40 || head.x > W + 40 || head.y < -40 || head.y > H + 40) continue;
      const depth = 1 - star.z;
      const a = (palette.dark ? 0.7 : 0.4) * depth;
      if (streaking) {
        const z2 = Math.max(0.05, Math.min(1.2, star.z + Math.sign(motion.speed || 1) * motion.streak));
        const tail = projectStar(star, z2, frame.vp, f);
        g.beginPath();
        g.moveTo(tail.x, tail.y);
        g.lineTo(head.x, head.y);
        g.strokeStyle = col([palette.dark ? "strong" : "accent", a]);
        g.lineWidth = 0.6 + depth * 1.6;
        g.stroke();
      } else {
        g.fillStyle = col([palette.dark ? "strong" : "muted", a * 0.9]);
        const size = 0.6 + depth * 1.4;
        g.fillRect(head.x, head.y, size, size);
      }
    }
  }

  function drawGate(gate, t) {
    const { x, y, r, p, amp } = gate;
    const rot = t / 1600;
    circlePath(x, y, r);
    g.strokeStyle = col(["accent", 0.75 * amp]);
    g.lineWidth = 2;
    g.stroke();
    circlePath(x, y, r * 1.08);
    g.strokeStyle = col(["accent", 0.3 * amp]);
    g.lineWidth = 1;
    g.stroke();
    const size = Math.max(5, Math.min(11, r * 0.035));
    for (let i = 0; i < 9; i += 1) {
      const angle = rot + (i * TAU) / 9;
      const lit = i / 9 < p * 1.25;
      g.save();
      g.translate(x + Math.cos(angle) * r * 1.04, y + Math.sin(angle) * r * 1.04);
      g.rotate(angle + Math.PI / 2);
      g.beginPath();
      g.moveTo(-size, -0.6 * size);
      g.lineTo(0, 0.6 * size);
      g.lineTo(size, -0.6 * size);
      g.strokeStyle = col([lit ? "accent" : "line", (lit ? 0.95 : 0.6) * amp]);
      g.lineWidth = 2;
      if (lit) glow(["accent", amp]);
      g.stroke();
      noGlow();
      g.restore();
    }
  }

  return {
    kind: "canvas2d",
    lost: false,
    setPalette(next) {
      palette = next;
    },
    resize(nextW, nextH, nextDpr) {
      W = Math.max(1, nextW);
      H = Math.max(1, nextH);
      displayDpr = nextDpr;
      dpr = Math.max(1, Math.min(displayDpr, quality.scale));
      canvas.width = Math.round(W * dpr);
      canvas.height = Math.round(H * dpr);
    },
    setQuality(next) {
      quality = { ...quality, ...next };
      this.resize(W, H, displayDpr);
    },
    get pixels() {
      return { width: canvas.width, height: canvas.height, scale: dpr };
    },
    render(frame, stars, motion, t) {
      if (!palette) return;
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.globalAlpha = 1;
      drawBackground(frame);
      drawStars(frame, stars, motion);
      for (const layer of frame.layers) {
        const { s, tx, ty } = layer.transform;
        g.setTransform(dpr * s, 0, 0, dpr * s, dpr * tx, dpr * ty);
        g.globalAlpha = layer.alpha;
        for (const item of layer.items) drawItem(item);
      }
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.globalAlpha = 1;
      if (frame.gate && frame.gate.amp > 0.001) drawGate(frame.gate, t);
    },
    dispose() {},
  };
}
