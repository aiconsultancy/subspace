import assert from "node:assert/strict";
import { test } from "node:test";
import { SCENE_KINDS } from "../src/conditions.js";
import { PREVIEW_NAMES, buildPreview, previewNameFromSearch } from "../src/preview.js";
import { buildView } from "../src/view-model.js";
import { NOW } from "./fixtures.js";

test("preview names come from the query string and unknown names are ignored", () => {
  assert.equal(previewNameFromSearch("?preview=rain"), "rain");
  assert.equal(previewNameFromSearch("?preview=Clear-Night"), "clear-night");
  assert.equal(previewNameFromSearch("?preview=hurricane"), null);
  assert.equal(previewNameFromSearch(""), null);
  assert.equal(previewNameFromSearch("?other=rain"), null);
});

test("previews cover every scene kind by day and by night", () => {
  const seen = new Set();
  for (const name of PREVIEW_NAMES) {
    const view = buildView(buildPreview(name, NOW), NOW);
    seen.add(`${view.scene.kind}/${view.scene.time}`);
    assert.equal(view.scene.time, name.endsWith("-night") ? "night" : "day", name);
  }
  for (const kind of SCENE_KINDS) {
    assert.ok(seen.has(`${kind}/day`) && seen.has(`${kind}/night`), `${kind} day and night`);
  }
});

test("every preview is unmistakably synthetic and has a full forecast", () => {
  for (const name of PREVIEW_NAMES) {
    const state = buildPreview(name, NOW);
    const view = buildView(state, NOW);
    assert.equal(state.synthetic, true);
    assert.equal(view.synthetic, true);
    assert.equal(view.banner.tone, "synthetic");
    assert.match(view.banner.text, /not a live forecast/);
    assert.equal(view.hero.title, "Sample location");
    assert.doesNotMatch(view.updated, /^Updated/);
    assert.equal(view.hours.length, 24);
    assert.equal(view.days.length, 7);
    assert.equal(view.details.length, 6);
    assert.equal(view.hours[0].time, "Now");
  }
});

test("a preview never claims to be a saved or stale forecast", () => {
  const later = NOW + 5 * 60 * 60_000;
  const view = buildView(buildPreview("rain", NOW), later);
  assert.equal(view.stale, false);
  assert.equal(view.banner.tone, "synthetic");
});
