import assert from "node:assert/strict";
import { test } from "node:test";
import { crewPanel, fleetPanel, shipPanel, tooltip } from "../src/inspector.js";
import { esc, issueTiles } from "../src/html.js";
import { crewContext, describeTier } from "../src/effects.js";
import { sampleResolve } from "../src/sample.js";
import { loadedState, NOW } from "./fixtures.js";

const EVIL = '<img src=x onerror="alert(1)">';

function model(state, issues = {}) {
  const answers = new Map(sampleResolve(["FLT-212", "FLT-205", "FLT-198"]).map((answer) => [answer.identifier, answer]));
  return {
    fleet: state.fleet,
    ships: state.ships,
    now: NOW,
    unavailableSince: state.unavailableSince,
    issues: { enabled: true, idsFor: () => ["FLT-212"], get: (id) => answers.get(id) ?? null, canOpen: true, ...issues },
  };
}

test("the fleet panel shows the admiral, host, uptimes, versions, flags, ships and issues", async () => {
  const html = fleetPanel(model(await loadedState()));
  for (const text of ["Rear admiral", "Kirrin", "Host", "/var/lib/stargate", "VM uptime", "9d 4h 12m", "Stargate uptime", "2d 6h 0m", "0.18.2", "1.4.7", "1.3.4", "Feature flags", "Quota watch", "Ships", "Galactica", "Idle watch", "Issues in flight", "FLT-212"]) {
    assert.ok(html.includes(text), text);
  }
  assert.match(html, /default on/, "a flag that differs from its default is marked");
  assert.match(html, /<span class="tag">03:30<\/span>/, "a flag's value is shown");
  assert.match(html, /data-issue="FLT-212"/);
});

test("the ship panel shows the captain, queue, crew and features; an unavailable ship says why", async () => {
  const state = await loadedState();
  const html = shipPanel(model(state), "galactica");
  for (const text of ["Captain", "Adama", "Queue", "Crew", "Starbuck", "Ship features", "Steer thread follow-ups"]) assert.ok(html.includes(text), text);
  const down = shipPanel(model(state), "resolute");
  assert.match(down, /Crew status unavailable/);
  assert.match(down, /gateway socket stopped answering/);
  assert.match(shipPanel(model(state), "nowhere"), /Ship not found/);
});

test("the crew panel shows task, live status, git, pull request, projects, queue, subagents and outcomes", async () => {
  const html = crewPanel(model(await loadedState()), "galactica", "starbuck");
  for (const text of ["Current task", "Live status: observed", "Implementing", "feat/flt-212-tier-warp", "a3f91c2", "origin/main · 6 ahead · 0 behind", "3 changed files", "example-org/subspace", "#81", "draft", "Projects", "stargate", "Queued", "Subagents", "explore · path builder", "Recent outcomes", "Theme tones for the fleet board"]) {
    assert.ok(html.includes(text), text);
  }
  const tali = crewPanel(model(await loadedState()), "endeavour", "tali");
  assert.match(tali, /Live status: not checked/);
});

test("API text is escaped everywhere it appears", async () => {
  const state = await loadedState();
  const ships = structuredClone(state.ships);
  const member = ships.galactica.crewMembers[0];
  member.name = EVIL;
  member.contexts[0].current.summary = EVIL;
  member.contexts[0].git.branch = EVIL;
  const fleet = { ...state.fleet, name: EVIL };
  const m = { ...model(state), fleet, ships };
  for (const html of [fleetPanel(m), shipPanel(m, "galactica"), crewPanel(m, "galactica", "starbuck"), tooltip(m, { kind: "crew", ship: "galactica", crew: "starbuck" })]) {
    assert.doesNotMatch(html, /<img/);
    assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
  }
  assert.equal(esc(`"'<>&`), "&quot;&#39;&lt;&gt;&amp;");
});

test("issue tiles: available ones are buttons with glyphs; others degrade to quiet rows", () => {
  const get = (id) => ({ "FLT-212": sampleResolve(["FLT-212"])[0], "ABC-1": { identifier: "ABC-1", state: "signed-out" } })[id] ?? null;
  const html = issueTiles(["FLT-212", "ABC-1", "ABC-2"], get, true);
  assert.match(html, /<button class="tile" type="button" data-issue="FLT-212" aria-label="Open FLT-212 in Issues, Tier warp/);
  assert.match(html, /class="glyph t-yellow"/);
  assert.match(html, /In Progress/);
  assert.match(html, /Sign in to Issues/);
  assert.match(html, /ABC-2<\/span><span>Checking…/);
  assert.doesNotMatch(issueTiles(["FLT-212"], get, false), /<button/);
  assert.equal(issueTiles([], get, true), "");
});

test("tooltips cover every hit kind", async () => {
  const m = model(await loadedState());
  assert.match(tooltip(m, { kind: "ship", ship: "galactica" }), /Click to warp in/);
  assert.match(tooltip(m, { kind: "ship", ship: "resolute" }), /stopped answering/);
  assert.match(tooltip(m, { kind: "admiral" }), /Rear admiral Kirrin/);
  assert.match(tooltip(m, { kind: "captain", ship: "pegasus" }), /Captain Garner/);
  assert.match(tooltip(m, { kind: "sub", name: "explore" }), /Subagent/);
  const recent = m.ships.galactica.crewMembers[0].contexts[0].recent[0];
  assert.match(tooltip(m, { kind: "recent", ref: recent }), /took 52m/);
});

test("chat context for a crew member: useful, bounded, and free of endpoints and tokens", async () => {
  const state = await loadedState();
  const value = crewContext(state.fleet, state.ships, { level: 2, ship: "galactica", crew: "starbuck" }, ["FLT-212"], NOW);
  // Subspace adds the Space title, so the label does not repeat it.
  assert.equal(value.label, "Starbuck on Galactica");
  for (const text of ["Ship: Galactica", "Task: Fleet bridge canvas", "Phase: Implementing", "Branch: feat/flt-212-tier-warp", "Pull request: #81", "Issues: FLT-212", "(observed live)"]) {
    assert.ok(value.content.includes(text), text);
  }
  assert.doesNotMatch(value.content, /token|https?:\/\/|Bearer/i);
  assert.ok(new TextEncoder().encode(value.content).length <= 8192 && value.label.length <= 100);
  assert.equal(crewContext(state.fleet, state.ships, { level: 1, ship: "galactica" }, [], NOW), null);
});

test("each tier has an accessible description", async () => {
  const state = await loadedState();
  assert.match(describeTier(state.fleet, state.ships, { level: 0 }), /Fleet colonial-fleet: 5 ships, 4 available/);
  assert.match(describeTier(state.fleet, state.ships, { level: 1, ship: "galactica" }), /5 crew members/);
  assert.match(describeTier(state.fleet, state.ships, { level: 1, ship: "resolute" }), /unavailable/);
  assert.match(describeTier(state.fleet, state.ships, { level: 2, ship: "galactica", crew: "helo" }), /uncertain/);
  assert.match(describeTier(null, {}, { level: 0 }), /Waiting/);
});
