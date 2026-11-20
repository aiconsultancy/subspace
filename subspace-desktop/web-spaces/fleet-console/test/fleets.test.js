import assert from "node:assert/strict";
import { test } from "node:test";
import { createFleetStore, MAX_LOCAL_FLEETS, newFleetId, packToken, STORE_KEY, STORE_VERSION, tokenKey } from "../src/fleet-store.js";
import { apiFleets, DEFAULT_FLEET_ID, fleetName, localFleets, mergeFleets, resolveFleets, settingsFleet } from "../src/fleet-sources.js";
import { normalizeEndpoint } from "../src/api.js";
import { addBlocker, sampleNote, describeTestError, describeTestSuccess, fleetForm, fleetSection, landscapePanel, validateFleetForm } from "../src/fleet-panel.js";
import { createFleetManager, createSampleFleetManager, nextPollAt, POLL_MS } from "../src/fleet-manager.js";
import { initialFleetState } from "../src/reducer.js";
import { chain, createNavigator, fleetTier, LANDSCAPE, parentOf, planSteps, sameSelection } from "../src/camera.js";
import { focusFor, landscapeLayout } from "../src/layout.js";
import { composeFrame } from "../src/scene/index.js";
import { fleetStatus } from "../src/scene/landscape.js";
import { crumbsHtml, depthHtml } from "../src/chrome.js";
import { crewContext, describeLandscape } from "../src/effects.js";
import { SAMPLE_FLEETS } from "../src/sample.js";
import { loadedState, NOW, sampleFleet } from "./fixtures.js";

function memoryStorage(initial = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (key) => (data.has(key) ? data.get(key) : null),
    setItem: (key, value) => data.set(key, String(value)),
    data,
  };
}

// ---------- store ----------

test("the store round-trips the non-secret list under a versioned key", () => {
  const storage = memoryStorage();
  const store = createFleetStore(storage);
  assert.deepEqual(store.load(), []);
  assert.equal(store.status, "empty");
  assert.equal(store.save([{ id: "fabc12345", name: " Lab ", endpoint: "https://lab.example.com", token: "never stored" }]), true);
  const raw = JSON.parse(storage.data.get(STORE_KEY));
  assert.equal(raw.version, STORE_VERSION);
  assert.deepEqual(raw.fleets, [{ id: "fabc12345", name: "Lab", endpoint: "https://lab.example.com" }], "names are trimmed and tokens never kept");
  assert.deepEqual(createFleetStore(storage).load(), raw.fleets);
});

test("storage that throws leaves the list empty instead of breaking the page", () => {
  const throwing = { getItem: () => { throw new Error("SecurityError"); }, setItem: () => { throw new Error("QuotaExceededError"); } };
  const store = createFleetStore(throwing);
  assert.deepEqual(store.load(), []);
  assert.equal(store.status, "unavailable");
  assert.equal(store.save([{ id: "fabc12345", name: "Lab", endpoint: "https://lab.example.com" }]), false);
  const missing = createFleetStore(null);
  assert.deepEqual(missing.load(), []);
  assert.equal(missing.save([]), false);
});

test("corrupt data is ignored and replaced on the next save; a newer version is never overwritten", () => {
  for (const raw of ["{not json", "42", JSON.stringify({ fleets: [] }), JSON.stringify({ version: 1, fleets: "nope" })]) {
    const storage = memoryStorage({ [STORE_KEY]: raw });
    const store = createFleetStore(storage);
    assert.deepEqual(store.load(), []);
    assert.equal(store.status, "corrupt", raw);
    assert.equal(store.save([]), true);
  }
  const newer = memoryStorage({ [STORE_KEY]: JSON.stringify({ version: STORE_VERSION + 1, fleets: [{ id: "fzzz99999", name: "Future", endpoint: "https://x.example.com", extra: 1 }] }) });
  const store = createFleetStore(newer);
  assert.deepEqual(store.load(), []);
  assert.equal(store.status, "newer");
  assert.equal(store.save([]), false, "a later version's data stays");
  assert.match(newer.data.get(STORE_KEY), /Future/);
});

