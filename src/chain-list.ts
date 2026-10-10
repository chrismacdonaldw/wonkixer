import { levelFraction, responsePath } from "./curves.ts";
import { create, element } from "./dom.ts";
import type { ChainSchema, StageSpec } from "./engine.ts";
import { formatValue } from "./format.ts";
import { bandFor, type MeterFeed, type MeterState } from "./meters.ts";
import { reductionText } from "./page-context.ts";
import type { Session } from "./session.ts";
import {
  bandGroups,
  displayedBypassed,
  displayedValues,
  effectiveBypassed,
  hasResponseCurve,
  LIBRARY,
  onCount,
  padIndex,
  reportsGainReduction,
  resolveSelection,
  stageParams,
  visibleStages,
  type SessionState,
} from "./session-state.ts";
import { FLAT, responseFor } from "./stage-curves.ts";

const SUMMARY_PARAM: Record<string, string> = { deesser: "deesser.frequency", compressor: "compressor.ratio" };
const GR_FULL_SCALE_DB = 24;
const HOLD_VISIBLE_DB = -59.5;
const SCALE_TICKS: Array<[string, string]> = [
  ["-60", "0%"],
  ["-40", "33.3%"],
  ["-20", "66.7%"],
  ["-12", "80%"],
  ["0", "100%"],
];

export function summaryFor(schema: ChainSchema, stage: StageSpec, values: Record<string, number>): string {
  if (stage.family === "eq") {
    return `${bandGroups(schema, stage.id).length} BANDS`;
  }
  const preferred = SUMMARY_PARAM[stage.id];
  const spec = schema.params.find((param) => param.id === preferred) ?? stageParams(schema, stage.id, "main")[0];
  return spec ? formatValue(spec, values[spec.id] ?? spec.default) : "";
}

interface Row {
  stage: StageSpec;
  item: HTMLLIElement;
  button: HTMLButtonElement;
  label: HTMLElement;
  value: HTMLElement;
  number: HTMLElement;
  cover?: HTMLElement;
  hold?: HTMLElement;
  reductionFill?: HTMLElement;
  reductionText?: HTMLElement;
  curveFill?: SVGPathElement;
  curveLine?: SVGPathElement;
  power?: HTMLButtonElement;
  clip?: HTMLButtonElement;
}

export function initChainList(session: Session, meters: MeterFeed, doc: Document): void {
  const list = element<HTMLOListElement>("#chain-list", doc);
  const count = element<HTMLElement>("#chain-count", doc);
  const dry = element<HTMLButtonElement>("#hear-dry", doc);
  const addEffect = element<HTMLButtonElement>("#add-effect", doc);
  const { schema } = session;
  let rows: Row[] = [];
  let rowKey = "";

  dry.addEventListener("click", () => session.toggleDry());
  addEffect.addEventListener("click", () => session.select(LIBRARY));

  const buildRows = (stages: StageSpec[]): void => {
    rows = stages.map((stage, index) => buildRow(doc, session, meters, stage, index));
    list.replaceChildren(...rows.map((row) => row.item));
  };

  const render = (state: SessionState, meter: MeterState): void => {
    const stages = visibleStages(schema, state);
    const key = stages.map((stage) => stage.id).join("|");
    if (key !== rowKey) {
      rowKey = key;
      buildRows(stages);
    }
    const selection = resolveSelection(schema, state, "compressor");
    const values = displayedValues(state);
    const bypassedSet = displayedBypassed(state);
    const bypassed = effectiveBypassed(schema, state);
    const { on, total } = onCount(schema, state);
    count.textContent = `${on}/${total} ON`;
    dry.setAttribute("aria-pressed", String(state.dry));
    dry.classList.toggle("is-on", state.dry);
    addEffect.setAttribute("aria-current", selection === LIBRARY ? "page" : "false");
    addEffect.classList.toggle("is-selected", selection === LIBRARY);

    for (const row of rows) {
      const { stage } = row;
      const selected = stage.id === selection;
      const off = Boolean(bypassed[stage.id]);
      row.button.setAttribute("aria-current", selected ? "page" : "false");
      row.item.classList.toggle("is-selected", selected);
      row.item.classList.toggle("is-bypassed", off);
      row.value.textContent = off ? (state.dry ? "DRY" : "OFF") : summaryFor(schema, stage, values);
      if (row.power) {
        row.power.setAttribute("aria-pressed", String(Boolean(bypassedSet[stage.id])));
      }
      if (row.cover && row.hold) {
        const input = stage.id === "input";
        const peak = meter.live ? (input ? meter.inputPeakDb : meter.outputPeakDb) : -60;
        const held = input ? meter.inputHoldDb : meter.outputHoldDb;
        row.cover.style.width = `${(100 - levelFraction(peak) * 100).toFixed(1)}%`;
        row.hold.style.left = `${(Math.min(0.985, levelFraction(held)) * 100).toFixed(1)}%`;
        row.hold.dataset.band = bandFor(held);
        row.hold.style.opacity = meter.live && held > HOLD_VISIBLE_DB ? "1" : "0";
      }
      if (row.reductionFill && row.reductionText) {
        const reduction = meter.live ? (meter.gainReduction[stage.id] ?? 0) : 0;
        row.reductionFill.style.width = `${Math.min(100, (reduction / GR_FULL_SCALE_DB) * 100).toFixed(1)}%`;
        row.reductionText.textContent = reductionText(reduction);
        row.reductionText.classList.toggle("is-active", reduction >= 0.05);
      }
      if (row.curveFill && row.curveLine) {
        const response = off ? FLAT : responseFor(schema, stage, values);
        const path = responsePath(response, 120, 20, 40, 2);
        row.curveFill.setAttribute("d", path.fill);
        row.curveLine.setAttribute("d", path.line);
      }
      if (row.clip) {
        row.clip.classList.toggle("is-clipped", meter.clipLatched);
      }
    }
  };

  session.store.subscribe((state) => render(state, meters.store.get()));
  meters.store.subscribe((meter) => render(session.store.get(), meter));
  render(session.store.get(), meters.store.get());
}

