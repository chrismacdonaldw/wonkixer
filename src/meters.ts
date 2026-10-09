import { element } from "./dom.ts";
import { STOPPED, type EngineApi, type EngineState, type MeterFrame } from "./engine.ts";
import { formatCount, formatHz, formatMs } from "./format.ts";

export const METER_FLOOR_DB = -60;
export const METER_CEILING_DB = 0;
export const SCALE_TICKS_DB = [-60, -40, -20, -12, -6, 0];

const HOT_DB = -12;
const CLIP_DB = -3;
const HOLD_MS = 1000;
const FALL_DB_PER_SECOND = 12;
const HOLD_MARKER_PX = 2;

export type Band = "signal" | "hot" | "clip";

export function levelFraction(db: number): number {
  const fraction = (db - METER_FLOOR_DB) / (METER_CEILING_DB - METER_FLOOR_DB);
  return Math.min(1, Math.max(0, fraction));
}

export function bandFor(db: number): Band {
  if (db >= CLIP_DB) {
    return "clip";
  }
  return db >= HOT_DB ? "hot" : "signal";
}

export class PeakHold {
  private heldDb = METER_FLOOR_DB;
  private heldSinceMs = Number.NEGATIVE_INFINITY;
  private lastUpdateMs: number | null = null;

  update(peakDb: number, nowMs: number): number {
    if (peakDb >= this.heldDb) {
      this.heldDb = peakDb;
      this.heldSinceMs = nowMs;
    } else {
      const fallStartMs = Math.max(this.lastUpdateMs ?? nowMs, this.heldSinceMs + HOLD_MS);
      const fallMs = Math.max(0, nowMs - fallStartMs);
      this.heldDb = Math.max(peakDb, this.heldDb - (FALL_DB_PER_SECOND * fallMs) / 1000);
    }
    this.lastUpdateMs = nowMs;
    return this.heldDb;
  }

  reset(): void {
    this.heldDb = METER_FLOOR_DB;
    this.heldSinceMs = Number.NEGATIVE_INFINITY;
    this.lastUpdateMs = null;
  }
}

export interface MeterPalette {
  track: string;
  signal: string;
  hot: string;
  clip: string;
}

export function paintMeter(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  levelDb: number,
  holdDb: number,
  palette: MeterPalette,
): void {
  context.clearRect(0, 0, width, height);
  context.fillStyle = palette.track;
  context.fillRect(0, 0, width, height);

  const level = levelFraction(levelDb) * width;
  const segments: Array<[number, number, string]> = [
    [0, levelFraction(HOT_DB), palette.signal],
    [levelFraction(HOT_DB), levelFraction(CLIP_DB), palette.hot],
    [levelFraction(CLIP_DB), 1, palette.clip],
  ];
  for (const [from, to, color] of segments) {
    const start = from * width;
    const end = Math.min(level, to * width);
    if (end > start) {
      context.fillStyle = color;
      context.fillRect(start, 0, end - start, height);
    }
  }

  if (holdDb > METER_FLOOR_DB) {
    context.fillStyle = palette[bandFor(holdDb)];
    const x = Math.min(width - HOLD_MARKER_PX, levelFraction(holdDb) * width);
    context.fillRect(x, 0, HOLD_MARKER_PX, height);
  }
}

interface MeterView {
  canvas: HTMLCanvasElement;
  context: CanvasRenderingContext2D;
  hold: PeakHold;
}

export async function initMeters(
  engine: EngineApi,
  doc: Document = document,
  now: () => number = () => performance.now(),
): Promise<void> {
  const palette = readPalette(doc);
  const input = meterView(element<HTMLCanvasElement>("#meter-input", doc));
  const output = meterView(element<HTMLCanvasElement>("#meter-output", doc));
  const clipIndicator = element<HTMLButtonElement>("#clip-indicator", doc);
  const rate = element<HTMLElement>("#readout-rate", doc);
  const latency = element<HTMLElement>("#readout-latency", doc);
  const dropouts = element<HTMLElement>("#readout-dropouts", doc);

  renderScale(element<HTMLElement>("#meter-scale", doc), doc);
  clipIndicator.addEventListener("click", () => clipIndicator.classList.remove("is-clipped"));

  const paint = (view: MeterView, levelDb: number, timestamp: number): void => {
    const holdDb = view.hold.update(levelDb, timestamp);
    paintMeter(view.context, view.canvas.width, view.canvas.height, levelDb, holdDb, palette);
  };

  const renderFrame = (frame: MeterFrame): void => {
    const timestamp = now();
    paint(input, frame.inputPeakDb, timestamp);
    paint(output, frame.outputPeakDb, timestamp);
    latency.textContent = formatMs(frame.latencyMs);
    dropouts.textContent = formatCount(frame.dropouts);
    if (frame.clipped) {
      clipIndicator.classList.add("is-clipped");
    }
  };

  const renderState = (state: EngineState): void => {
    rate.textContent = state.sampleRate === null ? "—" : formatHz(state.sampleRate);
    if (!state.running) {
      input.hold.reset();
      output.hold.reset();
      paint(input, METER_FLOOR_DB, now());
      paint(output, METER_FLOOR_DB, now());
      latency.textContent = "—";
      dropouts.textContent = "—";
    }
  };

  await engine.onMeters(renderFrame);
  await engine.onState(renderState);
  renderState(await engine.state().catch(() => STOPPED));
}

function meterView(canvas: HTMLCanvasElement): MeterView {
  const scale = canvas.ownerDocument.defaultView?.devicePixelRatio ?? 1;
  const bounds = canvas.getBoundingClientRect();
  canvas.width = Math.max(1, Math.round(bounds.width * scale));
  canvas.height = Math.max(1, Math.round(bounds.height * scale));
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("Canvas 2D context is unavailable");
  }
  return { canvas, context, hold: new PeakHold() };
}

function renderScale(scale: HTMLElement, doc: Document): void {
  scale.replaceChildren(
    ...SCALE_TICKS_DB.map((db) => {
      const tick = doc.createElement("span");
      const fraction = levelFraction(db);
      tick.textContent = String(db);
      tick.style.left = `${fraction * 100}%`;
      tick.style.transform = `translateX(-${fraction * 100}%)`;
      return tick;
    }),
  );
}

function readPalette(doc: Document): MeterPalette {
  const styles = doc.defaultView?.getComputedStyle(doc.documentElement);
  const token = (name: string, fallback: string): string => styles?.getPropertyValue(name).trim() || fallback;
  return {
    track: token("--line-bright", "#2d2a26"),
    signal: token("--signal", "#3fd3c1"),
    hot: token("--hot", "#f0b23c"),
    clip: token("--clip", "#ff4655"),
  };
}
