import { describe, expect, test } from "bun:test";
import { Schema } from "effect";
import { CreateFleet, CreateShip, IrcChannel, UpdateFleet } from "../src/fleets";

const accepts = Schema.is(IrcChannel);

describe("fleet and ship channel names follow the desktop's single-channel rule", () => {
  test.each(["#a", "#aurora-lyra", "#été", `#${"x".repeat(63)}`])("accepts %p", (name) => {
    expect(accepts(name)).toBe(true);
  });

  test.each([
    "",
    "#",
    "aurora",
    "&aurora",
    "#aurora:lyra",
    "#aurora,lyra",
    "#aurora lyra",
    "#aurora\tlyra",
    "#aurora lyra",
    "#aurora lyra",
    "#aurora\u0007lyra",
    "#aurora\u0085lyra",
    `#${"x".repeat(64)}`,
  ])("refuses %p", (name) => {
    expect(accepts(name)).toBe(false);
  });

  test("applies to the Starbase, the command channel and a ship's channel", () => {
    const fleet = {
      slug: "example",
      name: "Example",
      ircHost: "irc.example.test",
      ircPort: 6697,
      ircTls: true,
    };
    const decodeFleet = Schema.decodeUnknownExit(CreateFleet);
    expect(decodeFleet({ ...fleet, starbaseChannel: "#example:starbase" })._tag).toBe("Failure");
    expect(decodeFleet({ ...fleet, commandChannel: "#example:command" })._tag).toBe("Failure");
    expect(decodeFleet({ ...fleet, commandChannel: "#example-command" })._tag).toBe("Success");
    expect(Schema.decodeUnknownExit(UpdateFleet)({ starbaseChannel: "#a:b" })._tag).toBe("Failure");
    const decodeShip = Schema.decodeUnknownExit(CreateShip);
    expect(decodeShip({ name: "Lyra", channel: "#aurora:lyra" })._tag).toBe("Failure");
    expect(decodeShip({ name: "Lyra", channel: "#aurora-lyra" })._tag).toBe("Success");
  });
});
