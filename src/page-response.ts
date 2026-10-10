import { frequencyToUnit, gainToUnit, responsePath, unitToFrequency, unitToGain } from "./curves.ts";
import { create } from "./dom.ts";
import type { StageSpec } from "./engine.ts";
import { formatDb, formatHz } from "./format.ts";
import { createGraphHandle, type GraphHandle } from "./graph-handle.ts";
import type { MeterState } from "./meters.ts";
import { svgElement, type PageContext, type PageView } from "./page-context.ts";
import { bandGroups, displayedValues, effectiveBypassed, stageParams, type SessionState } from "./session-state.ts";
import { cutoffParamId, FLAT, responseFor } from "./stage-curves.ts";
import { switchOn } from "./value-math.ts";

const CHART_HEIGHT: Record<string, string> = { filter: "250px", sibilance: "230px", eq: "200px" };
const Y_LABELS: Array<[string, string]> = [
  ["+12", "19.3%"],
  ["+6", "34.7%"],
  ["0", "50%"],
  ["-6", "65.3%"],
  ["-12", "80.7%"],
];
const X_LABELS: Array<[string, string]> = [
  ["50", "13.3%"],
  ["100", "23.3%"],
  ["200", "33.3%"],
  ["500", "46.6%"],
  ["1k", "56.6%"],
  ["2k", "66.7%"],
  ["5k", "79.9%"],
  ["10k", "90%"],
];

interface HandleBinding {
  handle: GraphHandle;
  frequencyId: string;
  gainId?: string;
}

