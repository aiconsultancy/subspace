import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addDays,
  addHours,
  dayLabel,
  describeAge,
  isValidTimeZone,
  longDate,
  weekdayName,
  zonedNow,
} from "../src/time.js";

test("zonedNow converts one instant into each zone's wall clock", () => {
  const instant = Date.UTC(2026, 9, 31, 23, 30);
  assert.equal(zonedNow(instant, "Europe/London"), "2026-10-31T23:30");
  assert.equal(zonedNow(instant, "Asia/Tokyo"), "2026-11-01T08:30");
  assert.equal(zonedNow(instant, "America/New_York"), "2026-10-31T19:30");
});

test("zonedNow follows daylight saving changes", () => {
  const summer = Date.UTC(2026, 6, 1, 12, 0);
  const winter = Date.UTC(2026, 0, 1, 12, 0);
  assert.equal(zonedNow(summer, "Europe/London"), "2026-07-01T13:00");
  assert.equal(zonedNow(winter, "Europe/London"), "2026-01-01T12:00");
  // The UK clocks went back at 02:00 on 25 October 2026 (01:00 UTC).
  assert.equal(zonedNow(Date.UTC(2026, 9, 25, 0, 30), "Europe/London"), "2026-10-25T01:30");
  assert.equal(zonedNow(Date.UTC(2026, 9, 25, 1, 30), "Europe/London"), "2026-10-25T01:30");
});

test("zonedNow never reports hour 24 at midnight", () => {
  assert.equal(zonedNow(Date.UTC(2026, 0, 1, 0, 5), "Europe/London"), "2026-01-01T00:05");
});

test("isValidTimeZone", () => {
  assert.equal(isValidTimeZone("Europe/London"), true);
  assert.equal(isValidTimeZone("Nowhere/Land"), false);
  assert.equal(isValidTimeZone(""), false);
  assert.equal(isValidTimeZone(undefined), false);
});

test("calendar helpers use plain dates", () => {
  assert.equal(weekdayName("2026-10-31"), "Sat");
  assert.equal(dayLabel("2026-10-31", "2026-10-31"), "Today");
  assert.equal(dayLabel("2026-11-01", "2026-10-31"), "Sun");
  assert.equal(longDate("2026-10-31"), "Sat 31 Oct");
  assert.equal(addDays("2026-10-31", 1), "2026-11-01");
  assert.equal(addDays("2026-12-31", 1), "2027-01-01");
  assert.equal(addDays("2028-02-28", 1), "2028-02-29");
  assert.equal(addHours("2026-10-31T23:00", 2), "2026-11-01T01:00");
});

test("describeAge", () => {
  assert.equal(describeAge(10_000), "just now");
  assert.equal(describeAge(-5), "just now");
  assert.equal(describeAge(12 * 60_000), "12 min ago");
  assert.equal(describeAge(3 * 3_600_000), "3 h ago");
  assert.equal(describeAge(72 * 3_600_000), "3 days ago");
});
