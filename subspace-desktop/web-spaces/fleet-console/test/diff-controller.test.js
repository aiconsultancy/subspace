// The crew diff view's lifecycle in the inspector (diff-controller.js),
// with the watcher and panel stubbed: what it opens, draws, observes and
// tears down, and where it puts focus back.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { createBudget } from "../src/budget.js";
import { createDiffController } from "../src/diff-controller.js";

const SHIPS = {
  galactica: { crewMembers: [{ id: "starbuck", name: "Starbuck", contexts: [{ project: "subspace", git: { fingerprint: "fp-1" } }, { project: null, git: { fingerprint: "fp-default" } }] }] },
};

function button(ship, member, project) {
  const node = { dataset: { diffShip: ship, diffMember: member, ...(project ? { diffProject: project } : {}) }, focused: 0 };
  node.focus = () => (node.focused += 1);
  return node;
}

function controllerOver({ token = true, ships = SHIPS, budget = null } = {}) {
  const log = { opened: [], stopped: 0, toasts: [], announced: [], opening: 0, closed: [], watchers: [], panels: [] };
  const hooks = [];
  const buttons = [button("galactica", "starbuck", null), button("galactica", "starbuck", "subspace")];
  const heading = { focused: 0, focus: () => (heading.focused += 1) };
  const inspector = {
    children: [],
    scrollTop: 40,
    replaceChildren: (...nodes) => (inspector.children = nodes),
    querySelector: (selector) => (selector === "h2" ? heading : null),
    querySelectorAll: (selector) => (selector === "[data-diff-member]" ? buttons : []),
  };
  const frames = new Map();
  let nextFrame = 0;
  const controller = createDiffController({
    budget,
    inspector,
    doc: {},
    open: (hook) => {
      hooks.push(hook);
      return token ? { client: { name: "client" }, stop: () => (log.stopped += 1) } : null;
    },
    ships: () => ships,
    toast: (words) => log.toasts.push(words),
    announce: (words) => log.announced.push(words),
    onOpening: () => (log.opening += 1),
    onClosed: (restore) => log.closed.push(restore),
    frame: (draw) => {
      nextFrame += 1;
      frames.set(nextFrame, draw);
      return nextFrame;
    },
    cancelFrame: (id) => frames.delete(id),
    createWatcher: (options) => {
      const watcher = { options, started: 0, stopped: 0, resets: 0, observed: [], view: { status: "loading", n: log.watchers.length } };
      watcher.start = () => (watcher.started += 1);
      watcher.stop = () => (watcher.stopped += 1);
      watcher.reset = () => (watcher.resets += 1);
      watcher.observe = (fingerprint) => watcher.observed.push(fingerprint);
      log.watchers.push(watcher);
      return watcher;
    },
    createPanel: (options) => {
      const panel = { options, root: { panel: log.panels.length }, updates: [], destroyed: 0 };
      panel.update = (view, info) => panel.updates.push([view, info]);
      panel.destroy = () => (panel.destroyed += 1);
      log.panels.push(panel);
      return panel;
    },
  });
  const runFrames = () => {
    const due = [...frames.values()];
    frames.clear();
    for (const draw of due) draw();
  };
  return { controller, inspector, heading, buttons, frames, runFrames, log, hooks };
}

test("diff controller: opening shows the panel, reads the member's checkout and passes its live fingerprint", () => {
  const { controller, inspector, heading, log } = controllerOver();
  assert.equal(controller.open("galactica", "starbuck", "subspace"), true);
  assert.equal(controller.isOpen, true);
  assert.equal(log.opening, 1, "the shell hears first, so it can close the Activity view");
  const [watcher] = log.watchers;
  const [panel] = log.panels;
  assert.deepEqual(watcher.options.target, { ship: "galactica", member: "starbuck", project: "subspace" });
  assert.deepEqual(watcher.options.client, { name: "client" });
  assert.equal(panel.options.watcher, watcher);
  assert.deepEqual(inspector.children, [panel.root]);
  assert.equal(inspector.scrollTop, 0);
  assert.equal(watcher.started, 1);
  assert.deepEqual(watcher.observed, ["fp-1"]);
  assert.deepEqual(panel.updates.at(-1)[1], { name: "Starbuck" });
  assert.equal(heading.focused, 1);
  assert.deepEqual(log.announced, ["Diff of subspace for Starbuck"]);
});

test("diff controller: changes share one frame, and observe follows the ship event's fingerprint", () => {
  const ships = structuredClone(SHIPS);
  const { controller, frames, runFrames, log } = controllerOver({ ships });
  controller.open("galactica", "starbuck", null);
  const [watcher] = log.watchers;
  const [panel] = log.panels;
  const drawn = panel.updates.length;
  watcher.options.onChange();
  watcher.options.onChange();
  assert.equal(frames.size, 1);
  runFrames();
  assert.equal(panel.updates.length, drawn + 1);
  ships.galactica.crewMembers[0].contexts[1].git.fingerprint = "fp-next";
  controller.observe();
  assert.deepEqual(watcher.observed, ["fp-default", "fp-next"]);
  assert.match(log.announced[0], /the default project for Starbuck/);
});