export function mountResponsePage(context: PageContext, stage: StageSpec): PageView {
  const { doc, session, host } = context;
  const { schema } = session;
  const chart = create(doc, "div", "chart chart-response");
  chart.style.height = CHART_HEIGHT[stage.family] ?? "240px";
  const svg = svgElement(doc, "svg", { viewBox: "0 0 600 200", preserveAspectRatio: "none", "aria-hidden": "true" });
  const band = svgElement(doc, "rect", { x: "0", y: "0", width: "0", height: "200", class: "chart-band" });
  const grid = svgElement(doc, "path", {
    d: "M79.6 0V200M139.8 0V200M200 0V200M279.6 0V200M339.8 0V200M400 0V200M479.6 0V200M539.8 0V200M0 38.7H600M0 69.3H600M0 130.7H600M0 161.3H600",
    class: "chart-grid",
  });
  const zero = svgElement(doc, "path", { d: "M0 100H600", class: "chart-zero" });
  const contextLine = svgElement(doc, "path", { class: "chart-context" });
  const fill = svgElement(doc, "path", { class: "chart-resp-fill" });
  const line = svgElement(doc, "path", { class: "chart-resp-line" });
  svg.append(band, grid, zero, contextLine, fill, line);
  const labels = create(doc, "span", "chart-labels");
  labels.setAttribute("aria-hidden", "true");
  for (const [text, top] of Y_LABELS) {
    const label = create(doc, "span", "chart-y", text);
    label.style.top = top;
    labels.append(label);
  }
  for (const [text, left] of X_LABELS) {
    const label = create(doc, "span", "chart-x", text);
    label.style.left = left;
    labels.append(label);
  }
  const title = create(doc, "span", "chart-title", stage.family === "sibilance" ? "LIVE REDUCTION dB" : "RESPONSE dB");
  const bandTag = create(doc, "span", "chart-band-tag", "SIBILANCE BAND");
  bandTag.hidden = true;
  const tag = create(doc, "span", "chart-tag");
  tag.hidden = true;
  chart.append(svg, labels, title, bandTag);

  let dragging: HandleBinding | null = null;
  const bindings: HandleBinding[] = [];
  const bind = (binding: HandleBinding): void => {
    bindings.push(binding);
    chart.append(binding.handle.element);
  };
  const dragChanged = (binding: HandleBinding, active: boolean): void => {
    dragging = active ? binding : dragging === binding ? null : dragging;
    render(session.store.get(), context.meters.get());
  };

  if (stage.family === "eq") {
    bandGroups(schema, stage.id).forEach((group, index) => {
      const frequency = group.params.find((spec) => spec.unit === "hertz");
      const gain = group.params.find((spec) => spec.unit === "decibels");
      const q = group.params.find((spec) => spec.unit === "q");
      if (!frequency || !gain) {
        return;
      }
      let binding: HandleBinding;
      const handle = createGraphHandle(
        doc,
        {
          key: `eq${index}`,
          number: String(index + 1),
          xParam: frequency.id,
          yParam: gain.id,
          qParam: q?.id,
          fromX: unitToFrequency,
          fromY: unitToGain,
          aria: `${frequency.label} point. Drag, or use arrow keys: left and right for frequency, up and down for gain.`,
        },
        host,
        { onDragChange: (active) => dragChanged(binding, active) },
      );
      binding = { frequencyId: frequency.id, gainId: gain.id, handle };
      bind(binding);
    });
  } else {
    const cutoffId = cutoffParamId(schema, stage.id);
    if (cutoffId) {
      let binding: HandleBinding;
      const handle = createGraphHandle(
        doc,
        {
          key: stage.id,
          xParam: cutoffId,
          fromX: unitToFrequency,
          aria: `${stage.family === "filter" ? "Cutoff" : "Frequency"} point. Drag sideways, or use the arrow keys.`,
        },
        host,
        { onDragChange: (active) => dragChanged(binding, active) },
      );
      binding = { frequencyId: cutoffId, handle };
      bind(binding);
    }
  }
  chart.append(tag);

  const splitId = stageParams(schema, stage.id).find((spec) => spec.unit === "switch")?.id;

  const render = (state: SessionState, meter: MeterState): void => {
    const values = displayedValues(state);
    const bypassed = Boolean(effectiveBypassed(schema, state)[stage.id]);
    chart.classList.toggle("is-bypassed", bypassed);
    let response = bypassed ? FLAT : responseFor(schema, stage, values);
    tag.hidden = true;
    bandTag.hidden = true;
    band.style.opacity = "0";
    contextLine.style.opacity = "0";

    if (stage.family === "sibilance") {
      const frequencyId = bindings[0]?.frequencyId ?? "";
      const frequency = values[frequencyId] ?? 6500;
      const reduction = meter.live ? (meter.gainReduction[stage.id] ?? 0) : 0;
      const split = splitId ? switchOn(values[splitId] ?? 1) : true;
      response = bypassed ? FLAT : (f) => (split ? -reduction / (1 + Math.pow(frequency / f, 4)) : -reduction);
      const unit = frequencyToUnit(frequency);
      band.setAttribute("x", (unit * 600).toFixed(1));
      band.setAttribute("width", (600 - unit * 600).toFixed(1));
      band.style.opacity = bypassed ? "0" : "1";
      bandTag.hidden = bypassed;
      bandTag.style.left = `${(unit * 100).toFixed(1)}%`;
      placeTag(tag, unit, gainToUnit(0), formatHz(frequency));
      bindings[0]?.handle.place(unit, gainToUnit(0));
    } else if (stage.family === "filter") {
      const frequencyId = bindings[0]?.frequencyId ?? "";
      const cutoff = values[frequencyId] ?? 90;
      const unit = frequencyToUnit(cutoff);
      const y = gainToUnit(response(cutoff));
      placeTag(tag, unit, y, `${formatHz(cutoff)} · -3 dB`);
      bindings[0]?.handle.place(unit, y);
    } else {
      for (const binding of bindings) {
        const frequency = values[binding.frequencyId] ?? 1000;
        const gain = binding.gainId ? (values[binding.gainId] ?? 0) : 0;
        const unit = frequencyToUnit(frequency);
        binding.handle.place(unit, gainToUnit(gain));
        if (dragging === binding) {
          placeTag(tag, unit, gainToUnit(gain), `${formatHz(frequency)} · ${formatDb(gain)}`);
        }
      }
      const hpf = schema.stages.find((other) => other.family === "filter");
      if (hpf && !effectiveBypassed(schema, state)[hpf.id]) {
        contextLine.setAttribute("d", responsePath(responseFor(schema, hpf, values), 600, 200, 120, 8).line);
        contextLine.style.opacity = "0.7";
      }
    }
    const path = responsePath(response, 600, 200, 120, 8);
    fill.setAttribute("d", path.fill);
    line.setAttribute("d", path.line);
  };

  return { element: chart, update: render };
}

function placeTag(tag: HTMLElement, x: number, y: number, text: string): void {
  tag.hidden = false;
  tag.textContent = text;
  tag.style.left = `${(x * 100).toFixed(1)}%`;
  tag.style.top = `${(y * 100).toFixed(1)}%`;
}
