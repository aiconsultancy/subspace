import assert from "node:assert/strict";
import { join, dirname } from "node:path";

const runtime = await Bun.file("/etc/subspace-fleet/gateway.json").json();
const { QuotaWatch, quotaWatchSchema } = await import(`${dirname(runtime.entrypoint)}/quota-watch.ts`);
const settings = await Bun.file(runtime.config).json();
const stateFile = join(process.argv[2], "quota-fixture.json");
const config = quotaWatchSchema.parse({ ...settings.quotaWatch, stateFile });
assert(!quotaWatchSchema.safeParse({
  ...config,
  thresholds: { claude: { limitAtPercent: 35, pauseAtPercent: 20 } },
}).success);

const messages = [];
let now = Date.now();
let accounts;
const signal = new AbortController().signal;
const create = () => new QuotaWatch(
  config,
  { read: async () => ({ accounts }) },
  async (text) => { messages.push(text); },
  { warn: (message) => { throw new Error(message); } },
  () => now,
);
async function tick(claude, codex) {
  now += 120_000;
  accounts = Object.entries({ claude, codex }).map(([provider, remainingPercent]) => ({
    provider,
    status: "ok",
    observedAt: now,
    windows: [{ scope: "subscription", active: true, remainingPercent, resetAt: null }],
  }));
  await create().tick(signal);
}

await tick(26, 26);
assert.equal(messages.length, 0);
await tick(25, 25);
for (const provider of ["Claude", "Codex"]) {
  assert.ok(messages[0].includes(`${provider} crew quota: 25% or less remaining. Run at most one crew worker of this type per captain. Hold additional work.`));
}
await tick(15, 15);
for (const provider of ["Claude", "Codex"]) {
  assert.ok(messages[1].includes(`${provider} crew quota: 15% or less remaining. Pause crew launches and resumes. Checkpoint active crew work at its next safe point and preserve task claims.`));
}
await tick(14, 14);
assert.equal(messages.length, 2);
await tick(26, 26);
for (const provider of ["Claude", "Codex"]) {
  assert.ok(messages[2].includes(`${provider} crew quota: Quota restored. Quota has recovered above 25%. Resume normal quota-based crew operations.`));
}
await tick(27, 27);
assert.equal(messages.length, 3);
console.log("Stargate emitted the 25% and 15% warnings and the restoration notice.");
