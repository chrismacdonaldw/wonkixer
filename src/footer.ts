import { create, element } from "./dom.ts";
import { formatCount, formatHz, formatMs } from "./format.ts";
import type { MeterState } from "./meters.ts";
import { DASH } from "./page-context.ts";
import type { Store } from "./store.ts";

export const SHORTCUTS: Array<[string, string]> = [
  ["DRAG UP OR RIGHT", "Raise the value. Down or left lowers it."],
  ["HOLD SHIFT", "Ten times finer while dragging, stepping or scrolling."],
  ["CLICK", "Type an exact value: -18, 6.5k, 120ms, 4:1."],
  ["ENTER / ESC", "Keep or cancel. Esc during a drag puts it back."],
  ["↑ ↓", "One step. Works while typing too."],
  ["PAGE UP / DOWN", "Next useful value: 1, 2, 5 ms… or 2:1, 3:1, 4:1…"],
  ["HOME / END", "Minimum / maximum."],
  ["CTRL+CLICK, DEL", "Back to the default (the tick). Right-click for a menu."],
  ["WHEEL", "One step on the focused box."],
  ["GRAPH POINTS", "Drag. Alt locks one axis; wheel sets Q."],
];

export function initFooter(meters: Store<MeterState>, doc: Document): void {
  const rate = element<HTMLElement>("#readout-rate", doc);
  const latency = element<HTMLElement>("#readout-latency", doc);
  const dropouts = element<HTMLElement>("#readout-dropouts", doc);
  const toggle = element<HTMLButtonElement>("#shortcuts", doc);
  const dialog = element<HTMLElement>("#keys-dialog", doc);
  const list = element<HTMLElement>("#keys-list", doc);
  let open = false;

  list.replaceChildren(
    ...SHORTCUTS.map(([key, text]) => {
      const row = create(doc, "div", "keys-row");
      row.append(create(doc, "span", "keys-key", key), create(doc, "span", "keys-text", text));
      return row;
    }),
  );

  const renderOpen = (): void => {
    dialog.hidden = !open;
    toggle.setAttribute("aria-expanded", String(open));
    toggle.classList.toggle("is-open", open);
  };

  toggle.addEventListener("click", () => {
    open = !open;
    renderOpen();
  });
  doc.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && open) {
      open = false;
      renderOpen();
    }
  });
  doc.addEventListener("pointerdown", (event) => {
    const target = event.target as Node;
    if (open && !dialog.contains(target) && !toggle.contains(target)) {
      open = false;
      renderOpen();
    }
  });

  const render = (meter: MeterState): void => {
    rate.textContent = meter.live && meter.sampleRate !== null ? formatHz(meter.sampleRate) : DASH;
    latency.textContent = meter.live ? formatMs(meter.latencyMs) : DASH;
    dropouts.textContent = meter.live ? formatCount(meter.dropouts) : DASH;
  };

  meters.subscribe(render);
  render(meters.get());
  renderOpen();
}
