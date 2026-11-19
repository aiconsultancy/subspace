// The text overlay: a Canvas 2D layer above the scene that draws every
// label, pill and badge with the same camera transform as its tier, so text
// stays crisp at any zoom. Canvas text cannot ask for tabular figures, so the
// status lines whose numbers tick (busy counts, durations) use the mono family.
//
// It redraws only when what it would draw changed: the camera, a label's
// text or position, hover or selection. While the map idles that is about
// once a second, when a running time ticks.

const TEXT = new Set(["text", "run", "pill"]);

/** Everything the overlay would draw for `frame`, as one comparable string. */
export function overlaySignature(frame) {
  const parts = [];
  for (const layer of frame.layers) {
    const { s, tx, ty } = layer.transform;
    parts.push(`L${s.toFixed(4)},${tx.toFixed(2)},${ty.toFixed(2)},${(layer.textAlpha ?? layer.alpha).toFixed(3)}`);
    for (const item of layer.items) {
      if (!TEXT.has(item.k)) continue;
      if (item.k === "run") {
        parts.push(`R${item.x},${item.y},${item.max},${item.parts.map((p) => `${p.s}|${p.size}|${p.weight}|${p.mono}|${p.color}`).join("/")}`);
      } else {
        parts.push(`${item.k}${item.s}|${item.x},${item.y}|${item.size}|${item.weight}|${item.mono}|${item.color}|${item.align}|${item.base}|${item.max}|${item.bg}|${item.dot}|${item.h}`);
      }
    }
  }
  return parts.join("\n");
}

/** True when two frames' cameras differ, so the labels must move. */
export function camerasDiffer(a, b) {
  if (!a || a.length !== b.length) return true;
  for (let i = 0; i < a.length; i += 1) {
    const [x, y] = [a[i], b[i]];
    if (x.s !== y.s || x.tx !== y.tx || x.ty !== y.ty || x.alpha !== y.alpha) return true;
  }
  return false;
}

const cameraOf = (frame) => frame.layers.map((layer) => ({ ...layer.transform, alpha: layer.textAlpha ?? layer.alpha }));

