// Round 6 of #115 (SUB decision 533c2b29): Phobos's six round-5 findings,
// each a regression named for what it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { formatItem, itemDocument, itemTree, json, mount } from "../src/activity-view.js";
import { partBlocks, restOf, shownJsonPieces } from "../src/document.js";
import { stepItem, tokens, toolItem } from "./activity-fixtures.js";
import { fakeDoc } from "./activity-helpers.js";

// ---------- 5. The short step view leaves nothing out ----------

test("5: a step's model and tokens members the short view does not put in words still show, as the document has them", () => {
  const step = stepItem("s", 3, { model: { provider: "example", id: "model-large", variant: null, region: "MODEL-SIDE" }, tokens: { ...tokens(1200, 80), detail: { cached: "TOKENS-SIDE" } } });
  const shown = mount(itemTree("turn-a", step), fakeDoc).textContent;
  assert.match(shown, /example\/model-large/, "the model in words");
  assert.match(shown, /model \(other fields\)regionMODEL-SIDE/, "the model's other member");
  assert.match(shown, /tokens \(other fields\)detail\{\n {2}"cached": "TOKENS-SIDE"\n\}/, "the tokens' other member, as the document's JSON");
  const document = itemDocument(step);
  for (const text of ["MODEL-SIDE", "TOKENS-SIDE"]) assert.ok(document.includes(text), `the document, which is measured, holds ${text}`);
  const plain = mount(itemTree("turn-a", stepItem("p", 4)), fakeDoc).textContent;
  assert.ok(!plain.includes("other fields"), "nothing extra when every member is in words");
});

test("5: a step's model or tokens that is not a record shows whole", () => {
  const shown = mount(itemTree("turn-a", stepItem("s", 3, { model: "MODEL-AS-TEXT", tokens: [7, 8] })), fakeDoc).textContent;
  assert.match(shown, /Model not reported/);
  assert.match(shown, /modelMODEL-AS-TEXT/);
  assert.match(shown, /tokens\[\n {2}7,\n {2}8\n\]/);
});

test("5: the short view's JSON is the document's formatter", () => {
  const value = { outer: { "k\u202Eey": ["a\u0000b", "\ud800"], typed: "\\u0000", gone: undefined, nested: [undefined, 1.5] } };
  assert.equal(json(value), [...shownJsonPieces(value)].join(""));
});

// ---------- 6. Every own field survives the projection ----------

/** A tool call whose text part, parsed from JSON, has an own `__proto__` field. */
const withProto = (repeat) => JSON.parse(JSON.stringify(toolItem("ia", 9)).replace('"content":[{"type":"text","text":"README.md\\nsrc\\n"}]', `"content":[{"type":"text","text":"${"a".repeat(repeat)}","__proto__":"PROTO-SIDE"}],"__proto__":{"item":"ITEM-SIDE"}`));

test("6: restOf and partBlocks keep an own __proto__ field as a field", () => {
  const part = withProto(4).content[0];
  assert.ok(Object.hasOwn(part, "__proto__"), "the fixture has the own field");
  const rest = restOf(part, ["type", "text"]);
  assert.ok(rest && Object.hasOwn(rest, "__proto__"));
  assert.equal(rest.__proto__, "PROTO-SIDE");
  const [other] = partBlocks(part, 0);
  assert.equal(JSON.stringify(other.json), '{"__proto__":"PROTO-SIDE"}');
});

test("6: an own __proto__ field shows in the short view and in the windows", () => {
  const short = mount(itemTree("turn-b", withProto(100)), fakeDoc).textContent;
  assert.match(short, /"__proto__": "PROTO-SIDE"/, "the part's field in the short view");
  assert.match(short, /ITEM-SIDE/, "the item's field in the short view");
  const long = withProto(70_000);
  const { windows } = formatItem(long);
  assert.ok(windows.length > 1, "the long one shows in windows");
  const joined = windows.join("");
  assert.equal(joined, itemDocument(long));
  assert.match(joined, /content\[0\] \(other fields\)\n\{\n {2}"__proto__": "PROTO-SIDE"\n\}/, "the part's field in the windows");
  assert.match(joined, /__proto__\n\{\n {2}"item": "ITEM-SIDE"\n\}/, "the item's field in the windows");
});