test("invalid and duplicate entries are dropped, and the list is capped", () => {
  const fleets = [
    { id: "fabc12345", name: "One", endpoint: "https://one.example.com" },
    { id: "fabc12345", name: "Duplicate", endpoint: "https://two.example.com" },
    { id: "bad id", name: "Bad", endpoint: "https://three.example.com" },
    { id: "fdef67890", name: "", endpoint: "https://four.example.com" },
    { id: "fghi12345", name: "No address" },
    null,
    ...Array.from({ length: 40 }, (_, i) => ({ id: `fz${String(i).padStart(8, "0")}`, name: `F${i}`, endpoint: `https://f${i}.example.com` })),
  ];
  const store = createFleetStore(memoryStorage({ [STORE_KEY]: JSON.stringify({ version: 1, fleets }) }));
  const loaded = store.load();
  assert.equal(loaded[0].name, "One");
  assert.equal(loaded.length, MAX_LOCAL_FLEETS);
  assert.ok(!loaded.some((entry) => entry.name === "Duplicate" || entry.name === "Bad"));
});

test("saving more fleets than the store keeps, or an entry it cannot keep, writes nothing and says so", () => {
  const storage = memoryStorage();
  const store = createFleetStore(storage);
  const many = Array.from({ length: MAX_LOCAL_FLEETS }, (_, i) => ({ id: `fz${String(i).padStart(8, "0")}`, name: `F${i}`, endpoint: `https://f${i}.example.com` }));
  assert.equal(store.save(many), true);
  const before = storage.getItem(STORE_KEY);
  assert.equal(store.save([...many, { id: "fextra0001", name: "Extra", endpoint: "https://extra.example.com" }]), false, "no silent truncation");
  assert.equal(store.status, "full");
  assert.equal(storage.getItem(STORE_KEY), before, "the stored list is unchanged");
  assert.equal(store.save([{ id: "bad id", name: "Bad", endpoint: "https://bad.example.com" }]), false, "no silent drop");
  assert.equal(store.status, "invalid");
  assert.equal(storage.getItem(STORE_KEY), before);
});

test("fleet ids make valid Subspace secret keys", () => {
  let seed = 0;
  const id = newFleetId(() => ((seed += 0.137) % 1));
  assert.match(id, /^f[a-z0-9]{10}$/);
  assert.match(tokenKey(id), /^[A-Za-z][A-Za-z0-9_-]{0,39}$/);
});

// ---------- sources ----------

const allowedAll = { settings: true, secrets: true, saveSecrets: true };
const caps = { secrets: true, secretsSet: true };

test("the Settings fleet needs settings, an address, the secrets permission and a token", () => {
  const base = { allowed: allowedAll, settings: { endpoint: "https://fleet.example.com/" }, secrets: { token: " t " }, capabilities: caps, normalizeEndpoint };
  assert.deepEqual(settingsFleet(base).entry, { id: DEFAULT_FLEET_ID, name: null, endpoint: "https://fleet.example.com", origin: "settings", tokenKey: "token", token: "t", problem: null });
  assert.equal(settingsFleet({ ...base, allowed: { settings: false, secrets: true } }).problem, "no-settings");
  assert.equal(settingsFleet({ ...base, settings: {} }).problem, "no-endpoint");
  assert.equal(settingsFleet({ ...base, settings: { endpoint: "http://100.101.102.103:8787" } }).problem, "bad-endpoint");
  assert.equal(settingsFleet({ ...base, capabilities: { secrets: false } }).problem, "no-secrets-api");
  assert.equal(settingsFleet({ ...base, allowed: { settings: true, secrets: false } }).problem, "no-secrets");
  assert.equal(settingsFleet({ ...base, secrets: {} }).problem, "no-token");
});

test("page fleets take their tokens from fleet_<id> secrets, bound to their addresses", () => {
  const list = [
    { id: "fone000001", name: "One", endpoint: "https://one.example.com" },
    { id: "ftwo000002", name: "Two", endpoint: "https://two.example.com" },
    { id: "fbad000003", name: "Bad", endpoint: "http://10.0.0.5" },
  ];
  const fleets = localFleets({ list, secrets: { [tokenKey("fone000001")]: packToken("https://one.example.com", "secret-1") }, allowed: allowedAll, normalizeEndpoint });
  assert.deepEqual(fleets.map((fleet) => [fleet.id, fleet.token, fleet.problem]), [["fone000001", "secret-1", null], ["ftwo000002", null, "no-token"]]);
  assert.equal(localFleets({ list, secrets: { [tokenKey("fone000001")]: "x" }, allowed: { secrets: false }, normalizeEndpoint })[0].token, null);
});