export function createOverlay(canvas) {
  const g = canvas.getContext("2d");
  let W = 1;
  let H = 1;
  let dpr = 1;
  let palette = null;
  const fitCache = new Map();
  const widthCache = new Map();
  let shown = null;
  let shownCamera = null;
  // The context's font, tracked here: reading or setting ctx.font parses CSS.
  let currentFont = "";
  const fontCache = new Map();

  function setFont(weight, size, mono) {
    const key = mono ? -size - weight * 1000 : size + weight * 1000;
    let value = fontCache.get(key);
    if (value === undefined) {
      value = `${weight} ${size}px ${mono ? palette.fonts.mono : palette.fonts.ui}`;
      fontCache.set(key, value);
    }
    if (value !== currentFont) {
      g.font = value;
      currentFont = value;
    }
  }

  /** measureText with a cache: the same label is measured once per font. */
  function width(text) {
    const key = `${currentFont}|${text}`;
    let value = widthCache.get(key);
    if (value === undefined) {
      value = g.measureText(text).width;
      if (widthCache.size > 4000) widthCache.clear();
      widthCache.set(key, value);
    }
    return value;
  }

  const colourCache = new Map();
  const col = (spec) => {
    const key = `${spec[0]}${spec[1]}`;
    let value = colourCache.get(key);
    if (value === undefined) {
      const c = palette.rgb[spec[0]] ?? palette.rgb.fg;
      value = `rgba(${c[0]},${c[1]},${c[2]},${spec[1]})`;
      if (colourCache.size > 500) colourCache.clear();
      colourCache.set(key, value);
    }
    return value;
  };

  /** `text` cut with an ellipsis to fit `max` pixels in the current font. */
  function fit(text, max) {
    if (!max || width(text) <= max) return text;
    const key = `${currentFont}|${max}|${text}`;
    const cached = fitCache.get(key);
    if (cached !== undefined) return cached;
    let lo = 0;
    let hi = text.length;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (width(`${text.slice(0, mid)}…`) <= max) lo = mid;
      else hi = mid - 1;
    }
    const result = `${text.slice(0, lo).trimEnd()}…`;
    if (fitCache.size > 600) fitCache.clear();
    fitCache.set(key, result);
    return result;
  }

  // Labels off the canvas are skipped: while a warp zooms the parent tier, most are.
  let view = { s: 1, tx: 0, ty: 0 };
  function offscreen(x, y, reach) {
    const sx = x * view.s + view.tx;
    const sy = y * view.s + view.ty;
    const r = reach * view.s;
    return sx + r < 0 || sx - r > W || sy + r < 0 || sy - r > H;
  }

  function drawText(item) {
    if (offscreen(item.x, item.y, (item.max ?? 400) + item.size)) return;
    setFont(item.weight, item.size, item.mono);
    g.textAlign = item.align;
    g.textBaseline = item.base;
    g.fillStyle = col(item.color);
    g.fillText(fit(item.s, item.max), item.x, item.y);
  }

  function drawRun(item) {
    if (offscreen(item.x, item.y, (item.max ?? 600) + 20)) return;
    let x = item.x;
    const end = item.max ? item.x + item.max : Infinity;
    g.textAlign = "left";
    g.textBaseline = "alphabetic";
    item.parts.forEach((part, i) => {
      if (x >= end - 20) return;
      setFont(part.weight ?? 500, part.size ?? 12, part.mono);
      g.fillStyle = col(part.color ?? ["fg", 1]);
      const text = fit(part.s, end - x);
      g.fillText(text, x, item.y);
      x += width(text) + (i < item.parts.length - 1 ? item.gap : 0);
    });
  }

  function drawPill(item) {
    if (offscreen(item.x, item.y, item.max + item.h)) return;
    setFont(500, item.size, item.mono);
    const dotSpace = item.dot ? 10 : 0;
    const text = fit(item.s, item.max - 14 - dotSpace);
    const w = width(text) + 14 + dotSpace;
    const h = item.h;
    const x0 = item.align === "left" ? item.x : item.x - w / 2;
    g.beginPath();
    if (g.roundRect) g.roundRect(x0, item.y - h / 2, w, h, h / 2);
    else g.rect(x0, item.y - h / 2, w, h);
    g.fillStyle = col(item.bg);
    g.fill();
    if (item.dot) {
      g.beginPath();
      g.arc(x0 + 9, item.y, 3, 0, Math.PI * 2);
      g.fillStyle = col([item.dot, 1]);
      g.fill();
    }
    g.fillStyle = col(item.color);
    g.textAlign = "left";
    g.textBaseline = "middle";
    g.fillText(text, x0 + 7 + dotSpace, item.y + 0.5);
  }

  function forget() {
    shown = null;
    shownCamera = null;
    currentFont = "";
  }

  return {
    setPalette(next) {
      palette = next;
      fitCache.clear();
      widthCache.clear();
      fontCache.clear();
      colourCache.clear();
      forget();
    },
    resize(nextW, nextH, nextDpr) {
      W = Math.max(1, nextW);
      H = Math.max(1, nextH);
      dpr = nextDpr;
      canvas.width = Math.round(W * dpr);
      canvas.height = Math.round(H * dpr);
      forget();
    },
    /** Draw `frame`'s labels if they differ from what is shown; returns true when it drew. */
    render(frame) {
      if (!palette || !g) return false;
      const camera = cameraOf(frame);
      if (camerasDiffer(shownCamera, camera)) {
        // A moving camera always redraws; skip building the signature.
        shown = null;
      } else {
        const signature = overlaySignature(frame);
        if (signature === shown) return false;
        shown = signature;
      }
      shownCamera = camera;
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.globalAlpha = 1;
      g.clearRect(0, 0, canvas.width, canvas.height);
      for (const layer of frame.layers) {
        view = layer.transform;
        const { s, tx, ty } = view;
        g.setTransform(dpr * s, 0, 0, dpr * s, dpr * tx, dpr * ty);
        g.globalAlpha = layer.textAlpha ?? layer.alpha;
        if (g.globalAlpha < 0.01) continue;
        for (const item of layer.items) {
          if (item.k === "text") drawText(item);
          else if (item.k === "run") drawRun(item);
          else if (item.k === "pill") drawPill(item);
        }
      }
      g.globalAlpha = 1;
      return true;
    },
  };
}
