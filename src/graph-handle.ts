import { create } from "./dom.ts";
import type { ValueHost } from "./value-host.ts";
import { snapValue, stepped, wheelNotch, wheelSteps } from "./value-math.ts";

export interface HandleSpec {
  key: string;
  aria: string;
  number?: string;
  xParam?: string;
  yParam?: string;
  qParam?: string;
  fromX?: (unit: number) => number;
  fromY?: (unit: number) => number;
}

export interface GraphHandle {
  element: HTMLButtonElement;
  spec: HandleSpec;
  place(x: number, y: number): void;
  dragging(): boolean;
}

export interface HandleOptions {
  onDragChange?: (dragging: boolean) => void;
}

interface Drag {
  rect: DOMRect;
  lastX: number;
  lastY: number;
  x: number;
  y: number;
  originX: number;
  originY: number;
  travelX: number;
  travelY: number;
  recorded: boolean;
  pointerId: number;
}

export const HANDLE_FINE_FACTOR = 0.1;

const clamp = (value: number): number => Math.max(0, Math.min(1, value));

export function createGraphHandle(doc: Document, spec: HandleSpec, host: ValueHost, options: HandleOptions = {}): GraphHandle {
  const button = create(doc, "button", "graph-handle", spec.number ?? "");
  button.type = "button";
  button.setAttribute("aria-label", spec.aria);
  let drag: Drag | null = null;
  let placedX = 0;
  let placedY = 0;
  let wheelRemainder = 0;

  const setDragging = (dragging: boolean): void => {
    button.classList.toggle("is-dragging", dragging);
    options.onDragChange?.(dragging);
  };

  button.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    try {
      button.setPointerCapture(event.pointerId);
    } catch {
      drag = null;
    }
    button.focus({ preventScroll: true });
    const parent = button.parentElement;
    drag = {
      rect: (parent ?? button).getBoundingClientRect(),
      lastX: event.clientX,
      lastY: event.clientY,
      x: placedX,
      y: placedY,
      originX: placedX,
      originY: placedY,
      travelX: 0,
      travelY: 0,
      recorded: false,
      pointerId: event.pointerId,
    };
    setDragging(true);
  });

  button.addEventListener("pointermove", (event) => {
    if (!drag) {
      return;
    }
    if (!drag.recorded) {
      drag.recorded = true;
      host.record();
    }
    const factor = event.shiftKey ? HANDLE_FINE_FACTOR : 1;
    const width = drag.rect.width || 1;
    const height = drag.rect.height || 1;
    const dx = ((event.clientX - drag.lastX) / width) * factor;
    const dy = ((event.clientY - drag.lastY) / height) * factor;
    drag.lastX = event.clientX;
    drag.lastY = event.clientY;
    drag.travelX += dx;
    drag.travelY += dy;
    drag.x = clamp(drag.x + dx);
    drag.y = clamp(drag.y + dy);
    let x = drag.x;
    let y = drag.y;
    if (event.altKey) {
      if (Math.abs(drag.travelX) > Math.abs(drag.travelY)) {
        y = drag.originY;
      } else {
        x = drag.originX;
      }
    }
    const update: Record<string, number> = {};
    if (spec.xParam && spec.fromX) {
      update[spec.xParam] = snapValue(host.spec(spec.xParam), spec.fromX(x), event.shiftKey);
    }
    if (spec.yParam && spec.fromY) {
      update[spec.yParam] = snapValue(host.spec(spec.yParam), spec.fromY(y), event.shiftKey);
    }
    host.setValues(update, false);
  });

  const finish = (event: PointerEvent): void => {
    if (!drag) {
      return;
    }
    drag = null;
    try {
      button.releasePointerCapture(event.pointerId);
    } catch {
      setDragging(false);
      return;
    }
    setDragging(false);
  };
  button.addEventListener("pointerup", finish);
  button.addEventListener("pointercancel", finish);

  button.addEventListener("keydown", (event) => {
    const key = event.key;
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(key)) {
      return;
    }
    event.preventDefault();
    const direction: 1 | -1 = key === "ArrowRight" || key === "ArrowUp" ? 1 : -1;
    const vertical = key === "ArrowUp" || key === "ArrowDown";
    const id = vertical && spec.yParam ? spec.yParam : spec.xParam;
    if (!id) {
      return;
    }
    const target = host.spec(id);
    host.setValue(id, stepped(target, host.current(id), direction, event.shiftKey));
  });

  button.addEventListener(
    "wheel",
    (event) => {
      if (!spec.qParam) {
        return;
      }
      event.preventDefault();
      const result = wheelSteps(wheelRemainder, event.deltaY || event.deltaX, wheelNotch(event.deltaMode));
      wheelRemainder = result.remainder;
      if (result.steps === 0) {
        return;
      }
      const target = host.spec(spec.qParam);
      let value = host.current(spec.qParam);
      const direction: 1 | -1 = result.steps > 0 ? 1 : -1;
      for (let i = 0; i < Math.abs(result.steps); i++) {
        value = stepped(target, value, direction, event.shiftKey);
      }
      host.setValue(spec.qParam, value);
    },
    { passive: false },
  );

  return {
    element: button,
    spec,
    place(x, y) {
      placedX = x;
      placedY = y;
      button.style.left = `${(x * 100).toFixed(2)}%`;
      button.style.top = `${(y * 100).toFixed(2)}%`;
    },
    dragging: () => drag !== null,
  };
}