test("merging keeps Settings first, then the API, then the page, one entry per address", () => {
  const settings = { id: DEFAULT_FLEET_ID, endpoint: "https://a.example.com", origin: "settings" };
  const api = apiFleets([{ id: "i1", name: "From API", endpoint: "https://b.example.com", token: "k" }, { id: "i2", endpoint: "https://a.example.com/", token: "k" }], normalizeEndpoint);
  const local = [{ id: "floc000001", endpoint: "https://b.example.com", origin: "local" }, { id: "floc000002", endpoint: "https://c.example.com", origin: "local" }];
  const merged = mergeFleets({ settings, api, local });
  assert.deepEqual(merged.map((fleet) => [fleet.id, fleet.origin]), [[DEFAULT_FLEET_ID, "settings"], ["api_i1", "api"], ["floc000002", "local"]]);
  assert.deepEqual(mergeFleets({}), []);
});

test("resolving: sample without a usable fleet, live with any fleet that has a token", () => {
  const base = { present: true, allowed: allowedAll, capabilities: caps, settings: {}, secrets: {}, list: [], normalizeEndpoint };
  assert.equal(resolveFleets({ ...base, present: false }).reason, "no-bridge");
  const none = resolveFleets(base);
  assert.deepEqual([none.mode, none.reason, none.canStore], ["sample", "no-endpoint", true]);
  const pageOnly = resolveFleets({ ...base, list: [{ id: "fpage00001", name: "Page", endpoint: "https://p.example.com" }], secrets: { [tokenKey("fpage00001")]: packToken("https://p.example.com", "t") } });
  assert.equal(pageOnly.mode, "live", "a page fleet alone is enough");
  assert.equal(pageOnly.defaultProblem, "no-endpoint");
  const both = resolveFleets({ ...base, settings: { endpoint: "https://s.example.com", motion: "Reduced" }, secrets: { token: "t" }, list: [{ id: "fpage00001", name: "Page", endpoint: "https://p.example.com" }] });
  assert.deepEqual(both.fleets.map((fleet) => [fleet.origin, Boolean(fleet.token)]), [["settings", true], ["local", false]]);
  assert.equal(both.motion, "Reduced");
  assert.equal(resolveFleets({ ...base, capabilities: { secrets: true, secretsSet: false } }).canStore, false);
  assert.equal(resolveFleets({ ...base, allowed: { ...allowedAll, saveSecrets: false } }).canStore, false, "storing needs save-secrets");
});

test("resolving reads the frame-time readout setting in either mode", () => {
  const base = { present: true, allowed: allowedAll, capabilities: caps, settings: {}, secrets: {}, list: [], normalizeEndpoint };
  assert.equal(resolveFleets(base).perfReadout, false, "Off by default");
  assert.equal(resolveFleets({ ...base, settings: { perfReadout: "On" } }).perfReadout, true, "in sample mode");
  assert.equal(resolveFleets({ ...base, settings: { perfReadout: "On", endpoint: "https://s.example.com" }, secrets: { token: "t" } }).perfReadout, true, "in live mode");
  assert.equal(resolveFleets({ ...base, settings: { perfReadout: "Off" } }).perfReadout, false);
  assert.equal(resolveFleets({ ...base, allowed: { ...allowedAll, settings: false }, settings: { perfReadout: "On" } }).perfReadout, false, "without the settings permission");
  assert.equal(resolveFleets({ ...base, present: false }).perfReadout, false);
});

test("a fleet's name: the person's, else Stargate's, else its host", () => {
  assert.equal(fleetName({ name: "Lab", endpoint: "https://x.example.com" }, { name: "lab-vm" }), "Lab");
  assert.equal(fleetName({ name: null, endpoint: "https://x.example.com" }, { name: "lab-vm" }), "lab-vm");
  assert.equal(fleetName({ name: null, endpoint: "https://x.example.com:8443" }, null), "x.example.com:8443");
});

// ---------- form ----------

