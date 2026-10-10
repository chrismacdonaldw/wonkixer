export const LEVEL_FLOOR_DB = -60;
export const LEVEL_CEILING_DB = 0;
export const HISTORY_POINTS = 60;

export function levelFraction(db: number): number {
  const fraction = (db - LEVEL_FLOOR_DB) / (LEVEL_CEILING_DB - LEVEL_FLOOR_DB);
  return Math.min(1, Math.max(0, fraction));
}

export function highPassDb(frequency: number, cutoff: number): number {
  return -10 * Math.log10(1 + Math.pow(cutoff / frequency, 4));
}

export interface EqBand {
  frequency: number;
  gain: number;
  q?: number;
}

export function bellDb(frequency: number, band: EqBand): number {
  const q = band.q ?? 1;
  const r = frequency / band.frequency - band.frequency / frequency;
  return band.gain / (1 + q * q * r * r);
}

export function lowShelfDb(frequency: number, band: EqBand): number {
  return band.gain / (1 + Math.pow(frequency / band.frequency, 2));
}

export function highShelfDb(frequency: number, band: EqBand): number {
  return band.gain / (1 + Math.pow(band.frequency / frequency, 2));
}

export interface EqShape {
  lowShelf: EqBand;
  peaks: EqBand[];
  highShelf: EqBand;
}

export function eqDb(frequency: number, shape: EqShape): number {
  return (
    lowShelfDb(frequency, shape.lowShelf) +
    shape.peaks.reduce((sum, band) => sum + bellDb(frequency, band), 0) +
    highShelfDb(frequency, shape.highShelf)
  );
}

export function compressorOut(input: number, threshold: number, ratio: number, knee: number): number {
  if (2 * (input - threshold) < -knee) {
    return input;
  }
  if (knee > 0 && 2 * Math.abs(input - threshold) <= knee) {
    return input + ((1 / ratio - 1) * Math.pow(input - threshold + knee / 2, 2)) / (2 * knee);
  }
  return threshold + (input - threshold) / ratio;
}

export function expanderOut(input: number, threshold: number, ratio: number, knee: number): number {
  if (2 * (input - threshold) > knee) {
    return input;
  }
  if (knee > 0 && 2 * Math.abs(input - threshold) <= knee) {
    return input - ((ratio - 1) * Math.pow(input - threshold - knee / 2, 2)) / (2 * knee);
  }
  return threshold + ratio * (input - threshold);
}

export function compressorInputFor(reduction: number, threshold: number, ratio: number, knee: number): number {
  let low = threshold - knee / 2;
  let high = 24;
  for (let i = 0; i < 40; i++) {
    const mid = (low + high) / 2;
    if (mid - compressorOut(mid, threshold, ratio, knee) < reduction) {
      low = mid;
    } else {
      high = mid;
    }
  }
  return (low + high) / 2;
}

export function expanderInputFor(reduction: number, threshold: number, ratio: number, knee: number): number {
  let low = -80;
  let high = threshold + knee / 2;
  for (let i = 0; i < 40; i++) {
    const mid = (low + high) / 2;
    if (mid - expanderOut(mid, threshold, ratio, knee) > reduction) {
      low = mid;
    } else {
      high = mid;
    }
  }
  return (low + high) / 2;
}

export interface PathPair {
  line: string;
  fill: string;
}

export const RESPONSE_MIN_HZ = 20;
export const RESPONSE_DECADES = 3;

export function frequencyToUnit(frequency: number): number {
  return Math.log10(frequency / RESPONSE_MIN_HZ) / RESPONSE_DECADES;
}

export function unitToFrequency(unit: number): number {
  return RESPONSE_MIN_HZ * Math.pow(10, RESPONSE_DECADES * unit);
}

const RESPONSE_SPAN_DB = 18;
const RESPONSE_PLOT_HALF = 92;

export function gainToUnit(gain: number): number {
  return (100 - Math.max(-RESPONSE_SPAN_DB, Math.min(RESPONSE_SPAN_DB, gain)) * (RESPONSE_PLOT_HALF / RESPONSE_SPAN_DB)) / 200;
}

export function unitToGain(unit: number): number {
  return (100 - unit * 200) / (RESPONSE_PLOT_HALF / RESPONSE_SPAN_DB);
}

export function responsePath(
  gainAt: (frequency: number) => number,
  width: number,
  height: number,
  samples: number,
  margin: number,
): PathPair {
  const mid = height / 2;
  const scale = (mid - margin) / RESPONSE_SPAN_DB;
  let line = "";
  for (let i = 0; i <= samples; i++) {
    const frequency = unitToFrequency(i / samples);
    const gain = Math.max(-RESPONSE_SPAN_DB, Math.min(RESPONSE_SPAN_DB, gainAt(frequency)));
    line += `${i ? "L" : "M"}${((i / samples) * width).toFixed(1)} ${(mid - gain * scale).toFixed(1)} `;
  }
  return { line, fill: `${line}L${width} ${mid} L0 ${mid} Z` };
}

export function historyPath(values: number[], width: number, yOf: (value: number) => number, baseY: number): PathPair {
  if (!values.length) {
    return { line: "", fill: "" };
  }
  const dx = width / (HISTORY_POINTS - 1);
  const offset = HISTORY_POINTS - values.length;
  let line = "";
  for (let i = 0; i < values.length; i++) {
    line += `${i ? "L" : "M"}${((offset + i) * dx).toFixed(1)} ${yOf(values[i]).toFixed(1)} `;
  }
  return { line, fill: `${line}L${width} ${baseY} L${(offset * dx).toFixed(1)} ${baseY} Z` };
}

export const TRANSFER_MIN_DB = -80;

export function transferPath(outputFor: (input: number) => number): string {
  let path = "";
  for (let i = 0; i <= 80; i++) {
    const input = TRANSFER_MIN_DB + i;
    const output = Math.max(TRANSFER_MIN_DB, Math.min(0, outputFor(input)));
    path += `${i ? "L" : "M"}${((input - TRANSFER_MIN_DB) * 2.5).toFixed(1)} ${(200 - (output - TRANSFER_MIN_DB) * 2.5).toFixed(1)} `;
  }
  return path;
}

export function transferUnit(db: number): number {
  return Math.max(0, Math.min(1, (db - TRANSFER_MIN_DB) / -TRANSFER_MIN_DB));
}

export function levelChartY(db: number): number {
  return 8 + (-Math.max(LEVEL_FLOOR_DB, Math.min(LEVEL_CEILING_DB, db)) / -LEVEL_FLOOR_DB) * 184;
}

export function reductionChartY(reduction: number, maxReduction: number): number {
  return 8 + (Math.min(reduction, maxReduction) / maxReduction) * 184;
}

export function percent(fraction: number): string {
  return `${(fraction * 100).toFixed(1)}%`;
}
