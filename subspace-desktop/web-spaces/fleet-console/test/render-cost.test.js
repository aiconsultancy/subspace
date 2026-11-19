import assert from "node:assert/strict";
import { test } from "node:test";
import { overlaySignature } from "../src/overlay.js";
import { createBatcher, createInstances, ringPieces, ringSplit, sweepBounds, SHAPE_FLOATS } from "../src/gl/batches.js";
import { composeFrame } from "../src/scene/index.js";
import { createNavigator, TOP } from "../src/camera.js";
import { createFrameStats, createPerfToggle, createRing, hudLines, isPerfToggleKey } from "../src/perf.js";
import { loadedState, NOW } from "./fixtures.js";

async function context(overrides = {}) {
  const state = await loadedState();
  return { W: 1000, H: 640, now: NOW, clock: NOW, anim: 1000, reduced: false, dark: true, fleet: state.fleet, ships: state.ships, packets: [], unavailableSince: state.unavailableSince, hover: null, issueBadge: () => null, ...overrides };
}

function landedNav() {
  const nav = createNavigator({ start: 0 });
  nav.tick(5000);
  return nav;
}

test("the overlay signature ignores motion that has no text, and sees every text change", async () => {
  const nav = landedNav();
  const ctx = await context();
  const a = overlaySignature(composeFrame(nav.state, ctx, 9000, 0.016));
  // The sweep, orbits and dashes move with animation time; labels do not.
  const b = overlaySignature(composeFrame(nav.state, { ...ctx, anim: 4321 }, 9500, 0.016));
  assert.equal(a, b, "ambient animation alone does not redraw the labels");
  const hovered = overlaySignature(composeFrame(nav.state, { ...ctx, hover: "ship:galactica" }, 9000, 0.016));
  assert.equal(hovered, a, "hover changes shapes, not labels");
  const ticked = overlaySignature(composeFrame(nav.state, { ...ctx, now: NOW + 61_000 }, 9000, 0.016));
  assert.notEqual(ticked, a, "a running time ticking over redraws");
  const narrower = overlaySignature(composeFrame(nav.state, { ...ctx, W: 900 }, 9000, 0.016));
  assert.notEqual(narrower, a, "a resize redraws");
});

test("a moving camera always redraws without building a signature", async () => {
  const { camerasDiffer } = await import("../src/overlay.js");
  const at = (s, tx) => [{ s, tx, ty: 0, alpha: 1 }];
  assert.equal(camerasDiffer(null, at(1, 0)), true);
  assert.equal(camerasDiffer(at(1, 0), at(1, 0)), false);
  assert.equal(camerasDiffer(at(1, 0), at(1.01, 0)), true);
  assert.equal(camerasDiffer(at(1, 0), [...at(1, 0), ...at(2, 0)]), true);
});

test("the overlay signature changes every frame while the camera moves", async () => {
  const nav = landedNav();
  nav.go({ level: 1, ship: "galactica", crew: null }, 10_000);
  const ctx = await context();
  const first = overlaySignature(composeFrame(nav.state, ctx, 10_300, 0.016));
  const second = overlaySignature(composeFrame(nav.state, ctx, 10_316, 0.016));
  assert.notEqual(first, second);
  nav.tick(20_000);
  const settledA = overlaySignature(composeFrame(nav.state, ctx, 30_000, 0.016));
  const settledB = overlaySignature(composeFrame(nav.state, { ...ctx, anim: 999 }, 30_500, 0.016));
  assert.equal(settledA, settledB, "and stops once it lands");
});

test("an overlay redraws only when its signature changes", async () => {
  const { createOverlay } = await import("../src/overlay.js");
  let draws = 0;
  const g = {
    setTransform() {},
    clearRect() {
      draws += 1;
    },
    measureText: (text) => ({ width: text.length * 6 }),
    fillText() {},
    beginPath() {},
    roundRect() {},
    rect() {},
    arc() {},
    fill() {},
  };
  const canvas = { getContext: () => g, width: 0, height: 0 };
  const overlay = createOverlay(canvas);
  overlay.setPalette({ rgb: { fg: [1, 2, 3], strong: [1, 2, 3], muted: [1, 2, 3], accent: [1, 2, 3], select: [1, 2, 3] }, fonts: { ui: "sans-serif", mono: "monospace" } });
  overlay.resize(1000, 640, 2);
  const nav = landedNav();
  const ctx = await context();
  assert.equal(overlay.render(composeFrame(nav.state, ctx, 9000, 0.016)), true);
  // The first frame after the camera settles redraws once to learn the signature.
  assert.equal(overlay.render(composeFrame(nav.state, ctx, 9100, 0.016)), true);
  assert.equal(overlay.render(composeFrame(nav.state, { ...ctx, anim: 5000 }, 9300, 0.016)), false);
  assert.equal(overlay.render(composeFrame(nav.state, { ...ctx, now: NOW + 61_000 }, 9600, 0.016)), true);
  assert.equal(overlay.render(composeFrame(nav.state, { ...ctx, now: NOW + 61_000, anim: 7000 }, 9700, 0.016)), false);
  overlay.resize(1000, 640, 2);
  assert.equal(overlay.render(composeFrame(nav.state, { ...ctx, now: NOW + 61_000 }, 9900, 0.016)), true, "a resize clears the canvas, so it redraws");
  assert.equal(draws, 4);
});