test("diff controller: closing cancels the frame, stops the reads and the handle, destroys the panel and puts focus back", () => {
  const { controller, frames, buttons, log } = controllerOver();
  controller.open("galactica", "starbuck", "subspace");
  const [watcher] = log.watchers;
  const [panel] = log.panels;
  watcher.options.onChange();
  controller.close(true);
  assert.equal(controller.isOpen, false);
  assert.equal(frames.size, 0);
  assert.equal(watcher.stopped, 1);
  assert.equal(log.stopped, 1);
  assert.equal(panel.destroyed, 1);
  assert.deepEqual(log.closed, [true]);
  assert.deepEqual(buttons.map((node) => node.focused), [0, 1], "focus goes back to the button for that project");
  controller.close(true);
  assert.equal(log.closed.length, 1, "it closes once");
});

test("diff controller: a close without restore leaves focus alone; another open closes the first", () => {
  const { controller, buttons, log } = controllerOver();
  controller.open("galactica", "starbuck", "subspace");
  controller.open("galactica", "starbuck", null);
  assert.deepEqual(log.closed, [false]);
  assert.equal(log.watchers[0].stopped, 1);
  assert.equal(log.panels[0].destroyed, 1);
  assert.deepEqual(buttons.map((node) => node.focused), [0, 0]);
  assert.equal(log.opening, 2);
});

test("diff controller: the panel's close and the fleet's reset reach the open view", () => {
  const { controller, buttons, hooks, log } = controllerOver();
  controller.open("galactica", "starbuck", null);
  hooks[0].onReset();
  assert.equal(log.watchers[0].resets, 1);
  log.panels[0].options.onClose();
  assert.equal(controller.isOpen, false);
  assert.deepEqual(log.closed, [true]);
  assert.deepEqual(buttons.map((node) => node.focused), [1, 0]);
});

test("diff controller: the fleet's handle ending closes the view and says why; a stale end does nothing", () => {
  const { controller, hooks, log } = controllerOver();
  controller.open("galactica", "starbuck", "subspace");
  controller.open("galactica", "starbuck", null);
  hooks[0].onEnded();
  assert.equal(controller.isOpen, true, "the first view's end does not close the second");
  hooks[1].onEnded();
  assert.equal(controller.isOpen, false);
  assert.equal(log.stopped, 1, "the ended handle is not stopped again");
  assert.match(log.toasts[0], /Diff closed: the fleet's address or token changed/);
});

test("diff controller: with no token, or no ship or member, it never opens", () => {
  const refused = controllerOver({ token: false });
  assert.equal(refused.controller.open("galactica", "starbuck", null), false);
  assert.equal(refused.controller.isOpen, false);
  assert.deepEqual(refused.log.toasts, ["Diffs need the fleet's token."]);
  assert.equal(refused.inspector.children.length, 0);
  const blank = controllerOver();
  assert.equal(blank.controller.open("", "starbuck", null), false);
  assert.equal(blank.controller.open("galactica", null, null), false);
  assert.equal(blank.log.opening, 0);
  assert.equal(blank.hooks.length, 0);
});

test("diff controller: an unknown member opens under its ID with no fingerprint", () => {
  const { controller, log } = controllerOver({ ships: {} });
  controller.open("galactica", "apollo", "subspace");
  assert.deepEqual(log.watchers[0].observed, [undefined]);
  assert.deepEqual(log.panels[0].updates.at(-1)[1], { name: "apollo" });
});

test("diff controller: main.js leaves the diff view's lifecycle to the controller", () => {
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  for (const gone of ["createDiffWatcher", "createDiffPanel", "app.diff", "closeDiff", "drawDiff"]) assert.ok(!main.includes(gone), gone);
  assert.ok(main.includes("createDiffController"));
});

test("diff controller: the watcher holds content in its own scope of the console's budget", () => {
  const budget = createBudget(1000);
  const { controller, log } = controllerOver({ budget });
  controller.open("galactica", "starbuck", "subspace");
  const scoped = log.watchers[0].options.budget;
  scoped.reserve("part", 100, null);
  assert.equal(budget.bytes("diff\u0000part"), 100, "under the diff view's prefix");
  controller.close();
  const plain = controllerOver();
  plain.controller.open("galactica", "starbuck", null);
  assert.equal(plain.log.watchers[0].options.budget, undefined, "a controller without one gives its watcher none: it makes its own");
});
