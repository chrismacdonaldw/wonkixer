import assert from "node:assert/strict";
import { test } from "node:test";
import { createHistory } from "../src/history.ts";

test("undo and redo walk the stack and new pushes clear redo", () => {
  const history = createHistory();
  history.push("a");
  history.push("b");
  assert.equal(history.undoDepth(), 2);
  assert.equal(history.undo("c"), "b");
  assert.equal(history.redoDepth(), 1);
  assert.equal(history.redo("b"), "c");
  assert.equal(history.undo("c"), "b");
  history.push("d");
  assert.equal(history.redoDepth(), 0);
  assert.equal(history.undo("e"), "d");
  assert.equal(history.undo("d"), "a");
  assert.equal(history.undo("a"), null);
});

test("the stack keeps the last hundred entries and can drop a cancelled one", () => {
  const history = createHistory(100);
  for (let i = 0; i < 120; i++) history.push(i);
  assert.equal(history.undoDepth(), 100);
  history.dropLast();
  assert.equal(history.undoDepth(), 99);
  assert.equal(history.undo(999), 118);
});
