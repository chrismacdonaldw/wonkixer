import assert from "node:assert/strict";
import { test } from "node:test";
import { METER_FLOOR_DB, PeakHold, bandFor, levelFraction } from "../src/meters.ts";

test("level fraction maps the dB scale onto 0..1 and clamps", () => {
  assert.equal(levelFraction(-60), 0);
  assert.equal(levelFraction(-30), 0.5);
  assert.equal(levelFraction(0), 1);
  assert.equal(levelFraction(-80), 0);
  assert.equal(levelFraction(6), 1);
});

test("bands split at -12 and -3 dBFS", () => {
  assert.equal(bandFor(-20), "signal");
  assert.equal(bandFor(-12), "hot");
  assert.equal(bandFor(-3), "clip");
});

test("peak hold jumps up, holds for a second, then falls at 12 dB per second", () => {
  const hold = new PeakHold();

  assert.equal(hold.update(-20, 0), -20);
  assert.equal(hold.update(-40, 500), -20);
  assert.equal(hold.update(-40, 1000), -20);
  assert.equal(hold.update(-40, 1500), -26);
  assert.equal(hold.update(-40, 4000), -40);
  assert.equal(hold.update(-10, 4100), -10);

  hold.reset();
  assert.equal(hold.update(METER_FLOOR_DB, 5000), METER_FLOOR_DB);
});
