import { describe, expect, test } from "bun:test";
import { Schema } from "effect";
import { CreateFleet, FleetApiUrl, normaliseFleetApiUrl, UpdateFleet } from "../src/fleets";

const valid = Schema.is(FleetApiUrl);
const fleet = {
  slug: "example",
  name: "Example fleet",
  ircHost: "irc.example.com",
  ircPort: 6697,
  ircTls: true,
};

describe("a fleet's Fleet API address", () => {
  test.each([
    ["https://fleet.example.com", "https://fleet.example.com"],
    ["https://fleet.example.com/", "https://fleet.example.com"],
    ["HTTPS://Fleet.Example.COM:443/", "https://fleet.example.com"],
    ["https://fleet.example.com:8443/api/", "https://fleet.example.com:8443/api"],
    ["https://fleet.example.com/api//", "https://fleet.example.com/api"],
    ["https://100.64.0.7:7400", "https://100.64.0.7:7400"],
    ["http://127.0.0.1:7400", "http://127.0.0.1:7400"],
    ["http://127.0.0.1:7400/", "http://127.0.0.1:7400"],
    ["http://localhost:7400", "http://localhost:7400"],
    ["http://LOCALHOST:7400", "http://localhost:7400"],
    ["http://[::1]:7400", "http://[::1]:7400"],
    ["https://[::1]:7400", "https://[::1]:7400"],
  ])("accepts %s and stores %s", (input, stored) => {
    expect(valid(input)).toBe(true);
    expect(normaliseFleetApiUrl(input)).toBe(stored);
  });

  test.each([
    ["plain http on a remote host", "http://fleet.example.com:7400"],
    ["plain http on a tailnet address", "http://100.64.0.7:7400"],
    ["plain http on a lookalike of localhost", "http://localhost.example.com:7400"],
    ["plain http on all addresses", "http://0.0.0.0:7400"],
    ["another scheme", "ftp://fleet.example.com"],
    ["a WebSocket scheme", "wss://fleet.example.com"],
    ["a relative address", "/api"],
    ["a host without a scheme", "fleet.example.com"],
    ["no host", "https://"],
    ["a user name", "https://operator@fleet.example.com"],
    ["a user name and password", "https://operator:secret@fleet.example.com"],
    ["an empty user name", "https://@fleet.example.com"],
    ["a query", "https://fleet.example.com/?token=x"],
    ["an empty query", "https://fleet.example.com/?"],
    ["a fragment", "https://fleet.example.com/#token"],
    ["a space", "https://fleet.example.com/a b"],
    ["leading space", " https://fleet.example.com"],
    ["a control character", "https://fleet.example.com/\u0001"],
    ["a backslash", "https://fleet.example.com\\api"],
    ["an invalid port", "https://fleet.example.com:99999"],
    ["too long an address", `https://fleet.example.com/${"a".repeat(2048)}`],
  ])("refuses %s", (_, input) => {
    expect(valid(input)).toBe(false);
    expect(normaliseFleetApiUrl(input)).toBeUndefined();
  });

  test("is optional on create and nullable on update", () => {
    const decodeCreate = Schema.decodeUnknownSync(CreateFleet);
    const decodeUpdate = Schema.decodeUnknownSync(UpdateFleet);
    expect(decodeCreate(fleet)).not.toHaveProperty("fleetApiUrl");
    expect(decodeCreate({ ...fleet, fleetApiUrl: "https://fleet.example.com" })).toMatchObject({
      fleetApiUrl: "https://fleet.example.com",
    });
    expect(() => decodeCreate({ ...fleet, fleetApiUrl: "http://fleet.example.com" })).toThrow();
    expect(decodeUpdate({ fleetApiUrl: null })).toEqual({ fleetApiUrl: null });
    expect(decodeUpdate({ name: "Renamed" })).not.toHaveProperty("fleetApiUrl");
    expect(() => decodeUpdate({ fleetApiUrl: "https://a@fleet.example.com" })).toThrow();
  });
});
