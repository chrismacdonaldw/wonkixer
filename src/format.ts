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

export function isInvertedRatio(spec: ParamSpec): boolean {
  return spec.unit === "ratio" && spec.stage === "expander";
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
      return formatRatio(value, isInvertedRatio(spec));
    case "q":
      return value.toFixed(2);
    case "switch":
      return value >= 0.5 ? "ON" : "OFF";
  }
}

const UNIT_SUFFIX: Partial<Record<ParamSpec["unit"], string>> = { decibels: "dB", hertz: "Hz", milliseconds: "ms" };

export function unitSuffix(spec: ParamSpec): string {
  return UNIT_SUFFIX[spec.unit] ?? "";
}

export function formatNumber(spec: ParamSpec, value: number): string {
  const text = formatValue(spec, value);
  const suffix = unitSuffix(spec);
  return suffix ? text.slice(0, -(suffix.length + 1)) : text;
}

export function decimalsFor(spec: ParamSpec): number {
  switch (spec.unit) {
    case "q":
      return 2;
    case "hertz":
      return 0;
    default:
      return 1;
  }
}

export function editText(spec: ParamSpec, value: number): string {
  return value.toFixed(decimalsFor(spec));
}

const SPOKEN_UNIT: Partial<Record<ParamSpec["unit"], string>> = {
  decibels: " decibels",
  hertz: " hertz",
  milliseconds: " milliseconds",
};

export function speakValue(spec: ParamSpec, value: number): string {
  if (spec.unit === "ratio") {
    return isInvertedRatio(spec) ? `1 to ${value.toFixed(1)}` : `${value.toFixed(1)} to 1`;
  }
  const number = spec.unit === "q" ? value.toFixed(2) : String(Number(value.toFixed(1)));
  return `${number}${SPOKEN_UNIT[spec.unit] ?? ""}`;
}
