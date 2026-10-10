import { HISTORY_POINTS, LEVEL_FLOOR_DB, levelFraction } from "./curves.ts";
import type { EngineApi, EngineState, MeterFrame } from "./engine.ts";
import { createStore, type Store } from "./store.ts";

export { levelFraction };
export const METER_FLOOR_DB = LEVEL_FLOOR_DB;

const HOT_DB = -12;
const CLIP_DB = -3;
const HOLD_MS = 1000;
const FALL_DB_PER_SECOND = 12;
const FRAMES_PER_SAMPLE = 3;

export type Band = "signal" | "hot" | "clip";

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

export interface MeterHistory {
  input: number[];
  output: number[];
  reduction: Record<string, number[]>;
}

export interface MeterState {
  live: boolean;
  sampleRate: number | null;
  inputPeakDb: number;
  outputPeakDb: number;
  inputHoldDb: number;
  outputHoldDb: number;
  clipLatched: boolean;
  latencyMs: number;
  dropouts: number;
  gainReduction: Record<string, number>;
  detectorDb: Record<string, number>;
  history: MeterHistory;
}

export const SILENT_METERS: MeterState = {
  live: false,
  sampleRate: null,
  inputPeakDb: METER_FLOOR_DB,
  outputPeakDb: METER_FLOOR_DB,
  inputHoldDb: METER_FLOOR_DB,
  outputHoldDb: METER_FLOOR_DB,
  clipLatched: false,
  latencyMs: 0,
  dropouts: 0,
  gainReduction: {},
  detectorDb: {},
  history: { input: [], output: [], reduction: {} },
};

export function pushHistory(values: number[], value: number): number[] {
  const next = values.slice(-(HISTORY_POINTS - 1));
  next.push(value);
  return next;
}

export interface MeterFeed {
  store: Store<MeterState>;
  clearClip(): void;
  frame(frame: MeterFrame): void;
  state(state: EngineState): void;
}

export function createMeterFeed(now: () => number = () => performance.now()): MeterFeed {
  const store = createStore<MeterState>(SILENT_METERS);
  const inputHold = new PeakHold();
  const outputHold = new PeakHold();
  let pending = 0;
  let inputMax = METER_FLOOR_DB;
  let outputMax = METER_FLOOR_DB;
  let reductionMax: Record<string, number> = {};

  const resetSampling = (): void => {
    pending = 0;
    inputMax = METER_FLOOR_DB;
    outputMax = METER_FLOOR_DB;
    reductionMax = {};
  };

  return {
    store,
    clearClip: () => store.patch({ clipLatched: false }),
    frame(frame) {
      const timestamp = now();
      const inputPeakDb = Math.max(METER_FLOOR_DB, frame.inputPeakDb);
      const outputPeakDb = Math.max(METER_FLOOR_DB, frame.outputPeakDb);
      inputMax = Math.max(inputMax, inputPeakDb);
      outputMax = Math.max(outputMax, outputPeakDb);
      for (const [stage, reduction] of Object.entries(frame.gainReduction)) {
        reductionMax[stage] = Math.max(reductionMax[stage] ?? 0, reduction);
      }
      pending += 1;
      store.set((state) => {
        let history = state.history;
        if (pending >= FRAMES_PER_SAMPLE) {
          const reduction = { ...history.reduction };
          for (const [stage, value] of Object.entries(reductionMax)) {
            reduction[stage] = pushHistory(reduction[stage] ?? [], value);
          }
          history = { input: pushHistory(history.input, inputMax), output: pushHistory(history.output, outputMax), reduction };
          resetSampling();
        }
        return {
          ...state,
          live: true,
          inputPeakDb,
          outputPeakDb,
          inputHoldDb: inputHold.update(inputPeakDb, timestamp),
          outputHoldDb: outputHold.update(outputPeakDb, timestamp),
          clipLatched: state.clipLatched || frame.clipped,
          latencyMs: frame.latencyMs,
          dropouts: frame.dropouts,
          gainReduction: frame.gainReduction,
          detectorDb: frame.detectorDb,
          history,
        };
      });
    },
    state(state) {
      if (state.running) {
        store.set((current) => ({ ...current, live: true, sampleRate: state.sampleRate }));
        return;
      }
      inputHold.reset();
      outputHold.reset();
      resetSampling();
      store.set((current) => ({ ...SILENT_METERS, clipLatched: current.clipLatched, sampleRate: null }));
    },
  };
}

export async function connectMeters(engine: EngineApi, feed: MeterFeed): Promise<void> {
  await engine.onMeters((frame) => feed.frame(frame));
  await engine.onState((state) => feed.state(state));
}