test("big stroked rings split into tight pieces; small rings, fills and arcs stay single quads", () => {
  const ring = { k: "disk", x: 0, y: 0, r: 36, fill: null, stroke: ["line", 1], w: 1, a0: null, a1: null };
  assert.equal(ringSplit(ring, 1), true);
  assert.equal(ringSplit({ ...ring, r: 10 }, 1), false);
  assert.equal(ringSplit({ ...ring, r: 10 }, 4), true, "a small ring zoomed by the warp splits too");
  assert.equal(ringSplit({ ...ring, fill: ["pane", 1] }, 1), false);
  assert.equal(ringSplit({ ...ring, a0: 0, a1: 1 }, 1), false);
  const pieces = ringPieces({ ...ring, r: 300, w: 2 }, 1);
  const total = pieces.reduce((sum, piece) => sum + piece.span, 0);
  assert.ok(Math.abs(total - Math.PI * 2) < 1e-9, "slices cover the circle exactly once");
  const area = pieces.reduce((sum, piece) => sum + 4 * piece.hw * piece.hh, 0);
  assert.ok(area < (2 * 302) ** 2 * 0.1, `pieces cover under a tenth of the old quad (${Math.round(area)} px²)`);
  for (const piece of pieces) {
    assert.ok(piece.offset - piece.hw <= 300 * Math.cos(piece.span / 2) - 1 && piece.offset + piece.hw >= 301, "each piece holds the whole stroke");
  }
});

test("the sweep's quad hugs its wedge", () => {
  const item = { k: "sweep", x: 500, y: 300, rx: 400, ry: 260, angle: 0.3, trail: 0.72 };
  const box = sweepBounds(item);
  assert.ok(4 * box.hw * box.hh < 4 * item.rx * item.ry * 0.35, "well under the ellipse's box");
  for (let i = 0; i <= 20; i += 1) {
    const a = item.angle - (item.trail * i) / 20;
    for (const f of [0.2, 0.6, 1]) {
      const x = item.x + Math.cos(a) * item.rx * f;
      const y = item.y + Math.sin(a) * item.ry * f;
      assert.ok(Math.abs(x - box.x) <= box.hw && Math.abs(y - box.y) <= box.hh, "every wedge point is inside");
    }
  }
});

test("batching keeps painter's order, marks glowing batches and writes ring pieces as shapes", () => {
  const instances = { shape: createInstances(26), line: createInstances(17), packet: createInstances(9) };
  const zero = () => [0, 0, 0, 0];
  const build = createBatcher(instances, { rgba: (spec) => (spec ? [1, 1, 1, spec[1]] : [0, 0, 0, 0]), glowOf: (spec) => (spec ? [1, 1, 1, spec[1]] : zero()), rgb: () => [1, 1, 1] });
  const layer = {
    transform: { s: 1, tx: 0, ty: 0 },
    alpha: 1,
    items: [
      { k: "line", pts: [0, 0, 10, 0, 10, 10], closed: false, color: ["line", 1], w: 1, dash: null, dashOffset: 0, glow: null },
      { k: "disk", x: 0, y: 0, r: 40, fill: null, stroke: ["line", 1], w: 1, a0: null, a1: null, dash: [2, 4], dashOffset: 0, cap: "butt", glow: null },
      { k: "disk", x: 0, y: 0, r: 5, fill: ["pane", 1], stroke: null, w: 1, a0: null, a1: null, dash: null, dashOffset: 0, cap: "butt", glow: ["accent", 0.5] },
      { k: "text", s: "ignored" },
      { k: "packet", x: 1, y: 1, ux: 1, uy: 0, tone: "accent", len: 24, r: 3 },
    ],
  };
  const batches = build([layer]);
  assert.deepEqual(batches.map((b) => [b.kind, b.count, b.glow]), [["line", 2, false], ["shape", 9, true], ["packet", 1, true]]);
  assert.equal(instances.shape.count, 9, "eight ring pieces and one disc");
  assert.equal(instances.shape.data[5], 7, "ring pieces use their own shape type");
  assert.equal(instances.shape.data[8 * SHAPE_FLOATS + 5], 0, "the disc stays a disc");
});

