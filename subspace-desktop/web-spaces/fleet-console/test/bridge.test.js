import assert from "node:assert/strict";
import { test } from "node:test";
import { connectBridge, resolveConfig, SETTINGS_SECRETS } from "../src/bridge.js";
import { normalizeEndpoint } from "../src/api.js";

function fakeHost({ permissions = [], settings = {}, secrets = {}, withSecrets = true, withIssues = true, withTheme = true } = {}) {
  const calls = [];
  const listeners = {};
  const subspace = {
    info: async () => ({ version: 1, permissions }),
    settings: { get: async () => settings },
    setBadge: async (value) => calls.push(["badge", value]),
    setContext: async (value) => calls.push(["context", value]),
    notify: async (value) => calls.push(["notify", value]),
  };
  if (withSecrets) subspace.secrets = { get: async () => secrets };
  if (withTheme) subspace.theme = async () => ({ id: "one-dark", mode: "dark", tones: {} });
  if (withIssues) {
    subspace.issues = {
      scope: async () => ({ profileName: "p", appOrigin: "https://app.example.com", teamKeys: ["ABC"], linearWorkspace: null }),
      resolve: async (ids) => ids.map((identifier) => ({ identifier, state: "pending" })),
      open: async (id) => calls.push(["open", id]),
    };
  }
  return {
    subspace,
    calls,
    addEventListener: (name, fn) => (listeners[name] = fn),
    removeEventListener: (name) => delete listeners[name],
    listeners,
  };
}

const ALL = ["settings", "secrets", "issues", "badge", "context", "notifications"];

test("no bridge means sample mode", async () => {
  const bridge = await connectBridge({});
  assert.equal(bridge.present, false);
  assert.deepEqual(await resolveConfig(bridge, normalizeEndpoint), { mode: "sample", reason: "no-bridge", motion: null });
  assert.equal(await bridge.setBadge("1"), null);
});

test("each missing piece of setup has its own sample reason", async () => {
  const cases = [
    [{ permissions: [] }, "no-settings"],
    [{ permissions: ["settings"] }, "no-endpoint"],
    [{ permissions: ["settings"], settings: { endpoint: "http://100.101.102.103:8787" } }, "bad-endpoint"],
    [{ permissions: ["settings"], settings: { endpoint: "https://fleet.example.com" }, withSecrets: false }, "no-secrets-api"],
    [{ permissions: ["settings"], settings: { endpoint: "https://fleet.example.com" } }, "no-secrets"],
    [{ permissions: ["settings", "secrets"], settings: { endpoint: "https://fleet.example.com" }, secrets: {} }, "no-token"],
  ];
  for (const [options, reason] of cases) {
    const config = await resolveConfig(await connectBridge(fakeHost(options)), normalizeEndpoint);
    assert.equal(config.mode, "sample");
    assert.equal(config.reason, reason);
  }
});

test("a complete setup is live, with the motion setting", async () => {
  const host = fakeHost({ permissions: ["settings", "secrets"], settings: { endpoint: "https://fleet.example.com/", motion: "Reduced" }, secrets: { token: " abc " } });
  assert.deepEqual(await resolveConfig(await connectBridge(host), normalizeEndpoint), { mode: "live", endpoint: "https://fleet.example.com", token: "abc", motion: "Reduced" });
});

test("calls are gated by granted permissions and detected methods", async () => {
  const host = fakeHost({ permissions: ["badge"], withIssues: true });
  const bridge = await connectBridge(host);
  await bridge.setBadge("2");
  await bridge.setContext({ label: "x", content: "y" });
  await bridge.notify({ title: "t", body: "b" });
  assert.deepEqual(host.calls, [["badge", "2"]]);
  assert.equal(bridge.issues.available(), false, "issues need the permission");
  assert.deepEqual(bridge.capabilities(), { theme: true, secrets: true, secretsSet: false, issues: true, fleets: false });
  const granted = await connectBridge(fakeHost({ permissions: ALL }));
  assert.equal(granted.issues.available(), true);
  assert.deepEqual((await granted.issues.scope()).teamKeys, ["ABC"]);
  const old = await connectBridge(fakeHost({ permissions: ALL, withIssues: false, withTheme: false, withSecrets: false }));
  assert.equal(old.issues.available(), false);
  assert.equal(await old.theme(), null);
  assert.equal(await old.secrets(), null);
});

test("theme and issue events are forwarded", async () => {
  const host = fakeHost({ permissions: ALL });
  const bridge = await connectBridge(host);
  const seen = [];
  bridge.onTheme((theme) => seen.push(theme.id));
  host.listeners["subspace:theme"]({ detail: { id: "cupertino" } });
  bridge.issues.onChanged(() => seen.push("issues"));
  host.listeners["subspace:issues-changed"]();
  assert.deepEqual(seen, ["cupertino", "issues"]);
});

test("a throwing bridge call resolves to null", async () => {
  const host = fakeHost({ permissions: ALL });
  host.subspace.setBadge = async () => {
    throw new Error("Permission denied.");
  };
  assert.equal(await (await connectBridge(host)).setBadge("1"), null);
});

