import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { STOPPED } from "../src/engine.ts";
import { initEnginePanel, pillStateFor, preferredInput, preferredOutput } from "../src/engine-panel.ts";
import { BROADCAST, CABLE, HEADPHONES, MIC, RUNNING, fakeEngine, html, settle } from "./helpers.js";

let dom;

beforeEach(() => {
  dom = new JSDOM(html);
});

afterEach(() => {
  dom.window.close();
});

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
  assert.equal(query("#engine-route-text").textContent, `${MIC.name}  →  ${CABLE.name}`);
  assert.equal(query("#devices-popover").hidden, true);
});

test("saved devices are restored and auto start launches the engine", async () => {
  const engine = fakeEngine();
  await initEnginePanel(engine.api, dom.window.document, {
    devices: { inputId: BROADCAST.id, outputId: HEADPHONES.id },
    autoStart: true,
  });

  assert.deepEqual(engine.of("start"), [["start", BROADCAST.id, HEADPHONES.id]]);
  assert.equal(query("#engine-toggle").textContent, "STOP");
});

test("auto start is skipped when a saved device is missing", async () => {
  const engine = fakeEngine();
  await initEnginePanel(engine.api, dom.window.document, { devices: { inputId: "gone", outputId: CABLE.id }, autoStart: true });

  assert.deepEqual(engine.of("start"), []);
  assert.equal(query("#input-device").value, MIC.id);
});

test("start sends the selected devices, locks the pickers and renders the running state", async () => {
  const engine = fakeEngine();
  const changes = [];
  await initEnginePanel(engine.api, dom.window.document, { onDevicesChange: (devices) => changes.push(devices) });
  query("#input-device").value = BROADCAST.id;
  query("#input-device").dispatchEvent(new dom.window.Event("change"));

  query("#engine-toggle").click();
  await settle();

  assert.deepEqual(changes, [{ inputId: BROADCAST.id, outputId: CABLE.id }]);
  assert.deepEqual(engine.of("start"), [["start", BROADCAST.id, CABLE.id]]);
  assert.equal(query("#engine-toggle").textContent, "STOP");
  assert.equal(query("#input-device").disabled, true);
  assert.equal(query("#engine-pill").dataset.state, "live");
  assert.equal(query("#engine-pill-label").textContent, "LIVE");
  assert.equal(query("#engine-route-text").textContent, `${MIC.name}  →  ${CABLE.name}`);
  assert.equal(query("#input-device").value, MIC.id);
});

test("the route button opens the devices popover with the locked hint while live", async () => {
  const engine = fakeEngine();
  await initEnginePanel(engine.api, dom.window.document);

  query("#engine-route").click();
  assert.equal(query("#devices-popover").hidden, false);
  assert.equal(query("#engine-route").getAttribute("aria-expanded"), "true");
  assert.match(query("#devices-hint").textContent, /CABLE Output/);

  engine.emitState(RUNNING);
  assert.match(query("#devices-hint").textContent, /locked/);

  dom.window.document.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: "Escape" }));
  assert.equal(query("#devices-popover").hidden, true);
});

test("a failed start shows the message in the popover and marks the pill as an error", async () => {
  const engine = fakeEngine({ start: async () => Promise.reject(new Error("Could not open the input device")) });
  await initEnginePanel(engine.api, dom.window.document);

  query("#engine-toggle").click();
  await settle();

  assert.equal(query("#devices-popover").hidden, false);
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

  assert.deepEqual(engine.of("stop"), [["stop"]]);
  assert.equal(query("#engine-toggle").textContent, "START");
});

test("preferred devices fall back to defaults when there is no cable", () => {
  assert.equal(preferredInput([BROADCAST, MIC]), MIC);
  assert.equal(preferredOutput([HEADPHONES]), HEADPHONES);
  assert.equal(preferredOutput([CABLE, HEADPHONES]), CABLE);
  assert.equal(preferredOutput([]), undefined);
  assert.equal(pillStateFor({ ...STOPPED, error: "boom" }), "error");
  assert.equal(pillStateFor(RUNNING), "live");
});
