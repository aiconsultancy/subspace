// Regression tests for #115 review round 3's findings (SUB-69 comment
// 5d0227f2) and the decisions they follow (SUB ed71a299, 6f5e67dd,
// 0c13b784). Each fails without its fix. Every fake page reader stops
// itself after a few reads.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as activity from "../src/activity.js";
import * as view from "../src/activity-view.js";
import { activityEvent, activitySnapshot } from "./activity-fixtures.js";
import { fakeActivityClient, fakeDoc, flush, settle } from "./activity-helpers.js";

const { createActivityWatcher, decodeActivity } = activity;
const officer = { name: "Example", role: "captain", live: true };
const turnsOf = (panel) => panel.root.findAll((node) => node.tag === "details");

// ---------- P2: a turn whose items are not a list ----------

test("P2: a turn whose items is not a list stays an unreadable turn, its payload shown as sent", async () => {
  const snapshot = activitySnapshot({ seq: 9 });
  snapshot.turns[0].items = { note: "not a list" };
  const decoded = decodeActivity(snapshot);
  const kept = decoded.turns.find((each) => each.id === "turn-a");
  assert.equal(kept.status, "unreadable", "not a turn that observed nothing");
  assert.deepEqual(kept.items[0].raw, snapshot.turns[0], "its one item is the whole turn as sent");
  // Drawn through the bounded renderer, like any unreadable item.
  const client = fakeActivityClient([snapshot]);
  const watcher = createActivityWatcher({ client, agent: "example-captain", awaitStream: false });
  watcher.start();
  await settle();
  const panel = view.createActivityPanel({ doc: fakeDoc });
  panel.update(watcher.view, officer);
  const unreadable = turnsOf(panel).find((details) => details.textContent.includes("turn-a"));
  unreadable.setAttribute("open", "");
  unreadable.listeners.toggle?.();
  assert.ok(unreadable.textContent.includes("not a list"), "its content still shows");
  assert.ok(!unreadable.textContent.includes("Nothing observed in this turn yet."));
  // A turn event with items that are not a list asks for the snapshot again.
  const events = fakeActivityClient([activitySnapshot({ seq: 9 }), activitySnapshot({ seq: 11 })]);
  const live = createActivityWatcher({ client: events, agent: "example-captain", awaitStream: false });
  live.start();
  await settle();
  live.event(activityEvent(10, { type: "turn-updated", turn: { ...activitySnapshot().turns[1], seq: 10, items: "nope" } }));
  await flush();
  assert.equal(events.calls.length, 2);
  watcher.stop();
  live.stop();
});
