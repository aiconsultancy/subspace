import assert from "node:assert/strict";
import { test } from "node:test";
import { composeFrame } from "../src/scene/index.js";
import { createNavigator, TOP } from "../src/camera.js";
import { contrastRatio, deriveStage, fontStack, parseColor, readPalette, themeToCssVars, TONES } from "../src/theme.js";
import { readFileSync } from "node:fs";
import { loadedState, NOW } from "./fixtures.js";

async function context(overrides = {}) {
  const state = await loadedState();
  return { W: 1000, H: 640, now: NOW, clock: NOW, anim: 1000, reduced: false, dark: true, fleet: state.fleet, ships: state.ships, packets: [], unavailableSince: state.unavailableSince, hover: null, issueBadge: () => null, ...overrides };
}

function landed(selection) {
  const nav = createNavigator({ start: 0 });
  nav.tick(5000);
  if (selection.level > 0) {
    nav.go(selection, 5000);
    for (let t = 5000; nav.state.warp; t += 500) nav.tick(t);
  }
  return nav;
}

test("the fleet tier draws every ship, the admiral and a sweep, with hit regions", async () => {
  const frame = composeFrame(landed(TOP).state, await context(), 9000, 0.016);
  assert.equal(frame.layers.length, 1);
  assert.equal(frame.interactive, true);
  const items = frame.layers[0].items;
  assert.ok(items.some((item) => item.k === "sweep"));
  assert.ok(items.some((item) => item.k === "hex"), "the admiral");
  assert.equal(items.filter((item) => item.k === "arrow").length, 5, "one glyph per ship");
  assert.deepEqual(frame.hits.filter((hit) => hit.kind === "ship").map((hit) => hit.ship), ["galactica", "pegasus", "defiant", "endeavour", "resolute"]);
  assert.ok(items.some((item) => item.k === "text" && item.s === "peer captains"));
});

test("reduced motion draws no sweep and no orbit trails", async () => {
  const frame = composeFrame(landed(TOP).state, await context({ reduced: true }), 9000, 0.016);
  const items = frame.layers[0].items;
  assert.equal(items.some((item) => item.k === "sweep"), false);
  const tinyDots = items.filter((item) => item.k === "disk" && item.r < 2.5 && item.fill);
  assert.equal(tinyDots.length, 0);
});

test("the ship tier has one hit region per crew member plus the captain, and issue badges", async () => {
  const ctx = await context({ issueBadge: (ship, member) => (member === "starbuck" ? { identifier: "FLT-212", tone: "yellow" } : null) });
  const frame = composeFrame(landed({ level: 1, ship: "galactica" }).state, ctx, 99_000, 0.016);
  assert.deepEqual(frame.hits.filter((hit) => hit.kind === "crew").map((hit) => hit.crew), ["starbuck", "gaeta", "bosun", "apollo", "helo"]);
  assert.ok(frame.hits.some((hit) => hit.kind === "captain"));
  assert.ok(frame.layers[0].items.some((item) => item.k === "pill" && item.s === "FLT-212" && item.dot === "yellow"));
});

test("an unavailable ship's tier says so and guesses nothing", async () => {
  const frame = composeFrame(landed({ level: 1, ship: "resolute" }).state, await context(), 99_000, 0.016);
  assert.ok(frame.layers[0].items.some((item) => item.k === "text" && item.s === "Crew status unavailable"));
  assert.equal(frame.hits.filter((hit) => hit.kind === "crew").length, 0);
});

test("the crew tier draws the 24-hour ring, one card per context and orbiting subagents", async () => {
  const frame = composeFrame(landed({ level: 2, ship: "galactica", crew: "starbuck" }).state, await context(), 99_000, 0.016);
  assert.equal(frame.hits.filter((hit) => hit.kind === "ctx").length, 2);
  assert.equal(frame.hits.filter((hit) => hit.kind === "sub").length, 2);
  assert.ok(frame.hits.filter((hit) => hit.kind === "recent").length >= 2);
});

