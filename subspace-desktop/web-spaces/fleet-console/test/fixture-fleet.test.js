// The desktop's Stargate Fleet API fixture (scripts/fleet_api_test_server.py),
// which `make capture` serves on loopback, read through the console's own
// contract, reducer, model and inspector. Skipped where python3 is missing.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { isCrewDetail, isFleet, isShipDetail } from "../src/contract.js";
import { crewPanel, fleetPanel, shipPanel } from "../src/inspector.js";
import { extractIdentifiers } from "../src/issues.js";
import { allCrew, fleetStats, issueSources } from "../src/model.js";
import { initialFleetState, reduceFleet } from "../src/reducer.js";

const script = join(dirname(fileURLToPath(import.meta.url)), "../../../scripts/fleet_api_test_server.py");

function snapshot() {
  const run = spawnSync("python3", ["-I", script, "--snapshot"], { encoding: "utf8" });
  return run.status === 0 ? JSON.parse(run.stdout) : null;
}

const fixture = existsSync(script) ? snapshot() : null;
const skip = fixture ? false : "python3 or the desktop's Fleet API fixture is not available";

function model() {
  let state = reduceFleet(initialFleetState(), { type: "fleet", data: fixture.fleet }, fixture.fleet.generatedAt);
  for (const detail of Object.values(fixture.ships)) state = reduceFleet(state, { type: "ship", data: detail }, fixture.fleet.generatedAt);
  return {
    fleet: state.fleet,
    ships: state.ships,
    now: fixture.fleet.generatedAt,
    unavailableSince: state.unavailableSince,
    issues: { enabled: false },
  };
}

test("every fixture payload passes the console's v1 contract", { skip }, () => {
  assert.ok(isFleet(fixture.fleet));
  for (const detail of Object.values(fixture.ships)) assert.ok(isShipDetail(detail), detail.id);
  for (const members of Object.values(fixture.crew)) {
    for (const detail of Object.values(members)) assert.ok(isCrewDetail(detail), detail.member.id);
  }
});

test("the header strip has a busy fleet with queued, uncertain and idle crew", { skip }, () => {
  const { fleet, ships } = model();
  const stats = fleetStats(fleet, ships);
  assert.deepEqual(
    { online: stats.shipsOnline, total: stats.shipsTotal, busy: stats.busy, people: stats.people, queued: stats.queued, uncertain: stats.uncertain },
    { online: 4, total: 4, busy: 9, people: 16, queued: 5, uncertain: 1 },
  );
  assert.equal(stats.longestIdle.member, "basil");
  const states = new Set(allCrew(fleet, ships).map((entry) => entry.view.state));
  assert.deepEqual([...states].sort(), ["idle", "queued", "running", "uncertain", "unprovisioned"]);
});

test("the inspector shows the rear admiral, host and system at fleet tier, and ship and crew detail", { skip }, () => {
  const view = model();
  const fleet = fleetPanel(view);
  for (const text of ["aurora", "aurora-vm-01", "Stargate 0.43.0", "aurora-admiral", "claude-opus-5-5", "16 cores", "1, 5 and 15 min", "used of 64", "data", "VM uptime", "Stargate uptime", "2.3.2", "1.4.4", "Idle watch", "Basil"]) {
    assert.ok(fleet.includes(text), text);
  }
  assert.match(fleet, /Queue: 1 running · 2 queued · 57 done · 1 failed/);
  const ship = shipPanel(view, "aurora-orion");
  for (const text of ["Orion", "aurora-orion", "Marlow", "fix/relay-1-recover-queued-frames", "Uncertain"]) assert.ok(ship.includes(text), text);
  const crew = crewPanel(view, "aurora-orion", "ines");
  for (const text of ["Ines", "Show the survey corridor at every zoom level", "sector-atlas", "relay-handover", "explore · zoom bands", "None yet.", "Recent outcomes"]) {
    assert.ok(crew.includes(text), text);
  }
  assert.match(crewPanel(view, "aurora-lyra", "wren"), /#492/);
});

test("crew work links to the demo world's NAV, MAP, WARP and RELAY issues", { skip }, () => {
  const { fleet, ships } = model();
  const scope = { teamKeys: ["NAV", "MAP", "WARP", "RELAY"] };
  const ids = new Set(allCrew(fleet, ships).flatMap((entry) => extractIdentifiers(issueSources(entry.member), scope)));
  for (const id of ["NAV-1", "NAV-7", "NAV-11", "MAP-1", "MAP-9", "RELAY-1", "RELAY-3", "RELAY-6", "WARP-7"]) assert.ok(ids.has(id), id);
});
