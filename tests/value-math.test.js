import assert from "node:assert/strict";
import { test } from "node:test";
import { dragSpace, pageStep, parseInput, snapValue, stepped, wheelSteps } from "../src/value-math.ts";
import { spec } from "./helpers.js";

const GAIN = spec("input.gain");
const THRESHOLD = spec("compressor.threshold");
const RATIO = spec("compressor.ratio");
const CUTOFF = spec("hpf.cutoff");
const FREQUENCY = spec("deesser.frequency");
const ATTACK = spec("compressor.attack");
const HOLD = spec("expander.hold");
const Q = spec("eq.lowMid.q");

test("typing understands units, thousands and ratios per unit", () => {
  assert.equal(parseInput(GAIN, "-3"), -3);
  assert.equal(parseInput(GAIN, "+4.5"), 4.5);
  assert.equal(parseInput(GAIN, "-3dB"), -3);
  assert.equal(parseInput(GAIN, "2,5"), 2.5);
  assert.equal(parseInput(THRESHOLD, "18"), -18);
  assert.equal(parseInput(THRESHOLD, "+0"), 0);
  assert.equal(parseInput(RATIO, "4"), 4);
  assert.equal(parseInput(RATIO, "4:1"), 4);
  assert.equal(parseInput(spec("expander.ratio"), "1:2.5"), 2.5);
  assert.equal(parseInput(FREQUENCY, "6500"), 6500);
  assert.equal(parseInput(FREQUENCY, "6.5k"), 6500);
  assert.equal(parseInput(FREQUENCY, "6k5"), 6500);
  assert.equal(parseInput(FREQUENCY, "6.5kHz"), 6500);
  assert.equal(parseInput(FREQUENCY, "6500 Hz"), 6500);
  assert.equal(parseInput(ATTACK, "12"), 12);
  assert.equal(parseInput(ATTACK, "12ms"), 12);
  assert.equal(parseInput(ATTACK, "0.05s"), 50);
  assert.equal(parseInput(Q, "0.71"), 0.71);
  assert.equal(parseInput(GAIN, "abc"), null);
  assert.equal(parseInput(GAIN, ""), null);
  assert.equal(parseInput(GAIN, "99"), 24);
});

test("snapping uses the fine step when fine and the coarse step otherwise", () => {
  assert.equal(snapValue(GAIN, 3.26, false), 3.5);
  assert.equal(snapValue(GAIN, 3.26, true), 3.3);
  assert.equal(snapValue(Q, 0.713, true), 0.71);
  assert.equal(snapValue(Q, 0.713, false), 0.7);
  assert.equal(snapValue(GAIN, 99, true), 24);
});

test("arrow steps are per unit and never stall", () => {
  assert.equal(stepped(GAIN, 0, 1, false), 0.5);
  assert.equal(stepped(GAIN, 0, 1, true), 0.1);
  assert.equal(stepped(RATIO, 3, 1, false), 3.1);
  assert.equal(stepped(CUTOFF, 90, 1, false), 95);
  assert.equal(stepped(CUTOFF, 90, 1, true), 91);
  assert.equal(stepped(ATTACK, 8, 1, false), 8.4);
  assert.equal(stepped(ATTACK, 8, -1, true), 7.9);
  assert.equal(stepped(Q, 1, 1, false), 1.05);
  assert.equal(stepped(Q, 1, 1, true), 1.01);
  assert.equal(stepped(GAIN, 24, 1, false), 24);
});

test("page steps jump to the next useful value per unit", () => {
  assert.equal(pageStep(RATIO, 3, 1), 4);
  assert.equal(pageStep(RATIO, 3, -1), 2);
  assert.equal(pageStep(ATTACK, 8, 1), 10);
  assert.equal(pageStep(ATTACK, 8, -1), 5);
  assert.equal(pageStep(CUTOFF, 90, 1), 100);
  assert.equal(pageStep(FREQUENCY, 6500, 1), 8000);
  assert.equal(pageStep(Q, 1, 1), 1.4);
  assert.equal(pageStep(THRESHOLD, -18, 1), -12);
  assert.equal(pageStep(GAIN, 0, -1), -3);
  assert.equal(pageStep(RATIO, 20, 1), 20);
});

test("drag spaces give each unit its own travel and map both ways", () => {
  const gain = dragSpace(GAIN);
  assert.equal(gain.range, 48 * 8);
  assert.equal(gain.toUnit(0), 0.5);
  assert.equal(gain.fromUnit(1), 24);
  const threshold = dragSpace(THRESHOLD);
  assert.equal(threshold.range, 60 * 6);
  const ratio = dragSpace(RATIO);
  assert.equal(ratio.range, 300);
  assert.ok(ratio.toUnit(4) < 0.5);
  assert.ok(Math.abs(ratio.fromUnit(ratio.toUnit(4)) - 4) < 1e-9);
  const cutoff = dragSpace(CUTOFF);
  assert.equal(cutoff.range, Math.max(120, Math.log2(300 / 40) * 36));
  assert.equal(dragSpace(ATTACK).range, Math.log10(100 / 0.1) * 75);
  assert.equal(dragSpace(HOLD).range, 250);
  assert.equal(dragSpace(HOLD).fromUnit(0.5), 250);
  assert.equal(dragSpace(Q).range, 250);
});

test("wheel deltas add up until a notch is reached", () => {
  assert.deepEqual(wheelSteps(0, -30, 50), { steps: 0, remainder: -30 });
  assert.deepEqual(wheelSteps(-30, -30, 50), { steps: 1, remainder: -10 });
  assert.deepEqual(wheelSteps(0, 120, 50), { steps: -2, remainder: 20 });
  assert.deepEqual(wheelSteps(0, -1, 1), { steps: 1, remainder: 0 });
});
