import type { MeterState } from "./meters.ts";
import type { Session } from "./session.ts";
import type { SessionState } from "./session-state.ts";
import type { Store } from "./store.ts";
import type { ValueHost } from "./value-host.ts";

export interface PageContext {
  doc: Document;
  session: Session;
  meters: Store<MeterState>;
  host: ValueHost;
}

export interface PageView {
  element: HTMLElement;
  update(state: SessionState, meter: MeterState): void;
}

export const DASH = "—";

export function svgElement<K extends keyof SVGElementTagNameMap>(
  doc: Document,
  tag: K,
  attributes: Record<string, string>,
): SVGElementTagNameMap[K] {
  const node = doc.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [name, value] of Object.entries(attributes)) {
    node.setAttribute(name, value);
  }
  return node;
}

export function reductionText(reduction: number): string {
  return reduction >= 0.05 ? `-${reduction.toFixed(1)}` : "0.0";
}
