import type { Call, Subscribe, Unsubscribe } from "./ipc.ts";

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
  sampleRate: number;
  channels: number;
}

export interface AudioDevices {
  inputs: AudioDevice[];
  outputs: AudioDevice[];
}

export interface SetupStatus {
  cableInstalled: boolean;
  cableInputId: string | null;
  cableOutputId: string | null;
  inputs: number;
  outputs: number;
}

export interface Endpoint {
  id: string;
  name: string;
}

export interface EngineState {
  running: boolean;
  input: Endpoint | null;
  output: Endpoint | null;
  sampleRate: number | null;
  error: string | null;
  waitingForDevices: boolean;
}

export interface MeterFrame {
  inputPeakDb: number;
  outputPeakDb: number;
  clipped: boolean;
  latencyMs: number;
  dropouts: number;
  gainReduction: Record<string, number>;
  detectorDb: Record<string, number>;
}

export type Unit = "decibels" | "hertz" | "milliseconds" | "ratio" | "q" | "switch";
export type Scale = "linear" | "logarithmic";
export type Family = "level" | "filter" | "cleanup" | "dynamics" | "sibilance" | "eq";
export type Tier = "main" | "detail";

export interface StageSpec {
  id: string;
  key: string;
  label: string;
  title: string;
  family: Family;
  bypassable: boolean;
  optional: boolean;
  latencyMs: number;
  blurb: string;
}

export interface ParamSpec {
  id: string;
  stage: string;
  key: string;
  label: string;
  unit: Unit;
  scale: Scale;
  tier: Tier;
  min: number;
  max: number;
  default: number;
  step: number;
  coarse: number;
}

export interface ChainSchema {
  stages: StageSpec[];
  params: ParamSpec[];
}

export interface ParamDocument {
  values: Record<string, number>;
  bypassed: Record<string, boolean>;
}

export type PresetKind = "builtin" | "user";

export interface PresetRef {
  kind: PresetKind;
  name: string;
}

export interface PresetSummary {
  name: string;
  modifiedMs: number;
}

export interface PresetCatalog {
  builtin: string[];
  user: PresetSummary[];
}

export interface DevicePreferences {
  inputId: string | null;
  outputId: string | null;
}

export interface Preferences {
  version: number;
  devices: DevicePreferences;
  autoStart: boolean;
  session: ParamDocument;
  activePreset: PresetRef | null;
  launchAtLogin?: boolean;
  startMinimized?: boolean;
}

export type TakeRole = "silence" | "speech" | "loud";
export type Tone = "flat" | "neutral" | "warm" | "bright";

export interface TakeSummary {
  role: TakeRole;
  seconds: number;
  peakDb: number;
  clipped: boolean;
}

export interface LevelStats {
  p10: number;
  p50: number;
  p90: number;
  max: number;
}

export interface Sibilance {
  frequencyHz: number;
  levelDb: number;
  vowelLevelDb: number;
}

export interface SpectrumBand {
  frequencyHz: number;
  levelDb: number;
  referenceDb: number;
}

export interface VoiceProfile {
  sampleRate: number;
  noiseFloorDb: number;
  speech: LevelStats;
  speechSeconds: number;
  loud: LevelStats | null;
  pitchLowHz: number | null;
  pitchMedianHz: number | null;
  sibilance: Sibilance | null;
  spectrum: SpectrumBand[];
  clipped: boolean;
}

export interface CalibrationNote {
  stage: string;
  text: string;
}

export interface CalibrationResult {
  profile: VoiceProfile;
  values: ParamDocument;
  notes: CalibrationNote[];
}

