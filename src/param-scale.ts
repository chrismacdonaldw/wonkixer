import type { ParamSpec } from "./engine.ts";

export const POSITIONS = 1000;

export function toPosition(spec: ParamSpec, value: number): number {
  const clamped = Math.min(spec.max, Math.max(spec.min, value));
  const fraction = isLogarithmic(spec)
    ? Math.log(clamped / spec.min) / Math.log(spec.max / spec.min)
    : (clamped - spec.min) / (spec.max - spec.min);
  return Math.round(fraction * POSITIONS);
}

export function fromPosition(spec: ParamSpec, position: number): number {
  const fraction = Math.min(1, Math.max(0, position / POSITIONS));
  const raw = isLogarithmic(spec)
    ? spec.min * Math.pow(spec.max / spec.min, fraction)
    : spec.min + (spec.max - spec.min) * fraction;
  return snap(spec, raw);
}

export function snap(spec: ParamSpec, value: number): number {
  const stepped = Math.round(value / spec.step) * spec.step;
  const decimals = Math.max(0, -Math.floor(Math.log10(spec.step)));
  const rounded = Number(stepped.toFixed(decimals));
  return Math.min(spec.max, Math.max(spec.min, rounded));
}

function isLogarithmic(spec: ParamSpec): boolean {
  return spec.scale === "logarithmic" && spec.min > 0;
}
