import assert from "node:assert/strict";
import { test } from "node:test";
import { alertSnapshot, createAlertWatcher, createNotifier, createWatcherSet, diffAlerts, mergeAlerts, MAX_VANISHED, NOTIFY_SPACING_MS } from "../src/alerts.js";
import { badgeText } from "../src/model.js";
import { loadedState } from "./fixtures.js";

function withMember(state, shipId, memberId, change) {
  const ships = structuredClone(state.ships);
  change(ships[shipId].crewMembers.find((member) => member.id === memberId));
  return ships;
}

test("the badge counts uncertain crew and unavailable ships, null when none", async () => {
  const state = await loadedState();
  assert.equal(badgeText(state.fleet, state.ships), "2");
  const ships = withMember(state, "galactica", "helo", (member) => (member.activity = "idle"));
  const fleet = structuredClone(state.fleet);
  fleet.ships.find((ship) => ship.id === "resolute").available = true;
  ships.resolute.available = true;
  assert.equal(badgeText(fleet, ships), null);
  assert.equal(badgeText(null, {}), null);
});

test("alerts fire when a crew task turns uncertain or a ship becomes unavailable", async () => {
  const state = await loadedState();
  const before = alertSnapshot(state.fleet, state.ships);
  const ships = withMember(state, "pegasus", "fisk", (member) => (member.activity = "uncertain"));
  const fleet = structuredClone(state.fleet);
  fleet.ships.find((ship) => ship.id === "defiant").available = false;
  fleet.ships.find((ship) => ship.id === "defiant").error = "Backend timed out.";
  const alerts = diffAlerts(before, alertSnapshot(fleet, ships));
  assert.deepEqual(alerts.map((alert) => alert.title), ["Defiant is unavailable", "Fisk has no confirmed outcome"]);
  assert.match(alerts[0].body, /Backend timed out/);
  assert.match(alerts[1].body, /Pegasus: “Record idle-since in the crew journal”/);
  assert.deepEqual(diffAlerts(before, before), []);
});

test("the first snapshot is a silent baseline", async () => {
  const state = await loadedState();
  const pushed = [];
  const watcher = createAlertWatcher({ push: (alerts) => pushed.push(...alerts) });
  assert.deepEqual(watcher.observe(state.fleet, state.ships), []);
  const ships = withMember(state, "galactica", "gaeta", (member) => (member.activity = "uncertain"));
  assert.equal(watcher.observe(state.fleet, ships).length, 1);
  assert.equal(watcher.observe(state.fleet, ships).length, 0, "no repeat while it stays uncertain");
  assert.equal(pushed.length, 1);
});

test("notifications respect the five-second limit and merge what queues up", async () => {
  const sent = [];
  const timers = [];
  let clock = 0;
  const notifier = createNotifier({ send: async (message) => sent.push(message), now: () => clock, setTimer: (fn, ms) => timers.push({ fn, at: clock + ms }) });
  const alert = (n) => ({ key: `k${n}`, title: `Alert ${n}`, body: `Body ${n}` });
  notifier.push([alert(1)]);
  assert.equal(timers[0].at, 0);
  timers.shift().fn();
  await Promise.resolve();
  notifier.push([alert(2)]);
  notifier.push([alert(3)]);
  assert.equal(timers.length, 1, "one pending send");
  assert.equal(timers[0].at, NOTIFY_SPACING_MS);
  clock = NOTIFY_SPACING_MS;
  timers.shift().fn();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(sent.map((message) => message.title), ["Alert 1", "2 fleet alerts"]);
  assert.equal(sent[1].body, "Alert 2 · Alert 3");
});

test("merged notifications stay within the bridge limits", () => {
  const long = mergeAlerts([{ title: "x".repeat(300), body: "y".repeat(900) }]);
  assert.ok(long.title.length <= 100 && long.body.length <= 500);
});

test("a watcher forgets the oldest vanished keys beyond a limit", () => {
  const pushed = [];
  const watcher = createAlertWatcher({ push: (alerts) => pushed.push(...alerts) });
  const fleetOf = (ids) => ({ ships: ids.map((id) => ({ id, name: id, available: true, crew: { busy: 0, people: 0 } })) });
  watcher.observe(fleetOf(["s0"]), {});
  for (let i = 1; i <= MAX_VANISHED + 50; i += 1) watcher.observe(fleetOf([`s${i}`]), {});
  assert.ok(watcher.size <= MAX_VANISHED + 1, `baseline stays bounded: ${watcher.size}`);
});

test("watchers are kept per fleet and address, and pruned with the fleets", () => {
  const made = [];
  const watchers = createWatcherSet((fleet) => {
    const watcher = { fleet: fleet.id, endpoint: fleet.entry.endpoint };
    made.push(watcher);
    return watcher;
  });
  const lab = { id: "lab", entry: { endpoint: "https://lab.example.com" } };
  assert.equal(watchers.for(lab), watchers.for(lab));
  const moved = { id: "lab", entry: { endpoint: "https://lab2.example.com" } };
  assert.notEqual(watchers.for(moved), made[0], "a new address starts a new baseline");
  watchers.retain([moved]);
  assert.equal(watchers.size, 1);
  watchers.retain([]);
  assert.equal(watchers.size, 0);
});
