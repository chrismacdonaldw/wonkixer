import assert from "node:assert/strict";
import { test } from "node:test";
import { METER_FLOOR_DB, PeakHold, bandFor, createMeterFeed, levelFraction } from "../src/meters.ts";
import { frame } from "./helpers.js";

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

test("the meter feed keeps a 6 s history at 10 Hz, latches clips and resets on stop", () => {
  let now = 0;
  const feed = createMeterFeed(() => now);
  for (let i = 0; i < 9; i++) {
    now += 33;
    feed.frame(frame({ inputPeakDb: -30 + i, clipped: i === 4, gainReduction: { compressor: i } }));
  }
  const state = feed.store.get();
  assert.equal(state.live, true);
  assert.deepEqual(state.history.input, [-28, -25, -22]);
  assert.deepEqual(state.history.reduction.compressor, [2, 5, 8]);
  assert.equal(state.clipLatched, true);
  assert.equal(state.inputHoldDb, -22);
  feed.clearClip();
  assert.equal(feed.store.get().clipLatched, false);
  for (let i = 0; i < 200; i++) feed.frame(frame());
  assert.equal(feed.store.get().history.input.length, 60);
  feed.state({ running: false, input: null, output: null, sampleRate: null, error: null });
  assert.equal(feed.store.get().live, false);
  assert.deepEqual(feed.store.get().history.input, []);
});