test("address and field validation", () => {
  const ok = validateFleetForm({ name: " Lab ", endpoint: "https://lab.example.com/", token: " abc " });
  assert.deepEqual(ok, { ok: true, errors: {}, value: { name: "Lab", endpoint: "https://lab.example.com", token: "abc" } });
  assert.equal(validateFleetForm({ name: "L", endpoint: "http://localhost:8787", token: "t" }).ok, true, "loopback HTTP through a tunnel");
  assert.match(validateFleetForm({ name: "L", endpoint: "http://100.101.102.103:8787", token: "t" }).errors.endpoint, /HTTPS/);
  assert.match(validateFleetForm({ name: "L", endpoint: "https://u:p@x.example.com", token: "t" }).errors.endpoint, /user name/);
  assert.match(validateFleetForm({ name: "L", endpoint: "fleet", token: "t" }).errors.endpoint, /scheme/);
  assert.match(validateFleetForm({ name: "", endpoint: "", token: "" }).errors.name, /name/);
  assert.match(validateFleetForm({ name: "L", endpoint: "https://a.example.com", token: "t" }, { existing: ["https://a.example.com"] }).errors.endpoint, /already/);
  assert.match(validateFleetForm({ name: "L", endpoint: "https://a.example.com", token: "" }).errors.token, /stargate api token/);
  assert.equal(validateFleetForm({ name: "L", endpoint: "https://a.example.com", token: "" }, { editing: true }).value.token, null, "editing keeps the saved token");
  assert.match(validateFleetForm({ name: "L", endpoint: "https://a.example.com", token: "a\nb" }).errors.token, /token/);
});

test("test-connection words and the reasons adding is blocked", () => {
  assert.match(describeTestError({ status: 401 }), /refused the token/);
  assert.match(describeTestError({ status: 403 }), /network/);
  assert.match(describeTestError({ status: 404 }), /No Fleet API/);
  assert.match(describeTestError({ status: 502 }), /answered 502/);
  assert.match(describeTestError({ name: "TimeoutError" }), /ten seconds/);
  assert.match(describeTestError(new TypeError("Load failed")), /corsOrigins/);
  assert.equal(describeTestSuccess(sampleFleet().fleet), "Connected to “colonial-fleet”: 5 ships, Stargate 0.18.2.");
  assert.match(addBlocker({ present: false, capabilities: {}, allowed: {} }), /inside Subspace|running inside/);
  assert.match(addBlocker({ present: true, capabilities: { secretsSet: false }, allowed: {} }), /cannot store tokens/);
  assert.match(addBlocker({ present: true, capabilities: { secretsSet: true }, allowed: { secrets: false, saveSecrets: true } }), /read its saved secrets/);
  assert.match(addBlocker({ present: true, capabilities: { secretsSet: true }, allowed: { secrets: true, saveSecrets: false } }), /save its own secrets/);
  const neither = addBlocker({ present: true, capabilities: { secretsSet: true }, allowed: { secrets: false, saveSecrets: false } });
  assert.match(neither, /read its saved secrets/);
  assert.match(neither, /save its own secrets/);
  assert.equal(addBlocker({ present: true, capabilities: { secretsSet: true }, allowed: { secrets: true, saveSecrets: true } }), null);
});

test("the form never shows a saved token, and escapes names", () => {
  const html = fleetForm({ mode: "edit", entry: { id: "f1", name: "<b>Lab</b>", endpoint: "https://lab.example.com", token: "SECRET-TOKEN" } });
  assert.doesNotMatch(html, /SECRET-TOKEN/);
  assert.match(html, /type="password"/);
  assert.match(html, /Saved\. Leave blank to keep it\./);
  assert.match(html, /&lt;b&gt;Lab&lt;\/b&gt;/);
  assert.match(fleetForm({ mode: "settings", entry: null }), /Settings › Spaces › Fleet Console/);
  assert.match(fleetForm({ mode: "add", entry: null, blocker: "Not here." }), /Not here\./);
  assert.doesNotMatch(fleetForm({ mode: "add", entry: null, blocker: "Not here." }), /<form/);
});

// ---------- manager and polling ----------

function harness({ entries, failing = new Set(), statuses = {} }) {
  const timers = [];
  let clock = 1_000_000;
  const reads = [];
  const streams = [];
  const changes = [];
  const fleet = sampleFleet().fleet;
  const manager = createFleetManager({
    entries,
    now: () => clock,
    setTimer: (fn, ms) => {
      timers.push({ fn, at: clock + ms });
      return timers.length;
    },
    clearTimer: (id) => {
      if (timers[id - 1]) timers[id - 1].cancelled = true;
    },
    createClient: (entry) => ({
      fleet: async () => {
        reads.push(entry.id);
        if (statuses[entry.id]) throw Object.assign(new Error("http"), { status: statuses[entry.id] });
        if (failing.has(entry.id)) throw new TypeError("Load failed");
        return { ...fleet, name: `${entry.id}-fleet` };
      },
    }),
    createLive: ({ onConnection }) => {
      const live = { started: false, stopped: false, start() { this.started = true; onConnection({ status: "connected", attempt: 0, connectedOnce: true, retryAt: null }); }, stop() { this.stopped = true; }, retryNow() {}, readShip: async () => true, readCrew: async () => true };
      streams.push(live);
      return live;
    },
    onChange: (id) => changes.push(id),
  });
  return {
    manager,
    reads,
    streams,
    changes,
    async advance(ms) {
      clock += ms;
      for (const timer of [...timers]) {
        if (!timer.cancelled && !timer.done && timer.at <= clock) {
          timer.done = true;
          timer.fn();
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 0));
    },
    settle: () => new Promise((resolve) => setTimeout(resolve, 0)),
  };
}

