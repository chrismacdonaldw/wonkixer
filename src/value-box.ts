import { create } from "./dom.ts";
import type { ParamSpec } from "./engine.ts";
import { editText, formatNumber, formatValue, speakValue, unitSuffix } from "./format.ts";
import { POSITIONS, toPosition } from "./param-scale.ts";
import type { ValueHost } from "./value-host.ts";
import {
  dragSpace,
  isBipolarGain,
  isModified,
  isThresholdLike,
  pageStep,
  parseInput,
  snapValue,
  stepped,
  switchOn,
  wheelNotch,
  wheelSteps,
} from "./value-math.ts";

export const DRAG_DEAD_ZONE_PX = 3;
export const ZERO_STICK_PX = 6;
export const FINE_FACTOR = 10;
export const ERROR_FLASH_MS = 900;

export interface ValueBoxOptions {
  short?: string;
  compact?: boolean;
}

export interface ValueBox {
  element: HTMLElement;
  spec: ParamSpec;
  refresh(): void;
  setLevel(levelDb: number | null): void;
}

interface Scrub {
  lastX: number;
  lastY: number;
  unit: number;
  origin: number;
  moved: number;
  live: boolean;
  stick: number;
  pointerId: number;
}

const SWITCH_OPTIONS: Record<string, [string, string]> = { "deesser.splitBand": ["SPLIT BAND", "WIDEBAND"] };

export function domIdFor(paramId: string): string {
  return `f-${paramId.split(".").join("-")}`;
}

