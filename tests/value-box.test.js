import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { createValueBox } from "../src/value-box.ts";
import { spec } from "./helpers.js";

let dom;

beforeEach(() => {
  dom = new JSDOM("<!doctype html><body></body>", { pretendToBeVisual: true });
});

afterEach(() => {
  dom.window.close();
});

function host(initial) {
  const values = { ...initial };
  const calls = [];
  return {
    calls,
    values,
    spec: (id) => spec(id),
    current: (id) => values[id],
    record: () => calls.push(["record"]),
    setValue: (id, value, record = true) => {
      values[id] = value;
      calls.push(["set", id, value, record]);
    },
    setValues: (map, record = true) => {
      Object.assign(values, map);
      calls.push(["setValues", map, record]);
    },
    restoreValue: (id, value) => {
      values[id] = value;
      calls.push(["restore", id, value]);
    },
  };
}

function mount(id, initial) {
  const h = host({ [id]: initial });
  const box = createValueBox(dom.window.document, spec(id), h);
  dom.window.document.body.append(box.element);
  return { box, host: h, input: box.element.querySelector("input") };
}

function pointer(target, type, init = {}) {
  const Ctor = dom.window.PointerEvent ?? dom.window.MouseEvent;
  target.dispatchEvent(new Ctor(type, { bubbles: true, button: 0, pointerId: 1, clientX: 0, clientY: 0, ...init }));
}

function key(target, name, init = {}) {
  target.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...init }));
}

test("a value box is a spinbutton that shows the number, unit, gauge and modified dot", () => {
  const { box, input } = mount("compressor.threshold", -21);
  assert.equal(input.getAttribute("role"), "spinbutton");
  assert.equal(input.value, "-21.0");
  assert.equal(input.getAttribute("aria-valuetext"), "-21 decibels");
  assert.equal(box.element.querySelector(".vbox-unit").textContent, "dB");
  assert.equal(box.element.classList.contains("is-modified"), true);
  assert.equal(box.element.classList.contains("is-threshold"), true);
  assert.equal(parseFloat(box.element.querySelector(".vbox-tick").style.left), 70);
  box.setLevel(-10);
  assert.equal(parseFloat(box.element.querySelector(".vbox-level").style.width), 83.3);
  assert.equal(box.element.querySelector(".vbox-level").style.opacity, "0.9");
});

test("dragging past the dead zone records once, sends live values and shift goes fine", () => {
  const { box, host: h, input } = mount("input.gain", 0);
  pointer(input, "pointerdown", { clientX: 100, clientY: 100 });
  pointer(input, "pointermove", { clientX: 101, clientY: 100 });
  assert.deepEqual(h.calls, []);
  pointer(input, "pointermove", { clientX: 108, clientY: 100 });
  assert.deepEqual(h.calls[0], ["record"]);
  assert.equal(h.values["input.gain"], 1);
  assert.equal(h.calls[1][3], false);
  assert.equal(box.element.querySelector(".vbox-unit").textContent, "SHIFT·FINE");
  pointer(input, "pointermove", { clientX: 124, clientY: 100, shiftKey: true });
  assert.equal(h.values["input.gain"], 1.1);
  assert.equal(box.element.querySelector(".vbox-unit").textContent, "FINE");
  pointer(input, "pointerup", { clientX: 124, clientY: 100 });
  assert.equal(box.element.classList.contains("is-editing"), false);
  assert.equal(box.element.querySelector(".vbox-unit").textContent, "dB");
});

test("escape during a drag restores the value where the drag started", () => {
  const { host: h, input } = mount("input.gain", 2);
  pointer(input, "pointerdown", { clientX: 0, clientY: 0 });
  pointer(input, "pointermove", { clientX: 40, clientY: 0 });
  assert.equal(h.values["input.gain"], 7);
  key(input, "Escape");
  assert.deepEqual(h.calls.at(-1), ["restore", "input.gain", 2]);
});

