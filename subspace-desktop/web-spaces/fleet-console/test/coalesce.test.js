import assert from "node:assert/strict";
import { test } from "node:test";
import { coalesce } from "../src/coalesce.js";

function deferredTask() {
  const calls = [];
  const finishers = [];
  const task = (flag) =>
    new Promise((resolve) => {
      calls.push(flag);
      finishers.push(resolve);
    });
  return { task, calls, finish: () => finishers.shift()?.(), pending: () => finishers.length };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

test("a retry asked for while a plain refresh runs is not dropped", async () => {
  const { task, calls, finish } = deferredTask();
  const run = coalesce(task);
  const first = run(false);
  const second = run(true);
  assert.equal(second, first, "callers share the run in flight");
  assert.deepEqual(calls, [false]);
  finish();
  await tick();
  assert.deepEqual(calls, [false, true], "one more run follows, with the retry");
  finish();
  await first;
});

test("plain calls during a run share it without another run", async () => {
  const { task, calls, finish } = deferredTask();
  const run = coalesce(task);
  const first = run(false);
  run(false);
  run(false);
  finish();
  await first;
  assert.deepEqual(calls, [false]);
  const next = run(true);
  assert.notEqual(next, first, "a call after the run starts a new one");
  finish();
  await next;
  assert.deepEqual(calls, [false, true]);
});

test("a failed run still lets the queued retry happen", async () => {
  const calls = [];
  let fail = true;
  const run = coalesce(async (flag) => {
    calls.push(flag);
    await tick();
    if (fail) {
      fail = false;
      throw new Error("bridge hiccup");
    }
  });
  const first = run(false);
  run(true);
  await first;
  assert.deepEqual(calls, [false, true]);
});
