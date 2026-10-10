import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { startApp } from "../src/app.ts";
import { BROADCAST, CABLE, MIC, RUNNING, fakeEngine, fakeTimers, frame, html, settle, withValues } from "./helpers.js";

let dom;

beforeEach(() => {
  dom = new JSDOM(html, { pretendToBeVisual: true });
});

afterEach(() => {
  dom.window.close();
});

function query(selector) {
  return dom.window.document.querySelector(selector);
}

function all(selector) {
  return [...dom.window.document.querySelectorAll(selector)];
}

async function boot(options = {}) {
  const engine = fakeEngine(options);
  const timers = fakeTimers();
  const app = await startApp(engine.api, dom.window.document, { timers, now: () => 0 });
  await settle();
  engine.clear();
  return { engine, timers, app };
}

function key(target, name, init = {}) {
  target.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...init }));
}

test("chain rows render from the schema with the optional stage hidden and the compressor selected", async () => {
  await boot();
  const labels = all(".chain-label").map((label) => label.textContent);
  assert.deepEqual(labels, ["INPUT", "HIGH-PASS", "EXPANDER", "DE-ESSER", "EQ", "COMPRESSOR", "LIMITER", "OUTPUT"]);
  assert.equal(query(".chain-item.is-selected .chain-label").textContent, "COMPRESSOR");
  assert.equal(query("#page-title").textContent, "Compressor");
  assert.equal(query("#page-kicker").textContent, "STAGE 06 OF 08 · DYNAMICS");
  assert.equal(query("#chain-count").textContent, "6/6 ON");
  assert.equal(all(".chain-power").length, 6);
  assert.equal(query('.chain-row[data-stage="compressor"] .chain-value').textContent, "3.0:1");
  assert.equal(query('.chain-row[data-stage="eq"] .chain-value').textContent, "4 BANDS");
  assert.equal(query("#preset-name").textContent, "Headset clean-up");
  assert.equal(query("#preset-edited").hidden, true);
  assert.equal(all(".vbox").length, 6);
  assert.equal(query(".section-detail .field-grid").hidden, true);
  assert.match(query(".disclosure-summary").textContent, /Attack 8.0 ms/);
});

test("selecting a stage shows its page by family", async () => {
  const { engine } = await boot();
  query('.chain-row[data-stage="eq"]').click();
  assert.equal(query("#page-title").textContent, "EQ");
  assert.equal(all(".graph-handle").length, 4);
  assert.equal(all(".section-band").length, 4);
  query('.chain-row[data-stage="input"]').click();
  assert.equal(query(".chart-level") !== null, true);
  assert.equal(query(".level-hint").textContent, "Start the engine to see your levels.");
  query('.chain-row[data-stage="hpf"]').click();
  assert.equal(query(".chart-tag").textContent, "90 Hz · -3 dB");
  query('.chain-row[data-stage="deesser"]').click();
  assert.equal(query(".chart-band-tag").hidden, false);
  assert.equal(query("[role=radiogroup]") !== null, true);
  query("#add-effect").click();
  assert.equal(query("#page-title").textContent, "Add an effect");
  assert.equal(all(".library-card").length, 7);
  query(".library-card.is-available .button-primary").click();
  assert.equal(query("#page-title").textContent, "Noise suppression");
  assert.deepEqual(engine.of("set_bypass"), [["set_bypass", "rnnoise", false]]);
  assert.equal(all(".chain-label").length, 9);
  assert.equal(query(".chain-badge").textContent, "+10 ms");
  query("#page-remove").click();
  assert.equal(all(".chain-label").length, 8);
  assert.equal(query("#page-title").textContent, "Add an effect");
});

test("value boxes send set_param on drag, typing and keys, and bypass sends set_bypass", async () => {
  const { engine } = await boot();
  const input = query('[data-param="compressor.ratio"] input');
  const Pointer = dom.window.PointerEvent ?? dom.window.MouseEvent;
  input.dispatchEvent(new Pointer("pointerdown", { bubbles: true, button: 0, pointerId: 1, clientX: 0, clientY: 0 }));
  input.dispatchEvent(new Pointer("pointermove", { bubbles: true, pointerId: 1, clientX: 30, clientY: 0 }));
  input.dispatchEvent(new Pointer("pointerup", { bubbles: true, pointerId: 1, clientX: 30, clientY: 0 }));
  assert.equal(engine.of("set_param").length, 1);
  assert.equal(engine.of("set_param")[0][1], "compressor.ratio");
  assert.ok(engine.of("set_param")[0][2] > 3);
  assert.equal(query("#preset-edited").hidden, false);
  assert.equal(query("#undo").disabled, false);
  engine.clear();
  key(input, "ArrowUp");
  assert.equal(engine.of("set_param").length, 1);
  engine.clear();
  key(input, "Enter");
  input.value = "4:1";
  key(input, "Enter");
  assert.deepEqual(engine.of("set_param"), [["set_param", "compressor.ratio", 4]]);
  assert.equal(query('.chain-row[data-stage="compressor"] .chain-value').textContent, "4.0:1");
  engine.clear();
  query("#page-bypass").click();
  assert.deepEqual(engine.of("set_bypass"), [["set_bypass", "compressor", true]]);
  assert.equal(query("#page-bypass").getAttribute("aria-pressed"), "true");
  assert.equal(query('.chain-row[data-stage="compressor"] .chain-value').textContent, "OFF");
  query('.chain-item.is-selected .chain-power').click();
  assert.equal(engine.of("set_bypass").at(-1)[2], false);
  for (let i = 0; i < 5; i++) query("#undo").click();
  assert.equal(query("#preset-edited").hidden, true);
  assert.equal(query("#redo").disabled, false);
});

