import assert from "node:assert/strict";
import { test } from "node:test";
import { initialFleetState, livePackets, MAX_ACTIVITY, PACKET_LIFETIME_MS, reduceFleet, withCrewDetail } from "../src/reducer.js";
import { loadedState, NOW, sampleFleet } from "./fixtures.js";

test("fleet then ship events build the state; versions count changes", async () => {
  const state = await loadedState();
  assert.equal(state.fleet.name, "colonial-fleet");
  assert.deepEqual(Object.keys(state.ships).sort(), ["defiant", "endeavour", "galactica", "pegasus", "resolute"]);
  assert.equal(state.version, 6);
  assert.equal(state.lastEventAt, NOW);
});

test("invalid payloads and unknown types leave the state untouched", async () => {
  const state = await loadedState();
  assert.equal(reduceFleet(state, { type: "fleet", data: { name: "broken" } }, NOW), state);
  assert.equal(reduceFleet(state, { type: "ship", data: { id: "x" } }, NOW), state);
  assert.equal(reduceFleet(state, { type: "mystery", data: {} }, NOW), state);
  assert.equal(reduceFleet(state, null, NOW), state);
});

test("a ship event replaces its detail and the fleet's summary of it", async () => {
  const state = await loadedState();
  const detail = structuredClone(state.ships.galactica);
  detail.name = "Galactica II";
  detail.crewMembers = detail.crewMembers.slice(0, 2);
  const next = reduceFleet(state, { type: "ship", data: detail }, NOW + 10);
  assert.equal(next.ships.galactica.crewMembers.length, 2);
  assert.equal(next.fleet.ships.find((ship) => ship.id === "galactica").name, "Galactica II");
  assert.equal("crewMembers" in next.fleet.ships.find((ship) => ship.id === "galactica"), false);
  assert.equal(state.ships.galactica.crewMembers.length, 5, "the previous state is not mutated");
});

test("a fleet event drops ships it no longer lists and keeps crew detail for the rest", async () => {
  const state = await loadedState();
  const fleet = structuredClone(state.fleet);
  fleet.ships = fleet.ships.filter((ship) => ship.id !== "defiant");
  fleet.generatedAt = NOW + 1000;
  const next = reduceFleet(state, { type: "fleet", data: fleet }, NOW + 1000);
  assert.equal(next.ships.defiant, undefined);
  assert.equal(next.ships.galactica.crewMembers.length, 5);
});

test("unavailable ships are timed from when the page first saw them unavailable", async () => {
  const state = await loadedState();
  assert.equal(state.unavailableSince.resolute, NOW);
  const fleet = structuredClone(state.fleet);
  fleet.ships.find((ship) => ship.id === "pegasus").available = false;
  const later = reduceFleet(state, { type: "fleet", data: fleet }, NOW + 5000);
  assert.equal(later.unavailableSince.resolute, NOW);
  assert.equal(later.unavailableSince.pegasus, NOW + 5000);
  fleet.ships.find((ship) => ship.id === "pegasus").available = true;
  assert.equal(reduceFleet(later, { type: "fleet", data: fleet }, NOW + 9000).unavailableSince.pegasus, undefined);
});

test("host events update the fleet's host", async () => {
  const state = await loadedState();
  const host = { ...state.fleet.host, cpu: { ...state.fleet.host.cpu, usage: 0.91 } };
  assert.equal(reduceFleet(state, { type: "host", data: host }, NOW).fleet.host.cpu.usage, 0.91);
  assert.equal(reduceFleet(initialFleetState(), { type: "host", data: host }, NOW).fleet, null);
});

test("activity feeds the ticker newest first and launches dispatch and report packets", async () => {
  let state = await loadedState();
  const event = (kind, at, extra = {}) => ({ type: "activity", data: { ship: "galactica", member: "starbuck", kind, text: `${kind} at ${at}`, at, ...extra } });
  state = reduceFleet(state, event("dispatch", 1), NOW);
  state = reduceFleet(state, event("phase", 2), NOW + 100);
  state = reduceFleet(state, event("outcome", 3, { status: "completed" }), NOW + 200);
  assert.deepEqual(state.activity.map((entry) => entry.kind), ["outcome", "phase", "dispatch"]);
  assert.deepEqual(state.packets.map((packet) => packet.kind), ["dispatch", "report"]);
  assert.equal(livePackets(state, NOW + 300).length, 2);
  assert.equal(livePackets(state, NOW + 100 + PACKET_LIFETIME_MS).length, 1);
  for (let i = 0; i < MAX_ACTIVITY + 5; i += 1) state = reduceFleet(state, event("phase", 10 + i), NOW);
  assert.equal(state.activity.length, MAX_ACTIVITY);
});

test("heartbeats record the server clock and smooth the skew", async () => {
  let state = await loadedState();
  state = reduceFleet(state, { type: "heartbeat", data: { at: NOW + 60_000 } }, NOW);
  assert.equal(state.heartbeatAt, NOW + 60_000);
  assert.equal(state.skew, 60_000, "a large jump is taken at once");
  state = reduceFleet(state, { type: "heartbeat", data: { at: NOW + 61_000 } }, NOW);
  assert.equal(state.skew, 60_000 * 0.8 + 61_000 * 0.2);
});