function buildRow(
  doc: Document,
  session: Session,
  meters: MeterFeed,
  stage: StageSpec,
  index: number,
): Row {
  const item = create(doc, "li", "chain-item");
  const button = create(doc, "button", "chain-row");
  button.type = "button";
  button.dataset.stage = stage.id;
  button.addEventListener("click", () => session.select(stage.id));
  const head = create(doc, "span", "chain-head");
  const label = create(doc, "span", "chain-label", stage.label);
  head.append(label);
  if (stage.optional && stage.latencyMs > 0) {
    head.append(create(doc, "span", "chain-badge", `+${Math.round(stage.latencyMs)} ms`));
  }
  const value = create(doc, "span", "chain-value");
  head.append(value);
  button.append(head);
  const row: Row = { stage, item, button, label, value, number: create(doc, "span", "chain-num", padIndex(index + 1)) };

  if (stage.family === "level") {
    item.classList.add("is-level");
    const level = create(doc, "span", "chain-level");
    const meter = create(doc, "span", "chain-meter");
    row.cover = create(doc, "span", "chain-cover");
    row.hold = create(doc, "span", "chain-hold");
    meter.append(row.cover, row.hold);
    const scale = create(doc, "span", "chain-scale");
    scale.setAttribute("aria-hidden", "true");
    for (const [text, left] of SCALE_TICKS) {
      const tick = create(doc, "span", "", text);
      tick.style.left = left;
      tick.style.transform = `translateX(-${left})`;
      scale.append(tick);
    }
    level.append(meter, scale);
    button.append(level);
  } else if (reportsGainReduction(stage)) {
    const reduction = create(doc, "span", "chain-gr");
    const track = create(doc, "span", "chain-gr-track");
    row.reductionFill = create(doc, "span", "chain-gr-fill");
    track.append(row.reductionFill);
    row.reductionText = create(doc, "span", "chain-gr-text", "0.0");
    reduction.append(track, row.reductionText);
    button.append(reduction);
  } else if (hasResponseCurve(stage)) {
    const svg = doc.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("class", "chain-curve");
    svg.setAttribute("viewBox", "0 0 120 20");
    svg.setAttribute("preserveAspectRatio", "none");
    svg.setAttribute("aria-hidden", "true");
    const zero = doc.createElementNS("http://www.w3.org/2000/svg", "path");
    zero.setAttribute("class", "chain-curve-zero");
    zero.setAttribute("d", "M0 10H120");
    row.curveFill = doc.createElementNS("http://www.w3.org/2000/svg", "path");
    row.curveFill.setAttribute("class", "chain-curve-fill");
    row.curveLine = doc.createElementNS("http://www.w3.org/2000/svg", "path");
    row.curveLine.setAttribute("class", "chain-curve-line");
    svg.append(zero, row.curveFill, row.curveLine);
    button.append(svg);
  }

  item.append(button, row.number);
  row.number.setAttribute("aria-hidden", "true");

  if (stage.bypassable) {
    const power = create(doc, "button", "chain-power");
    power.type = "button";
    power.setAttribute("aria-label", `Bypass ${stage.title}`);
    power.innerHTML =
      '<svg width="13" height="13" viewBox="0 0 14 14" aria-hidden="true"><path d="M7 1.8v4.4"></path><path d="M4.1 3.6a4.6 4.6 0 1 0 5.8 0"></path></svg>';
    power.addEventListener("click", () => session.toggleBypass(stage.id));
    row.power = power;
    item.append(power);
  }
  if (stage.id === "output") {
    const clip = create(doc, "button", "chain-clip", "CLIP");
    clip.type = "button";
    clip.setAttribute("aria-label", "Clear clip indicator");
    clip.addEventListener("click", () => meters.clearClip());
    row.clip = clip;
    item.append(clip);
  }
  return row;
}
