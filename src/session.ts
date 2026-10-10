import type { ChainSchema, EngineApi, ParamDocument, ParamSpec, PresetCatalog, PresetRef, StageSpec } from "./engine.ts";
import { createHistory, type History } from "./history.ts";
import {
  addedFrom,
  cloneDocument,
  completeDocument,
  currentDocument,
  displayedValues,
  effectiveDocument,
  LIBRARY,
  snapshotOf,
  type SessionState,
  type Snapshot,
} from "./session-state.ts";
import { createStore, type Store } from "./store.ts";

export interface SessionOptions {
  document: ParamDocument;
  activePreset: PresetRef | null;
  activeDocument: ParamDocument | null;
  catalog: PresetCatalog;
  selection?: string;
  onError?: (message: string) => void;
}

export interface Session {
  schema: ChainSchema;
  store: Store<SessionState>;
  spec(id: string): ParamSpec;
  stage(id: string): StageSpec;
  current(id: string): number;
  record(): void;
  setValue(id: string, value: number, record?: boolean): void;
  setValues(values: Record<string, number>, record?: boolean): void;
  restoreValue(id: string, value: number): void;
  toggleBypass(stageId: string): void;
  resetStage(stageId: string): void;
  applyDocument(doc: ParamDocument, active: PresetRef | null): void;
  markSaved(active: PresetRef): void;
  setCatalog(catalog: PresetCatalog): void;
  forgetPreset(name: string): void;
  renamePreset(from: string, to: string): void;
  undo(): void;
  redo(): void;
  toggleCompare(): void;
  toggleDry(): void;
  addStage(stageId: string): void;
  removeStage(stageId: string): void;
  select(target: string): void;
  toggleDetail(stageId: string): void;
  effective(): ParamDocument;
}