const entry = (id, token = "t") => ({ id, name: id, endpoint: `https://${id}.example.com`, origin: "local", tokenKey: `fleet_${id}`, token, problem: token ? null : "no-token" });

test("poll timing: once on load, then every interval only while the Fleets tier shows", () => {
  const runtime = { client: {}, streaming: false, inflight: false, lastReadAt: null };
  assert.equal(nextPollAt(runtime, { landscape: false }), 0);
  assert.equal(nextPollAt({ ...runtime, lastReadAt: 100 }, { landscape: false }), Infinity);
  assert.equal(nextPollAt({ ...runtime, lastReadAt: 100 }, { landscape: true }), 100 + POLL_MS);
  assert.equal(nextPollAt({ ...runtime, streaming: true }, { landscape: true }), Infinity);
  assert.equal(nextPollAt({ ...runtime, inflight: true }, { landscape: true }), Infinity);
  assert.equal(nextPollAt({ ...runtime, client: null }, { landscape: true }), Infinity);
});

test("the active fleet streams; the others get one read on load and a poll every 30 s on the Fleets tier", async () => {
  const h = harness({ entries: [entry("a"), entry("b"), entry("c")] });
  h.manager.activate("a");
  h.manager.start();
  await h.settle();
  assert.equal(h.streams.length, 1);
  assert.deepEqual(h.reads.sort(), ["b", "c"], "one read each on load, none for the streamed fleet");
  assert.equal(h.manager.get("b").state.fleet.name, "b-fleet");
  assert.equal(h.manager.get("b").connection.status, "connected");
  await h.advance(POLL_MS + 10);
  assert.equal(h.reads.length, 2, "no polling while the Fleets tier is hidden");
  h.manager.setLandscape(true);
  await h.settle();
  assert.equal(h.reads.length, 4, "showing the tier reads stale fleets at once");
  await h.advance(POLL_MS / 2);
  assert.equal(h.reads.length, 4);
  await h.advance(POLL_MS / 2 + 10);
  assert.equal(h.reads.length, 6, "and again after the interval");
  h.manager.setLandscape(false);
  await h.advance(POLL_MS * 3);
  assert.equal(h.reads.length, 6);
  h.manager.stop();
});

test("each fleet keeps its own connection state and error", async () => {
  const h = harness({ entries: [entry("a"), entry("b"), entry("c"), entry("d", null)], failing: new Set(["c"]), statuses: { b: 401 } });
  h.manager.activate("a");
  h.manager.start();
  await h.settle();
  const status = (id) => h.manager.get(id).connection.status;
  assert.equal(status("a"), "connected");
  assert.equal(status("b"), "signed-out");
  assert.equal(status("c"), "unreachable");
  assert.equal(status("d"), "signed-out");
  assert.equal(h.manager.get("d").connection.reason, "no-token");
  assert.equal(h.manager.get("c").connection.retryAt, null, "no countdown while the Fleets tier is hidden");
  assert.ok(!h.reads.includes("d"), "a fleet without a token is never read");
});

test("a successful poll clears the failure count, so later blips still read as reconnecting", async () => {
  const failing = new Set(["b"]);
  const h = harness({ entries: [entry("a"), entry("b")], failing });
  h.manager.activate("a");
  h.manager.setLandscape(true);
  h.manager.start();
  await h.settle();
  await h.advance(POLL_MS + 10);
  assert.equal(h.manager.get("b").connection.attempt, 2);
  failing.delete("b");
  await h.advance(POLL_MS + 10);
  assert.equal(h.manager.get("b").connection.status, "connected");
  assert.equal(h.manager.get("b").connection.attempt, 0);
});

