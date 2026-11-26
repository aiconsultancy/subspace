// Round 6 of #115 (SUB decision 533c2b29): Phobos's six round-5 findings,
// each a regression named for what it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { formatItem, itemDocument, itemTree, mount } from "../src/activity-view.js";
import { partBlocks, restOf } from "../src/document.js";
import { toolItem } from "./activity-fixtures.js";
import { fakeDoc } from "./activity-helpers.js";

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
