import assert from "node:assert/strict";
import { test } from "node:test";
import { createPreferencesSaver } from "../src/preferences.ts";
import { fakeTimers, settle } from "./helpers.js";

test("saves are debounced and only the last scheduled preferences are written", async () => {
  const timers = fakeTimers();
  const saved = [];
  const saver = createPreferencesSaver(async (preferences) => saved.push(preferences), timers, 300);
  saver.schedule({ version: 1, autoStart: true });
  timers.advance(200);
  saver.schedule({ version: 2, autoStart: true });
  timers.advance(200);
  assert.deepEqual(saved, []);
  timers.advance(100);
  await settle();
  assert.deepEqual(saved, [{ version: 2, autoStart: true }]);
});

test("flush writes pending preferences immediately and cancels the timer", async () => {
  const timers = fakeTimers();
  const saved = [];
  const saver = createPreferencesSaver(async (preferences) => saved.push(preferences), timers, 300);
  saver.schedule({ version: 3 });
  await saver.flush();
  assert.deepEqual(saved, [{ version: 3 }]);
  assert.equal(timers.pending(), 0);
  await saver.flush();
  assert.equal(saved.length, 1);
});

test("a save failure does not block later saves", async () => {
  const timers = fakeTimers();
  const saved = [];
  let fail = true;
  const saver = createPreferencesSaver(
    async (preferences) => {
      if (fail) throw new Error("disk");
      saved.push(preferences);
    },
    timers,
    10,
  );
  saver.schedule({ version: 1 });
  timers.advance(10);
  await settle();
  fail = false;
  saver.schedule({ version: 2 });
  timers.advance(10);
  await settle();
  assert.deepEqual(saved, [{ version: 2 }]);
});
