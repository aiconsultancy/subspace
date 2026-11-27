// The inline charge: the turns held, as their compact JSON, measured once
// per turn's own fields and once per item, so an event on a long running
// turn measures only what changed (SUB-69).

import assert from "node:assert/strict";
import { test } from "node:test";
import { applyChange, createActivityWatcher, decodeActivity, INLINE_KEY, turnSizer } from "../src/activity.js";
import { jsonBytes } from "../src/document.js";
import { activityEvent, activitySnapshot, T0, textItem, toolItem, turn } from "./activity-fixtures.js";
import { fakeActivityClient, settle } from "./activity-helpers.js";

/** A snapshot with varied turns: one with no items, unreadable items and turns, an unknown kind, and text outside ASCII. */
function variedSnapshot() {
  const snapshot = activitySnapshot();
  snapshot.turns[0].items.push({ seq: 3, kind: "text" }, { id: "i4", seq: 3, kind: "patch", files: ["src/é.rs"] }, "not an item");
  snapshot.turns.unshift(turn("turn-0", 1, { items: [] }), { id: "turn-bad", seq: 2, status: "completed" });
  snapshot.turns.at(-1).items.push(textItem("i3", 9, "naïve 🚀 ".repeat(50)));
  return snapshot;
}

const inlineOf = (turns) => turns.reduce((total, each) => total + jsonBytes(each), 0);

test("a turn's measured length equals its compact JSON for varied turns", () => {
  const sizeOf = turnSizer();
  const { turns } = decodeActivity(variedSnapshot());
  assert.ok(turns.some((each) => each.items.length === 0), "a turn with no items");
  assert.ok(turns.some((each) => each.unreadable), "an unreadable turn");
  assert.ok(turns.some((each) => each.items.some((item) => item.kind === "unreadable")), "an unreadable item");
  assert.ok(turns.some((each) => each.items.some((item) => item.kind === "patch")), "an unknown kind");
  for (const each of turns) assert.equal(sizeOf(each), jsonBytes(each), each.id);
  const one = { ...turns.at(-1), items: turns.at(-1).items.slice(0, 1) };
  assert.equal(sizeOf(one), jsonBytes(one), "a turn with one item");
});

test("the watcher's inline charge stays the turns' compact JSON as events change them", async () => {
  const watcher = createActivityWatcher({ client: fakeActivityClient([variedSnapshot()]), agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const charged = () => watcher.view.budget.bytes(INLINE_KEY);
  assert.equal(charged(), inlineOf(watcher.view.turns));
  const events = [
    activityEvent(10, { type: "item", item: textItem("i5", 10, "first") }),
    activityEvent(11, { type: "item", item: textItem("i5", 11, "first and then a longer second line, ü") }),
    activityEvent(12, { type: "item", item: { id: "i6", seq: 12, kind: "unknown-later-kind", detail: { nested: [1, 2, 3] } } }),
    activityEvent(13, { type: "item", item: { id: "i7", kind: "text" } }),
    activityEvent(14, { type: "turn-updated", turn: { id: "turn-b", seq: 14, status: "running", startedAt: T0, updatedAt: T0 + 99, title: "A title" } }),
    activityEvent(15, { type: "turn-started", turn: { id: "turn-c", seq: 15, status: "running", startedAt: T0 + 500 } }, { turnId: "turn-c" }),
    activityEvent(16, { type: "item", item: toolItem("i8", 16) }, { turnId: "turn-c" }),
  ];
  for (const event of events) {
    watcher.event(event);
    assert.equal(charged(), inlineOf(watcher.view.turns), `after seq ${event.seq}`);
  }
  assert.equal(watcher.view.turns[0].id, "turn-c");
  watcher.stop();
});

test("an event measures only the changed turn's own fields and the changed item, never the whole running turn", () => {
  const measured = [];
  const sizeOf = turnSizer((value) => {
    measured.push(value);
    return jsonBytes(value);
  });
  const snapshot = activitySnapshot();
  // A long running turn: one large item beside the ones that change.
  snapshot.turns[1].items.push(textItem("big", 9, "x".repeat(200_000)));
  let { turns } = decodeActivity(snapshot);
  const charge = () => turns.reduce((total, each) => total + sizeOf(each), 0);
  charge();
  const bytesMeasured = () => measured.reduce((total, value) => total + jsonBytes(value), 0);

  // An item changes: its turn's fields and the new item are measured.
  measured.length = 0;
  const item = textItem("i1", 10, "a delta");
  turns = applyChange(turns, activityEvent(10, { type: "item", item }));
  assert.equal(charge(), inlineOf(turns));
  const running = turns[1];
  assert.equal(measured.length, 2);
  assert.deepEqual(measured[0], { ...running, items: [] }, "the running turn's own fields");
  assert.equal(measured[1], running.items.find((each) => each.id === "i1"), "the changed item");
  assert.equal(bytesMeasured(), jsonBytes({ ...running, items: [] }) + jsonBytes(item));
  assert.ok(bytesMeasured() < 1_000 && jsonBytes(running) > 200_000, "a few hundred bytes, not the 200 KB turn");

  // A new item: the same.
  measured.length = 0;
  turns = applyChange(turns, activityEvent(11, { type: "item", item: textItem("i9", 11, "new") }));
  assert.equal(charge(), inlineOf(turns));
  assert.equal(measured.length, 2);

  // The turn's own fields change: only they are measured; its items are kept.
  measured.length = 0;
  turns = applyChange(turns, activityEvent(12, { type: "turn-updated", turn: { id: "turn-b", seq: 12, status: "running", startedAt: T0, updatedAt: T0 + 90 } }));
  assert.equal(charge(), inlineOf(turns));
  assert.deepEqual(measured, [{ ...turns[1], items: [] }]);

  // Nothing changed: nothing is measured.
  measured.length = 0;
  assert.equal(charge(), inlineOf(turns));
  assert.deepEqual(measured, []);
});