test("a failed permission refresh keeps the permissions it had", async () => {
  const host = fakeHost({ permissions: ALL, settings: { endpoint: "https://fleet.example.com" }, secrets: { token: "t" } });
  const bridge = await connectBridge(host);
  host.subspace.info = async () => {
    throw new Error("Subspace request timed out.");
  };
  await bridge.refresh();
  assert.equal(bridge.has("secrets"), true);
  assert.equal((await resolveConfig(bridge, normalizeEndpoint)).mode, "live");
  host.subspace.info = async () => ({ version: 1, permissions: ["settings"] });
  await bridge.refresh();
  assert.equal(bridge.has("secrets"), false);
});

test("a bridge whose first info() fails allows nothing", async () => {
  const host = fakeHost({ permissions: ALL });
  host.subspace.info = async () => {
    throw new Error("Subspace request timed out.");
  };
  assert.equal((await connectBridge(host)).has("settings"), false);
});

test("writing a secret needs save-secrets; secrets alone is read-only", async () => {
  const writes = [];
  const withSet = (permissions) => {
    const host = fakeHost({ permissions });
    host.subspace.secrets.set = async (key, value) => writes.push([key, value]);
    return host;
  };
  const readOnly = await connectBridge(withSet(["settings", "secrets"]));
  await assert.rejects(readOnly.setSecret("fleet_fabc000001", "v"), /save its own secrets/);
  assert.deepEqual(writes, [], "secrets.set is not called without save-secrets");
  const granted = await connectBridge(withSet(["settings", "secrets", "save-secrets"]));
  await granted.setSecret("fleet_fabc000001", "v");
  await granted.setSecret("fleet_fabc000001", null);
  assert.deepEqual(writes, [["fleet_fabc000001", "v"], ["fleet_fabc000001", null]]);
  const old = await connectBridge(fakeHost({ permissions: ["secrets", "save-secrets"] }));
  await assert.rejects(old.setSecret("fleet_fabc000001", "v"), /cannot store secrets/);
});

test("the Settings token is never written through secrets.set", async () => {
  const writes = [];
  const host = fakeHost({ permissions: ["settings", "secrets", "save-secrets"] });
  host.subspace.secrets.set = async (key, value) => writes.push([key, value]);
  const bridge = await connectBridge(host);
  assert.ok(SETTINGS_SECRETS.has("token"));
  for (const value of ["replacement", null]) await assert.rejects(bridge.setSecret("token", value), /Settings/);
  assert.deepEqual(writes, []);
});

test("the app's fleets are read only with the secrets permission, and changes are heard", async () => {
  const host = fakeHost({ permissions: ["secrets"] });
  const fleets = [{ id: "aurora", name: "Aurora", endpoint: "https://aurora.example.com", token: "k", problem: null }];
  host.subspace.fleets = { get: async () => fleets };
  const bridge = await connectBridge(host);
  assert.equal(bridge.capabilities().fleets, true);
  assert.deepEqual(await bridge.fleets(), fleets);
  let heard = 0;
  const stop = bridge.onFleetsChanged(() => heard++);
  host.listeners["subspace:fleets-changed"]();
  assert.equal(heard, 1);
  stop();
  assert.equal(host.listeners["subspace:fleets-changed"], undefined);

  const denied = fakeHost({ permissions: [] });
  denied.subspace.fleets = { get: async () => fleets };
  assert.equal(await (await connectBridge(denied)).fleets(), null);
  // An older Subspace, or another app's page, has no fleets.
  const older = await connectBridge(fakeHost({ permissions: ["secrets"] }));
  assert.equal(older.capabilities().fleets, false);
  assert.equal(await older.fleets(), null);
  // A refusal from Subspace (any page but the built-in Fleet Console) reads as none.
  const refused = fakeHost({ permissions: ["secrets"] });
  refused.subspace.fleets = { get: async () => Promise.reject(new Error("Only the built-in Fleet Console reads the fleets of your initiatives.")) };
  assert.equal(await (await connectBridge(refused)).fleets(), null);
  assert.equal(await (await connectBridge({})).fleets(), null);
});

test("only a Retry asks Subspace to read its keychain again", async () => {
  const host = fakeHost({ permissions: ["secrets"] });
  const asked = [];
  host.subspace.fleets = { get: async () => (asked.push("get"), []), retry: async () => (asked.push("retry"), []) };
  const bridge = await connectBridge(host);
  await bridge.fleets();
  await bridge.fleets(false);
  await bridge.fleets("yes");
  assert.deepEqual(asked, ["get", "get", "get"]);
  await bridge.fleets(true);
  assert.deepEqual(asked, ["get", "get", "get", "retry"]);
  // An older Subspace without retry is asked plainly.
  const older = fakeHost({ permissions: ["secrets"] });
  older.subspace.fleets = { get: async () => (asked.push("older"), []) };
  await (await connectBridge(older)).fleets(true);
  assert.equal(asked.at(-1), "older");
});

test("the page asks for a re-read only from its Retry button", async () => {
  const { readFileSync } = await import("node:fs");
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  // Every reconfigure but the Retry button's is a plain one, so a
  // subspace:fleets-changed, a focus or a visibility change never reads
  // the keychain again.
  const calls = [...main.matchAll(/\breconfigure\(([^)]*)\)/g)].map((match) => match[1]);
  assert.deepEqual(calls.filter((argument) => argument !== "" && argument !== "retry = false"), ["true"]);
  assert.match(main, /retry\.addEventListener\("click", \(\) => \{\s*reconfigure\(true\);/);
  assert.match(main, /onFleetsChanged\(\(\) => reconfigure\(\)\)/);
  assert.match(main, /bridge\.fleets\(retry\)/);
});
