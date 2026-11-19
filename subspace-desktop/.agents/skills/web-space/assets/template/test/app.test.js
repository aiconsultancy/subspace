import assert from "node:assert/strict";
import { test } from "node:test";
import { greeting } from "../app.js";

const bridge = (permissions, settings = {}) => ({
  info: async () => ({ version: 1, permissions, notificationPermission: "denied" }),
  settings: { get: async () => settings },
});

test("works without the bridge", async () => {
  assert.equal(await greeting(undefined), "Hello from a browser");
});

test("uses the greeting setting when allowed", async () => {
  assert.equal(await greeting(bridge(["settings"], { greeting: "Hi" })), "Hi from Subspace");
});

test("falls back when settings are not allowed", async () => {
  assert.equal(await greeting(bridge([])), "Hello from Subspace");
});
