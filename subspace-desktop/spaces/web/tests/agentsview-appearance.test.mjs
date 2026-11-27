import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const read = file => readFileSync(new URL(`../src/agentsview/${file}`, import.meta.url), "utf8");
const script = read("appearance.js");
const css = read("twilight-two.css");
const origin = "https://fleet.example.com";

function preview(page = origin) {
  let mode;
  let style;
  let classChanged;
  const document = {
    head: { append(next) { style = next; } },
    documentElement: {},
    createElement: () => ({}),
    getElementById: () => style,
  };
  const window = { location: { origin: page }, localStorage: { getItem: () => mode } };
  class MutationObserver {
    constructor(callback) { classChanged = callback; }
    observe() {}
  }
  return {
    apply(value) {
      runInNewContext(script.replace("/*__SUBSPACE_AGENTSVIEW_APPEARANCE__*/",
        JSON.stringify({ origin, css: value })), { window, document, MutationObserver });
      return style;
    },
    pick(next) { mode = next; classChanged?.(); },
  };
}

test("the stylesheet is confined to the configured origin and follows the selected Subspace theme", () => {
  assert.equal(preview("https://elsewhere.example.com").apply(css), undefined);
  const view = preview();
  assert.equal(view.apply(null), undefined);
  const style = view.apply(css);
  assert.equal(style.textContent, css);
  assert.equal(view.apply(null).textContent, "");
  assert.equal(view.apply(css), style);
});

test("AgentsView can still select Light and its inline appearance settings win", () => {
  const view = preview();
  view.pick("light");
  const style = view.apply(css);
  assert.equal(style.disabled, true);
  view.pick("dark");
  assert.equal(style.disabled, false);
  assert.match(css, /:root:not\(\.high-contrast\)/);
  assert.doesNotMatch(css, /!important/);
});
