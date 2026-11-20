import assert from "node:assert/strict";
import { test } from "node:test";
import { createFleetForm } from "../src/fleet-form.js";
import { createFleetStore, MAX_LOCAL_FLEETS, packToken, tokenKey } from "../src/fleet-store.js";
import { localFleets } from "../src/fleet-sources.js";
import { normalizeEndpoint } from "../src/api.js";
import { fleetStatus } from "../src/scene/landscape.js";
import { fleetSection } from "../src/fleet-panel.js";
import { initialConnection } from "../src/connection.js";
import { initialFleetState } from "../src/reducer.js";

// A stand-in for the inspector: every `#id` the form asks for is an element
// with a value, text and attributes, and the form's submit handler is kept.
function fakeInspector() {
  const elements = new Map();
  const element = (id) => {
    if (!elements.has(id)) {
      const attributes = new Map();
      const listeners = new Map();
      elements.set(id, {
        id,
        value: "",
        textContent: "",
        hidden: true,
        dataset: {},
        setAttribute: (name, value) => attributes.set(name, value),
        removeAttribute: (name) => attributes.delete(name),
        getAttribute: (name) => attributes.get(name) ?? null,
        addEventListener: (type, handler) => listeners.set(type, handler),
        dispatch: (type) => listeners.get(type)?.({ preventDefault() {} }),
        focus() {},
      });
    }
    return elements.get(id);
  };
  return {
    querySelector: (selector) => (selector.startsWith("#") ? element(selector.slice(1)) : null),
    element,
  };
}

function memoryStorage(initial = {}) {
  const data = new Map(Object.entries(initial));
  return { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => data.set(key, String(value)), removeItem: (key) => data.delete(key) };
}

const target = (attribute) => ({ closest: (selector) => (selector === `[${attribute}]` ? {} : null) });
const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

/** A form over a store holding `list` and secrets holding `secrets`, with every bridge and network call recorded. */
function harness({ list = [], secrets = {}, blocker = null, failSave = false } = {}) {
  const storage = memoryStorage();
  const store = createFleetStore(storage);
  store.save(list);
  if (failSave) storage.setItem = () => {
    throw new Error("QuotaExceededError");
  };
  const saved = { ...secrets };
  const calls = { set: [], read: [], changed: [] };
  const inspector = fakeInspector();
  const entries = () => localFleets({ list: store.load(), secrets: saved, allowed: { secrets: true }, normalizeEndpoint });
  const form = createFleetForm({
    inspector,
    write: () => {},
    bridge: () => ({
      setSecret: async (key, value) => {
        calls.set.push([key, value]);
        if (value === null) delete saved[key];
        else saved[key] = value;
      },
    }),
    store,
    entries,
    blocker: () => blocker,
    readFleet: async (endpoint, token) => {
      calls.read.push([endpoint, token]);
      return { name: "Lab", ships: [], gateway: { version: "1" } };
    },
    onChanged: (change) => calls.changed.push(change),
    onClose: () => {},
  });
  const type = (fields) => {
    for (const [name, value] of Object.entries(fields)) inspector.element(`fleet-${name}`).value = value;
  };
  return {
    form,
    store,
    saved,
    calls,
    entries,
    type,
    error: (field) => inspector.element(`fleet-${field}-error`).textContent,
    status: () => inspector.element("fleet-status").textContent,
    async test() {
      form.click(target("data-fleet-test"));
      await settle();
    },
    async submit() {
      inspector.element("fleet-form").dispatch("submit");
      await settle();
    },
  };
}

const LAB = { id: "flab000001", name: "Lab", endpoint: "https://lab.example.com" };
const labSecrets = () => ({ [tokenKey(LAB.id)]: packToken(LAB.endpoint, "lab-token") });

// ---------- C1: a saved token stays with the address it was saved for ----------

test("editing: Test connection never sends the saved token to a changed address", async () => {
  const h = harness({ list: [LAB], secrets: labSecrets() });
  h.form.open("edit", LAB.id);
  h.type({ name: "Lab", endpoint: "https://elsewhere.example.net", token: "" });
  await h.test();
  assert.deepEqual(h.calls.read, [], "nothing is sent");
  assert.match(h.error("token"), /address changed/i);
});

test("editing: Save refuses a changed address without a newly entered token", async () => {
  const h = harness({ list: [LAB], secrets: labSecrets() });
  h.form.open("edit", LAB.id);
  h.type({ name: "Lab", endpoint: "https://elsewhere.example.net", token: "" });
  await h.submit();
  assert.deepEqual(h.store.load(), [LAB], "the list keeps the old address");
  assert.deepEqual(h.calls.set, []);
  assert.deepEqual(h.calls.changed, []);
  assert.match(h.error("token"), /address changed/i);
});

