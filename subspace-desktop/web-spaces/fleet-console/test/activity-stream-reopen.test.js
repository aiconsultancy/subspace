// A snapshot read in flight when the stream reopens or ends is let go: it
// started before the new stream registered the watch, and a read that
// never settles would otherwise hold every later read behind it (SUB-69).

import assert from "node:assert/strict";
import { test } from "node:test";
import { createActivityWatcher } from "../src/activity.js";
import { activitySnapshot } from "./activity-fixtures.js";
import { fakeActivityClient, flush } from "./activity-helpers.js";

function watcherWith(snapshots) {
  const client = fakeActivityClient(snapshots);
  const watcher = createActivityWatcher({ client, agent: "example-captain" });
  watcher.start();
  return { client, watcher };
}

test("a reopened stream reads the snapshot again past a read that never settles, and the view leaves loading", async () => {
  const { client, watcher } = watcherWith(["hold", activitySnapshot()]);
  watcher.reopened();
  await flush();
  assert.equal(client.calls.length, 1);
  assert.equal(watcher.view.status, "loading");

  watcher.reopened();
  await flush();
  assert.equal(client.calls.length, 2, "read again, not queued behind the read that never settles");
  assert.equal(watcher.view.status, "ready");
  assert.equal(watcher.view.refreshing, false);
  assert.equal(watcher.view.turns.length, 2);
  watcher.stop();
});

test("a stream that ends lets go of the read in flight; the next answer reads again", async () => {
  const { client, watcher } = watcherWith(["hold", activitySnapshot()]);
  watcher.reopened();
  await flush();
  watcher.disconnected();
  await flush();
  assert.equal(client.calls.length, 1, "nothing is read before the next stream answers");
  assert.equal(watcher.view.waiting, true);

  watcher.reopened();
  await flush();
  assert.equal(client.calls.length, 2);
  assert.equal(watcher.view.status, "ready");
  assert.equal(watcher.view.refreshing, false);
  watcher.stop();
});

test("the read let go answers late and is dropped", async () => {
  const late = activitySnapshot({ seq: 3 });
  late.turns = late.turns.slice(0, 1);
  const { client, watcher } = watcherWith(["hold", activitySnapshot({ seq: 9 })]);
  watcher.reopened();
  await flush();
  watcher.reopened();
  await flush();
  assert.equal(watcher.view.cursor.seq, 9);

  client.pending[0](late);
  await flush();
  assert.equal(client.calls.length, 2);
  assert.equal(watcher.view.cursor.seq, 9, "the newer snapshot stays");
  assert.equal(watcher.view.turns.length, 2);
  watcher.stop();
});
