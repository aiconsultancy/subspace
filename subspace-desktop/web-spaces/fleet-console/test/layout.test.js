import assert from "node:assert/strict";
import { test } from "node:test";
import { crewLayout, fleetLayout, focusFor, shipLayout } from "../src/layout.js";
import { bracketOffset, chain, createNavigator, introFrame, parentOf, planSteps, TOP, warpFrame } from "../src/camera.js";

const SIZES = [[1280, 720], [920, 640], [390, 520], [600, 900]];
const overlap = (a, b) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

test("ships sit on the ellipse around the admiral, inside the stage, apart from each other", () => {
  for (const [W, H] of SIZES) {
    for (const n of [1, 3, 5, 8]) {
      const L = fleetLayout(W, H, Array.from({ length: n }, (_, i) => `s${i}`));
      assert.equal(L.ships.length, n);
      for (const ship of L.ships) {
        const e = ((ship.x - L.C.x) / L.rx) ** 2 + ((ship.y - L.C.y) / L.ry) ** 2;
        assert.ok(Math.abs(e - 1) < 1e-9, "on the ellipse");
        assert.ok(ship.x - ship.orbit >= 0 && ship.x + ship.orbit <= W && ship.y - ship.orbit >= 0 && ship.y + ship.orbit <= H, `${W}x${H} n=${n} inside`);
      }
      assert.equal(L.ships[0].angle, -Math.PI / 2, "the first ship is at the top");
    }
  }
});

test("portrait stages spread the fleet vertically", () => {
  const wide = fleetLayout(900, 600, ["a", "b"]);
  const tall = fleetLayout(390, 520, ["a", "b"]);
  assert.ok(tall.ry / tall.rx > wide.ry / wide.rx);
});

test("crew dials keep clear of the captain and of each other", () => {
  for (const [W, H] of SIZES) {
    for (const n of [1, 2, 4, 6]) {
      const L = shipLayout(W, H, Array.from({ length: n }, (_, i) => `c${i}`));
      for (const node of L.crew) assert.ok(Math.hypot(node.x - L.C.x, node.y - L.C.y) > L.captain.r + node.r, "clear of the captain");
      for (let i = 0; i < n; i += 1) for (let j = i + 1; j < n; j += 1) {
        assert.ok(Math.hypot(L.crew[i].x - L.crew[j].x, L.crew[i].y - L.crew[j].y) > L.crew[i].r * 2, `${W}x${H} n=${n}`);
      }
    }
  }
});

test("context cards never overlap each other or the dial, wide or narrow", () => {
  for (const [W, H] of [[1280, 720], [920, 640], [390, 640]]) {
    for (const n of [1, 2, 3, 4, 5]) {
      const L = crewLayout(W, H, n);
      assert.equal(L.cards.length, n);
      assert.equal(L.narrow, W < 600);
      const dial = { x: L.C.x - L.R, y: L.C.y - L.R, w: L.R * 2, h: L.R * 2 };
      for (let i = 0; i < n; i += 1) {
        assert.equal(overlap(L.cards[i], dial), false, `${W} n=${n} card ${i} clear of dial`);
        for (let j = i + 1; j < n; j += 1) assert.equal(overlap(L.cards[i], L.cards[j]), false, `${W} n=${n} cards ${i},${j}`);
      }
    }
  }
});

test("the warp focus is the child's node in the parent layout", () => {
  const ids = ["a", "b", "c"];
  const node = fleetLayout(1000, 700, ids).ships[1];
  assert.deepEqual(focusFor(TOP, { level: 1, ship: "b" }, 1000, 700, ids), { x: node.x, y: node.y, r: node.orbit });
  const crew = shipLayout(1000, 700, ids).crew[2];
  assert.deepEqual(focusFor({ level: 1, ship: "s" }, { level: 2, ship: "s", crew: "c" }, 1000, 700, ids), { x: crew.x, y: crew.y, r: crew.r });
  assert.deepEqual(focusFor(TOP, { level: 1, ship: "missing" }, 1000, 700, ids), { x: 500, y: 350, r: 36 });
});

