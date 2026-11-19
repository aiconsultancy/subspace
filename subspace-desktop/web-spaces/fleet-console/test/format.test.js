import assert from "node:assert/strict";
import { test } from "node:test";
import { formatBytes, formatClock, formatDuration, formatLoad, formatPercent, formatUptime, initials, plural } from "../src/format.js";

test("durations are compact and never negative", () => {
  assert.equal(formatDuration(-5000), "0s");
  assert.equal(formatDuration(undefined), "0s");
  assert.equal(formatDuration(42_000), "42s");
  assert.equal(formatDuration(17 * 60_000 + 59_000), "17m");
  assert.equal(formatDuration(2 * 3600e3 + 5 * 60_000), "2h 05m");
  assert.equal(formatDuration(47 * 3600e3), "47h 00m");
  assert.equal(formatDuration(3 * 86400e3 + 4 * 3600e3), "3d 4h");
});

test("uptime reads days, hours and minutes", () => {
  assert.equal(formatUptime(45), "45s");
  assert.equal(formatUptime(12 * 60), "12m");
  assert.equal(formatUptime(4 * 3600 + 12 * 60), "4h 12m");
  assert.equal(formatUptime(9 * 86400 + 4 * 3600 + 12 * 60 + 30), "9d 4h 12m");
  assert.equal(formatUptime(-1), "0s");
});

test("bytes use binary units with sensible precision", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(1536), "1.5 KiB");
  assert.equal(formatBytes(1024 ** 2), "1 MiB");
  assert.equal(formatBytes(15.6 * 1024 ** 3), "15.6 GiB");
  assert.equal(formatBytes(234 * 1024 ** 3), "234 GiB");
  assert.equal(formatBytes(1.8 * 1024 ** 4), "1.8 TiB");
  assert.equal(formatBytes(-1), "—");
  assert.equal(formatBytes(Number.NaN), "—");
});

test("percent, load, clock, initials and plurals", () => {
  assert.equal(formatPercent(0.374), "37%");
  assert.equal(formatPercent(null), "—");
  assert.equal(formatPercent(1.7), "100%");
  assert.equal(formatLoad([1.917, 0.5, 2]), "1.92 0.50 2.00");
  assert.match(formatClock(new Date(2026, 10, 19, 9, 5, 3).getTime()), /^09:05:03$/);
  assert.equal(formatClock("nonsense"), "--:--:--");
  assert.equal(initials("Starbuck"), "ST");
  assert.equal(initials("Lee Adama"), "LA");
  assert.equal(initials(""), "?");
  assert.equal(plural(1, "file"), "1 file");
  assert.equal(plural(3, "file"), "3 files");
});