test("editing: the same address may reuse the saved token; a new address takes a new token bound to it", async () => {
  const h = harness({ list: [LAB], secrets: labSecrets() });
  h.form.open("edit", LAB.id);
  h.type({ name: "Lab", endpoint: "https://lab.example.com/", token: "" });
  await h.test();
  assert.deepEqual(h.calls.read, [["https://lab.example.com", "lab-token"]]);

  h.type({ name: "Lab two", endpoint: "https://lab2.example.com", token: "new-token" });
  await h.test();
  assert.deepEqual(h.calls.read.at(-1), ["https://lab2.example.com", "new-token"]);
  await h.submit();
  assert.deepEqual(h.calls.set, [[tokenKey(LAB.id), packToken("https://lab2.example.com", "new-token")]]);
  assert.deepEqual(h.store.load(), [{ id: LAB.id, name: "Lab two", endpoint: "https://lab2.example.com" }]);
  assert.equal(h.entries()[0].token, "new-token");
});

test("editing only the name keeps the token and writes no secret", async () => {
  const h = harness({ list: [LAB], secrets: labSecrets() });
  h.form.open("edit", LAB.id);
  h.type({ name: "Lab renamed", endpoint: LAB.endpoint, token: "" });
  await h.submit();
  assert.deepEqual(h.calls.set, []);
  assert.equal(h.entries()[0].name, "Lab renamed");
  assert.equal(h.entries()[0].token, "lab-token");
});

test("adding binds the new token to the fleet's address", async () => {
  const h = harness();
  h.form.open("add");
  h.type({ name: "Edge", endpoint: "https://edge.example.com/api/v1", token: "edge-token" });
  await h.submit();
  const [key, value] = h.calls.set[0];
  assert.equal(value, packToken("https://edge.example.com", "edge-token"));
  assert.equal(h.entries().find((entry) => entry.tokenKey === key).token, "edge-token");
});

test("loading: a token kept for another address, or with no address, is not used", () => {
  const list = [LAB, { id: "fold000002", name: "Old", endpoint: "https://old.example.com" }, { id: "fmov000003", name: "Moved", endpoint: "https://moved.example.com" }];
  const secrets = {
    [tokenKey(LAB.id)]: packToken("https://lab.example.com/", "lab-token"),
    [tokenKey("fold000002")]: "plain-token-from-before",
    [tokenKey("fmov000003")]: packToken("https://lab.example.com", "lab-token"),
  };
  const fleets = localFleets({ list, secrets, allowed: { secrets: true }, normalizeEndpoint });
  assert.deepEqual(
    fleets.map((fleet) => [fleet.id, fleet.token, fleet.problem]),
    [
      [LAB.id, "lab-token", null],
      ["fold000002", null, "token-unbound"],
      ["fmov000003", null, "token-address"],
    ],
  );
  const view = (entry) => ({ id: entry.id, name: entry.name, entry, state: initialFleetState(), connection: { ...initialConnection(), status: "signed-out", reason: "no-token" }, streaming: false, lastReadAt: null });
  assert.equal(fleetStatus(view(fleets[2])).text, "Token for another address");
  assert.equal(fleetStatus(view(fleets[1])).text, "Paste the token again");
  assert.match(fleetSection({ fleet: view(fleets[2]), total: 3, sample: false, confirming: false, now: 0 }), /saved for another address/);
});

// ---------- C3: a fleet beyond capacity is refused before its token is stored ----------

const fleetList = (count) => Array.from({ length: count }, (_, i) => ({ id: `fz${String(i).padStart(8, "0")}`, name: `F${i}`, endpoint: `https://f${i}.example.com` }));

test("adding a fleet at capacity is refused before any token is stored", async () => {
  const h = harness({ list: fleetList(MAX_LOCAL_FLEETS) });
  h.form.open("add");
  h.type({ name: "One more", endpoint: "https://more.example.com", token: "more-token" });
  await h.submit();
  assert.deepEqual(h.calls.set, [], "no secret is written");
  assert.equal(h.store.load().length, MAX_LOCAL_FLEETS);
  assert.deepEqual(h.calls.changed, []);
  assert.match(h.status(), new RegExp(`${MAX_LOCAL_FLEETS} fleets`));
});

test("when the list cannot be written, the new token is deleted again", async () => {
  const h = harness({ failSave: true });
  h.form.open("add");
  h.type({ name: "Edge", endpoint: "https://edge.example.com", token: "edge-token" });
  await h.submit();
  assert.equal(h.calls.set.length, 2);
  assert.deepEqual(h.calls.set[1], [h.calls.set[0][0], null], "rolled back");
  assert.deepEqual(h.saved, {});
  assert.deepEqual(h.calls.changed, []);
});

test("when an edit cannot be written, the token saved before it is put back", async () => {
  const h = harness({ list: [LAB], secrets: labSecrets(), failSave: true });
  h.form.open("edit", LAB.id);
  h.type({ name: "Lab", endpoint: "https://lab2.example.com", token: "new-token" });
  await h.submit();
  assert.deepEqual(h.saved, labSecrets(), "the old token, bound to the old address, is back");
  assert.deepEqual(h.calls.changed, []);
});