test("frame statistics and the readout", () => {
  const ring = createRing(4);
  [5, 1, 3, 2, 4].forEach((v) => ring.push(v));
  assert.equal(ring.count, 4);
  assert.equal(ring.percentile(0.5), 3);
  const stats = createFrameStats();
  for (let i = 0; i < 20; i += 1) stats.frame(16 + (i % 2), 2 + (i === 19 ? 10 : 0));
  stats.overlay();
  const snapshot = stats.snapshot();
  assert.equal(snapshot.frameMedian, 17);
  assert.equal(snapshot.cpuP95, 12);
  assert.equal(snapshot.gpuMedian, null);
  stats.rates(0);
  const rates = stats.rates(1000);
  const lines = hudLines(snapshot, rates, { gpuTimer: false, level: 2, pinned: false, label: "1.25× scene", width: 1400, height: 1050, scale: 1.25, renderer: "webgl2", verdict: "ok" });
  assert.match(lines[0], /^frame 17\.0 \/ 17\.0 ms/);
  assert.match(lines[2], /not measurable here/);
  assert.match(lines[4], /level 2 · 1\.25× scene/);
  assert.match(lines[5], /1400×1050 @1\.25×/);
  assert.match(lines.at(-1), /Shift\+P hides/);
});

test("the frame-time readout: off by default, on from ?perf=1 or the setting, flipped by Shift+P", () => {
  const plain = createPerfToggle();
  assert.equal(plain.shown, false, "off by default");
  assert.equal(plain.toggle(), true, "Shift+P shows it");
  assert.equal(plain.shown, true);
  plain.setSetting(false);
  assert.equal(plain.shown, true, "reading the same setting again keeps the flip");
  assert.equal(plain.toggle(), false, "Shift+P hides it again");

  const fromSetting = createPerfToggle();
  fromSetting.setSetting(true);
  assert.equal(fromSetting.shown, true, "the setting turns it on");
  fromSetting.toggle();
  assert.equal(fromSetting.shown, false);
  fromSetting.setSetting(true);
  assert.equal(fromSetting.shown, false, "an unchanged setting does not undo Shift+P");
  fromSetting.setSetting(false);
  fromSetting.setSetting(true);
  assert.equal(fromSetting.shown, true, "a changed setting decides again");

  const forced = createPerfToggle({ forced: true });
  assert.equal(forced.shown, true, "?perf=1 shows it with the setting off");
  forced.setSetting(false);
  assert.equal(forced.shown, true);
  assert.equal(forced.toggle(), false, "Shift+P can still hide it");
});

test("only Shift+P toggles the readout", () => {
  const key = (init) => ({ shiftKey: false, metaKey: false, ctrlKey: false, altKey: false, repeat: false, key: "", code: "", ...init });
  assert.equal(isPerfToggleKey(key({ shiftKey: true, key: "P", code: "KeyP" })), true);
  assert.equal(isPerfToggleKey(key({ shiftKey: true, key: "Π", code: "KeyP" })), true, "by physical key on other layouts");
  assert.equal(isPerfToggleKey(key({ key: "p", code: "KeyP" })), false, "plain p");
  assert.equal(isPerfToggleKey(key({ shiftKey: true, metaKey: true, key: "P", code: "KeyP" })), false, "Shift+Cmd+P is the app's");
  assert.equal(isPerfToggleKey(key({ shiftKey: true, ctrlKey: true, key: "P", code: "KeyP" })), false);
  assert.equal(isPerfToggleKey(key({ shiftKey: true, altKey: true, key: "P", code: "KeyP" })), false);
  assert.equal(isPerfToggleKey(key({ shiftKey: true, repeat: true, key: "P", code: "KeyP" })), false, "holding it does not flicker");
  assert.equal(isPerfToggleKey(key({ shiftKey: true, key: "O", code: "KeyO" })), false);
});
