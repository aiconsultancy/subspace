import assert from "node:assert/strict";
import { test } from "node:test";
import {
  formatPostcode,
  locationKey,
  normalisePostcode,
  parsePostcodeResponse,
  postcodeUrl,
  validateLocation,
} from "../src/postcode.js";
import { rawPostcode } from "./fixtures.js";

test("normalisePostcode accepts valid UK formats in any case and spacing", () => {
  const valid = ["SW1A 1AA", "sw1a1aa", " M1 1AE ", "B33 8TH", "CR2 6XH", "DN55 1PT", "EC1A 1BB", "GIR 0AA", "BT1 5GS"];
  for (const input of valid) {
    assert.ok(normalisePostcode(input), `${input} should be accepted`);
  }
  assert.equal(normalisePostcode(" sw1a  1aa "), "SW1A1AA");
});

test("normalisePostcode rejects text that is not a UK postcode", () => {
  const invalid = ["", "   ", "12345", "SW1A", "SW1A 1A", "QV1 1AA", "SW1A 1AAA", "90210", "../etc", "SW1A 1AA; DROP", null, undefined, 42];
  for (const input of invalid) {
    assert.equal(normalisePostcode(input), null, `${String(input)} should be rejected`);
  }
});

test("formatPostcode puts a space before the inward code", () => {
  assert.equal(formatPostcode("SW1A1AA"), "SW1A 1AA");
  assert.equal(formatPostcode("M11AE"), "M1 1AE");
});

test("postcodeUrl targets Postcodes.io only and encodes the code", () => {
  assert.equal(postcodeUrl("SW1A1AA"), "https://api.postcodes.io/postcodes/SW1A1AA");
});

test("parsePostcodeResponse keeps coordinates and place names", () => {
  const location = parsePostcodeResponse(rawPostcode());
  assert.equal(location.postcode, "SW1A 1AA");
  assert.equal(location.name, "Westminster");
  assert.deepEqual(location.area, ["St James's", "London"]);
  assert.equal(location.latitude, 51.501009);
  assert.equal(location.country, "England");
});

test("parsePostcodeResponse falls back through name fields", () => {
  const scottish = parsePostcodeResponse(
    rawPostcode({ postcode: "EH1 1YZ", admin_district: null, parish: null, region: null, country: "Scotland", admin_ward: null }),
  );
  assert.equal(scottish.name, "Scotland");
  assert.deepEqual(scottish.area, ["Scotland"]);
});

test("parsePostcodeResponse rejects a postcode without coordinates", () => {
  assert.throws(
    () => parsePostcodeResponse(rawPostcode({ latitude: null, longitude: null })),
    { kind: "no-coordinates" },
  );
});

test("parsePostcodeResponse rejects malformed responses", () => {
  for (const json of [null, {}, { result: null }, { result: "x" }]) {
    assert.throws(() => parsePostcodeResponse(json), { kind: "bad-data" });
  }
  assert.throws(() => parsePostcodeResponse(rawPostcode({ latitude: 951, longitude: 0 })), { kind: "bad-data" });
  assert.throws(() => parsePostcodeResponse(rawPostcode({ postcode: "not one" })), { kind: "bad-data" });
});

test("validateLocation rejects tampered stored values", () => {
  const good = parsePostcodeResponse(rawPostcode());
  assert.deepEqual(validateLocation(JSON.parse(JSON.stringify(good))), good);
  assert.throws(() => validateLocation({ ...good, latitude: "51" }), { kind: "bad-data" });
  assert.throws(() => validateLocation({ ...good, area: "London" }), { kind: "bad-data" });
  assert.throws(() => validateLocation(null), { kind: "bad-data" });
});

test("locationKey rounds to four decimals", () => {
  assert.equal(locationKey({ latitude: 51.501009, longitude: -0.141588 }), "51.5010,-0.1416");
});
