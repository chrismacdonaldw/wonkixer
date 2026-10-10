import assert from "node:assert/strict";
import { test } from "node:test";
import {
  compressorInputFor,
  compressorOut,
  eqDb,
  expanderOut,
  frequencyToUnit,
  gainToUnit,
  highPassDb,
  historyPath,
  levelChartY,
  responsePath,
  transferPath,
  transferUnit,
  unitToFrequency,
  unitToGain,
} from "../src/curves.ts";
import { defaults, schema } from "./helpers.js";
import { eqShapeFor, responseFor } from "../src/stage-curves.ts";

test("the high-pass response is -3 dB at the cutoff and flat far above it", () => {
  assert.ok(Math.abs(highPassDb(90, 90) + 3.01) < 0.02);
  assert.ok(highPassDb(9000, 90) > -0.01);
  assert.ok(highPassDb(45, 90) < -12);
});

test("the eq response sums shelves and bells from the schema", () => {
  const shape = eqShapeFor(schema, "eq", defaults().values);
  assert.equal(shape.peaks.length, 2);
  assert.equal(shape.lowShelf.frequency, 120);
  assert.equal(shape.highShelf.frequency, 10000);
  assert.ok(Math.abs(eqDb(250, shape) - -2.5) < 0.6);
  assert.ok(eqDb(4000, shape) > 2);
  const flat = responseFor(schema, schema.stages.find((stage) => stage.id === "hpf"), defaults().values);
  assert.ok(Math.abs(flat(90) + 3.01) < 0.02);
});

test("compressor and expander transfer curves follow threshold, ratio and knee", () => {
  assert.equal(compressorOut(-40, -18, 3, 0), -40);
  assert.equal(compressorOut(0, -18, 3, 0), -12);
  assert.ok(Math.abs(compressorOut(-18, -18, 3, 6) - (-18 - 0.5)) < 1e-9);
  assert.equal(expanderOut(-20, -45, 2.5, 0), -20);
  assert.equal(expanderOut(-55, -45, 2.5, 0), -70);
  const input = compressorInputFor(4, -18, 3, 0);
  assert.ok(Math.abs(input - compressorOut(input, -18, 3, 0) - 4) < 1e-6);
});

test("chart coordinate helpers round trip", () => {
  assert.ok(Math.abs(unitToFrequency(frequencyToUnit(1000)) - 1000) < 1e-6);
  assert.equal(frequencyToUnit(20), 0);
  assert.equal(frequencyToUnit(20000), 1);
  assert.equal(gainToUnit(0), 0.5);
  assert.ok(Math.abs(unitToGain(gainToUnit(6)) - 6) < 1e-9);
  assert.equal(transferUnit(-80), 0);
  assert.equal(transferUnit(0), 1);
  assert.equal(levelChartY(0), 8);
  assert.equal(levelChartY(-60), 192);
});

test("paths are built from samples and close their fills", () => {
  const response = responsePath(() => 0, 600, 200, 4, 8);
  assert.equal(response.line.trim(), "M0.0 100.0 L150.0 100.0 L300.0 100.0 L450.0 100.0 L600.0 100.0");
  assert.ok(response.fill.endsWith("L600 100 L0 100 Z"));
  const history = historyPath([-20, -10], 600, (db) => -db, 200);
  assert.equal(history.line.trim(), "M589.8 20.0 L600.0 10.0");
  assert.ok(history.fill.endsWith("L600 200 L589.8 200 Z"));
  assert.deepEqual(historyPath([], 600, (db) => db, 200), { line: "", fill: "" });
  const transfer = transferPath((x) => x);
  assert.ok(transfer.startsWith("M0.0 200.0 "));
  assert.ok(transfer.trim().endsWith("L200.0 0.0"));
});
