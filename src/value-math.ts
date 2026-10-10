import type { ParamSpec } from "./engine.ts";

export function fineStep(spec: ParamSpec): number {
  return spec.step;
}

export function coarseStep(spec: ParamSpec): number {
  return spec.coarse;
}

export function snapValue(spec: ParamSpec, value: number, fine: boolean): number {
  const step = fine ? fineStep(spec) : coarseStep(spec);
  const stepped = Math.round(value / step) * step;
  const decimals = Math.max(0, -Math.floor(Math.log10(step) + 1e-9));
  const rounded = Number(stepped.toFixed(decimals));
  return Math.min(spec.max, Math.max(spec.min, rounded));
}

export function isModified(spec: ParamSpec, value: number): boolean {
  return Math.abs(value - spec.default) > 1e-6;
}

export function isBipolarGain(spec: ParamSpec): boolean {
  return spec.unit === "decibels" && spec.min < 0 && spec.max > 0;
}

export function isThresholdLike(spec: ParamSpec): boolean {
  return /threshold$/.test(spec.id) || spec.id === "limiter.ceiling";
}

export interface DragSpace {
  range: number;
  toUnit(value: number): number;
  fromUnit(unit: number): number;
}

const LOG_UNITS: ReadonlySet<ParamSpec["unit"]> = new Set(["ratio", "hertz", "milliseconds", "q"]);
const MIN_DRAG_PX = 120;

export function dragSpace(spec: ParamSpec): DragSpace {
  const logarithmic = spec.min > 0 && LOG_UNITS.has(spec.unit);
  const low = logarithmic ? Math.log(spec.min) : spec.min;
  const high = logarithmic ? Math.log(spec.max) : spec.max;
  return {
    range: Math.max(MIN_DRAG_PX, dragPixels(spec)),
    toUnit: (value) => ((logarithmic ? Math.log(Math.max(spec.min, value)) : value) - low) / (high - low),
    fromUnit: (unit) => {
      const x = low + Math.max(0, Math.min(1, unit)) * (high - low);
      return logarithmic ? Math.exp(x) : x;
    },
  };
}

function dragPixels(spec: ParamSpec): number {
  switch (spec.unit) {
    case "decibels":
      return (spec.max - spec.min) * (/\.gain$|makeup$/.test(spec.id) ? 8 : 6);
    case "hertz":
      return Math.log2(spec.max / spec.min) * 36;
    case "milliseconds":
      return spec.min > 0 ? Math.log10(spec.max / spec.min) * 75 : 250;
    case "ratio":
      return 300;
    default:
      return 250;
  }
}

export function parseInput(spec: ParamSpec, text: string): number | null {
  const raw = String(text).trim().toLowerCase().replace(/\s+/g, "").replace(",", ".");
  let body = raw;
  let multiplier = 1;
  if (spec.unit === "ratio") {
    body = body.replace(/^1:/, "").replace(/:1$/, "");
  }
  if (spec.unit === "decibels") {
    body = body.replace(/db$/, "");
  }
  if (spec.unit === "hertz") {
    const split = body.match(/^(\d+)k(\d+)$/);
    if (split) {
      body = `${split[1]}.${split[2]}`;
      multiplier = 1000;
    } else if (/khz$/.test(body)) {
      multiplier = 1000;
      body = body.slice(0, -3);
    } else if (/hz$/.test(body)) {
      body = body.slice(0, -2);
    } else if (/k$/.test(body)) {
      multiplier = 1000;
      body = body.slice(0, -1);
    }
  }
  if (spec.unit === "milliseconds") {
    if (/ms$/.test(body)) {
      body = body.slice(0, -2);
    } else if (/s$/.test(body)) {
      multiplier = 1000;
      body = body.slice(0, -1);
    }
  }
  if (!/^[-+]?(\d+\.?\d*|\.\d+)$/.test(body)) {
    return null;
  }
  let value = parseFloat(body) * multiplier;
  if (spec.max <= 0 && value > 0 && body.charAt(0) !== "+") {
    value = -value;
  }
  return snapValue(spec, value, true);
}

export function stepped(spec: ParamSpec, value: number, direction: 1 | -1, fine: boolean): number {
  let next: number;
  switch (spec.unit) {
    case "hertz":
      next = value * Math.pow(2, direction * (fine ? 1 / 120 : 1 / 12));
      break;
    case "milliseconds":
      next = value * Math.pow(fine ? 1.005 : 1.05, direction);
      break;
    case "q":
      next = value * Math.pow(fine ? 1.006 : 1.06, direction);
      break;
    case "decibels":
      next = value + direction * (fine ? fineStep(spec) : Math.max(0.5, coarseStep(spec)));
      break;
    default:
      next = value + direction * (fine ? fineStep(spec) : coarseStep(spec));
  }
  let snapped = snapValue(spec, next, fine);
  if (snapped === value) {
    snapped = snapValue(spec, value + direction * (fine ? fineStep(spec) : coarseStep(spec)), fine);
  }
  return snapped;
}

const PAGE_SERIES: Partial<Record<ParamSpec["unit"], number[]>> = {
  ratio: [1, 1.5, 2, 3, 4, 6, 8, 10, 20],
  milliseconds: [0, 0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000],
  hertz: [
    20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000,
    6300, 8000, 10000, 12500, 16000,
  ],
  q: [0.3, 0.5, 0.71, 1, 1.4, 2, 2.8, 4],
};

export function pageStep(spec: ParamSpec, value: number, direction: 1 | -1): number {
  const series = PAGE_SERIES[spec.unit];
  if (series) {
    const inRange = series.filter((x) => x >= spec.min && x <= spec.max);
    const next =
      direction > 0 ? inRange.find((x) => x > value + 1e-9) : [...inRange].reverse().find((x) => x < value - 1e-9);
    return next === undefined ? value : snapValue(spec, next, true);
  }
  return snapValue(spec, value + direction * (/threshold$/.test(spec.id) ? 6 : 3), false);
}

export interface WheelResult {
  steps: number;
  remainder: number;
}

export function wheelSteps(accumulated: number, delta: number, notch: number): WheelResult {
  let remainder = accumulated + delta;
  let steps = 0;
  while (Math.abs(remainder) >= notch) {
    const direction = remainder < 0 ? 1 : -1;
    remainder += direction * notch;
    steps += direction;
  }
  return { steps, remainder };
}

export function wheelNotch(deltaMode: number): number {
  return deltaMode === 1 ? 1 : 50;
}

export function switchOn(value: number): boolean {
  return value >= 0.5;
}