test("hear dry and compare drive the engine without changing the session", async () => {
  const { engine } = await boot();
  query("#hear-dry").click();
  assert.equal(query("#hear-dry").getAttribute("aria-pressed"), "true");
  assert.equal(query("#chain-count").textContent, "0/6 ON");
  assert.equal(query('.chain-row[data-stage="eq"] .chain-value').textContent, "DRY");
  assert.equal(engine.of("set_params")[0][1].bypassed.eq, true);
  query("#hear-dry").click();
  assert.equal(query("#chain-count").textContent, "6/6 ON");
  assert.equal(query("#compare").disabled, true);
  engine.clear();
  const input = query('[data-param="compressor.makeup"] input');
  key(input, "End");
  assert.equal(query("#compare").disabled, false);
  query("#compare").click();
  assert.equal(query("#compare-banner").hidden, false);
  assert.equal(engine.of("set_params").at(-1)[1].values["compressor.makeup"], 6);
  assert.equal(input.value, "6.0");
  query("#compare-back").click();
  assert.equal(query("#compare-banner").hidden, true);
  assert.equal(input.value, "24.0");
});

test("the presets dialog lists built-ins and yours, picks, saves as and validates names", async () => {
  const { engine } = await boot({ user: [["Discord", withValues({ "compressor.ratio": 3.5 })]] });
  query("#preset-open").click();
  assert.equal(query("#presets-dialog").hidden, false);
  assert.deepEqual(all("#presets-builtin .preset-row-name").map((row) => row.textContent), ["Flat", "Headset clean-up", "Podcast"]);
  assert.deepEqual(all("#presets-user .preset-row-name").map((row) => row.textContent), ["Discord"]);
  assert.equal(query("#presets-user-count").textContent, "1");
  assert.equal(query("#preset-save").disabled, true);
  assert.equal(query("#presets-builtin .preset-row.is-active .preset-row-name").textContent, "Headset clean-up");

  all("#presets-user .preset-row")[0].click();
  await settle();
  assert.deepEqual(engine.of("load_preset"), [["load_preset", "user", "Discord"]]);
  assert.equal(engine.of("set_params").length, 1);
  assert.equal(query("#preset-name").textContent, "Discord");
  assert.equal(query("#presets-dialog").hidden, true);
  assert.equal(query('.chain-row[data-stage="compressor"] .chain-value').textContent, "3.5:1");

  key(query('[data-param="compressor.ratio"] input'), "PageUp");
  assert.equal(query("#quick-save").textContent, "SAVE");
  query("#preset-open").click();
  assert.equal(query("#preset-save").disabled, false);
  assert.equal(query("#preset-revert").hidden, false);
  assert.equal(query("#presets-user .preset-tag").textContent, "EDITED");
  engine.clear();
  query("#preset-save").click();
  await settle();
  assert.equal(engine.of("save_user_preset")[0][1], "Discord");
  assert.equal(engine.of("save_user_preset")[0][2].values["compressor.ratio"], 4);
  assert.equal(query("#preset-edited").hidden, true);
  assert.match(query("#presets-note").textContent, /Saved Discord/);

  query("#preset-save-as").click();
  assert.equal(query("#name-dialog").hidden, false);
  const name = query("#preset-name-input");
  assert.equal(name.value, "Discord (mine)");
  name.value = "Podcast";
  name.dispatchEvent(new dom.window.Event("input"));
  assert.equal(query("#dialog-confirm").disabled, true);
  assert.match(query("#dialog-note").textContent, /built-in/);
  name.value = "Dis|cord";
  name.dispatchEvent(new dom.window.Event("input"));
  assert.match(query("#dialog-note").textContent, /cannot contain/);
  name.value = "Discord";
  name.dispatchEvent(new dom.window.Event("input"));
  assert.equal(query("#dialog-confirm").textContent, "REPLACE");
  name.value = "Stream";
  name.dispatchEvent(new dom.window.Event("input"));
  assert.equal(query("#dialog-confirm").disabled, false);
  assert.equal(query("#dialog-path").textContent, "%APPDATA%\\com.chrismacdonaldw.wonkixer\\presets\\Stream.json");
  engine.clear();
  key(name, "Enter");
  await settle();
  assert.equal(engine.of("save_user_preset")[0][1], "Stream");
  assert.equal(query("#name-dialog").hidden, true);
  assert.equal(query("#preset-name").textContent, "Stream");

  query("#preset-open").click();
  engine.clear();
  query('[aria-label="Delete Stream"]').click();
  await settle();
  assert.deepEqual(engine.of("delete_user_preset"), [["delete_user_preset", "Stream"]]);
  assert.equal(query("#preset-name").textContent, "Unsaved settings");
  assert.match(query("#presets-note").textContent, /Recycle Bin/);
  query('[aria-label="Rename Discord"]').click();
  assert.equal(query("#dialog-title").textContent, "Rename preset");
  query("#preset-name-input").value = "Chat";
  key(query("#preset-name-input"), "Enter");
  await settle();
  assert.deepEqual(engine.of("rename_user_preset"), [["rename_user_preset", "Discord", "Chat"]]);
  query("#preset-import").click();
  query("#preset-export").click();
  query("#presets-folder").click();
  await settle();
  assert.equal(engine.of("import_preset").length, 1);
  assert.equal(engine.of("export_preset").length, 1);
  assert.equal(engine.of("open_presets_folder").length, 1);
});

