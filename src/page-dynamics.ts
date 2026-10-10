import {
  compressorInputFor,
  compressorOut,
  expanderInputFor,
  expanderOut,
  historyPath,
  reductionChartY,
  transferPath,
  transferUnit,
} from "./curves.ts";
import { create } from "./dom.ts";
import type { StageSpec } from "./engine.ts";
import { formatDb, formatRatio } from "./format.ts";
import { createGraphHandle, type GraphHandle } from "./graph-handle.ts";
import type { MeterState } from "./meters.ts";
import { svgElement, type PageContext, type PageView } from "./page-context.ts";
import { displayedValues, effectiveBypassed, stageParams, type SessionState } from "./session-state.ts";

type Kind = "compressor" | "expander" | "limiter";

const MAX_REDUCTION: Record<Kind, number> = { expander: 36, compressor: 18, limiter: 12 };

interface Model {
  kind: Kind;
  threshold: number;
  curve(input: number): number;
  summary: string;
  liveInput(reduction: number): number | null;
}

export function dynamicsKind(stage: StageSpec, paramIds: string[]): Kind {
  if (paramIds.includes(`${stage.id}.range`)) {
    return "expander";
  }
  return paramIds.includes(`${stage.id}.ratio`) ? "compressor" : "limiter";
}

export function dynamicsModel(stage: StageSpec, kind: Kind, values: Record<string, number>, bypassed: boolean): Model {
  const value = (name: string, fallback: number): number => values[`${stage.id}.${name}`] ?? fallback;
  if (kind === "compressor") {
    const threshold = value("threshold", -18);
    const ratio = value("ratio", 3);
    const knee = value("knee", 6);
    return {
      kind,
      threshold,
      curve: (x) => (bypassed ? x : compressorOut(x, threshold, ratio, knee)),
      summary: `THRESHOLD ${formatDb(threshold)} · ${formatRatio(ratio, false)}\nKNEE ${formatDb(knee)} · MAKEUP +${formatDb(value("makeup", 0))}`,
      liveInput: (reduction) => (!bypassed && reduction >= 0.05 ? compressorInputFor(reduction, threshold, ratio, knee) : null),
    };
  }
  if (kind === "expander") {
    const threshold = value("threshold", -45);
    const ratio = value("ratio", 2.5);
    const knee = value("knee", 6);
    const range = value("range", 30);
    return {
      kind,
      threshold,
      curve: (x) => (bypassed ? x : Math.max(expanderOut(x, threshold, ratio, knee), x - range)),
      summary: `THRESHOLD ${formatDb(threshold)} · ${formatRatio(ratio, true)}\nRANGE ${formatDb(range)} · KNEE ${formatDb(knee)}`,
      liveInput: (reduction) =>
        !bypassed && reduction >= 0.05 && reduction < range - 0.05 ? expanderInputFor(reduction, threshold, ratio, knee) : null,
    };
  }
  const ceiling = value("ceiling", -1);
  return {
    kind,
    threshold: ceiling,
    curve: (x) => (bypassed ? x : Math.min(x, ceiling)),
    summary: `CEILING ${formatDb(ceiling)}\nLOOKAHEAD ${stage.latencyMs.toFixed(1)} ms (FIXED)`,
    liveInput: (reduction) => (!bypassed && reduction >= 0.05 ? ceiling + reduction : null),
  };
}

