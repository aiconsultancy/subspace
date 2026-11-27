// The readings the host sends and the pure rules that turn them into history
// and text. Nothing here touches the UI.
//
// The formatters and the `null`-means-unavailable rule are adapted from
// sysmon (https://github.com/huacnlee/sysmon, src/model.ts at 8defd3c),
// Copyright (c) 2026 Jason Lee, MIT licence. See NOTICE.

/** The host-channel names this package calls; `nsh.json` declares them. */
export const HOST = {
  metrics: "subspace.system.metrics",
  apps: "subspace.system.apps",
  facts: "subspace.system.facts",
} as const;

export type TierName = keyof typeof HOST;

/** Samples kept per graph. */
export const CAPACITY = 60;

/** How often the host samples each tier, in seconds. */
export const SAMPLE_SECONDS = { metrics: 1, apps: 2 } as const;

/** `subspace.system.metrics`. `null` means unavailable, never zero. */
export interface Metrics {
  /** Milliseconds since the Unix epoch. */
  at: number;
  cpu: number | null;
  cores: number[];
  memory: { used: number | null; total: number | null; swapUsed: number | null; swapTotal: number | null };
  network: { rx: number | null; tx: number | null };
}

export interface AppUsage {
  id: string;
  name: string;
  memory: number;
  processes: number;
}

/** `subspace.system.apps`. */
export interface Apps {
  at: number;
  apps: AppUsage[];
}

export interface Display {
  width: number;
  height: number;
  scale: number;
}

/** `subspace.system.facts`. */
export interface Facts {
  at: number;
  os: string | null;
  osVersion: string | null;
  kernel: string | null;
  architecture: string | null;
  machineModel: string | null;
  cpuModel: string | null;
  physicalCores: number | null;
  logicalCores: number | null;
  gpuModel: string | null;
  memoryTotal: number | null;
  uptime: number | null;
  displays: Display[] | null;
  shell: string | null;
  subspaceVersion: string | null;
}

/** Where a tier stands for this view. */
export type Tier<T> =
  | { kind: "loading" }
  | { kind: "refused" }
  | { kind: "standalone" }
  | { kind: "failed"; message: string }
  | { kind: "ready"; reading: T };

/** The message a host's handler refuses an ungranted tier with, where nsh
 * cannot gate the name itself (spec R-SYS-7). */
export const NOT_GRANTED = "not granted";

/** What a failed `host.call` means for the view. */
export function failure<T>(error: unknown): Tier<T> {
  const code = (error as { code?: string } | null)?.code;
  const message = error instanceof Error ? error.message : String(error);
  if (code === "ERR_ACCESS_DENIED" || message === NOT_GRANTED) return { kind: "refused" };
  if (code === "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM") return { kind: "standalone" };
  return { kind: "failed", message };
}

/** A graph's samples, oldest first; `null` is a gap. */
export type Series = (number | null)[];

export interface History {
  cpu: Series;
  cores: Series[];
  memory: Series;
  rx: Series;
  tx: Series;
}

export const emptyHistory: History = { cpu: [], cores: [], memory: [], rx: [], tx: [] };

/** `series` with `value` added, trimmed to the last `CAPACITY` samples. */
export function push(series: Series, value: number | null): Series {
  const next = series.length >= CAPACITY ? series.slice(series.length - CAPACITY + 1) : series.slice();
  next.push(value === null || !Number.isFinite(value) ? null : value);
  return next;
}

/** Memory use in percent of the total, or `null`. */
export function memoryPercent(memory: Metrics["memory"]): number | null {
  return memory.used === null || !memory.total ? null : (memory.used / memory.total) * 100;
}

export function record(history: History, reading: Metrics): History {
  const cores = reading.cores.map((value, index) => push(history.cores[index] ?? [], value));
  return {
    cpu: push(history.cpu, reading.cpu),
    cores,
    memory: push(history.memory, memoryPercent(reading.memory)),
    rx: push(history.rx, reading.network.rx),
    tx: push(history.tx, reading.network.tx),
  };
}

export const UNAVAILABLE = "Unavailable";

/** `format(value)`, or "Unavailable" for `null`. */
export function reading<T>(value: T | null | undefined, format: (value: T) => string): string {
  return value === null || value === undefined ? UNAVAILABLE : format(value);
}

export function bytes(n: number): string {
  return n < 1e9 ? `${(n / 1e6).toFixed(0)} MB` : `${(n / 1e9).toFixed(2)} GB`;
}

export function rate(n: number): string {
  if (n < 1e3) return `${Math.round(n)} B/s`;
  if (n < 1e6) return `${(n / 1e3).toFixed(1)} kB/s`;
  if (n < 1e9) return `${(n / 1e6).toFixed(1)} MB/s`;
  return `${(n / 1e9).toFixed(1)} GB/s`;
}

export function percent(n: number): string {
  return `${Math.round(n)}%`;
}

export function duration(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

/** `HH:MM:SS` in local time. */
export function clock(at: number): string {
  const time = new Date(at);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(time.getHours())}:${two(time.getMinutes())}:${two(time.getSeconds())}`;
}

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** The facts panel's rows, in the order fastfetch shows them. */
export function factRows(facts: Facts): [string, string][] {
  const text = (value: string | null) => value ?? UNAVAILABLE;
  const os = [facts.os, facts.osVersion].filter((part) => part !== null).join(" ");
  const cores =
    facts.physicalCores === null && facts.logicalCores === null
      ? UNAVAILABLE
      : `${facts.physicalCores ?? "?"} physical, ${facts.logicalCores ?? "?"} logical`;
  const displays =
    facts.displays === null || facts.displays.length === 0
      ? UNAVAILABLE
      : facts.displays.map((d) => `${d.width}×${d.height} @${d.scale}x`).join(", ");
  return [
    ["OS", os === "" ? UNAVAILABLE : os],
    ["Kernel", text(facts.kernel)],
    ["Architecture", text(facts.architecture)],
    ["Model", text(facts.machineModel)],
    ["CPU", text(facts.cpuModel)],
    ["Cores", cores],
    ["GPU", text(facts.gpuModel)],
    ["Memory", reading(facts.memoryTotal, bytes)],
    ["Uptime", reading(facts.uptime, duration)],
    ["Displays", displays],
    ["Shell", text(facts.shell)],
    ["Subspace", text(facts.subspaceVersion)],
  ];
}