export function createValueBox(doc: Document, spec: ParamSpec, host: ValueHost, options: ValueBoxOptions = {}): ValueBox {
  if (spec.unit === "switch") {
    return createModeSwitch(doc, spec, host, options);
  }
  const root = create(doc, "div", "vbox");
  root.dataset.param = spec.id;
  root.classList.toggle("is-compact", Boolean(options.compact));
  const threshold = isThresholdLike(spec);
  root.classList.toggle("is-threshold", threshold);

  const label = create(doc, "label", "vbox-label");
  label.htmlFor = domIdFor(spec.id);
  label.append(create(doc, "span", "vbox-dot"), doc.createTextNode((options.short ?? spec.label).toUpperCase()));
  const unit = create(doc, "span", "vbox-unit");
  const input = create(doc, "input", "vbox-input");
  input.id = domIdFor(spec.id);
  input.type = "text";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.readOnly = true;
  input.setAttribute("role", "spinbutton");
  input.setAttribute("aria-valuemin", String(spec.min));
  input.setAttribute("aria-valuemax", String(spec.max));
  const gauge = create(doc, "span", "vbox-gauge");
  const level = create(doc, "span", "vbox-level");
  const fill = create(doc, "span", "vbox-fill");
  const mark = create(doc, "span", "vbox-mark");
  const tick = create(doc, "span", "vbox-tick");
  gauge.append(level, fill, mark, tick);
  const menu = create(doc, "div", "vbox-menu");
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", `${spec.label} options`);
  menu.hidden = true;
  const resetItem = create(doc, "button", "vbox-menu-item", `Reset to ${formatValue(spec, spec.default)}`);
  resetItem.type = "button";
  resetItem.setAttribute("role", "menuitem");
  const typeItem = create(doc, "button", "vbox-menu-item", "Type a value");
  typeItem.type = "button";
  typeItem.setAttribute("role", "menuitem");
  menu.append(resetItem, typeItem, create(doc, "span", "vbox-menu-hint", "CTRL+CLICK ALSO RESETS"));
  root.append(label, unit, input, gauge, menu);

  let editing = false;
  let scrub: Scrub | null = null;
  let fine = false;
  let focused = false;
  let menuOpen = false;
  let wheelRemainder = 0;
  let errorTimer: ReturnType<typeof setTimeout> | null = null;

  const current = (): number => host.current(spec.id);

  const render = (): void => {
    const value = current();
    if (!editing) {
      input.value = formatNumber(spec, value);
    }
    const scrubbing = scrub?.live ?? false;
    unit.textContent = scrubbing ? (fine ? "FINE" : "SHIFT·FINE") : editing ? "ENTER" : unitSuffix(spec);
    input.setAttribute("aria-valuenow", String(value));
    input.setAttribute("aria-valuetext", speakValue(spec, value));
    root.classList.toggle("is-modified", isModified(spec, value));
    root.classList.toggle("is-scrub", scrubbing);
    root.classList.toggle("is-editing", editing);
    root.classList.toggle("is-focus", focused || editing || scrubbing);
    root.classList.toggle("is-menu", menuOpen);
    const position = (toPosition(spec, value) / POSITIONS) * 100;
    const zero = isBipolarGain(spec) ? (toPosition(spec, 0) / POSITIONS) * 100 : 0;
    fill.style.left = threshold ? "0%" : `${Math.min(position, zero).toFixed(1)}%`;
    fill.style.width = threshold ? "0%" : `${Math.abs(position - zero).toFixed(1)}%`;
    mark.style.left = `${position.toFixed(1)}%`;
    tick.style.left = `${((toPosition(spec, spec.default) / POSITIONS) * 100).toFixed(1)}%`;
  };

  const closeMenu = (): void => {
    menuOpen = false;
    menu.hidden = true;
    root.classList.remove("is-menu");
  };

  const openMenu = (): void => {
    menuOpen = true;
    menu.hidden = false;
    root.classList.add("is-menu");
  };

  const endEdit = (): void => {
    editing = false;
    input.readOnly = true;
    render();
  };

  const beginEdit = (initial?: string): void => {
    closeMenu();
    editing = true;
    input.readOnly = false;
    input.value = initial ?? editText(spec, current());
    render();
    if (initial === undefined) {
      input.select();
    } else {
      input.setSelectionRange(input.value.length, input.value.length);
    }
  };

  const flashError = (): void => {
    root.classList.add("is-error");
    if (errorTimer !== null) {
      clearTimeout(errorTimer);
    }
    errorTimer = setTimeout(() => {
      errorTimer = null;
      root.classList.remove("is-error");
    }, ERROR_FLASH_MS);
  };

  const commit = (): void => {
    if (!editing) {
      return;
    }
    const parsed = parseInput(spec, input.value);
    endEdit();
    if (parsed === null) {
      flashError();
      return;
    }
    host.setValue(spec.id, parsed);
    render();
  };

  const set = (value: number): void => {
    host.setValue(spec.id, value);
    render();
  };

  const releasePointer = (pointerId: number): void => {
    try {
      input.releasePointerCapture(pointerId);
    } catch {
      return;
    }
  };

  const exitPointerLock = (): void => {
    if (doc.pointerLockElement === input && typeof doc.exitPointerLock === "function") {
      doc.exitPointerLock();
    }
  };

  const requestPointerLock = (): void => {
    if (typeof input.requestPointerLock !== "function") {
      return;
    }
    try {
      const lockOptions = { unadjustedMovement: true };
      Promise.resolve(input.requestPointerLock(lockOptions)).catch(() => undefined);
    } catch {
      return;
    }
  };

  input.addEventListener("pointerdown", (event) => {
    if (editing || event.button !== 0) {
      return;
    }
    event.preventDefault();
    input.focus({ preventScroll: true });
    closeMenu();
    if (event.ctrlKey || event.altKey || event.metaKey) {
      set(spec.default);
      return;
    }
    try {
      input.setPointerCapture(event.pointerId);
    } catch {
      scrub = null;
    }
    const value = current();
    scrub = {
      lastX: event.clientX,
      lastY: event.clientY,
      unit: dragSpace(spec).toUnit(value),
      origin: value,
      moved: 0,
      live: false,
      stick: 0,
      pointerId: event.pointerId,
    };
  });

  input.addEventListener("pointermove", (event) => {
    if (!scrub) {
      return;
    }
    const locked = doc.pointerLockElement === input;
    const delta = locked ? event.movementX - event.movementY : event.clientX - scrub.lastX - (event.clientY - scrub.lastY);
    scrub.lastX = event.clientX;
    scrub.lastY = event.clientY;
    if (!scrub.live) {
      scrub.moved += Math.abs(delta);
      if (scrub.moved < DRAG_DEAD_ZONE_PX) {
        return;
      }
      scrub.live = true;
      host.record();
      requestPointerLock();
    }
    fine = event.shiftKey;
    if (scrub.stick > 0 && !fine) {
      scrub.stick -= Math.abs(delta);
      if (scrub.stick > 0) {
        render();
        return;
      }
    }
    const space = dragSpace(spec);
    const before = snapValue(spec, space.fromUnit(scrub.unit), fine);
    scrub.unit = Math.max(0, Math.min(1, scrub.unit + delta / (space.range * (fine ? FINE_FACTOR : 1))));
    let value = snapValue(spec, space.fromUnit(scrub.unit), fine);
    if (!fine && isBipolarGain(spec) && ((before < 0 && value >= 0) || (before > 0 && value <= 0))) {
      value = 0;
      scrub.unit = space.toUnit(0);
      scrub.stick = ZERO_STICK_PX;
    }
    host.setValue(spec.id, value, false);
    render();
  });

  const finishPointer = (event: PointerEvent): void => {
    if (!scrub) {
      return;
    }
    const session = scrub;
    scrub = null;
    releasePointer(event.pointerId);
    exitPointerLock();
    if (session.live) {
      fine = false;
      render();
      return;
    }
    beginEdit();
  };
  input.addEventListener("pointerup", finishPointer);
  input.addEventListener("pointercancel", finishPointer);

  input.addEventListener("keydown", (event) => {
    const key = event.key;
    if (scrub && key === "Escape") {
      event.preventDefault();
      const session = scrub;
      scrub = null;
      releasePointer(session.pointerId);
      exitPointerLock();
      if (session.live) {
        host.restoreValue(spec.id, session.origin);
      }
      fine = false;
      render();
      return;
    }
    if (menuOpen && key === "Escape") {
      event.preventDefault();
      closeMenu();
      return;
    }
    if (editing) {
      if (key === "Enter") {
        event.preventDefault();
        commit();
      } else if (key === "Escape") {
        event.preventDefault();
        endEdit();
      } else if (key === "ArrowUp" || key === "ArrowDown") {
        event.preventDefault();
        const typed = parseInput(spec, input.value);
        const next = stepped(spec, typed ?? current(), key === "ArrowUp" ? 1 : -1, event.shiftKey);
        host.setValue(spec.id, next);
        input.value = editText(spec, next);
        input.select();
      }
      return;
    }
    switch (key) {
      case "ArrowUp":
      case "ArrowRight":
        event.preventDefault();
        set(stepped(spec, current(), 1, event.shiftKey));
        return;
      case "ArrowDown":
      case "ArrowLeft":
        event.preventDefault();
        set(stepped(spec, current(), -1, event.shiftKey));
        return;
      case "PageUp":
        event.preventDefault();
        set(pageStep(spec, current(), 1));
        return;
      case "PageDown":
        event.preventDefault();
        set(pageStep(spec, current(), -1));
        return;
      case "Home":
        event.preventDefault();
        set(spec.min);
        return;
      case "End":
        event.preventDefault();
        set(spec.max);
        return;
      case "Delete":
      case "Backspace":
        event.preventDefault();
        set(spec.default);
        return;
      case "Enter":
      case "F2":
        event.preventDefault();
        beginEdit();
        return;
      default:
        if (key.length === 1 && !event.ctrlKey && !event.altKey && !event.metaKey) {
          event.preventDefault();
          if (/[0-9.\-+]/.test(key)) {
            beginEdit(key);
          }
        }
    }
  });

  input.addEventListener("focus", () => {
    focused = true;
    render();
  });

  input.addEventListener("blur", () => {
    if (editing) {
      commit();
    }
    focused = false;
    closeMenu();
    render();
  });

  input.addEventListener(
    "wheel",
    (event) => {
      if (!focused || editing) {
        return;
      }
      event.preventDefault();
      const result = wheelSteps(wheelRemainder, event.deltaY || event.deltaX, wheelNotch(event.deltaMode));
      wheelRemainder = result.remainder;
      if (result.steps === 0) {
        return;
      }
      let value = current();
      const direction: 1 | -1 = result.steps > 0 ? 1 : -1;
      for (let i = 0; i < Math.abs(result.steps); i++) {
        value = stepped(spec, value, direction, event.shiftKey);
      }
      set(value);
    },
    { passive: false },
  );

  input.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    input.focus({ preventScroll: true });
    openMenu();
  });

  for (const item of [resetItem, typeItem]) {
    item.addEventListener("pointerdown", (event) => event.preventDefault());
  }
  resetItem.addEventListener("click", () => {
    closeMenu();
    set(spec.default);
  });
  typeItem.addEventListener("click", () => {
    closeMenu();
    input.focus({ preventScroll: true });
    beginEdit();
  });

  render();

  return {
    element: root,
    spec,
    refresh: render,
    setLevel(levelDb) {
      if (!threshold || levelDb === null) {
        level.style.opacity = "0";
        return;
      }
      const fraction = Math.max(0, Math.min(1, (levelDb - spec.min) / (spec.max - spec.min)));
      level.style.width = `${(fraction * 100).toFixed(1)}%`;
      level.style.opacity = levelDb >= current() ? "0.9" : "0.45";
    },
  };
}

