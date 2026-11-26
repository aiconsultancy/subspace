// Frame statistics and the frame-time readout: median and p95 frame time,
// CPU and (where the engine exposes timer queries) GPU time, achieved frame
// rate, overlay redraws and the current quality level. The readout shows with
// `?perf=1`, the "Frame-time readout" setting, or Shift+P on the map.

const RING = 240;

/** A fixed-size ring of numbers with percentiles. */
export function createRing(size = RING) {
  const values = new Float64Array(size);
  let count = 0;
  let next = 0;
  return {
    push(value) {
      values[next] = value;
      next = (next + 1) % size;
      count = Math.min(size, count + 1);
    },
    percentile(p) {
      if (!count) return null;
      const sorted = Array.from(values.subarray(0, count)).sort((a, b) => a - b);
      return sorted[Math.min(count - 1, Math.floor(p * count))];
    },
    get count() {
      return count;
    },
    clear() {
      count = 0;
      next = 0;
    },
  };
}

/** Frame statistics: per drawn frame, CPU work and the interval since the previous drawn frame. */
export function createFrameStats() {
  const work = createRing();
  const frame = createRing();
  const gpu = createRing();
  let drawn = 0;
  let overlayDraws = 0;
  let since = null;
  return {
    frame(intervalMs, workMs) {
      frame.push(intervalMs);
      work.push(workMs);
      drawn += 1;
    },
    gpu(ms) {
      gpu.push(ms);
    },
    overlay() {
      overlayDraws += 1;
    },
    /** Rates since the last call. */
    rates(now) {
      const span = since === null ? null : (now - since) / 1000;
      const result = span ? { fps: drawn / span, overlayPerSecond: overlayDraws / span } : { fps: null, overlayPerSecond: null };
      since = now;
      drawn = 0;
      overlayDraws = 0;
      return result;
    },
    snapshot() {
      const round = (v) => (v === null ? null : Math.round(v * 100) / 100);
      return {
        frameMedian: round(frame.percentile(0.5)),
        frameP95: round(frame.percentile(0.95)),
        cpuMedian: round(work.percentile(0.5)),
        cpuP95: round(work.percentile(0.95)),
        gpuMedian: round(gpu.percentile(0.5)),
        gpuP95: round(gpu.percentile(0.95)),
        samples: frame.count,
      };
    },
    clear() {
      work.clear();
      frame.clear();
      gpu.clear();
    },
  };
}

const ms = (v) => (v === null || v === undefined ? "–" : v.toFixed(1));

/** The readout's text lines. Pure, for tests. */
export function hudLines(snapshot, rates, info) {
  return [
    `frame ${ms(snapshot.frameMedian)} / ${ms(snapshot.frameP95)} ms  (median / p95)`,
    `cpu   ${ms(snapshot.cpuMedian)} / ${ms(snapshot.cpuP95)} ms`,
    `gpu   ${info.gpuTimer ? `${ms(snapshot.gpuMedian)} / ${ms(snapshot.gpuP95)} ms` : "not measurable here"}`,
    `fps   ${rates.fps === null ? "–" : rates.fps.toFixed(0)}  overlay ${rates.overlayPerSecond === null ? "–" : rates.overlayPerSecond.toFixed(1)}/s`,
    `level ${info.level} ${info.pinned ? "(pinned)" : ""}· ${info.label}`,
    `scene ${info.width}×${info.height} @${info.scale.toFixed(2)}× · ${info.renderer} · ${info.verdict}`,
    "Shift+P hides this readout",
  ];
}

/**
 * GPU timing through EXT_disjoint_timer_query_webgl2, when the engine has it
 * (Chromium does; WebKit does not). Results arrive a few frames late.
 */
