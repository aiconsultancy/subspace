import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { webcrypto } from "node:crypto";
import test from "node:test";
import vm from "node:vm";

const maxPendingRequests = 32;
const maxMessageBytes = 16 * 1024;
const appOrigin = "subspace://notes";

function fixture(origin = appOrigin, main = true) {
  const messages = [];
  const timers = new Map();
  let nextTimer = 0;
  const window = {
    ipc: { postMessage: text => messages.push(JSON.parse(text)) },
    location: { origin }, crypto: webcrypto, EventTarget, Event, TextEncoder,
    setTimeout(fn) {
      nextTimer += 1;
      const id = nextTimer;
      timers.set(id, () => { timers.delete(id); fn(); });
      return id;
    },
    clearTimeout(id) { timers.delete(id); },
    CustomEvent,
  };
  const events = new EventTarget();
  window.addEventListener = events.addEventListener.bind(events);
  window.dispatchEvent = events.dispatchEvent.bind(events);
  window.top = main ? window : {};
  window.self = window;
  const script = readFileSync(new URL("../src/bridge/init.js", import.meta.url), "utf8")
    .replace('"__SUBSPACE_APP_ORIGIN__"', JSON.stringify(appOrigin));
  vm.runInNewContext(script, window);
  function answer(request, value, error) {
    window.__subspaceReply({ id: request.id, document: request.document, value, error });
  }
  return { window, messages, timers, answer };
}

test("only the app main document receives the bridge", () => {
  for (const [origin, main] of [["https://remote.example", true], [appOrigin, false]]) {
    const { window, messages } = fixture(origin, main);
    assert.equal(window.subspace, undefined);
    assert.equal(messages.length, 0);
  }
});

test("settings replies and errors are delivered only to the matching document", async () => {
  const { window, messages, answer } = fixture();
  answer(messages[0], { notificationPermission: "denied" });
  const request = window.subspace.settings.get();
  assert.equal(messages[1].operation.method, "settings.get");
  window.__subspaceReply({ ...messages[1], document: "old-document", value: "wrong" });
  answer(messages[1], { compact: true });
  assert.equal((await request).compact, true);
  const denied = window.subspace.setBadge("3");
  answer(messages[2], null, "Permission denied");
  await assert.rejects(denied, /Permission denied/);
});

test("requestPermission reports the native decision without granting anything", async () => {
  const { window, messages, answer } = fixture();
  answer(messages[0], { notificationPermission: "denied" });
  const permission = window.Notification.requestPermission();
  assert.equal(messages[1].operation.method, "info");
  answer(messages[1], { notificationPermission: "denied" });
  assert.equal(await permission, "denied");
  assert.equal(window.Notification.permission, "denied");
  const notification = new window.Notification("Hello", { body: "Ready" });
  const error = new Promise(resolve => { notification.onerror = resolve; });
  assert.equal(messages[2].operation.method, "notify");
  answer(messages[2], null, "Permission denied");
  await error;
});

test("requests time out and pending requests and payloads are bounded", async () => {
  const { window, timers, messages, answer } = fixture();
  answer(messages[0], { notificationPermission: "denied" });
  const request = window.subspace.info();
  for (const timeout of timers.values()) {
    timeout();
  }
  await assert.rejects(request, /timed out/);
  await assert.rejects(window.subspace.setBadge("a".repeat(maxMessageBytes)), /too large/);
  await assert.rejects(window.subspace.setBadge("界".repeat(maxMessageBytes / 2)), /too large/);
  const requests = Array.from({ length: maxPendingRequests }, () => window.subspace.info());
  await assert.rejects(window.subspace.info(), /Too many/);
  for (const message of messages.slice(-maxPendingRequests)) {
    answer(message, { notificationPermission: "denied" });
  }
  await Promise.all(requests);
  assert.equal(timers.size, 0);
});

test("serialization errors do not consume pending request slots", async () => {
  const { window, messages, timers, answer } = fixture();
  answer(messages[0], { notificationPermission: "denied" });
  const context = {};
  context.cycle = context;
  for (let i = 0; i <= maxPendingRequests; i++) {
    await assert.rejects(window.subspace.setContext(context), /circular|cyclic/i);
  }
  assert.equal(messages.length, 1);
  assert.equal(timers.size, 0);
});

test("theme, secrets and issues send their documented methods and values", async () => {
  const { window, messages, answer } = fixture();
  answer(messages[0], { notificationPermission: "denied" });
  const theme = window.subspace.theme();
  assert.deepEqual(messages[1].operation, { method: "theme" });
  answer(messages[1], { id: "one-dark", mode: "dark" });
  assert.equal((await theme).mode, "dark");
  const secrets = window.subspace.secrets.get();
  assert.deepEqual(messages[2].operation, { method: "secrets.get" });
  answer(messages[2], { token: "t" });
  assert.equal((await secrets).token, "t");
  const scope = window.subspace.issues.scope();
  assert.deepEqual(messages[3].operation, { method: "issues.scope" });
  answer(messages[3], null);
  assert.equal(await scope, null);
  const resolved = window.subspace.issues.resolve(["ABC-1", 7]);
  assert.deepEqual(messages[4].operation, { method: "issues.resolve", value: ["ABC-1", "7"] });
  answer(messages[4], [{ identifier: "ABC-1", state: "pending" }]);
  assert.equal((await resolved)[0].state, "pending");
  const opened = window.subspace.issues.open("ABC-1");
  assert.deepEqual(messages[5].operation, { method: "issues.open", value: "ABC-1" });
  answer(messages[5], null);
  assert.equal(await opened, undefined);
  await assert.rejects(window.subspace.issues.resolve("ABC-1"), /expects an array/);
  assert.equal(messages.length, 6);
  assert.ok(Object.isFrozen(window.subspace.issues) && Object.isFrozen(window.subspace.secrets));
});

test("native pushes become window events", () => {
  const { window } = fixture();
  const seen = [];
  window.addEventListener("subspace:theme", event => seen.push(["theme", event.detail.mode]));
  window.addEventListener("subspace:issues-changed", event => seen.push([event.type]));
  window.__subspaceEvent("theme", { mode: "light" });
  window.__subspaceEvent("issues-changed", null);
  window.__subspaceEvent("unknown", {});
  assert.deepEqual(seen, [["theme", "light"], ["subspace:issues-changed"]]);
});

test("secrets.set sends a key and a value, or null to delete, and nothing else", async () => {
  const { window, messages, answer } = fixture();
  answer(messages[0], { notificationPermission: "denied" });
  const saved = window.subspace.secrets.set("fleet-a", "token-a");
  assert.deepEqual(messages[1].operation, { method: "secrets.set", value: { key: "fleet-a", value: "token-a" } });
  answer(messages[1], null);
  assert.equal(await saved, undefined);
  const deleted = window.subspace.secrets.set("fleet-a", null);
  assert.deepEqual(messages[2].operation.value, { key: "fleet-a", value: null });
  answer(messages[2], null, "Open this Space to save a secret.");
  await assert.rejects(deleted, /Open this Space/);
  for (const [key, value] of [[1, "t"], ["fleet-a", undefined], ["fleet-a", 7]]) {
    await assert.rejects(window.subspace.secrets.set(key, value), /expects a key/);
  }
  assert.equal(messages.length, 3);
});
