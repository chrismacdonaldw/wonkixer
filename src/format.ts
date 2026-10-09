import type { ParamSpec } from "./engine.ts";

const THOUSANDS_SEPARATOR = " ";

export function formatDb(value: number): string {
  return `${value.toFixed(1)} dB`;
}

export function formatMs(value: number): string {
  return value < 10 ? `${value.toFixed(1)} ms` : `${Math.round(value)} ms`;
}

export function formatHz(value: number): string {
  return `${formatCount(value)} Hz`;
}

export function formatCount(value: number): string {
  return Math.round(value)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, THOUSANDS_SEPARATOR);
}

export function formatRatio(value: number, inverted: boolean): string {
  return inverted ? `1:${value.toFixed(1)}` : `${value.toFixed(1)}:1`;
}

export function formatValue(spec: ParamSpec, value: number): string {
  switch (spec.unit) {
    case "decibels":
      return formatDb(value);
    case "hertz":
      return formatHz(value);
    case "milliseconds":
      return formatMs(value);
    case "ratio":
      return formatRatio(value, spec.stage === "expander");
    case "q":
      return value.toFixed(2);
    case "switch":
      return value >= 0.5 ? "ON" : "OFF";
  }
}