function createModeSwitch(doc: Document, spec: ParamSpec, host: ValueHost, options: ValueBoxOptions): ValueBox {
  const [onText, offText] = SWITCH_OPTIONS[spec.id] ?? ["ON", "OFF"];
  const root = create(doc, "div", "vbox vbox-switch");
  root.dataset.param = spec.id;
  root.classList.toggle("is-compact", Boolean(options.compact));
  root.setAttribute("role", "radiogroup");
  root.setAttribute("aria-label", `${spec.label} mode`);
  const label = create(doc, "span", "vbox-label");
  label.append(create(doc, "span", "vbox-dot"), doc.createTextNode((options.short ?? spec.label).toUpperCase()));
  const choices = create(doc, "span", "vbox-choices");
  const buttons = [
    [onText, 1],
    [offText, 0],
  ].map(([text, value]) => {
    const button = create(doc, "button", "vbox-choice", String(text));
    button.type = "button";
    button.setAttribute("role", "radio");
    button.dataset.value = String(value);
    button.addEventListener("click", () => {
      host.setValue(spec.id, Number(value));
      render();
    });
    button.addEventListener("keydown", (event) => {
      if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
        event.preventDefault();
        host.setValue(spec.id, switchOn(host.current(spec.id)) ? 0 : 1);
        render();
      }
    });
    choices.append(button);
    return button;
  });
  root.append(label, choices);

  const render = (): void => {
    const value = host.current(spec.id);
    const on = switchOn(value);
    root.classList.toggle("is-modified", isModified(spec, value));
    for (const button of buttons) {
      const chosen = (button.dataset.value === "1") === on;
      button.setAttribute("aria-checked", String(chosen));
      button.classList.toggle("is-chosen", chosen);
      button.tabIndex = chosen ? 0 : -1;
    }
  };

  render();
  return { element: root, spec, refresh: render, setLevel: () => undefined };
}
