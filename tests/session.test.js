import assert from "node:assert/strict";
import { test } from "node:test";
import { createSession } from "../src/session.ts";
import { canCompare, canSave, isEdited, onCount, visibleStages } from "../src/session-state.ts";
import { defaults, fakeEngine, schema, withValues } from "./helpers.js";

function session(options = {}) {
  const engine = fakeEngine();
  const active = options.active ?? { kind: "builtin", name: "Headset clean-up" };
  const created = createSession(engine.api, schema, {
    document: options.document ?? defaults(),
    activePreset: active,
    activeDocument: options.activeDocument ?? defaults(),
    catalog: { builtin: ["Flat", "Headset clean-up"], user: [{ name: "Discord", modifiedMs: 1 }] },
  });
  return { engine, session: created };
}

test("single edits reach the engine as set_param and set_bypass", () => {
  const { engine, session: s } = session();
  s.setValue("compressor.ratio", 4);
  s.toggleBypass("eq");
  assert.deepEqual(engine.calls, [
    ["set_param", "compressor.ratio", 4],
    ["set_bypass", "eq", true],
  ]);
  assert.equal(isEdited(schema, s.store.get()), true);
  assert.equal(s.store.get().undoDepth, 2);
});

test("drags record one history entry at the start and esc restores the origin", () => {
  const { engine, session: s } = session();
  s.record();
  s.setValue("input.gain", 1, false);
  s.setValue("input.gain", 2, false);
  assert.equal(s.store.get().undoDepth, 1);
  s.restoreValue("input.gain", 0);
  assert.equal(s.store.get().undoDepth, 0);
  assert.equal(s.current("input.gain"), 0);
  assert.deepEqual(engine.of("set_param").map((call) => call[2]), [1, 2, 0]);
});

test("undo and redo restore values, bypass and added stages", () => {
  const { session: s } = session();
  s.setValue("compressor.makeup", 9);
  s.addStage("rnnoise");
  assert.equal(visibleStages(schema, s.store.get()).length, 9);
  s.undo();
  assert.equal(visibleStages(schema, s.store.get()).length, 8);
  assert.equal(s.store.get().bypassed.rnnoise, true);
  s.undo();
  assert.equal(s.current("compressor.makeup"), 6);
  assert.equal(s.store.get().redoDepth, 2);
  s.redo();
  assert.equal(s.current("compressor.makeup"), 9);
});

test("applying a preset document swaps everything with set_params and marks it active", () => {
  const { engine, session: s } = session();
  s.applyDocument(withValues({ "compressor.threshold": -24, "eq.presence.gain": 3 }, { rnnoise: false }), { kind: "user", name: "Discord" });
  const state = s.store.get();
  assert.equal(state.activePreset.name, "Discord");
  assert.equal(state.added.rnnoise, true);
  assert.equal(isEdited(schema, state), false);
  assert.equal(engine.of("set_params").length, 1);
  assert.equal(engine.of("set_params")[0][1].values["compressor.threshold"], -24);
  assert.equal(onCount(schema, state).total, 7);
  s.setValue("compressor.threshold", -20);
  assert.equal(isEdited(schema, s.store.get()), true);
  assert.equal(canSave(s.store.get(), true), true);
});

test("compare swaps the engine to the saved document and any edit returns to the edits", () => {
  const { engine, session: s } = session();
  s.setValue("compressor.ratio", 5);
  assert.equal(canCompare(s.store.get(), true), true);
  engine.clear();
  s.toggleCompare();
  assert.equal(s.current("compressor.ratio"), 3);
  assert.equal(engine.calls[0][0], "set_params");
  assert.equal(engine.calls[0][1].values["compressor.ratio"], 3);
  s.setValue("compressor.ratio", 6);
  assert.equal(s.store.get().compare, null);
  assert.deepEqual(engine.calls[1], ["set_param", "compressor.ratio", 6]);
  s.toggleCompare();
  s.toggleCompare();
  assert.equal(s.current("compressor.ratio"), 6);
});

test("hear dry bypasses every bypassable stage in the engine without touching the saved state", () => {
  const { engine, session: s } = session();
  engine.clear();
  s.toggleDry();
  const sent = engine.of("set_params")[0][1];
  assert.equal(sent.bypassed.compressor, true);
  assert.equal(sent.bypassed.input, false);
  assert.equal(s.store.get().bypassed.compressor, false);
  assert.equal(isEdited(schema, s.store.get()), false);
  engine.clear();
  s.toggleDry();
  assert.equal(engine.of("set_params")[0][1].bypassed.compressor, false);
});

test("optional stages are added and removed through bypass and selection", () => {
  const { engine, session: s } = session();
  engine.clear();
  s.addStage("rnnoise");
  assert.deepEqual(engine.calls, [["set_bypass", "rnnoise", false]]);
  assert.equal(s.store.get().selection, "rnnoise");
  s.removeStage("rnnoise");
  assert.equal(s.store.get().selection, "library");
  assert.equal(s.store.get().bypassed.rnnoise, true);
});

test("reset stage returns every parameter of the stage to its default", () => {
  const { session: s } = session();
  s.setValues({ "compressor.ratio": 8, "compressor.makeup": 12 });
  s.resetStage("compressor");
  assert.equal(s.current("compressor.ratio"), 3);
  assert.equal(s.current("compressor.makeup"), 6);
  assert.equal(s.current("input.gain"), 0);
});
