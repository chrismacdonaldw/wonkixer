import assert from "node:assert/strict";
import { test } from "node:test";
import { POSITIONS, fromPosition, snap, toPosition } from "../src/param-scale.ts";

const GAIN = { id: "input.gain", stage: "input", key: "input.gain", label: "Gain", unit: "decibels", scale: "linear", min: -24, max: 24, default: 0, step: 0.5 };
const CUTOFF = { id: "hpf.cutoff", stage: "hpf", key: "hpf.cutoff", label: "Cutoff", unit: "hertz", scale: "logarithmic", min: 40, max: 300, default: 90, step: 1 };
const HOLD = { id: "expander.hold", stage: "expander", key: "expander.hold", label: "Hold", unit: "milliseconds", scale: "logarithmic", min: 0, max: 500, default: 80, step: 0.1 };

test("linear parameters map proportionally onto the slider", () => {
  assert.equal(toPosition(GAIN, -24), 0);
  assert.equal(toPosition(GAIN, 0), POSITIONS / 2);
  assert.equal(toPosition(GAIN, 24), POSITIONS);
  assert.equal(fromPosition(GAIN, POSITIONS / 2), 0);
  assert.equal(fromPosition(GAIN, POSITIONS), 24);
  assert.equal(fromPosition(GAIN, 510), 0.5);
});

test("logarithmic parameters give the low end more travel and round trip", () => {
  assert.equal(toPosition(CUTOFF, 40), 0);
  assert.equal(toPosition(CUTOFF, 300), POSITIONS);
  assert.ok(toPosition(CUTOFF, 90) < POSITIONS / 2);
  assert.equal(fromPosition(CUTOFF, toPosition(CUTOFF, 90)), 90);
  assert.equal(fromPosition(CUTOFF, 0), 40);
  assert.equal(fromPosition(CUTOFF, POSITIONS), 300);
});

test("a logarithmic range that starts at zero falls back to linear travel", () => {
  assert.equal(toPosition(HOLD, 250), POSITIONS / 2);
  assert.equal(fromPosition(HOLD, POSITIONS / 2), 250);
});

test("snap rounds to the step, trims float noise and clamps", () => {
  assert.equal(snap(GAIN, 3.26), 3.5);
  assert.equal(snap(GAIN, 99), 24);
  assert.equal(snap(HOLD, 12.34), 12.3);
  assert.equal(snap(CUTOFF, 0.2 + 0.1 + 89), 89);
});
