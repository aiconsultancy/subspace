import { expect, test } from "bun:test";
import { missingFleetFields, nickEdit } from "../scripts/demo/backfill";
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

const fixture = { ircHost: "127.0.0.1", ircPort: 6667, ircTls: false, commandChannel: "#c" };
const seeded = {
  irc: { host: "127.0.0.1", port: 6667, tls: false },
  starbaseChannel: null,
  commandChannel: null,
};

test("a re-run records the fixture's channels on a fleet that has none", () => {
  expect(missingFleetFields(seeded, fixture)).toEqual({ commandChannel: "#c" });
  expect(missingFleetFields(seeded, { ...fixture, starbaseChannel: "#s" })).toEqual({
    starbaseChannel: "#s",
    commandChannel: "#c",
  });
});

test("a re-run keeps a channel someone set and changes only a different IRC server", () => {
  expect(missingFleetFields({ ...seeded, commandChannel: "#kept" }, fixture)).toEqual({});
  expect(
    missingFleetFields(
      { ...seeded, irc: { ...seeded.irc, port: 7000 }, commandChannel: "#k" },
      fixture,
    ),
  ).toEqual({ ircHost: "127.0.0.1", ircPort: 6667, ircTls: false });
});

test("a re-run gives an agent its nick only when it has none", () => {
  const agent = {
    userId: "00000000-0000-4000-8000-000000000001",
    name: "aurora-lyra",
    rank: "captain",
    fleet: "aurora",
    ship: "Lyra",
    fleetId: "00000000-0000-4000-8000-000000000002",
    shipId: "00000000-0000-4000-8000-000000000003",
    defaultModel: "rehearsal-model",
    serviceTokenClientId: "aurora-lyra@example.com",
    ircNick: null,
    version: 4,
  };
  const { userId: _userId, ...registration } = agent;
  expect(nickEdit(agent, "aurora-lyra")).toEqual({ ...registration, ircNick: "aurora-lyra" });
  expect(nickEdit({ ...agent, ircNick: "renamed" }, "aurora-lyra")).toBeNull();
});