test("polling pauses while the page is hidden and catches up when it shows again", async () => {
  const h = harness({ entries: [entry("a"), entry("b")] });
  h.manager.activate("a");
  h.manager.setLandscape(true);
  h.manager.start();
  await h.settle();
  assert.equal(h.reads.length, 1);
  h.manager.setHidden(true);
  await h.advance(POLL_MS * 3);
  assert.equal(h.reads.length, 1, "no reads while hidden");
  h.manager.setHidden(false);
  await h.settle();
  assert.equal(h.reads.length, 2, "one read on showing again");
  assert.equal(nextPollAt({ client: {}, streaming: false, inflight: false, lastReadAt: null }, { landscape: true, hidden: true }), Infinity);
});

test("switching fleets moves the stream; the previous fleet goes back to polling", async () => {
  const h = harness({ entries: [entry("a"), entry("b")] });
  h.manager.activate("a");
  h.manager.start();
  await h.settle();
  h.manager.activate("b");
  assert.equal(h.streams[0].stopped, true);
  assert.equal(h.streams.length, 2);
  assert.equal(h.manager.active, "b");
  assert.equal(h.manager.get("a").streaming, false);
  h.manager.setLandscape(true);
  await h.advance(POLL_MS + 10);
  assert.ok(h.reads.filter((id) => id === "a").length >= 1, "a is polled again");
});

test("updating the sources keeps unchanged fleets, rebuilds changed ones and drops removed ones", async () => {
  const h = harness({ entries: [entry("a"), entry("b")] });
  h.manager.activate("a");
  h.manager.start();
  await h.settle();
  const before = h.manager.get("b").state;
  h.manager.update([entry("a"), { ...entry("b"), name: "Renamed" }, entry("c")]);
  await h.settle();
  assert.equal(h.manager.get("b").state, before, "a rename keeps the data");
  assert.equal(h.manager.get("b").entry.name, "Renamed");
  assert.ok(h.reads.includes("c"), "a new fleet is read once");
  h.manager.update([{ ...entry("a"), endpoint: "https://moved.example.com" }]);
  assert.equal(h.manager.get("b"), null);
  assert.equal(h.streams[0].stopped, true, "the changed active fleet restarts its stream");
  assert.equal(h.streams.length, 2);
});

test("sample mode has three fleets: two simulated, one that never answers", () => {
  const sources = [];
  const manager = createSampleFleetManager({ createSource: ({ variant }) => ({ variant, start() { sources.push(variant); }, stop() {} }), onChange: () => {} });
  manager.start();
  assert.deepEqual(manager.list().map((fleet) => fleet.id), SAMPLE_FLEETS.map((fleet) => fleet.id));
  assert.deepEqual(sources, ["colonial", "frontier"]);
  assert.equal(manager.get("sample-outer-rim").connection.status, "unreachable");
  assert.equal(manager.get("sample-colonial").connection.status, "sample");
});

// ---------- landscape ----------

test("the landscape puts two fleets left and right of this Subspace, all inside the stage", () => {
  const two = landscapeLayout(1000, 640, ["a", "b"]);
  assert.ok(two.nodes[0].x > two.C.x && two.nodes[1].x < two.C.x);
  assert.ok(Math.abs(two.nodes[0].y - two.nodes[1].y) < 1e-6);
  for (const [W, H] of [[1280, 720], [390, 520]]) {
    for (const n of [2, 3, 5]) {
      for (const node of landscapeLayout(W, H, Array.from({ length: n }, (_, i) => `f${i}`)).nodes) {
        assert.ok(node.x - node.orbit >= 0 && node.x + node.orbit <= W && node.y - node.orbit >= 0 && node.y + node.orbit <= H, `${W}x${H} n=${n}`);
      }
    }
  }
  const node = two.nodes[1];
  assert.deepEqual(focusFor(LANDSCAPE, fleetTier("b"), 1000, 640, ["a", "b"]), { x: node.x, y: node.y, r: node.orbit });
});

