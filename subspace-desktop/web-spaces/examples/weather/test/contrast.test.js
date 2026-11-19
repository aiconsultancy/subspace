import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { SCENE_KINDS } from "../src/conditions.js";

// Reads the palette from styles.css and checks the worst-case text contrast in
// every scene. Worst case means the lightest thing that can sit behind text:
// the lightest sky stop or a full-strength cloud.

const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
const MINIMUM = 4.5;

function hex(color) {
  const value = color.replace("#", "");
  const full = value.length === 3 ? [...value].map((c) => c + c).join("") : value;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16));
}

const channel = (value) => {
  const v = value / 255;
  return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};
const luminance = ([r, g, b]) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
const contrast = (a, b) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const over = (top, alpha, bottom) => top.map((v, i) => v * alpha + bottom[i] * (1 - alpha));

function rootNumber(name) {
  const match = css.match(new RegExp(`${name}:\\s*([0-9.]+)\\s*;`));
  assert.ok(match, `${name} is defined in :root`);
  return Number(match[1]);
}
function rootRgb(name) {
  const match = css.match(new RegExp(`${name}:\\s*(\\d+)\\s+(\\d+)\\s+(\\d+)\\s*;`));
  assert.ok(match, `${name} is defined in :root`);
  return match.slice(1, 4).map(Number);
}

const panelRgb = rootRgb("--panel-rgb");
const panelAlpha = rootNumber("--panel-alpha");
const heroClear = rootNumber("--hero-clear");
const softAlpha = rootNumber("--text-soft-alpha");
const tileAlpha = rootNumber("--tile-alpha");
const WHITE = [255, 255, 255];
const CHANCE = hex(css.match(/--chance:\s*(#[0-9a-fA-F]{6})/)[1]);

const SUN = hex("#fff1c4");

function palettes() {
  const found = [];
  const pattern = /body\[data-kind="(\w+)"\](?:\[data-time="(\w+)"\])?\s*\{([^}]*)\}/g;
  for (const [, kind, time, body] of css.matchAll(pattern)) {
    const color = (name) => body.match(new RegExp(`${name}:\\s*(#[0-9a-fA-F]{3,6})`))?.[1];
    const number = (name) => Number(body.match(new RegExp(`${name}:\\s*([0-9.]+)`))?.[1] ?? 0);
    if (color("--sky-top")) {
      found.push({
        name: `${kind}/${time}`,
        kind,
        time,
        top: hex(color("--sky-top")),
        mid: hex(color("--sky-mid")),
        low: hex(color("--sky-low")),
        cloud: hex(color("--cloud-color")),
        cloudOpacity: Math.max(number("--cloud-1"), number("--cloud-2"), number("--cloud-3")),
        sun: number("--sun"),
        fog: number("--fog"),
      });
    }
  }
  return found;
}

const FOG = hex("#e8eef4");

/**
 * Everything that can sit behind text at a sky stop: the sky, the strongest
 * cloud, fog and the sun glow. `clear` scales clouds and fog (1 = full strength).
 * The sun sits top-right, beside the hero, so the hero check leaves it out.
 */
function backdrops(palette, stops, { sun = true, clear = 1 } = {}) {
  return stops.flatMap((sky) => [
    sky,
    over(palette.cloud, palette.cloudOpacity * clear, sky),
    ...(palette.fog > 0 ? [over(FOG, 0.5 * palette.fog * clear, sky)] : []),
    ...(sun && palette.sun > 0 ? [over(SUN, 0.8 * palette.sun, sky)] : []),
  ]);
}

const lightestOf = (colors) => colors.sort((a, b) => luminance(b) - luminance(a))[0];

test("every weather kind has a day and a night palette", () => {
  const found = palettes();
  for (const kind of SCENE_KINDS) {
    for (const time of ["day", "night"]) {
      assert.ok(found.some((p) => p.kind === kind && p.time === time), `${kind}/${time} is styled`);
    }
  }
});

test("white hero text is readable where clouds and fog fade out behind it", () => {
  for (const p of palettes()) {
    for (const [label, sky] of [["top", p.top], ["mid", p.mid]]) {
      assert.ok(contrast(WHITE, sky) >= MINIMUM, `${p.name}: white on sky ${label} is ${contrast(WHITE, sky).toFixed(2)}`);
    }
    const behind = lightestOf(backdrops(p, [p.top, p.mid], { sun: false, clear: heroClear }));
    assert.ok(contrast(WHITE, behind) >= MINIMUM, `${p.name}: hero text on clouds and fog is ${contrast(WHITE, behind).toFixed(2)}`);
  }
});

test("panel text, soft text and rain chance are readable on every scene", () => {
  for (const p of palettes()) {
    const panel = over(panelRgb, panelAlpha, lightestOf(backdrops(p, [p.top, p.mid, p.low])));
    const tile = over(WHITE, tileAlpha, panel);
    for (const [label, surface] of [["panel", panel], ["tile", tile]]) {
      const soft = over(WHITE, softAlpha, surface);
      assert.ok(contrast(WHITE, surface) >= MINIMUM, `${p.name}: white on ${label} is ${contrast(WHITE, surface).toFixed(2)}`);
      assert.ok(contrast(soft, surface) >= MINIMUM, `${p.name}: soft text on ${label} is ${contrast(soft, surface).toFixed(2)}`);
      assert.ok(contrast(CHANCE, surface) >= MINIMUM, `${p.name}: rain chance on ${label} is ${contrast(CHANCE, surface).toFixed(2)}`);
    }
  }
});

test("the synthetic banner has high contrast", () => {
  assert.ok(contrast(hex("#1d1500"), hex("#ffd45e")) >= 7);
});
