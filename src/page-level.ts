import { historyPath, levelChartY } from "./curves.ts";
import { create } from "./dom.ts";
import type { StageSpec } from "./engine.ts";
import { formatDb } from "./format.ts";
import type { MeterState } from "./meters.ts";
import { DASH, svgElement, type PageContext, type PageView } from "./page-context.ts";
import { displayedValues, effectiveBypassed, stageParams, type SessionState } from "./session-state.ts";

const Y_LABELS: Array<[string, string]> = [
  ["0", "4%"],
  ["-6", "13.2%"],
  ["-12", "22.4%"],
  ["-20", "34.7%"],
  ["-40", "65.3%"],
  ["-60", "96%"],
];

type Tone = "ok" | "warn" | "bad" | "idle";

export function inputHint(live: boolean, intoChainDb: number): [Tone, string] {
  if (!live) {
    return ["idle", "Start the engine to see your levels."];
  }
  if (intoChainDb > -6) {
    return ["warn", "Hot. Pull the gain down a few dB."];
  }
  if (intoChainDb < -20) {
    return ["warn", "Quiet. Raise the gain until peaks reach about -12 dBFS."];
  }
  return ["ok", "Healthy. Peaks are landing around -12 dBFS."];
}

export function outputHint(live: boolean, clipped: boolean): [Tone, string] {
  if (!live) {
    return ["idle", "Start the engine to see your levels."];
  }
  return clipped
    ? ["bad", "The 0 dBFS safety clip was hit. Lower the output gain or switch the limiter on."]
    : ["ok", "Clean. Nothing is reaching the safety clip."];
}

export function mountLevelPage(context: PageContext, stage: StageSpec): PageView {
  const { doc, session } = context;
  const isInput = stage.id === "input";
  const gainId = stageParams(session.schema, stage.id, "main")[0]?.id ?? "";
  const root = create(doc, "div", "page-level");
  const chart = create(doc, "div", "chart chart-level");
  const svg = svgElement(doc, "svg", { viewBox: "0 0 600 200", preserveAspectRatio: "none", "aria-hidden": "true" });
  const target = svgElement(doc, "rect", { x: "0", y: "26.4", width: "600", height: "36.8", class: "chart-target" });
  target.style.opacity = isInput ? "1" : "0";
  const grid = svgElement(doc, "path", { d: "M0 26.4H600M0 44.8H600M0 69.3H600M0 130.7H600M0 192H600", class: "chart-grid" });
  const histFill = svgElement(doc, "path", { class: "chart-hist-fill" });
  const histLine = svgElement(doc, "path", { class: "chart-hist-line" });
  histLine.style.opacity = isInput ? "0.55" : "1";
  const trimLine = svgElement(doc, "path", { class: "chart-trim-line" });
  trimLine.style.opacity = isInput ? "1" : "0";
  const ceilingLine = svgElement(doc, "line", { x1: "0", x2: "600", class: "chart-ceiling" });
  const clipLine = svgElement(doc, "line", { x1: "0", x2: "600", y1: "8", y2: "8", class: "chart-clip" });
  clipLine.style.opacity = isInput ? "0" : "0.8";
  svg.append(target, grid, histFill, histLine, trimLine, ceilingLine, clipLine);
  const labels = create(doc, "span", "chart-labels");
  labels.setAttribute("aria-hidden", "true");
  for (const [text, top] of Y_LABELS) {
    const label = create(doc, "span", "chart-y", text);
    label.style.top = top;
    labels.append(label);
  }
  const axis = create(doc, "span", "chart-time");
  axis.append(create(doc, "span", "", "6 s AGO"), create(doc, "span", "", "NOW"));
  labels.append(axis);
  const title = create(doc, "span", "chart-title", isInput ? "MIC PEAK dBFS" : "OUTPUT PEAK dBFS");
  chart.append(svg, labels, title);
  if (isInput) {
    const legend = create(doc, "span", "chart-legend");
    legend.innerHTML =
      '<span><span class="swatch swatch-mic"></span>MIC</span><span><span class="swatch swatch-trim"></span>AFTER TRIM</span><span><span class="swatch swatch-aim"></span>AIM</span>';
    chart.append(legend);
  }
  const ceilingTag = create(doc, "span", "chart-ceiling-tag");
  ceilingTag.hidden = true;
  const stopped = create(doc, "span", "chart-stopped", "ENGINE STOPPED");
  chart.append(ceilingTag, stopped);

  const info = create(doc, "div", "level-info");
  const readouts = create(doc, "dl", "readouts");
  const rowKeys = isInput ? ["PEAK", "HOLD", "INTO CHAIN", "AIM"] : ["PEAK", "HOLD", "CEILING", "SAFETY CLIP"];
  const cells = rowKeys.map((key) => {
    const row = create(doc, "div", "");
    const dd = create(doc, "dd", "", DASH);
    row.append(create(doc, "dt", "", key), dd);
    readouts.append(row);
    return dd;
  });
  const hint = create(doc, "p", "level-hint");
  hint.setAttribute("role", "status");
  info.append(readouts, hint);
  root.append(chart, info);

  const update = (state: SessionState, meter: MeterState): void => {
    const values = displayedValues(state);
    const bypassed = effectiveBypassed(session.schema, state);
    const gain = values[gainId] ?? 0;
    const ceiling = (values["limiter.ceiling"] ?? 0) + (values["output.gain"] ?? 0);
    const limiterOff = Boolean(bypassed.limiter);
    const history = meter.live ? (isInput ? meter.history.input : meter.history.output) : [];
    const path = historyPath(history, 600, levelChartY, 200);
    histFill.setAttribute("d", path.fill);
    histLine.setAttribute("d", path.line);
    if (isInput) {
      trimLine.setAttribute("d", historyPath(history.map((db) => db + gain), 600, levelChartY, 200).line);
    }
    const ceilingY = levelChartY(ceiling);
    ceilingLine.setAttribute("y1", ceilingY.toFixed(1));
    ceilingLine.setAttribute("y2", ceilingY.toFixed(1));
    const showCeiling = !isInput && !limiterOff;
    ceilingLine.style.opacity = showCeiling ? "0.9" : "0";
    ceilingTag.hidden = !showCeiling;
    ceilingTag.textContent = `LIMITER CEILING ${ceiling.toFixed(1)}`;
    ceilingTag.style.top = `${(ceilingY / 2).toFixed(1)}%`;
    stopped.hidden = meter.live;

    const peak = isInput ? meter.inputPeakDb : meter.outputPeakDb;
    const held = isInput ? meter.inputHoldDb : meter.outputHoldDb;
    const live = meter.live;
    cells[0].textContent = live ? formatDb(peak) : DASH;
    cells[1].textContent = live ? formatDb(held) : DASH;
    if (isInput) {
      const into = held + gain;
      cells[2].textContent = live ? formatDb(into) : DASH;
      cells[3].textContent = "-12.0 dB ± 6";
      cells[3].classList.add("is-muted");
      applyHint(hint, inputHint(live, into));
    } else {
      cells[2].textContent = limiterOff ? "LIMITER OFF" : formatDb(ceiling);
      cells[2].classList.toggle("is-hot", limiterOff);
      cells[3].textContent = meter.clipLatched ? "HIT" : "CLEAN";
      cells[3].classList.toggle("is-bad", meter.clipLatched);
      applyHint(hint, outputHint(live, meter.clipLatched));
    }
  };

  return { element: root, update };
}

function applyHint(hint: HTMLElement, [tone, text]: [Tone, string]): void {
  hint.dataset.tone = tone;
  hint.textContent = text;
}
