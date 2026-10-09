import { element, elements } from "./dom.ts";
import { isResizeDirection, type WindowControls } from "./window-controls.ts";

const INTERACTIVE_SELECTOR = "button, input, select, option, label, form, a, [role='button']";
const MAXIMIZED_CLASS = "window-maximized";

export type TitlebarPress = "drag" | "toggle-maximize" | "ignore";

export function classifyTitlebarPress(event: MouseEvent): TitlebarPress {
  if (event.buttons !== 1 || isInteractive(event.target)) {
    return "ignore";
  }
  return event.detail === 2 ? "toggle-maximize" : "drag";
}

function isInteractive(target: EventTarget | null): boolean {
  return hasClosest(target) && target.closest(INTERACTIVE_SELECTOR) !== null;
}

function hasClosest(target: EventTarget | null): target is Element {
  return target !== null && "closest" in target;
}

export async function initTitlebar(controls: WindowControls, doc: Document = document): Promise<void> {
  const topbar = element<HTMLElement>(".topbar", doc);
  const minimizeButton = element<HTMLButtonElement>("#window-minimize", doc);
  const maximizeButton = element<HTMLButtonElement>("#window-maximize", doc);
  const closeButton = element<HTMLButtonElement>("#window-close", doc);

  const syncMaximized = async (): Promise<void> => {
    const maximized = await controls.isMaximized();
    doc.body.classList.toggle(MAXIMIZED_CLASS, maximized);
    maximizeButton.setAttribute("aria-label", maximized ? "Restore" : "Maximize");
  };

  const toggleMaximize = async (): Promise<void> => {
    await controls.toggleMaximize();
    await syncMaximized();
  };

  minimizeButton.addEventListener("click", () => void controls.minimize());
  maximizeButton.addEventListener("click", () => void toggleMaximize());
  closeButton.addEventListener("click", () => void controls.close());

  topbar.addEventListener("mousedown", (event) => {
    switch (classifyTitlebarPress(event)) {
      case "drag":
        void controls.startDragging();
        break;
      case "toggle-maximize":
        void toggleMaximize();
        break;
      case "ignore":
        break;
    }
  });

  for (const handle of elements<HTMLElement>("[data-resize-direction]", doc)) {
    const direction = handle.dataset.resizeDirection;
    if (!isResizeDirection(direction)) {
      throw new Error(`Unknown resize direction: ${direction}`);
    }
    handle.addEventListener("pointerdown", () => void controls.startResizeDragging(direction));
  }

  await controls.onResized(() => void syncMaximized());
  await syncMaximized();
}
