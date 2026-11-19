import assert from "node:assert/strict";
import { test } from "node:test";
import { compass, describeCondition, SCENE_KINDS, sceneFor } from "../src/conditions.js";

test("every supported WMO code maps to a known scene kind", () => {
  const codes = [0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82, 85, 86, 95, 96, 99];
  for (const code of codes) {
    const condition = describeCondition(code);
    assert.ok(condition.known, `code ${code} is known`);
    assert.ok(SCENE_KINDS.includes(condition.kind), `code ${code} has kind ${condition.kind}`);
    assert.notEqual(condition.label, "");
  }
});

test("codes group into clear, cloud, rain, snow and storm scenes", () => {
  assert.equal(describeCondition(0).kind, "clear");
  assert.equal(describeCondition(2).kind, "partly");
  assert.equal(describeCondition(3).kind, "cloudy");
  assert.equal(describeCondition(45).kind, "fog");
  assert.equal(describeCondition(61).kind, "rain");
  assert.equal(describeCondition(80).kind, "rain");
  assert.equal(describeCondition(73).kind, "snow");
  assert.equal(describeCondition(85).kind, "snow");
  assert.equal(describeCondition(95).kind, "storm");
  assert.equal(describeCondition(99).kind, "storm");
});

test("rain and snow intensity rises with the code", () => {
  assert.ok(describeCondition(61).intensity < describeCondition(65).intensity);
  assert.ok(describeCondition(71).intensity < describeCondition(75).intensity);
});

test("an unknown code is labelled unavailable instead of invented", () => {
  for (const code of [null, undefined, 4, 1000]) {
    const condition = describeCondition(code);
    assert.equal(condition.known, false);
    assert.equal(condition.label, "Conditions unavailable");
  }
});

test("sceneFor selects day or night", () => {
  assert.deepEqual(
    { kind: sceneFor(0, true).kind, time: sceneFor(0, true).time },
    { kind: "clear", time: "day" },
  );
  assert.equal(sceneFor(95, false).time, "night");
});

test("compass names the 16 points and wraps", () => {
  assert.equal(compass(0), "N");
  assert.equal(compass(225), "SW");
  assert.equal(compass(359), "N");
  assert.equal(compass(-90), "W");
  assert.equal(compass(null), "");
  assert.equal(compass(Number.NaN), "");
});