test("detail reads merge a crew member into its ship", async () => {
  const state = await loadedState();
  const member = structuredClone(state.ships.pegasus.crewMembers[0]);
  member.role = "Gateway routes and SSE";
  const next = reduceFleet(state, { type: "crew-detail", data: { ship: "pegasus", member } }, NOW);
  const crew = { ship: "pegasus", crew: member.id };
  assert.equal(withCrewDetail(next, crew).ships.pegasus.crewMembers[0].role, "Gateway routes and SSE");
  assert.equal(reduceFleet(state, { type: "crew-detail", data: { ship: "nowhere", member } }, NOW), state);
  const { ships } = sampleFleet();
  assert.equal(reduceFleet(initialFleetState(), { type: "ship-detail", data: ships.galactica }, NOW).ships.galactica.name, "Galactica");
});


test("bounded ship events preserve the selected crew member's recent project and idle age", async () => {
  const state = await loadedState();
  const ship = structuredClone(state.ships.pegasus);
  const member = structuredClone(ship.crewMembers[0]);
  const selection = { ship: ship.id, crew: member.id };
  const latest = NOW - 1000;
  member.since = latest;
  member.idleSince = latest;
  const task = {
    id: "preview-1", kind: "assignment", status: "completed", summary: "Published the design preview", phase: "completed",
    progress: "", startedAt: NOW - 60_000, createdAt: NOW - 60_000, finishedAt: latest, updatedAt: latest,
  };
  member.contexts[0] = { ...member.contexts[0], lastOutcome: task, recent: [task] };
  member.contexts.push({
    ...member.contexts[0], project: "design-preview", recent: [task],
    lastOutcome: task, recentCount: 1,
  });
  const drilledIn = reduceFleet(state, { type: "crew-detail", data: { ship: ship.id, member } }, NOW);
  const updated = reduceFleet(drilledIn, { type: "ship", data: ship }, NOW + 5000);
  const selected = withCrewDetail(updated, selection).ships.pegasus.crewMembers[0];
  assert.equal(selected.since, latest);
  assert.equal(selected.contexts.at(-1).recent[0].id, task.id);
  assert.ok(updated.ships.pegasus.crewMembers[0].contexts.every((context) => context.project !== "design-preview"));
  const stale = reduceFleet(updated, {
    type: "ship", data: { ...ship, available: false, detailStale: true },
  }, NOW + 6000);
  const projected = withCrewDetail(stale, selection).ships.pegasus;
  assert.equal(projected.available, false);
  assert.equal(projected.crewMembers[0].contexts.at(-1).recent[0].id, task.id);
  const summaryOutage = reduceFleet(updated, {
    type: "fleet", data: {
      ...updated.fleet, ships: updated.fleet.ships.map((entry) =>
        entry.id === ship.id ? { ...entry, available: false } : entry),
    },
  }, NOW + 7000);
  assert.equal(withCrewDetail(summaryOutage, selection).ships.pegasus.crewMembers[0].contexts.at(-1).recent[0].id, task.id);

  const bounded = ship.crewMembers[0];
  const running = reduceFleet(updated, {
    type: "ship", data: {
      ...ship, crewMembers: [{
        ...bounded, activity: "running", since: NOW + 8000, idleSince: null,
        contexts: [{
          ...bounded.contexts[0],
          recent: [{ ...task, summary: "Published…" }],
        }, ...bounded.contexts.slice(1)],
      }, ...ship.crewMembers.slice(1)],
    },
  }, NOW + 8000);
  const runningMember = withCrewDetail(running, selection).ships.pegasus.crewMembers[0];
  assert.equal(runningMember.activity, "running");
  assert.equal(runningMember.since, NOW + 8000);
  assert.equal(runningMember.contexts.at(-1).recent[0].id, task.id);
  assert.equal(runningMember.contexts[0].lastOutcome.id, task.id);
  assert.equal(runningMember.contexts[0].recent[0].summary, task.summary);

  const queued = reduceFleet(updated, {
    type: "ship", data: {
      ...ship, crewMembers: [{
        ...bounded, contexts: [{ ...bounded.contexts[0], queuedCount: 1 }, ...bounded.contexts.slice(1)],
      }, ...ship.crewMembers.slice(1)],
    },
  }, NOW + 9000);
  const queuedMember = withCrewDetail(queued, selection).ships.pegasus.crewMembers[0];
  assert.equal(queuedMember.contexts[0].queuedCount, 1);
  assert.equal(queuedMember.contexts[0].lastOutcome.id, task.id);
  const inFlight = reduceFleet(queued, {
    type: "crew-detail", data: { ship: ship.id, member }, shipAdvanced: true,
  }, NOW + 9999);
  const inFlightMember = withCrewDetail(inFlight, selection).ships.pegasus.crewMembers[0];
  assert.equal(inFlightMember.contexts[0].queuedCount, 1);
  assert.equal(inFlightMember.contexts.at(-1).recent[0].id, task.id);
  const offlineAfterQueue = reduceFleet(inFlight, {
    type: "fleet", data: {
      ...inFlight.fleet, ships: inFlight.fleet.ships.map((entry) =>
        entry.id === ship.id ? { ...entry, available: false } : entry),
    },
  }, NOW + 10_000);
  const offlineMember = withCrewDetail(offlineAfterQueue, selection).ships.pegasus.crewMembers[0];
  assert.equal(offlineMember.contexts[0].queuedCount, 1);
  assert.equal(offlineMember.contexts.at(-1).recent[0].id, task.id);
});
