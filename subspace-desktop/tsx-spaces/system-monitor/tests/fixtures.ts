// Readings in the shapes of spec §5, as the host sends them.
import type { Apps, Facts, Metrics } from "../src/model";

export const AT = Date.UTC(2026, 10, 26, 9, 30, 5);

export function metrics(at = AT, cpu: number | null = 42): Metrics {
  return {
    at,
    cpu,
    cores: [40, 44, 38, 46],
    memory: { used: 8e9, total: 16e9, swapUsed: 1e9, swapTotal: 2e9 },
    network: { rx: 2048, tx: 512 },
  };
}

export const apps: Apps = {
  at: AT,
  apps: [
    { id: "app-0000000000000001", name: "Firefox", memory: 2.5e9, processes: 12 },
    { id: "app-0000000000000002", name: "Subspace", memory: 9e8, processes: 1 },
  ],
};

export const facts: Facts = {
  at: AT,
  os: "macOS",
  osVersion: "26.1",
  kernel: "25.1.0",
  architecture: "arm64",
  machineModel: "Mac16,5",
  cpuModel: "Apple M4 Max",
  physicalCores: 16,
  logicalCores: 16,
  gpuModel: null,
  memoryTotal: 64e9,
  uptime: 93_780,
  displays: [{ width: 1512, height: 982, scale: 2 }],
  shell: "zsh",
  subspaceVersion: "0.12.1",
};

// A host's handler refuses an ungranted tier with this message (spec R-SYS-7).
const denied = () => {
  throw new Error("not granted");
};

/** Host handlers for `open`, with the named tiers refused. */
export function handlers(refuse: ("metrics" | "apps" | "facts")[] = []) {
  return {
    "subspace.system.metrics": refuse.includes("metrics") ? denied : () => metrics(),
    "subspace.system.apps": refuse.includes("apps") ? denied : () => apps,
    "subspace.system.facts": refuse.includes("facts") ? denied : () => facts,
  };
}