test("mid-warp there are two layers, a gate and star velocity; text of the zoomed parent fades first", async () => {
  const nav = landed(TOP);
  nav.go({ level: 1, ship: "pegasus" }, 10_000);
  const ctx = await context();
  composeFrame(nav.state, ctx, 10_400, 0.016);
  const frame = composeFrame(nav.state, ctx, 10_650, 0.016);
  assert.equal(frame.layers.length, 2);
  assert.ok(frame.gate && frame.gate.amp > 0.5);
  assert.ok(frame.velocity > 0, "stars fly inward while diving");
  assert.ok(frame.layers[0].textAlpha < frame.layers[0].alpha);
  assert.equal(frame.interactive, false);
  nav.tick(20_000);
  nav.go(TOP, 20_000);
  composeFrame(nav.state, ctx, 20_300, 0.016);
  assert.ok(composeFrame(nav.state, ctx, 20_500, 0.016).velocity < 0, "and outward when pulling out");
});

test("before any data the stage says it is connecting", async () => {
  const frame = composeFrame(landed(TOP).state, await context({ fleet: null, ships: {}, emptyMessage: "Cannot reach https://fleet.example.com" }), 9000, 0.016);
  assert.ok(frame.layers[0].items.some((item) => item.k === "text" && item.s.startsWith("Cannot reach")));
});

const DARK = { bg: "#101115", pane: "#15171b", line: "#313640", hair: "#1d2026", select: "#2c313a", fg: "#abb2bf", strong: "#d1d5dc", muted: "#8b919c", accent: "#7dd3fc", red: "#e06c75", orange: "#d19a66", yellow: "#e5c07b", green: "#98c379", cyan: "#56b6c2", blue: "#61afef", magenta: "#c678dd" };

test("bridge themes map to CSS properties, a stage tone and font stacks", () => {
  const mapped = themeToCssVars({ id: "cupertino", mode: "light", tones: { ...DARK, bg: "#f7f7f5" }, fonts: { ui: "SF Pro Text", mono: "SF Mono" } });
  assert.equal(mapped.mode, "light");
  assert.equal(mapped.vars["--bg"], "#f7f7f5");
  assert.equal(mapped.vars["--stage"], deriveStage("#f7f7f5", "light"));
  assert.match(mapped.vars["--ui"], /^"SF Pro Text", /);
  assert.equal(Object.keys(mapped.vars).filter((key) => TONES.includes(key.slice(2))).length, TONES.length);
  assert.equal(themeToCssVars({ tones: { ...DARK, red: "red" } }), null, "malformed tones are rejected");
  assert.equal(themeToCssVars(null), null);
  assert.equal(fontStack("Inter;}", "x"), "x");
  assert.equal(deriveStage("#101115", "dark"), "#0b0c0f");
});

test("the palette reader decides dark or light from the background", () => {
  const light = readPalette((name) => ({ "--bg": "#ffffff", "--ui": "Inter" })[name] ?? "#808080");
  assert.equal(light.dark, false);
  assert.equal(light.fonts.ui, "Inter");
  const dark = readPalette((name) => ({ "--bg": "rgb(16, 17, 21)" })[name] ?? "");
  assert.equal(dark.dark, true);
  assert.deepEqual(parseColor("rgb(16, 17, 21)"), [16, 17, 21]);
});

test("the default light and dark tones keep text readable on the stage", () => {
  const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
  const block = (selector) => css.slice(css.indexOf(selector)).split("}")[0];
  for (const selector of [":root {", ":root:not([data-mode]) {"]) {
    const text = block(selector);
    const tone = (name) => parseColor(text.match(new RegExp(`--${name}: (#[0-9a-f]{6})`, "i"))[1]);
    for (const name of ["fg", "strong", "muted", "accent", "red", "green", "yellow", "orange"]) {
      assert.ok(contrastRatio(tone(name), tone("stage")) >= 3, `${selector} ${name}`);
    }
    assert.ok(contrastRatio(tone("strong"), tone("bg")) >= 7, `${selector} strong text`);
  }
});
