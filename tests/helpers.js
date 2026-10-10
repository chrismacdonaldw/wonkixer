import { readFileSync } from "node:fs";

export const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
export const schema = JSON.parse(readFileSync(new URL("../docs/chain-schema.json", import.meta.url), "utf8"));

export const MIC = { id: "in:mic", name: "Microphone (HyperX)", isDefault: true, sampleRate: 48000, channels: 1 };
export const BROADCAST = { id: "in:broadcast", name: "Microphone (NVIDIA Broadcast)", isDefault: false, sampleRate: 48000, channels: 2 };
export const HEADPHONES = { id: "out:headphones", name: "Headphones (HyperX)", isDefault: true, sampleRate: 48000, channels: 2 };
export const CABLE = { id: "out:cable", name: "CABLE Input (VB-Audio Virtual Cable)", isDefault: false, sampleRate: 48000, channels: 2 };
export const DEVICES = { inputs: [BROADCAST, MIC], outputs: [HEADPHONES, CABLE] };
export const STOPPED = { running: false, input: null, output: null, sampleRate: null, error: null };
export const RUNNING = { running: true, input: MIC, output: CABLE, sampleRate: 48000, error: null };

export function spec(id) {
  const found = schema.params.find((param) => param.id === id);
  if (!found) throw new Error(`no spec ${id}`);
  return found;
}

export function defaults() {
  return {
    values: Object.fromEntries(schema.params.map((param) => [param.id, param.default])),
    bypassed: Object.fromEntries(schema.stages.map((stage) => [stage.id, stage.optional])),
  };
}

export function withValues(overrides, bypassed = {}) {
  const doc = defaults();
  return { values: { ...doc.values, ...overrides }, bypassed: { ...doc.bypassed, ...bypassed } };
}

export function fakeEngine(options = {}) {
  const calls = [];
  const handlers = { state: [], meters: [] };
  let stored = options.params ?? defaults();
  const user = new Map(options.user ?? []);
  const builtin = options.builtin ?? { Flat: defaults(), "Headset clean-up": defaults(), Podcast: withValues({ "compressor.ratio": 2.5 }) };
  const preferences = options.preferences ?? {
    version: 1,
    devices: { inputId: null, outputId: null },
    autoStart: false,
    session: defaults(),
    activePreset: { kind: "builtin", name: "Headset clean-up" },
  };
  const record = (name, ...args) => calls.push([name, ...args]);
  const api = {
    listDevices: async () => options.devices ?? DEVICES,
    start: async (inputId, outputId) => {
      record("start", inputId, outputId);
      if (options.start) return options.start();
      return RUNNING;
    },
    stop: async () => {
      record("stop");
      return STOPPED;
    },
    state: async () => STOPPED,
    schema: async () => schema,
    params: async () => stored,
    setParam: async (id, value) => {
      record("set_param", id, value);
      stored = { ...stored, values: { ...stored.values, [id]: value } };
    },
    setBypass: async (stage, bypassed) => {
      record("set_bypass", stage, bypassed);
      stored = { ...stored, bypassed: { ...stored.bypassed, [stage]: bypassed } };
    },
    replaceParams: async (values) => {
      record("set_params", values);
      const base = defaults();
      stored = { values: { ...base.values, ...values.values }, bypassed: { ...base.bypassed, ...values.bypassed } };
      return stored;
    },
    preferences: async () => preferences,
    savePreferences: async (next) => {
      record("save_preferences", next);
      return next;
    },
    listPresets: async () => ({ builtin: Object.keys(builtin), user: [...user.keys()].map((name) => ({ name, modifiedMs: 1 })) }),
    loadPreset: async (ref) => {
      record("load_preset", ref.kind, ref.name);
      const doc = ref.kind === "builtin" ? builtin[ref.name] : user.get(ref.name);
      if (!doc) throw new Error(`There is no preset named "${ref.name}".`);
      return doc;
    },
    saveUserPreset: async (name, values) => {
      record("save_user_preset", name, values);
      user.set(name, values);
      return { name, modifiedMs: 2 };
    },
    renameUserPreset: async (from, to) => {
      record("rename_user_preset", from, to);
      user.set(to, user.get(from));
      user.delete(from);
      return { name: to, modifiedMs: 3 };
    },
    deleteUserPreset: async (name) => {
      record("delete_user_preset", name);
      user.delete(name);
    },
    importPreset: async () => {
      record("import_preset");
      return options.imported ?? null;
    },
    exportPreset: async (name, values) => {
      record("export_preset", name, values);
      return options.exported ?? null;
    },
    openPresetsFolder: async () => record("open_presets_folder"),
    onState: async (handler) => {
      handlers.state.push(handler);
      return () => undefined;
    },
    onMeters: async (handler) => {
      handlers.meters.push(handler);
      return () => undefined;
    },
  };
  return {
    api,
    calls,
    user,
    stored: () => stored,
    emitState: (state) => handlers.state.forEach((handler) => handler(state)),
    emitMeters: (frame) => handlers.meters.forEach((handler) => handler(frame)),
    of: (name) => calls.filter((call) => call[0] === name),
    clear: () => calls.splice(0, calls.length),
  };
}

export function fakeTimers() {
  const pending = new Map();
  let next = 1;
  let now = 0;
  return {
    setTimeout: (handler, delay) => {
      const id = next++;
      pending.set(id, { handler, at: now + delay });
      return id;
    },
    clearTimeout: (id) => pending.delete(id),
    advance(ms) {
      now += ms;
      for (const [id, entry] of [...pending]) {
        if (entry.at <= now) {
          pending.delete(id);
          entry.handler();
        }
      }
    },
    pending: () => pending.size,
  };
}

export function settle(rounds = 3) {
  let promise = Promise.resolve();
  for (let i = 0; i < rounds; i++) {
    promise = promise.then(() => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  return promise;
}

export function frame(overrides = {}) {
  return {
    inputPeakDb: -20,
    outputPeakDb: -18,
    clipped: false,
    latencyMs: 31,
    dropouts: 0,
    gainReduction: { expander: 0, deesser: 0, compressor: 4, limiter: 0 },
    detectorDb: { expander: -30, deesser: -40, compressor: -10, limiter: -5 },
    ...overrides,
  };
}
