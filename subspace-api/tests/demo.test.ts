import { expect, test } from "bun:test";
import { startOfUtcWeek } from "../scripts/demo/data";

test("the rehearsal week starts on Monday in UTC", () => {
  const expected = Date.parse("2026-11-16T00:00:00Z");
  expect(startOfUtcWeek(new Date("2026-11-16T00:00:00Z"))).toBe(expected);
  expect(startOfUtcWeek(new Date("2026-11-22T18:59:59-05:00"))).toBe(expected);
  expect(startOfUtcWeek(new Date("2026-11-22T19:00:00-05:00"))).toBe(expected + 7 * 86_400_000);
});

test("the rehearsal week may start in the previous year", () => {
  expect(startOfUtcWeek(new Date("2027-01-01T12:25:00Z"))).toBe(Date.parse("2026-12-28T00:00:00Z"));
});
