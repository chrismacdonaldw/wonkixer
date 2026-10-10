import assert from "node:assert/strict";
import { test } from "node:test";
import { checkPresetName, duplicateName, presetFilePath, suggestedSaveName } from "../src/presets-logic.ts";

const context = { builtin: ["Flat", "Podcast"], user: ["Discord"], mode: "save" };

test("preset names are validated the way the dialog explains them", () => {
  assert.equal(checkPresetName("", context).note, "Give it a name.");
  assert.equal(checkPresetName("   ", context).blocked, true);
  assert.match(checkPresetName("Podcast", context).note, /built-in/);
  assert.equal(checkPresetName("Podcast", context).tone, "bad");
  assert.match(checkPresetName("a/b", context).note, /cannot contain/);
  const replace = checkPresetName("Discord", context);
  assert.equal(replace.blocked, false);
  assert.equal(replace.replaces, true);
  assert.equal(replace.confirmText, "REPLACE");
  assert.equal(replace.tone, "warn");
  const fresh = checkPresetName(" Stream ", context);
  assert.equal(fresh.trimmed, "Stream");
  assert.equal(fresh.confirmText, "SAVE");
  assert.match(fresh.note, /Saves every effect/);
});

test("renaming to the same name is allowed and labelled", () => {
  const rename = checkPresetName("Discord", { ...context, mode: "rename", from: "Discord" });
  assert.equal(rename.replaces, false);
  assert.equal(rename.confirmText, "RENAME");
  assert.match(rename.note, /Renames the file/);
});

test("names for copies, suggestions and paths", () => {
  assert.equal(duplicateName("Discord", ["Discord"]), "Discord copy");
  assert.equal(duplicateName("Discord", ["Discord", "Discord copy"]), "Discord copy 2");
  assert.equal(suggestedSaveName("Podcast"), "Podcast (mine)");
  assert.equal(suggestedSaveName(null), "My preset (mine)");
  assert.equal(presetFilePath("Stream"), "%APPDATA%\\com.chrismacdonaldw.wonkixer\\presets\\Stream.json");
  assert.equal(presetFilePath(""), "%APPDATA%\\com.chrismacdonaldw.wonkixer\\presets\\….json");
});
