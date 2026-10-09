import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { classifyTitlebarPress, initTitlebar } from "../src/titlebar.ts";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");

let dom;

beforeEach(() => {
  dom = new JSDOM(html);
});

afterEach(() => {
  dom.window.close();
});

function fakeControls() {
  const calls = [];
  let maximized = false;
  const record =
    (name) =>
    (...args) => {
      calls.push([name, ...args]);
      return Promise.resolve();
    };
  return {
    calls,
    minimize: record("minimize"),
    close: record("close"),
    startDragging: record("startDragging"),
    startResizeDragging: record("startResizeDragging"),
    toggleMaximize: () => {
      maximized = !maximized;
      calls.push(["toggleMaximize"]);
      return Promise.resolve();
    },
    isMaximized: () => Promise.resolve(maximized),
    onResized: () => Promise.resolve(() => undefined),
  };
}

function press(target, init) {
  target.dispatchEvent(new dom.window.MouseEvent("mousedown", { bubbles: true, buttons: 1, ...init }));
}

function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

test("window control buttons call through to the window", async () => {
  const controls = fakeControls();
  await initTitlebar(controls, dom.window.document);

  dom.window.document.querySelector("#window-minimize").click();
  dom.window.document.querySelector("#window-close").click();

  assert.deepEqual(controls.calls, [["minimize"], ["close"]]);
});

test("pressing the bar starts a window drag", async () => {
  const controls = fakeControls();
  await initTitlebar(controls, dom.window.document);

  press(dom.window.document.querySelector(".app-name"));

  assert.deepEqual(controls.calls, [["startDragging"]]);
});

test("pressing a control inside the bar does not start a drag", async () => {
  const controls = fakeControls();
  await initTitlebar(controls, dom.window.document);

  press(dom.window.document.querySelector("#window-minimize"));

  assert.deepEqual(controls.calls, []);
});

test("double pressing the bar toggles maximize and marks the body", async () => {
  const controls = fakeControls();
  const { document } = dom.window;
  await initTitlebar(controls, document);

  press(document.querySelector(".topbar"), { detail: 2 });
  await settle();

  assert.deepEqual(controls.calls, [["toggleMaximize"]]);
  assert.equal(document.body.classList.contains("window-maximized"), true);
  assert.equal(document.querySelector("#window-maximize").getAttribute("aria-label"), "Restore");
});

test("resize handles request their own direction", async () => {
  const controls = fakeControls();
  await initTitlebar(controls, dom.window.document);

  const handle = dom.window.document.querySelector('[data-resize-direction="SouthEast"]');
  handle.dispatchEvent(new dom.window.Event("pointerdown", { bubbles: true }));

  assert.deepEqual(controls.calls, [["startResizeDragging", "SouthEast"]]);
});

test("only a primary button press without interactive targets drags", () => {
  const { document, MouseEvent } = dom.window;
  const bar = document.querySelector(".topbar");
  const button = document.querySelector("#window-close");

  const classify = (target, init) => {
    let result;
    const listener = (event) => {
      result = classifyTitlebarPress(event);
    };
    bar.addEventListener("mousedown", listener);
    target.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, ...init }));
    bar.removeEventListener("mousedown", listener);
    return result;
  };

  assert.equal(classify(bar, { buttons: 1 }), "drag");
  assert.equal(classify(bar, { buttons: 1, detail: 2 }), "toggle-maximize");
  assert.equal(classify(bar, { buttons: 2 }), "ignore");
  assert.equal(classify(button, { buttons: 1 }), "ignore");
});
