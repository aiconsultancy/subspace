import { expect, test } from "nsh/test";
import { CAPACITY, clock, emptyHistory, factRows, failure, push, rate, record } from "../src/model";
import { facts, metrics } from "./fixtures";

test("history keeps the last 60 samples", () => {
  let series: (number | null)[] = [];
  for (let i = 0; i < CAPACITY + 15; i++) series = push(series, i);
  expect(series.length).toBe(CAPACITY);
  expect(series[0]).toBe(15);
  expect(series[CAPACITY - 1]).toBe(CAPACITY + 14);
});

test("history records gaps for unavailable values", () => {
  const history = record(record(emptyHistory, metrics()), metrics(2, null));
  expect(history.cpu).toEqual([42, null]);
  expect(history.memory).toEqual([50, 50]);
  expect(history.cores.length).toBe(4);
  expect(history.cores[3]).toEqual([46, 46]);
});

test("refusals and missing hosts map to their states", () => {
  expect(failure({ code: "ERR_ACCESS_DENIED" })).toEqual({ kind: "refused" });
  expect(failure(new Error("not granted"))).toEqual({ kind: "refused" });
  expect(failure({ code: "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM" })).toEqual({ kind: "standalone" });
  expect(failure(new Error("boom"))).toEqual({ kind: "failed", message: "boom" });
});

test("facts read Unavailable when the platform cannot give them", () => {
  const rows = new Map(factRows({ ...facts, gpuModel: null, displays: null, os: null, osVersion: null }));
  expect(rows.get("GPU")).toBe("Unavailable");
  expect(rows.get("Displays")).toBe("Unavailable");
  expect(rows.get("OS")).toBe("Unavailable");
  expect(rows.get("Cores")).toBe("16 physical, 16 logical");
});

test("clock reads hours, minutes and seconds", () => {
  expect(clock(new Date(2026, 0, 1, 7, 5, 9).getTime())).toBe("07:05:09");
});

test("rates format in decimal units", () => {
  expect(rate(999)).toBe("999 B/s");
  expect(rate(1500)).toBe("1.5 kB/s");
  expect(rate(2.5e6)).toBe("2.5 MB/s");
});