export function mountDynamicsPage(context: PageContext, stage: StageSpec): PageView {
  const { doc, session, host } = context;
  const { schema } = session;
  const paramIds = stageParams(schema, stage.id).map((spec) => spec.id);
  const kind = dynamicsKind(stage, paramIds);
  const maxReduction = MAX_REDUCTION[kind];

  const root = create(doc, "div", "page-dynamics");
  const left = create(doc, "div", "transfer-column");
  const transfer = create(doc, "div", "chart chart-transfer");
  const svg = svgElement(doc, "svg", { viewBox: "0 0 200 200", preserveAspectRatio: "none", "aria-hidden": "true" });
  const grid = svgElement(doc, "path", { d: "M50 0V200M100 0V200M150 0V200M0 50H200M0 100H200M0 150H200", class: "chart-grid" });
  const diagonal = svgElement(doc, "path", { d: "M0 200L200 0", class: "chart-diagonal" });
  const thresholdLine = svgElement(doc, "line", { y1: "0", y2: "200", class: "chart-threshold" });
  const curve = svgElement(doc, "path", { class: "chart-transfer-line" });
  svg.append(grid, diagonal, thresholdLine, curve);
  const labels = create(doc, "span", "chart-labels");
  labels.setAttribute("aria-hidden", "true");
  labels.innerHTML =
    '<span class="tf-out">OUT</span><span class="tf-in">IN</span><span class="tf-min">-80</span><span class="tf-mid-x">-40</span><span class="tf-mid-y">-40</span>';
  const dot = create(doc, "span", "transfer-dot");
  dot.setAttribute("aria-hidden", "true");
  dot.hidden = true;
  transfer.append(svg, labels, dot);
  let handle: GraphHandle | null = null;
  if (kind !== "limiter") {
    handle = createGraphHandle(
      doc,
      {
        key: "threshold",
        xParam: `${stage.id}.threshold`,
        fromX: (unit) => -80 + 80 * unit,
        aria: "Threshold point. Drag sideways, or use the arrow keys.",
      },
      host,
    );
    transfer.append(handle.element);
  }
  const summary = create(doc, "span", "transfer-summary");
  left.append(transfer, summary);

  const history = create(doc, "div", "chart chart-gr");
  const historySvg = svgElement(doc, "svg", { viewBox: "0 0 400 200", preserveAspectRatio: "none", "aria-hidden": "true" });
  const historyGrid = svgElement(doc, "path", { d: "M0 8H400M0 100H400M0 192H400", class: "chart-grid" });
  const historyFill = svgElement(doc, "path", { class: "chart-gr-fill" });
  const historyLine = svgElement(doc, "path", { class: "chart-gr-line" });
  historySvg.append(historyGrid, historyFill, historyLine);
  history.append(historySvg, create(doc, "span", "chart-gr-label", "GAIN REDUCTION · LAST 6 s"));
  for (const mark of [0, maxReduction / 2, maxReduction]) {
    const tick = create(doc, "span", "chart-gr-tick", mark ? `-${mark}` : "0");
    tick.setAttribute("aria-hidden", "true");
    tick.style.top = `${(reductionChartY(mark, maxReduction) / 2).toFixed(1)}%`;
    history.append(tick);
  }
  const stopped = create(doc, "span", "chart-stopped", "ENGINE STOPPED");
  history.append(stopped);
  root.append(left, history);

  const update = (state: SessionState, meter: MeterState): void => {
    const values = displayedValues(state);
    const bypassed = Boolean(effectiveBypassed(schema, state)[stage.id]);
    const model = dynamicsModel(stage, kind, values, bypassed);
    transfer.classList.toggle("is-bypassed", bypassed);
    curve.setAttribute("d", transferPath(model.curve));
    const thresholdX = ((model.threshold + 80) * 2.5).toFixed(1);
    thresholdLine.setAttribute("x1", thresholdX);
    thresholdLine.setAttribute("x2", thresholdX);
    summary.textContent = model.summary;
    handle?.place(transferUnit(model.threshold), 1 - transferUnit(model.curve(model.threshold)));

    const reduction = meter.live ? (meter.gainReduction[stage.id] ?? 0) : 0;
    const liveInput = meter.live ? model.liveInput(reduction) : null;
    dot.hidden = liveInput === null;
    if (liveInput !== null) {
      dot.style.left = `${(transferUnit(liveInput) * 100).toFixed(1)}%`;
      dot.style.top = `${((1 - transferUnit(model.curve(liveInput))) * 100).toFixed(1)}%`;
    }
    const series = meter.live ? (meter.history.reduction[stage.id] ?? []) : [];
    const path = historyPath(series, 400, (value) => reductionChartY(value, maxReduction), 8);
    historyFill.setAttribute("d", path.fill);
    historyLine.setAttribute("d", path.line);
    stopped.hidden = meter.live;
  };

  return { element: root, update };
}
