import assert from "node:assert/strict";
import { test } from "node:test";
import { createSseParser } from "../src/sse.js";

function parse(chunks, options) {
  const events = [];
  const parser = createSseParser((event) => events.push(event), options);
  for (const chunk of chunks) parser.push(chunk);
  parser.end();
  return events;
}

test("parses typed events with JSON data", () => {
  const events = parse(['event: fleet\ndata: {"a":1}\n\n', "event: heartbeat\ndata: {\"at\":5}\n\n"]);
  assert.deepEqual(events.map((e) => [e.type, e.data]), [["fleet", '{"a":1}'], ["heartbeat", '{"at":5}']]);
});

test("survives any chunk boundary, including inside a field name and between CR and LF", () => {
  const stream = 'event: ship\r\ndata: {"id":"galactica"}\r\n\r\nevent: host\r\ndata: {"x":2}\r\n\r\n';
  const whole = parse([stream]);
  for (let size = 1; size <= 7; size += 1) {
    const chunks = [];
    for (let i = 0; i < stream.length; i += size) chunks.push(stream.slice(i, i + size));
    assert.deepEqual(parse(chunks), whole, `chunk size ${size}`);
  }
  assert.equal(whole.length, 2);
});

test("a CR at the end of one chunk and LF at the start of the next is one line ending", () => {
  const events = parse(["data: one\r", "\n\r", "\ndata: two\r", "", "\n\n"]);
  assert.deepEqual(events.map((e) => e.data), ["one", "two"]);
});

test("bare CR line endings work", () => {
  assert.deepEqual(parse(["data: a\r\rdata: b\r\r"]).map((e) => e.data), ["a", "b"]);
});

test("joins multi-line data with newlines and strips one leading space", () => {
  const [event] = parse(["data:first\ndata:  second\ndata\n\n"]);
  assert.equal(event.data, "first\n second\n");
  assert.equal(event.type, "message");
});

test("comments are ignored but reported, and keep nothing buffered", () => {
  const comments = [];
  const events = parse([": keep-alive\n\n:ping\ndata: x\n\n"], { onComment: (text) => comments.push(text) });
  assert.deepEqual(comments, ["keep-alive", "ping"]);
  assert.deepEqual(events.map((e) => e.data), ["x"]);
});

test("heartbeats arrive as ordinary typed events", () => {
  const events = parse(["event: heartbeat\ndata: {\"at\":1795000000000}\n\n"]);
  assert.equal(events[0].type, "heartbeat");
  assert.equal(JSON.parse(events[0].data).at, 1795000000000);
});

test("a blank line without data dispatches nothing and resets the event type", () => {
  const events = parse(["event: fleet\n\ndata: plain\n\n"]);
  assert.deepEqual(events.map((e) => e.type), ["message"]);
});

test("ids persist across events, ids with NUL are ignored, retry needs digits", () => {
  const retries = [];
  const events = parse(["id: 7\ndata: a\n\ndata: b\n\nid: bad\0id\nretry: 15x\nretry: 2500\ndata: c\n\n"], { onRetry: (ms) => retries.push(ms) });
  assert.deepEqual(events.map((e) => e.id), ["7", "7", "7"]);
  assert.deepEqual(retries, [2500]);
});

test("a byte order mark at the start is dropped", () => {
  assert.equal(parse(["﻿data: x\n\n"])[0].data, "x");
});

test("an event cut off by the end of the stream is discarded", () => {
  assert.deepEqual(parse(["data: complete\n\ndata: partial"]).map((e) => e.data), ["complete"]);
});

test("a line without a colon is a field with an empty value", () => {
  assert.deepEqual(parse(["data\n\n"]).map((e) => e.data), [""]);
});

test("refuses an unbounded line", () => {
  const parser = createSseParser(() => {}, { maxLine: 16 });
  assert.throws(() => parser.push("data: 0123456789abcdef0123"), /longer than/);
});