export interface EngineApi {
  listDevices(): Promise<AudioDevices>;
  start(inputId: string, outputId: string): Promise<EngineState>;
  stop(): Promise<EngineState>;
  state(): Promise<EngineState>;
  setSupervision(enabled: boolean): Promise<EngineState>;
  setupStatus(): Promise<SetupStatus>;
  schema(): Promise<ChainSchema>;
  params(): Promise<ParamDocument>;
  setParam(id: string, value: number): Promise<void>;
  setBypass(stage: string, bypassed: boolean): Promise<void>;
  replaceParams(values: ParamDocument): Promise<ParamDocument>;
  preferences(): Promise<Preferences>;
  savePreferences(preferences: Preferences): Promise<Preferences>;
  setLaunchAtLogin(enabled: boolean): Promise<Preferences>;
  setStartMinimized(enabled: boolean): Promise<Preferences>;
  listPresets(): Promise<PresetCatalog>;
  loadPreset(ref: PresetRef): Promise<ParamDocument>;
  saveUserPreset(name: string, values: ParamDocument): Promise<PresetSummary>;
  renameUserPreset(from: string, to: string): Promise<PresetSummary>;
  deleteUserPreset(name: string): Promise<void>;
  importPreset(): Promise<PresetSummary | null>;
  exportPreset(name: string, values: ParamDocument): Promise<string | null>;
  openPresetsFolder(): Promise<void>;
  beginTake(): Promise<void>;
  endTake(role: TakeRole): Promise<TakeSummary>;
  listTakes(): Promise<TakeSummary[]>;
  discardTakes(): Promise<void>;
  analyzeTakes(tone: Tone): Promise<CalibrationResult>;
  onState(handler: (state: EngineState) => void): Promise<Unsubscribe>;
  onMeters(handler: (frame: MeterFrame) => void): Promise<Unsubscribe>;
}

export const STOPPED: EngineState = {
  running: false,
  input: null,
  output: null,
  sampleRate: null,
  error: null,
  waitingForDevices: false,
};

export function createEngineApi(call: Call, subscribe: Subscribe): EngineApi {
  return {
    listDevices: () => call<AudioDevices>("list_audio_devices"),
    start: (inputId, outputId) => call<EngineState>("start_engine", { inputId, outputId }),
    stop: () => call<EngineState>("stop_engine"),
    state: () => call<EngineState>("get_engine_state"),
    setSupervision: (enabled) => call<EngineState>("set_engine_supervision", { enabled }),
    setupStatus: () => call<SetupStatus>("get_setup_status"),
    schema: () => call<ChainSchema>("get_schema"),
    params: () => call<ParamDocument>("get_params"),
    setParam: (id, value) => call<void>("set_param", { id, value }),
    setBypass: (stage, bypassed) => call<void>("set_bypass", { stage, bypassed }),
    replaceParams: (values) => call<ParamDocument>("set_params", { values }),
    preferences: () => call<Preferences>("get_preferences"),
    savePreferences: (preferences) => call<Preferences>("save_preferences", { preferences }),
    setLaunchAtLogin: (enabled) => call<Preferences>("set_launch_at_login", { enabled }),
    setStartMinimized: (enabled) => call<Preferences>("set_start_minimized", { enabled }),
    listPresets: () => call<PresetCatalog>("list_presets"),
    loadPreset: (ref) => call<ParamDocument>("load_preset", { kind: ref.kind, name: ref.name }),
    saveUserPreset: (name, values) => call<PresetSummary>("save_user_preset", { name, values }),
    renameUserPreset: (from, to) => call<PresetSummary>("rename_user_preset", { from, to }),
    deleteUserPreset: (name) => call<void>("delete_user_preset", { name }),
    importPreset: () => call<PresetSummary | null>("import_preset"),
    exportPreset: (name, values) => call<string | null>("export_preset", { name, values }),
    openPresetsFolder: () => call<void>("open_presets_folder"),
    beginTake: () => call<void>("begin_take"),
    endTake: (role) => call<TakeSummary>("end_take", { role }),
    listTakes: () => call<TakeSummary[]>("list_takes"),
    discardTakes: () => call<void>("discard_takes"),
    analyzeTakes: (tone) => call<CalibrationResult>("analyze_takes", { tone }),
    onState: (handler) => subscribe<EngineState>("engine:state", handler),
    onMeters: (handler) => subscribe<MeterFrame>("engine:meters", handler),
  };
}
