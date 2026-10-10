import { eqDb, highPassDb, type EqBand, type EqShape } from "./curves.ts";
import type { ChainSchema, StageSpec } from "./engine.ts";
import { bandGroups, stageParams } from "./session-state.ts";

export type Response = (frequency: number) => number;

export function cutoffParamId(schema: ChainSchema, stageId: string): string | null {
  return stageParams(schema, stageId).find((spec) => spec.unit === "hertz")?.id ?? null;
}

export function eqShapeFor(schema: ChainSchema, stageId: string, values: Record<string, number>): EqShape {
  const bands = bandGroups(schema, stageId).map((group) => {
    const frequency = group.params.find((spec) => spec.unit === "hertz");
    const gain = group.params.find((spec) => spec.unit === "decibels");
    const q = group.params.find((spec) => spec.unit === "q");
    const band: EqBand = {
      frequency: frequency ? (values[frequency.id] ?? frequency.default) : 1000,
      gain: gain ? (values[gain.id] ?? gain.default) : 0,
    };
    if (q) {
      band.q = values[q.id] ?? q.default;
    }
    return band;
  });
  const shelves = bands.filter((band) => band.q === undefined);
  const peaks = bands.filter((band) => band.q !== undefined);
  const silent: EqBand = { frequency: 1000, gain: 0 };
  return {
    lowShelf: shelves[0] ?? silent,
    peaks,
    highShelf: shelves.length > 1 ? shelves[shelves.length - 1] : silent,
  };
}

export function responseFor(schema: ChainSchema, stage: StageSpec, values: Record<string, number>): Response {
  if (stage.family === "eq") {
    const shape = eqShapeFor(schema, stage.id, values);
    return (frequency) => eqDb(frequency, shape);
  }
  const cutoffId = cutoffParamId(schema, stage.id);
  if (!cutoffId) {
    return () => 0;
  }
  const cutoff = values[cutoffId] ?? 0;
  return (frequency) => highPassDb(frequency, cutoff);
}

export const FLAT: Response = () => 0;
