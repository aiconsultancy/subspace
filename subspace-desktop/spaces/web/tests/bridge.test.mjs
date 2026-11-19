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
  };
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