test("landscape labels keep clear of the tier stack, the title and this Subspace's label", () => {
  const ids = (n) => Array.from({ length: n }, (_, i) => `f${i}`);
  const hits = (a, b) => Math.min(a.x1, b.x1) > Math.max(a.x0, b.x0) && Math.min(a.y1, b.y1) > Math.max(a.y0, b.y0);
  // Subspace's default window gives the stage about 920 x 600, with the tier stack bottom left.
  for (const [W, H] of [[920, 607], [920, 520], [1180, 700]]) {
    const stack = { x0: 14, y0: H - 170, x1: 190, y1: H - 14 };
    for (const n of [2, 3, 4, 5]) {
      const layout = landscapeLayout(W, H, ids(n), [stack]);
      const hub = { x0: layout.C.x - 70, y0: layout.C.y - 24, x1: layout.C.x + 70, y1: layout.C.y + 54 };
      for (const node of layout.nodes) {
        const { box } = node.label;
        const where = `${W}x${H} n=${n} ${node.id}`;
        assert.equal(hits(box, stack), false, `${where}: clear of the tier stack`);
        assert.equal(hits(box, { x0: 0, y0: 0, x1: Math.min(W * 0.4, 300), y1: 64 }), false, `${where}: clear of the title`);
        assert.equal(hits(box, hub), false, `${where}: clear of this Subspace`);
        assert.ok(box.y1 <= H - 23.5 && box.x0 >= 8 && box.x1 <= W - 8 && node.x - node.orbit >= 0 && node.x + node.orbit <= W, `${where}: inside the frame`);
      }
    }
  }
  // With room to spare nothing moves: three fleets start at the top, centred.
  const roomy = landscapeLayout(1400, 900, ids(3), [{ x0: 14, y0: 730, x1: 190, y1: 886 }]);
  assert.equal(roomy.C.x, 700);
  assert.ok(Math.abs(roomy.nodes[0].angle + Math.PI / 2) < 1e-9);
  // focusFor flies through the node where it was drawn.
  const tight = [{ x0: 14, y0: 437, x1: 190, y1: 593 }];
  const drawn = landscapeLayout(920, 607, ids(3), tight).nodes[2];
  assert.deepEqual(focusFor(LANDSCAPE, fleetTier("f2"), 920, 607, ids(3), tight), { x: drawn.x, y: drawn.y, r: drawn.orbit });
});

test("navigation passes through the Fleets tier only when it exists", () => {
  const crewA = { level: 2, fleet: "a", ship: "s", crew: "c" };
  assert.deepEqual(chain(crewA, true).map((sel) => sel.level), [-1, 0, 1, 2]);
  assert.deepEqual(chain(crewA).map((sel) => sel.level), [0, 1, 2]);
  assert.deepEqual(parentOf(fleetTier("a"), true), LANDSCAPE);
  assert.deepEqual(parentOf(fleetTier("a"), false), fleetTier("a"), "no landscape, no parent above a fleet");
  const shipB = { level: 1, fleet: "b", ship: "t", crew: null };
  assert.deepEqual(planSteps(crewA, shipB, true).map((sel) => [sel.level, sel.fleet]), [[1, "a"], [0, "a"], [-1, null], [0, "b"], [1, "b"]]);
  assert.equal(sameSelection(fleetTier("a"), fleetTier("b")), false);
  const nav = createNavigator({ start: 0, landscape: () => true });
  nav.jump(LANDSCAPE, 0);
  nav.go(crewA, 10);
  assert.equal(nav.state.sel.level, 0, "first step: into fleet a");
  let t = 10;
  while (nav.state.warp && t < 10_000) nav.tick((t += 100));
  assert.deepEqual(nav.state.sel, crewA);
});

test("the breadcrumb and tier stack gain the Fleets level with two or more fleets", async () => {
  const state = await loadedState();
  const sel = { level: 2, fleet: "lab", ship: "galactica", crew: "starbuck" };
  const crumbs = crumbsHtml(state.fleet, state.ships, sel, "Lab", true);
  assert.match(crumbs, /data-go="fleets"><b>Fleets<\/b>/);
  assert.match(crumbs, /data-go="fleet" data-fleet="lab"><b>Lab<\/b>/);
  assert.match(crumbs, /data-go="ship" data-fleet="lab" data-ship="galactica">Galactica/);
  assert.match(crumbs, /aria-current="page">Starbuck/);
  assert.match(crumbsHtml(state.fleet, state.ships, LANDSCAPE, "", true), /^<span class="crumb cur" aria-current="page"><b>Fleets<\/b><\/span>$/);
  assert.match(crumbsHtml(state.fleet, state.ships, fleetTier("lab"), "Lab", false), /Fleet <b>Lab<\/b>/);
  assert.doesNotMatch(crumbsHtml(state.fleet, state.ships, fleetTier("lab"), "Lab", false), /Fleets/);
  const stack = depthHtml(sel, true);
  assert.match(stack, /aria-label="Fleets tier, L0 landscape"/);
  assert.match(stack, /data-go="fleets"/);
  assert.doesNotMatch(depthHtml(sel, false), /Fleets/);
});

