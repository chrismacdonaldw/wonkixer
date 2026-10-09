import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { STOPPED } from "../src/engine.ts";
import { initEnginePanel, pillStateFor, preferredInput, preferredOutput } from "../src/engine-panel.ts";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");

const MIC = { id: "in:mic", name: "Microphone (HyperX)", isDefault: true, sampleRate: 48000, channels: 1 };
const BROADCAST = { id: "in:broadcast", name: "Microphone (NVIDIA Broadcast)", isDefault: false, sampleRate: 48000, channels: 2 };
const HEADPHONES = { id: "out:headphones", name: "Headphones (HyperX)", isDefault: true, sampleRate: 48000, channels: 2 };
const CABLE = { id: "out:cable", name: "CABLE Input (VB-Audio Virtual Cable)", isDefault: false, sampleRate: 48000, channels: 2 };
const DEVICES = { inputs: [BROADCAST, MIC], outputs: [HEADPHONES, CABLE] };
const RUNNING = { running: true, input: MIC, output: CABLE, sampleRate: 48000, error: null };

let dom;

beforeEach(() => {
  dom = new JSDOM(html);
});

afterEach(() => {
  dom.window.close();
});

function fakeEngine({ devices = DEVICES, start = async () => RUNNING } = {}) {
  const calls = [];
  let stateHandler = () => undefined;
  return {
    calls,
    emitState: (state) => stateHandler(state),
    api: {
      listDevices: async () => devices,
      start: async (inputId, outputId) => {
        calls.push(["start", inputId, outputId]);
        return start();
      },
      stop: async () => {
        calls.push(["stop"]);
        return STOPPED;
      },
      state: async () => STOPPED,
      onState: async (handler) => {
        stateHandler = handler;
        return () => undefined;
      },
      onMeters: async () => () => undefined,
    },
  };
}

function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function query(selector) {
  return dom.window.document.querySelector(selector);
}

test("device pickers list devices and preselect the default mic and the virtual cable", async () => {
  const engine = fakeEngine();
  await initEnginePanel(engine.api, dom.window.document);

  const inputs = Array.from(query("#input-device").options).map((option) => option.textContent);
  assert.deepEqual(inputs, [BROADCAST.name, MIC.name]);
  assert.equal(query("#input-device").value, MIC.id);
  assert.equal(query("#output-device").value, CABLE.id);
  assert.equal(query("#engine-toggle").textContent, "START");
  assert.equal(query("#engine-pill").dataset.state, "stopped");
});

test("start sends the selected devices and renders the running state", async () => {
  const engine = fakeEngine();
  await initEnginePanel(engine.api, dom.window.document);
  query("#input-device").value = BROADCAST.id;

  query("#engine-toggle").click();
  await settle();

  assert.deepEqual(engine.calls, [["start", BROADCAST.id, CABLE.id]]);
  assert.equal(query("#engine-toggle").textContent, "STOP");
  assert.equal(query("#input-device").disabled, true);
  assert.equal(query("#engine-pill").dataset.state, "live");
  assert.equal(query("#engine-pill-label").textContent, "LIVE");
  assert.equal(query("#engine-route").textContent, `${MIC.name} → ${CABLE.name}`);
  assert.equal(query("#input-device").value, MIC.id);
});

test("a failed start shows the message and marks the pill as an error", async () => {
  const engine = fakeEngine({ start: async () => Promise.reject(new Error("Could not open the input device")) });
  await initEnginePanel(engine.api, dom.window.document);

  query("#engine-toggle").click();
  await settle();

  assert.equal(query("#engine-error").hidden, false);
  assert.equal(query("#engine-error").textContent, "Could not open the input device");
  assert.equal(query("#engine-pill").dataset.state, "error");
  assert.equal(query("#engine-toggle").textContent, "START");
});

test("stop is sent while running and state events re-render the panel", async () => {
  const engine = fakeEngine();
  await initEnginePanel(engine.api, dom.window.document);

  engine.emitState(RUNNING);
  assert.equal(query("#engine-toggle").textContent, "STOP");

  query("#engine-toggle").click();
  await settle();

  assert.deepEqual(engine.calls, [["stop"]]);
  assert.equal(query("#engine-toggle").textContent, "START");
  assert.equal(query("#engine-route").textContent, "");
});

test("preferred devices fall back to defaults when there is no cable", () => {
  assert.equal(preferredInput([BROADCAST, MIC]), MIC);
  assert.equal(preferredOutput([HEADPHONES]), HEADPHONES);
  assert.equal(preferredOutput([CABLE, HEADPHONES]), CABLE);
  assert.equal(preferredOutput([]), undefined);
  assert.equal(pillStateFor({ ...STOPPED, error: "boom" }), "error");
  assert.equal(pillStateFor(RUNNING), "live");
});