export function createSession(engine: EngineApi, schema: ChainSchema, options: SessionOptions): Session {
  const specs = new Map(schema.params.map((spec) => [spec.id, spec]));
  const stages = new Map(schema.stages.map((stage) => [stage.id, stage]));
  const history: History<Snapshot> = createHistory();
  const document = completeDocument(schema, options.document);
  const store = createStore<SessionState>({
    values: document.values,
    bypassed: document.bypassed,
    added: addedFrom(schema, document.bypassed),
    selection: options.selection ?? defaultSelection(schema),
    activePreset: options.activePreset,
    activeDocument: options.activeDocument ? completeDocument(schema, options.activeDocument) : null,
    compare: null,
    dry: false,
    detailOpen: {},
    undoDepth: 0,
    redoDepth: 0,
    catalog: options.catalog,
  });
  const report = options.onError ?? (() => undefined);
  let sent = effectiveDocument(schema, store.get());
  let bulk = false;

  const send = (promise: Promise<unknown>): void => {
    promise.catch((error: unknown) => report(String(error)));
  };

  store.subscribe((state) => {
    const next = effectiveDocument(schema, state);
    const changedValues = Object.keys(next.values).filter((id) => next.values[id] !== sent.values[id]);
    const changedStages = Object.keys(next.bypassed).filter((id) => Boolean(next.bypassed[id]) !== Boolean(sent.bypassed[id]));
    sent = next;
    const wholeDocument = bulk;
    bulk = false;
    if (wholeDocument) {
      send(engine.replaceParams(next));
      return;
    }
    for (const id of changedValues) {
      send(engine.setParam(id, next.values[id]));
    }
    for (const id of changedStages) {
      send(engine.setBypass(id, next.bypassed[id]));
    }
  });

  const depths = (): Pick<SessionState, "undoDepth" | "redoDepth"> => ({
    undoDepth: history.undoDepth(),
    redoDepth: history.redoDepth(),
  });

  const record = (): void => {
    history.push(snapshotOf(store.get()));
  };

  const spec = (id: string): ParamSpec => {
    const found = specs.get(id);
    if (!found) {
      throw new Error(`Unknown parameter: ${id}`);
    }
    return found;
  };

  const stage = (id: string): StageSpec => {
    const found = stages.get(id);
    if (!found) {
      throw new Error(`Unknown stage: ${id}`);
    }
    return found;
  };

  const setValues = (values: Record<string, number>, shouldRecord = true): void => {
    if (shouldRecord) {
      record();
    }
    store.set((state) => ({ ...state, values: { ...state.values, ...values }, compare: null, ...depths() }));
  };

  const applySnapshot = (snapshot: Snapshot): void => {
    bulk = true;
    store.set((state) => ({ ...state, ...snapshot, compare: null, ...depths() }));
  };

  return {
    schema,
    store,
    spec,
    stage,
    current: (id) => displayedValues(store.get())[id] ?? spec(id).default,
    record,
    setValue: (id, value, shouldRecord = true) => setValues({ [id]: value }, shouldRecord),
    setValues,
    restoreValue(id, value) {
      history.dropLast();
      setValues({ [id]: value }, false);
    },
    toggleBypass(stageId) {
      record();
      store.set((state) => ({
        ...state,
        bypassed: { ...state.bypassed, [stageId]: !state.bypassed[stageId] },
        compare: null,
        ...depths(),
      }));
    },
    resetStage(stageId) {
      const defaults = Object.fromEntries(
        schema.params.filter((param) => param.stage === stageId).map((param) => [param.id, param.default]),
      );
      setValues(defaults);
    },
    applyDocument(doc, active) {
      record();
      const complete = completeDocument(schema, doc);
      bulk = true;
      store.set((state) => ({
        ...state,
        values: complete.values,
        bypassed: complete.bypassed,
        added: addedFrom(schema, complete.bypassed),
        activePreset: active,
        activeDocument: active ? cloneDocument(complete) : null,
        compare: null,
        ...depths(),
      }));
    },
    markSaved(active) {
      store.set((state) => ({ ...state, activePreset: active, activeDocument: currentDocument(state), compare: null }));
    },
    setCatalog(catalog) {
      store.patch({ catalog });
    },
    forgetPreset(name) {
      store.set((state) =>
        state.activePreset?.kind === "user" && state.activePreset.name === name
          ? { ...state, activePreset: null, activeDocument: null, compare: null }
          : state,
      );
    },
    renamePreset(from, to) {
      store.set((state) =>
        state.activePreset?.kind === "user" && state.activePreset.name === from
          ? { ...state, activePreset: { kind: "user", name: to } }
          : state,
      );
    },
    undo() {
      const previous = history.undo(snapshotOf(store.get()));
      if (previous) {
        applySnapshot(previous);
      }
    },
    redo() {
      const next = history.redo(snapshotOf(store.get()));
      if (next) {
        applySnapshot(next);
      }
    },
    toggleCompare() {
      bulk = true;
      store.set((state) => ({
        ...state,
        compare: state.compare ? null : state.activeDocument ? cloneDocument(state.activeDocument) : null,
      }));
    },
    toggleDry() {
      bulk = true;
      store.set((state) => ({ ...state, dry: !state.dry }));
    },
    addStage(stageId) {
      record();
      store.set((state) => ({
        ...state,
        added: { ...state.added, [stageId]: true },
        bypassed: { ...state.bypassed, [stageId]: false },
        selection: stageId,
        compare: null,
        ...depths(),
      }));
    },
    removeStage(stageId) {
      record();
      store.set((state) => ({
        ...state,
        added: { ...state.added, [stageId]: false },
        bypassed: { ...state.bypassed, [stageId]: true },
        selection: LIBRARY,
        compare: null,
        ...depths(),
      }));
    },
    select(target) {
      store.set((state) => (state.selection === target ? state : { ...state, selection: target }));
    },
    toggleDetail(stageId) {
      store.set((state) => ({ ...state, detailOpen: { ...state.detailOpen, [stageId]: !state.detailOpen[stageId] } }));
    },
    effective: () => effectiveDocument(schema, store.get()),
  };
}

function defaultSelection(schema: ChainSchema): string {
  return schema.stages.find((stage) => stage.id === "compressor")?.id ?? schema.stages[0]?.id ?? LIBRARY;
}