export function createGpuTimer(gl, onResult) {
  const ext = gl?.getExtension?.("EXT_disjoint_timer_query_webgl2");
  if (!ext) return null;
  const pending = [];
  let active = null;
  return {
    begin() {
      if (active || pending.length > 4) return;
      active = gl.createQuery();
      gl.beginQuery(ext.TIME_ELAPSED_EXT, active);
    },
    end() {
      if (!active) return;
      gl.endQuery(ext.TIME_ELAPSED_EXT);
      pending.push(active);
      active = null;
    },
    poll() {
      while (pending.length) {
        const query = pending[0];
        if (!gl.getQueryParameter(query, gl.QUERY_RESULT_AVAILABLE)) break;
        const disjoint = gl.getParameter(ext.GPU_DISJOINT_EXT);
        const ns = gl.getQueryParameter(query, gl.QUERY_RESULT);
        gl.deleteQuery(query);
        pending.shift();
        if (!disjoint) onResult(ns / 1e6);
      }
    },
  };
}

/**
 * Whether the readout shows. `?perf=1` (`forced`) or the setting turn it on;
 * Shift+P flips what is shown until the setting changes, which then decides
 * again. Settings are read again on focus, so only a change resets the flip.
 */
export function createPerfToggle({ forced = false } = {}) {
  let setting = false;
  let flipped = null;
  const base = () => forced || setting;
  return {
    get shown() {
      return flipped ?? base();
    },
    /** The "Frame-time readout" setting: true for On. */
    setSetting(on) {
      if (Boolean(on) === setting) return;
      setting = Boolean(on);
      flipped = null;
    },
    /** Shift+P: show it if hidden, hide it if shown. Returns the new state. */
    toggle() {
      flipped = !this.shown;
      return flipped;
    },
  };
}

/** Shift+P, without other modifiers or key repeat. */
export function isPerfToggleKey(event) {
  if (!event.shiftKey || event.metaKey || event.ctrlKey || event.altKey || event.repeat) return false;
  return event.code === "KeyP" || event.key === "P" || event.key === "p";
}

/** Mount the readout in `parent`, hidden; returns {element, update(lines), show(on)}. */
export function mountHud(parent) {
  const box = document.createElement("pre");
  box.className = "perf-hud";
  box.setAttribute("aria-hidden", "true");
  box.hidden = true;
  parent.append(box);
  return {
    element: box,
    update(lines) {
      box.textContent = lines.join("\n");
    },
    show(on) {
      box.hidden = !on;
    },
  };
}

/**
 * The frame-time readout: frame times, CPU and GPU split where measurable,
 * and the quality level, mounted hidden in `parent`. Shown by `?perf=1`, the
 * setting or Shift+P, and updated only while it is visible. `onGpu` hears
 * each GPU time measured; `onMount` gets the readout's element once mounted.
 * Returns {show(on)}.
 */
export function createPerfReadout({ renderer, stats, governor, parent, onGpu = () => {}, onMount = () => {} }) {
  const gpuTimer = renderer.enableGpuTimer?.((ms) => {
    onGpu(ms);
    stats.gpu(ms);
  });
  const hud = mountHud(parent);
  onMount(hud.element);
  const snapshot = () => {
    const pixels = renderer.pixels ?? { width: 0, height: 0, scale: 1 };
    const settings = governor.settings;
    return { ...stats.snapshot(), level: governor.level, label: settings.label, bloom: settings.bloom, scale: pixels.scale, width: pixels.width, height: pixels.height, renderer: renderer.kind, verdict: governor.verdict, refresh: governor.refresh, history: governor.history, gpuTimer: Boolean(gpuTimer) };
  };
  Object.defineProperty(window, "__fleetPerf", { value: Object.freeze({ snapshot }) });
  let timer = 0;
  const update = () => {
    if (document.hidden) return;
    const info = snapshot();
    hud.update(hudLines(info, stats.rates(performance.now()), { ...info, pinned: governor.pinned }));
  };
  return {
    show(on) {
      hud.show(on);
      if (on && !timer) {
        stats.rates(performance.now());
        update();
        timer = setInterval(update, 500);
      } else if (!on && timer) {
        clearInterval(timer);
        timer = 0;
      }
    },
  };
}
