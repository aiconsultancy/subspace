import assert from "node:assert/strict";
import { test } from "node:test";
import { isCrewMember, isFleet, isShipDetail, isTask, isValidEvent } from "../src/contract.js";
import { createSampleSim, deriveMember, SAMPLE_SCOPE, sampleResolve } from "../src/sample.js";
import { createSampleSource } from "../src/sample-source.js";
import { extractIdentifiers } from "../src/issues.js";
import { issueSources } from "../src/model.js";
import { NOW, sampleFleet, seeded } from "./fixtures.js";

/** Stricter than the reducer's checks: every field the contract names, with its type. */
function strictMember(member) {
  const required = ["id", "name", "role", "model", "backend", "activity", "since", "idleSince", "contexts", "subagents"];
  for (const key of required) assert.ok(key in member, `member.${key}`);
  for (const context of member.contexts) {
    for (const key of ["project", "activity", "provisioned", "observed", "current", "queuedCount", "queued", "lastOutcome", "recent", "recentCount", "git", "pullRequest"]) assert.ok(key in context, `context.${key}`);
    assert.ok(context.queued.length <= 5, "at most five queued");
    assert.ok(context.recent.length <= 8, "at most eight recent");
    for (const task of [...context.queued, ...context.recent]) {
      assert.ok(isTask(task));
      assert.ok(task.summary.length <= 180 && task.phase.length <= 80 && task.progress.length <= 400);
    }
  }
  assert.equal(member.idleSince !== null, member.activity === "idle", "idleSince only while idle");
}

test("the sample fleet conforms to the Fleet API v1 contract", () => {
  const { fleet, ships } = sampleFleet();
  assert.ok(isFleet(fleet));
  assert.equal(fleet.apiVersion, 1);
  for (const detail of Object.values(ships)) {
    assert.ok(isShipDetail(detail), detail.id);
    for (const member of detail.crewMembers) {
      assert.ok(isCrewMember(member), member.id);
      strictMember(member);
    }
  }
  assert.ok(fleet.ships.every((ship) => !("crewMembers" in ship)), "the fleet lists summaries only");
  assert.equal(fleet.admiral.role, "rear_admiral");
  assert.ok(fleet.ships.every((ship) => ship.captain.role === "captain"));
  const unavailable = fleet.ships.filter((ship) => !ship.available);
  assert.deepEqual(unavailable.map((ship) => [ship.id, ship.crew, typeof ship.error]), [["resolute", null, "string"]]);
  assert.ok(fleet.longestIdle && fleet.longestIdle.since < NOW);
});

test("the sample has no real hosts, tokens or people", () => {
  const text = JSON.stringify(sampleFleet());
  for (const [, host] of text.matchAll(/https?:\/\/([^/"]+)/g)) assert.ok(["github.com", "linear.app"].includes(host), host);
});

test("the simulation keeps changing and only ever emits valid events", () => {
  const random = seeded(11);
  let clock = NOW;
  const sim = createSampleSim({ now: () => clock, random });
  const kinds = new Set();
  for (const event of sim.initial()) assert.ok(isValidEvent(event.type, event.data), event.type);
  for (let i = 0; i < 300; i += 1) {
    clock += 5600;
    for (const event of [...sim.tick(), ...sim.hostTick(), ...sim.heartbeat()]) {
      assert.ok(isValidEvent(event.type, event.data), `${event.type} at tick ${i}`);
      if (event.type === "activity") kinds.add(event.data.kind);
      if (event.type === "ship") for (const member of event.data.crewMembers) strictMember(member);
    }
  }
  for (const kind of ["dispatch", "phase", "outcome", "git"]) assert.ok(kinds.has(kind), kind);
});

test("member state is derived from contexts as the contract says", () => {
  const member = {
    id: "m",
    name: "M",
    role: null,
    model: null,
    backend: "opencode",
    activity: "idle",
    since: null,
    idleSince: null,
    subagents: [],
    contexts: [
      { project: "a", activity: "idle", provisioned: "ready", observed: false, current: null, queuedCount: 1, queued: [{ id: "q", status: "queued", kind: "assignment", summary: "q", phase: "", progress: "", createdAt: 100, startedAt: null, updatedAt: 100, finishedAt: null }], lastOutcome: null, recent: [], recentCount: 0, git: null, pullRequest: null },
      { project: "b", activity: "idle", provisioned: "ready", observed: false, current: null, queuedCount: 0, queued: [], lastOutcome: { id: "u", status: "uncertain", kind: "assignment", summary: "u", phase: "", progress: "", createdAt: 1, startedAt: 2, updatedAt: 50, finishedAt: 60 }, recent: [], recentCount: 0, git: null, pullRequest: null },
    ],
  };
  deriveMember(member);
  assert.deepEqual(member.contexts.map((context) => context.activity), ["queued", "uncertain"]);
  assert.equal(member.activity, "uncertain", "uncertain outranks queued");
  member.contexts[1].lastOutcome.status = "completed";
  deriveMember(member);
  assert.equal(member.activity, "queued");
  assert.equal(member.since, 100);
});

test("sample issue scope and answers match the identifiers in the sample", () => {
  const { ships } = sampleFleet();
  const ids = Object.values(ships).flatMap((detail) => detail.crewMembers.flatMap((member) => extractIdentifiers(issueSources(member), SAMPLE_SCOPE)));
  assert.ok(ids.length >= 6);
  for (const answer of sampleResolve(ids)) {
    assert.equal(answer.state, "available", answer.identifier);
    assert.equal(answer.summary.identifier, answer.identifier);
  }
  assert.equal(sampleResolve(["FLT-1"])[0].state, "unavailable");
});

test("the sample source emits the connect snapshot, then ticks on timers, and stops cleanly", () => {
  const events = [];
  const timers = [];
  const intervals = [];
  const source = createSampleSource({
    onEvent: (event) => events.push(event),
    now: () => NOW,
    random: seeded(3),
    setTimer: (fn) => timers.push(fn),
    clearTimer: () => {},
    every: (fn, ms) => intervals.push({ fn, ms }),
    stopEvery: () => {},
  });
  source.start();
  assert.deepEqual(events.map((event) => event.type), ["fleet", "ship", "ship", "ship", "ship", "ship"]);
  assert.deepEqual(intervals.map((entry) => entry.ms), [5600, 10_000, 15_000]);
  intervals[2].fn();
  assert.equal(events.at(-1).type, "heartbeat");
  source.stop();
});
