import { create } from "./dom.ts";
import type { ParamSpec, StageSpec } from "./engine.ts";
import { formatValue } from "./format.ts";
import type { MeterState } from "./meters.ts";
import type { PageContext, PageView } from "./page-context.ts";
import { bandGroups, displayedValues, stageParams, type SessionState } from "./session-state.ts";
import { createValueBox, type ValueBox } from "./value-box.ts";
import { isThresholdLike } from "./value-math.ts";

const SHORT_LABEL: Partial<Record<ParamSpec["unit"], string>> = { hertz: "Freq", q: "Q", decibels: "Gain" };

export function detailSummary(params: ParamSpec[], values: Record<string, number>): string {
  return params.map((spec) => `${spec.label} ${formatValue(spec, values[spec.id] ?? spec.default)}`).join("  ·  ");
}

export function mountSections(context: PageContext, stage: StageSpec): PageView | null {
  const { doc, session, host } = context;
  const { schema } = session;
  const boxes: ValueBox[] = [];
  const root = create(doc, "div", "sections");
  let detailGrid: HTMLElement | null = null;
  let detailButton: HTMLButtonElement | null = null;
  let detailSummaryText: HTMLElement | null = null;
  let detailParams: ParamSpec[] = [];

  const grid = (params: ParamSpec[], compact: boolean, short: (spec: ParamSpec) => string | undefined): HTMLElement => {
    const container = create(doc, "div", "field-grid");
    for (const spec of params) {
      const box = createValueBox(doc, spec, host, { compact, short: short(spec) });
      boxes.push(box);
      container.append(box.element);
    }
    return container;
  };

  if (stage.family === "eq") {
    root.classList.add("is-bands");
    bandGroups(schema, stage.id).forEach((band, index) => {
      const section = create(doc, "div", "section section-band");
      section.setAttribute("role", "group");
      section.setAttribute("aria-label", `${band.label.toLowerCase()} band`);
      const header = create(doc, "div", "band-head");
      const number = create(doc, "span", "band-number", String(index + 1));
      number.setAttribute("aria-hidden", "true");
      header.append(number, create(doc, "h2", "section-name", band.label));
      section.append(header, grid(band.params, true, (spec) => SHORT_LABEL[spec.unit]));
      root.append(section);
    });
  } else {
    const main = stageParams(schema, stage.id, "main");
    detailParams = stageParams(schema, stage.id, "detail");
    if (!main.length && !detailParams.length) {
      return null;
    }
    const mainSection = create(doc, "div", "section");
    mainSection.setAttribute("role", "group");
    mainSection.setAttribute("aria-label", "Main settings");
    mainSection.append(grid(main, false, () => undefined));
    root.append(mainSection);
    if (detailParams.length) {
      const section = create(doc, "div", "section section-detail");
      section.setAttribute("role", "group");
      section.setAttribute("aria-label", "Detail settings");
      detailButton = create(doc, "button", "disclosure");
      detailButton.type = "button";
      detailButton.innerHTML =
        '<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 4l2.5 2.5L7.5 4"></path></svg>';
      detailButton.append(create(doc, "span", "section-name", "DETAIL"));
      detailSummaryText = create(doc, "span", "disclosure-summary");
      detailButton.append(detailSummaryText);
      detailButton.addEventListener("click", () => session.toggleDetail(stage.id));
      detailGrid = grid(detailParams, false, () => undefined);
      section.append(detailButton, detailGrid);
      root.append(section);
    }
  }

  const update = (state: SessionState, meter: MeterState): void => {
    const values = displayedValues(state);
    const level = meter.live ? (meter.detectorDb[stage.id] ?? null) : null;
    for (const box of boxes) {
      box.refresh();
      if (isThresholdLike(box.spec)) {
        box.setLevel(level);
      }
    }
    if (detailGrid && detailButton && detailSummaryText) {
      const open = Boolean(state.detailOpen[stage.id]);
      detailButton.setAttribute("aria-expanded", String(open));
      detailButton.classList.toggle("is-open", open);
      detailGrid.hidden = !open;
      detailSummaryText.textContent = open ? "" : detailSummary(detailParams, values);
    }
  };

  return { element: root, update };
}
