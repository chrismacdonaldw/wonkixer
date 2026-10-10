import type { ChainSchema, ParamDocument, ParamSpec, PresetCatalog, PresetRef, StageSpec, Tier } from "./engine.ts";

export const LIBRARY = "library";

export interface Snapshot {
  values: Record<string, number>;
  bypassed: Record<string, boolean>;
  added: Record<string, boolean>;
}

export interface SessionState extends Snapshot {
  selection: string;
  activePreset: PresetRef | null;
  activeDocument: ParamDocument | null;
  compare: ParamDocument | null;
  dry: boolean;
  detailOpen: Record<string, boolean>;
  undoDepth: number;
  redoDepth: number;
  catalog: PresetCatalog;
}

export function defaultValues(schema: ChainSchema): Record<string, number> {
  return Object.fromEntries(schema.params.map((spec) => [spec.id, spec.default]));
}

export function defaultBypassed(schema: ChainSchema): Record<string, boolean> {
  return Object.fromEntries(schema.stages.map((stage) => [stage.id, stage.optional]));
}

export function completeDocument(schema: ChainSchema, partial: Partial<ParamDocument> | null | undefined): ParamDocument {
  return {
    values: { ...defaultValues(schema), ...(partial?.values ?? {}) },
    bypassed: { ...defaultBypassed(schema), ...(partial?.bypassed ?? {}) },
  };
}

export function addedFrom(schema: ChainSchema, bypassed: Record<string, boolean>): Record<string, boolean> {
  return Object.fromEntries(schema.stages.filter((stage) => stage.optional).map((stage) => [stage.id, !bypassed[stage.id]]));
}

export function cloneDocument(doc: ParamDocument): ParamDocument {
  return { values: { ...doc.values }, bypassed: { ...doc.bypassed } };
}

export function snapshotOf(state: Snapshot): Snapshot {
  return { values: state.values, bypassed: state.bypassed, added: state.added };
}

export function currentDocument(state: Snapshot): ParamDocument {
  return { values: { ...state.values }, bypassed: { ...state.bypassed } };
}

export function matchesDocument(schema: ChainSchema, state: Snapshot, doc: ParamDocument | null): boolean {
  if (!doc) {
    return false;
  }
  for (const spec of schema.params) {
    if (Math.abs((state.values[spec.id] ?? spec.default) - (doc.values[spec.id] ?? spec.default)) > 1e-6) {
      return false;
    }
  }
  for (const stage of schema.stages) {
    if (stage.bypassable && Boolean(state.bypassed[stage.id]) !== Boolean(doc.bypassed[stage.id])) {
      return false;
    }
  }
  return true;
}

export function isEdited(schema: ChainSchema, state: SessionState): boolean {
  return !matchesDocument(schema, state, state.activeDocument);
}

export function displayedValues(state: SessionState): Record<string, number> {
  return state.compare ? state.compare.values : state.values;
}

export function displayedBypassed(state: SessionState): Record<string, boolean> {
  return state.compare ? state.compare.bypassed : state.bypassed;
}

export function displayedAdded(schema: ChainSchema, state: SessionState): Record<string, boolean> {
  return state.compare ? addedFrom(schema, state.compare.bypassed) : state.added;
}

export function effectiveBypassed(schema: ChainSchema, state: SessionState): Record<string, boolean> {
  if (state.dry) {
    return Object.fromEntries(schema.stages.map((stage) => [stage.id, stage.bypassable || stage.optional]));
  }
  return displayedBypassed(state);
}

export function effectiveDocument(schema: ChainSchema, state: SessionState): ParamDocument {
  return { values: { ...displayedValues(state) }, bypassed: { ...effectiveBypassed(schema, state) } };
}

export function visibleStages(schema: ChainSchema, state: SessionState): StageSpec[] {
  const added = displayedAdded(schema, state);
  return schema.stages.filter((stage) => !stage.optional || added[stage.id]);
}

export function resolveSelection(schema: ChainSchema, state: SessionState, fallback: string): string {
  if (state.selection === LIBRARY) {
    return LIBRARY;
  }
  return visibleStages(schema, state).some((stage) => stage.id === state.selection) ? state.selection : fallback;
}

export function stageParams(schema: ChainSchema, stageId: string, tier?: Tier): ParamSpec[] {
  return schema.params.filter((spec) => spec.stage === stageId && (tier === undefined || spec.tier === tier));
}

export interface BandGroup {
  key: string;
  label: string;
  params: ParamSpec[];
}

export function bandGroups(schema: ChainSchema, stageId: string): BandGroup[] {
  const groups = new Map<string, BandGroup>();
  for (const spec of stageParams(schema, stageId)) {
    const parts = spec.id.split(".");
    if (parts.length < 3) {
      continue;
    }
    const key = parts[1];
    const group = groups.get(key) ?? { key, label: spec.label.toUpperCase(), params: [] };
    if (spec.unit === "hertz") {
      group.label = spec.label.toUpperCase();
    }
    group.params.push(spec);
    groups.set(key, group);
  }
  return [...groups.values()];
}

export function canSave(state: SessionState, edited: boolean): boolean {
  return state.activePreset?.kind === "user" && edited;
}

export function canCompare(state: SessionState, edited: boolean): boolean {
  return state.compare !== null || (edited && state.activeDocument !== null);
}

export function presetName(state: SessionState): string {
  return state.activePreset ? state.activePreset.name : "Unsaved settings";
}

export function onCount(schema: ChainSchema, state: SessionState): { on: number; total: number } {
  const bypassed = effectiveBypassed(schema, state);
  const stages = visibleStages(schema, state).filter((stage) => stage.bypassable);
  return { on: stages.filter((stage) => !bypassed[stage.id]).length, total: stages.length };
}

export const FAMILY_LABELS: Record<StageSpec["family"], string> = {
  level: "GAIN",
  filter: "FILTER",
  cleanup: "CLEAN-UP",
  dynamics: "DYNAMICS",
  sibilance: "DYNAMICS",
  eq: "TONE",
};

export function padIndex(index: number): string {
  return index < 10 ? `0${index}` : String(index);
}

export function reportsGainReduction(stage: StageSpec): boolean {
  return stage.family === "dynamics" || stage.family === "sibilance";
}

export function hasResponseCurve(stage: StageSpec): boolean {
  return stage.family === "filter" || stage.family === "eq";
}