test("navigation passes through every tier in between", () => {
  const crew = { level: 2, ship: "g", crew: "s" };
  assert.deepEqual(chain(crew).map((sel) => sel.level), [0, 1, 2]);
  assert.deepEqual(planSteps(TOP, crew).map((sel) => sel.level), [1, 2]);
  assert.deepEqual(planSteps(crew, TOP).map((sel) => sel.level), [1, 0]);
  const other = { level: 2, ship: "p", crew: "k" };
  assert.deepEqual(planSteps(crew, other).map((sel) => [sel.level, sel.ship]), [[1, "g"], [0, null], [1, "p"], [2, "p"]]);
  assert.deepEqual(planSteps(crew, crew), []);
  assert.deepEqual(parentOf(crew), { level: 1, fleet: null, ship: "g", crew: null });
});

test("the navigator queues chained warps and lands on the target", () => {
  const nav = createNavigator({ start: 0 });
  nav.tick(2000);
  assert.equal(nav.go({ level: 2, ship: "g", crew: "s" }, 2000), true);
  assert.equal(nav.state.warp.dur, 780, "chained steps are quicker");
  assert.equal(nav.state.sel.level, 1);
  nav.go(TOP, 2100);
  assert.deepEqual(nav.target(), TOP, "a request during a warp waits");
  let t = 2000;
  while (nav.state.warp && t < 10_000) {
    t += 100;
    nav.tick(t);
  }
  assert.deepEqual(nav.state.sel, TOP);
  const reduced = createNavigator({ reduced: () => true, start: 0 });
  assert.equal(reduced.state.intro, null, "no intro with reduced motion");
  reduced.go({ level: 1, ship: "g" }, 10);
  assert.equal(reduced.state.warp.dur, 260, "a short crossfade");
});

test("warp camera: the parent is untouched at the start and the child fills the stage at the end", () => {
  const W = 1000;
  const H = 700;
  const F = { x: 800, y: 200, r: 30 };
  const warp = { dir: 1, t0: 0, dur: 1000 };
  const start = warpFrame(warp, 0, F, W, H, false);
  assert.deepEqual([start.parent.s, start.parent.tx, start.parent.ty], [1, 0, 0]);
  assert.equal(start.child.alpha, 0);
  const end = warpFrame(warp, 1000, F, W, H, false);
  assert.ok(Math.abs(end.child.s - 1) < 1e-9 && Math.abs(end.child.tx) < 1e-9 && Math.abs(end.child.ty) < 1e-9);
  assert.equal(end.parent.alpha, 0);
  assert.ok(Math.abs(end.gate.r - Math.min(W, H) * 0.46) < 1e-6, "the gate grows to frame the child");
  const mid = warpFrame(warp, 500, F, W, H, false);
  assert.ok(mid.parent.s > 1 && mid.gate.amp > 0.99 && mid.tunnel > 0.99);
  const out = warpFrame({ dir: -1, t0: 0, dur: 1000 }, 0, F, W, H, false);
  assert.ok(Math.abs(out.child.s - 1) < 1e-9, "pulling out starts from the child");
  const fade = warpFrame(warp, 500, F, W, H, true);
  assert.deepEqual([fade.parent.s, fade.child.s, fade.gate], [1, 1, null]);
  assert.equal(fade.parent.alpha + fade.child.alpha, 1);
});

test("intro and brackets settle", () => {
  assert.ok(introFrame(0, 0, 800, 600).layer.s < 0.6);
  assert.equal(introFrame(0, 5000, 800, 600).layer.s, 1);
  assert.equal(bracketOffset(0, 0, false), 24);
  assert.equal(bracketOffset(0, 1000, false), 0);
  assert.equal(bracketOffset(0, 0, true), 0);
});

test("each page band keeps its grid row when the banner is hidden", async () => {
  const { readFileSync } = await import("node:fs");
  const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const bands = [...html.matchAll(/<(?:header|div|footer) class="(toolbar|banner|body|ticker)"/g)].map((match) => match[1]);
  assert.deepEqual(bands, ["toolbar", "banner", "body", "ticker"]);
  bands.forEach((band, index) => {
    assert.match(css, new RegExp(`\\.app > \\.${band} \\{ grid-row: ${index + 1}; \\}`));
  });
});