function view(id, state, connection, extra = {}) {
  return { id, name: id, entry: { endpoint: `https://${id}.example.com`, origin: "local" }, state, connection, streaming: false, lastReadAt: 1, ...extra };
}

test("the Fleets tier draws a node per fleet, and failed fleets plainly", async () => {
  const state = await loadedState();
  const fleets = [
    view("lab", state, { status: "connected" }, { streaming: true }),
    view("edge", initialFleetState(), { status: "signed-out", reason: null }),
    view("dark", initialFleetState(), { status: "unreachable" }),
  ];
  const nav = createNavigator({ start: 0, landscape: () => true });
  nav.tick(5000);
  nav.jump(LANDSCAPE, 5000);
  const frame = composeFrame(nav.state, { W: 1000, H: 640, now: NOW, clock: NOW, anim: 1, reduced: false, dark: true, fleets, packets: [], hover: null }, 9000, 0.016);
  assert.deepEqual(frame.hits.filter((hit) => hit.kind === "fleet").map((hit) => hit.fleet), ["lab", "edge", "dark"]);
  const texts = frame.layers[0].items.flatMap((item) => (item.k === "text" ? [item.s] : item.k === "run" ? item.parts.map((part) => part.s) : []));
  for (const text of ["Fleets", "Subspace", "Signed out · token refused", "Unreachable", "4/5 ships · 5/16 busy", "CPU 37% · MEM 58%"]) assert.ok(texts.some((s) => s.includes(text)), text);
  assert.ok(frame.layers[0].items.some((item) => item.k === "disk" && item.stroke?.[0] === "red" && item.dash), "a failed fleet's node is red and dashed");
  assert.deepEqual(fleetStatus(view("x", initialFleetState(), { status: "signed-out", reason: "no-token" })), { tone: "red", bad: true, text: "No token saved" });
});

test("panels: the Fleets list, a fleet's connection with Edit and Remove, the Settings fleet's pointer", async () => {
  const state = await loadedState();
  const local = view("lab", state, { status: "connected" });
  const html = landscapePanel({ fleets: [local, view("dark", initialFleetState(), { status: "unreachable" })], sample: false, blocker: null });
  assert.match(html, /Fleets · L0 · landscape/);
  assert.match(html, /data-go="fleet" data-fleet="lab"/);
  assert.match(html, /data-fleet-edit="lab"/);
  assert.match(html, /data-fleet-add/);
  const section = fleetSection({ fleet: local, total: 1, sample: false, confirming: false, now: NOW });
  assert.match(section, /data-fleet-remove="lab"/);
  const { addFleetBar } = await import("../src/fleet-panel.js");
  assert.match(addFleetBar(1), /data-fleet-add/, "with one fleet, Add fleet is offered under the heading");
  assert.equal(addFleetBar(2), "");
  assert.match(fleetSection({ fleet: local, total: 2, sample: false, confirming: true, now: NOW }), /Its saved token is deleted too/);
  const settings = fleetSection({ fleet: { ...local, entry: { ...local.entry, origin: "settings" } }, total: 2, sample: false, confirming: false, now: NOW });
  assert.match(settings, /Settings › Spaces › Fleet Console/);
  assert.doesNotMatch(settings, /data-fleet-remove/);
});

test("chat context names the fleet when there are several; the landscape has a sentence", async () => {
  const state = await loadedState();
  const sel = { level: 2, fleet: "lab", ship: "galactica", crew: "starbuck" };
  const single = crewContext(state.fleet, state.ships, sel, [], NOW);
  assert.equal(single.label, "Starbuck on Galactica");
  const multi = crewContext(state.fleet, state.ships, sel, [], NOW, "Lab");
  assert.equal(multi.label, "Starbuck on Galactica · Lab");
  assert.match(multi.content, /^Fleet: Lab\n/);
  assert.equal(describeLandscape([{ name: "Lab", answering: true, bad: false }, { name: "Dark", answering: false, bad: true }]), "Fleets: 2 fleets, 1 answering; Dark cannot be read. Use the inspector to move between tiers.");
});

test("sample fleets say how to see real ones, inside Subspace or not", () => {
  assert.match(sampleNote(null), /Add a fleet, or set the Stargate API address in Settings/);
  assert.equal(sampleNote("Fleets can be added only inside Subspace."), "These are sample fleets. Fleets can be added only inside Subspace.");
  const html = landscapePanel({ fleets: [], sample: true, blocker: null });
  assert.doesNotMatch(html, /once Fleet Console runs inside Subspace/);
});
