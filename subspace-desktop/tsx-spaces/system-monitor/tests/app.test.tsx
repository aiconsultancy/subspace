import { expect, open, test } from "nsh/test";
import { clock } from "../src/model";
import { AT, handlers, metrics } from "./fixtures";

const desktop = { width: 900, height: 900, platform: "macos" as const, colorScheme: "dark" as const };

test("shows every tier with data", async () => {
  const app = await open("../App.tsx", { ...desktop, host: { handlers: handlers() } });
  await app.waitFor("id:graph-cpu");
  expect(app.shows("text:42%")).toBe(true);
  expect(app.shows("text:Core 4 · 46%")).toBe(true);
  expect(app.shows("text:8.00 GB of 16.00 GB · 50%")).toBe(true);
  expect(app.shows("text:2.0 kB/s")).toBe(true);
  expect(app.shows("text:512 B/s")).toBe(true);
  expect(app.shows("id:graph-memory")).toBe(true);
  expect(app.shows("id:graph-network")).toBe(true);
  await app.waitFor("text:Firefox");
  expect(app.shows("text:12 processes")).toBe(true);
  expect(app.shows("text:2.50 GB")).toBe(true);
  await app.waitFor("text:Apple M4 Max");
  expect(app.shows("text:macOS 26.1")).toBe(true);
  expect(app.shows("text:1d 2h")).toBe(true);
  expect(app.shows("text:1512×982 @2x")).toBe(true);
  expect(app.shows("text:zsh")).toBe(true);
  expect(app.texts()).toContain(`Sampled every 1 s · last sample ${clock(AT)}`);
  expect(app.host.calls().map((call) => call.name).sort()).toEqual([
    "subspace.system.apps",
    "subspace.system.facts",
    "subspace.system.metrics",
  ]);
});

test("follows readings the host sends", async () => {
  const app = await open("../App.tsx", { ...desktop, host: { handlers: handlers() } });
  await app.waitFor("id:graph-cpu");
  await app.host.send("subspace.system.metrics", metrics(AT + 1000, 75));
  await app.waitFor("text:75%");
  expect(app.texts()).toContain(`Sampled every 1 s · last sample ${clock(AT + 1000)}`);
});

test("an unavailable value reads Unavailable, not zero", async () => {
  const app = await open("../App.tsx", { ...desktop, host: { handlers: handlers() } });
  await app.waitFor("id:graph-cpu");
  await app.host.send("subspace.system.metrics", metrics(AT + 1000, null));
  await app.waitFor("text:Unavailable");
  expect(app.shows("text:0%")).toBe(false);
});

for (const tier of ["metrics", "apps", "facts"] as const) {
  test(`explains a refused ${tier} tier and keeps the others`, async () => {
    const app = await open("../App.tsx", { ...desktop, host: { handlers: handlers([tier]) } });
    await app.waitFor(`id:empty-${tier}`);
    expect(app.texts().some((text) => text.includes("permission"))).toBe(true);
    if (tier !== "metrics") expect(app.shows("id:graph-cpu")).toBe(true);
    if (tier !== "apps") await app.waitFor("text:Firefox");
    if (tier !== "facts") await app.waitFor("text:Apple M4 Max");
  });
}