test("a bipolar gain sticks at zero for six pixels when dragged across it", () => {
  const { host: h, input } = mount("input.gain", -1);
  pointer(input, "pointerdown", { clientX: 0, clientY: 0 });
  pointer(input, "pointermove", { clientX: 4, clientY: 0 });
  pointer(input, "pointermove", { clientX: 12, clientY: 0 });
  assert.equal(h.values["input.gain"], 0);
  pointer(input, "pointermove", { clientX: 16, clientY: 0 });
  assert.equal(h.values["input.gain"], 0);
  pointer(input, "pointermove", { clientX: 30, clientY: 0 });
  assert.ok(h.values["input.gain"] > 0);
});

test("a click opens typing, enter parses units and a bad entry flashes and keeps the value", () => {
  const { box, host: h, input } = mount("deesser.frequency", 6500);
  pointer(input, "pointerdown");
  pointer(input, "pointerup");
  assert.equal(box.element.classList.contains("is-editing"), true);
  assert.equal(input.readOnly, false);
  assert.equal(box.element.querySelector(".vbox-unit").textContent, "ENTER");
  input.value = "7k2";
  key(input, "Enter");
  assert.equal(h.values["deesser.frequency"], 7200);
  assert.equal(input.value, "7 200");
  pointer(input, "pointerdown");
  pointer(input, "pointerup");
  input.value = "loud";
  key(input, "Enter");
  assert.equal(h.values["deesser.frequency"], 7200);
  assert.equal(box.element.classList.contains("is-error"), true);
  assert.equal(input.readOnly, true);
});

test("keys step, page, jump and reset; typing a digit starts an entry", () => {
  const { box, host: h, input } = mount("compressor.ratio", 3);
  key(input, "ArrowUp");
  assert.equal(h.values["compressor.ratio"], 3.1);
  key(input, "ArrowDown", { shiftKey: true });
  assert.equal(h.values["compressor.ratio"], 3);
  key(input, "PageUp");
  assert.equal(h.values["compressor.ratio"], 4);
  key(input, "End");
  assert.equal(h.values["compressor.ratio"], 20);
  key(input, "Home");
  assert.equal(h.values["compressor.ratio"], 1);
  key(input, "Delete");
  assert.equal(h.values["compressor.ratio"], 3);
  key(input, "5");
  assert.equal(box.element.classList.contains("is-editing"), true);
  assert.equal(input.value, "5");
  key(input, "Escape");
  assert.equal(box.element.classList.contains("is-editing"), false);
  assert.equal(h.values["compressor.ratio"], 3);
});

test("ctrl-click resets and the context menu offers reset and typing", () => {
  const { box, host: h, input } = mount("compressor.makeup", 9);
  pointer(input, "pointerdown", { ctrlKey: true });
  assert.equal(h.values["compressor.makeup"], 6);
  input.dispatchEvent(new dom.window.MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  const menu = box.element.querySelector(".vbox-menu");
  assert.equal(menu.hidden, false);
  assert.equal(menu.querySelectorAll("[role=menuitem]")[0].textContent, "Reset to 6.0 dB");
  menu.querySelectorAll("[role=menuitem]")[1].click();
  assert.equal(menu.hidden, true);
  assert.equal(box.element.classList.contains("is-editing"), true);
});

test("the wheel steps only the focused box and accumulates trackpad deltas", () => {
  const { host: h, input } = mount("compressor.attack", 8);
  const wheel = (deltaY) => input.dispatchEvent(new dom.window.WheelEvent("wheel", { deltaY, deltaMode: 0, bubbles: true, cancelable: true }));
  wheel(-100);
  assert.equal(h.values["compressor.attack"], 8);
  input.focus();
  wheel(-30);
  assert.equal(h.values["compressor.attack"], 8);
  wheel(-30);
  assert.equal(h.values["compressor.attack"], 8.4);
});

test("the de-esser mode is a radio group switched by click or arrows", () => {
  const { box, host: h } = mount("deesser.splitBand", 1);
  const options = box.element.querySelectorAll("[role=radio]");
  assert.equal(box.element.getAttribute("role"), "radiogroup");
  assert.deepEqual([...options].map((option) => option.textContent), ["SPLIT BAND", "WIDEBAND"]);
  assert.equal(options[0].getAttribute("aria-checked"), "true");
  options[1].click();
  assert.equal(h.values["deesser.splitBand"], 0);
  assert.equal(options[1].getAttribute("aria-checked"), "true");
  key(options[1], "ArrowLeft");
  assert.equal(h.values["deesser.splitBand"], 1);
});