test("preferences restore the session and devices, then save after a debounce and on flush", async () => {
  const preferences = {
    version: 1,
    devices: { inputId: BROADCAST.id, outputId: CABLE.id },
    autoStart: false,
    session: withValues({ "compressor.threshold": -21 }, { rnnoise: false }),
    activePreset: { kind: "builtin", name: "Podcast" },
  };
  const engine = fakeEngine({ preferences });
  const timers = fakeTimers();
  const app = await startApp(engine.api, dom.window.document, { timers, now: () => 0 });
  await settle();
  assert.equal(engine.of("set_params")[0][1].values["compressor.threshold"], -21);
  assert.equal(query("#input-device").value, BROADCAST.id);
  assert.equal(all(".chain-label").length, 9);
  assert.equal(query("#preset-name").textContent, "Podcast");
  assert.equal(query("#preset-edited").hidden, false);
  assert.equal(engine.of("save_preferences").length, 0);

  key(query('[data-param="compressor.ratio"] input'), "ArrowUp");
  key(query('[data-param="compressor.ratio"] input'), "ArrowUp");
  timers.advance(299);
  await settle();
  assert.equal(engine.of("save_preferences").length, 0);
  timers.advance(1);
  await settle();
  assert.equal(engine.of("save_preferences").length, 1);
  const saved = engine.of("save_preferences")[0][1];
  assert.equal(saved.session.values["compressor.ratio"], 3.2);
  assert.equal(saved.session.bypassed.rnnoise, false);
  assert.deepEqual(saved.devices, { inputId: BROADCAST.id, outputId: CABLE.id });
  assert.deepEqual(saved.activePreset, { kind: "builtin", name: "Podcast" });

  query("#output-device").value = "out:headphones";
  query("#output-device").dispatchEvent(new dom.window.Event("change"));
  await app.flush();
  assert.equal(engine.of("save_preferences").length, 2);
  assert.equal(engine.of("save_preferences")[1][1].devices.outputId, "out:headphones");
});

test("meter frames drive the chain meters, readouts, GR and footer", async () => {
  const { engine } = await boot();
  engine.emitState(RUNNING);
  engine.emitMeters(frame({ inputPeakDb: -12, outputPeakDb: -6, clipped: true, latencyMs: 31.4, dropouts: 2 }));
  assert.equal(parseFloat(query(".chain-item.is-level .chain-cover").style.width), 20);
  assert.equal(query('.chain-row[data-stage="compressor"] .chain-gr-text').textContent, "-4.0");
  assert.equal(query("#page-gr").textContent, "GR -4.0 dB");
  assert.equal(query(".chain-clip").classList.contains("is-clipped"), true);
  assert.equal(query("#readout-latency").textContent, "31 ms");
  assert.equal(query("#readout-dropouts").textContent, "2");
  assert.equal(query("#readout-rate").textContent, "48 000 Hz");
  assert.equal(query('[data-param="compressor.threshold"] .vbox-level').style.opacity, "0.9");
  assert.equal(query(".transfer-dot").hidden, false);
  query(".chain-clip").click();
  assert.equal(query(".chain-clip").classList.contains("is-clipped"), false);
  query("#shortcuts").click();
  assert.equal(query("#keys-dialog").hidden, false);
  assert.equal(all(".keys-row").length, 10);
  engine.emitState({ ...RUNNING, running: false, sampleRate: null });
  assert.equal(query("#readout-rate").textContent, "—");
  assert.equal(query("#page-gr").textContent, "GR 0.0 dB");
});
