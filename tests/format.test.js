import assert from "node:assert/strict";
import { test } from "node:test";
import { formatCount, formatDb, formatHz, formatMs, formatValue } from "../src/format.ts";

const spec = (overrides) => ({ id: "x", stage: "compressor", key: "x", label: "X", scale: "linear", min: 0, max: 1, default: 0, step: 1, ...overrides });

test("formatters round and group the way the readouts expect", () => {
  assert.equal(formatDb(-18.04), "-18.0 dB");
  assert.equal(formatMs(31.4), "31 ms");
  assert.equal(formatMs(0.5), "0.5 ms");
  assert.equal(formatHz(48000), "48 000 Hz");
  assert.equal(formatCount(1234567), "1 234 567");
  assert.equal(formatCount(999), "999");
});

test("parameter values format by unit and stage", () => {
  assert.equal(formatValue(spec({ unit: "decibels" }), -45), "-45.0 dB");
  assert.equal(formatValue(spec({ unit: "hertz" }), 6500), "6 500 Hz");
  assert.equal(formatValue(spec({ unit: "milliseconds" }), 120), "120 ms");
  assert.equal(formatValue(spec({ unit: "ratio" }), 3), "3.0:1");
  assert.equal(formatValue(spec({ unit: "ratio", stage: "expander" }), 2.5), "1:2.5");
  assert.equal(formatValue(spec({ unit: "q" }), 0.8), "0.80");
  assert.equal(formatValue(spec({ unit: "switch" }), 1), "ON");
  assert.equal(formatValue(spec({ unit: "switch" }), 0), "OFF");
});
