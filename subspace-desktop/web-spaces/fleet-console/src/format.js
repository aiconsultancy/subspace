// Formatters for durations, uptime, bytes and clocks. Pure: no DOM, no Date.now().

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A compact elapsed time: `42s`, `17m`, `2h 05m`, `3d 4h`. Negative or missing reads as 0s. */
export function formatDuration(ms) {
  const s = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  if (s < MINUTE) return `${s}s`;
  const m = Math.floor(s / MINUTE);
  if (s < HOUR) return `${m}m`;
  const h = Math.floor(s / HOUR);
  if (s < 2 * DAY) return `${h}h ${String(m % 60).padStart(2, "0")}m`;
  return `${Math.floor(s / DAY)}d ${h % 24}h`;
}

/** Uptime from seconds: `45s`, `12m`, `4h 12m`, `3d 4h 12m`. */
export function formatUptime(seconds) {
  const s = Math.max(0, Math.floor(Number(seconds) || 0));
  if (s < MINUTE) return `${s}s`;
  const d = Math.floor(s / DAY);
  const h = Math.floor((s % DAY) / HOUR);
  const m = Math.floor((s % HOUR) / MINUTE);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

const UNITS = ["KiB", "MiB", "GiB", "TiB", "PiB"];

/** Binary byte sizes: `512 B`, `1.5 KiB`, `15.6 GiB`, `234 GiB`. */
export function formatBytes(bytes) {
  const n = Number(bytes);
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${Math.round(n)} B`;
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = value < 100 ? value.toFixed(1) : String(Math.round(value));
  return `${text.replace(/\.0$/, "")} ${UNITS[unit]}`;
}

/** A 0..1 fraction as a whole percentage; null or missing reads as an em dash. */
export function formatPercent(fraction) {
  if (fraction === null || fraction === undefined || !Number.isFinite(Number(fraction))) return "—";
  return `${Math.round(Math.max(0, Math.min(1, Number(fraction))) * 100)}%`;
}

/** Load averages to two decimals, separated by spaces. */
export function formatLoad(load) {
  if (!Array.isArray(load)) return "—";
  return load.map((value) => (Number.isFinite(value) ? value.toFixed(2) : "—")).join(" ");
}

/** Local wall-clock time `HH:MM:SS` for an epoch-millisecond time. */
export function formatClock(time) {
  const date = new Date(time);
  if (Number.isNaN(date.getTime())) return "--:--:--";
  return [date.getHours(), date.getMinutes(), date.getSeconds()].map((v) => String(v).padStart(2, "0")).join(":");
}

const initialsCache = new Map();

/** Two upper-case initials for an avatar: `Starbuck` → `ST`, `Lee Adama` → `LA`. */
export function initials(name) {
  const key = String(name ?? "");
  let value = initialsCache.get(key);
  if (value === undefined) {
    value = computeInitials(key);
    if (initialsCache.size > 500) initialsCache.clear();
    initialsCache.set(key, value);
  }
  return value;
}

function computeInitials(name) {
  const words = String(name ?? "").trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

/** `1 file`, `3 files`. */
export function plural(count, one, many = `${one}s`) {
  return `${count} ${count === 1 ? one : many}`;
}
