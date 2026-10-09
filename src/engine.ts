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
}

export interface GainReductionFrame {
  expander: number;
  deesser: number;
  compressor: number;
  limiter: number;
}

export interface MeterFrame {
  inputPeakDb: number;
  outputPeakDb: number;
  clipped: boolean;
  latencyMs: number;
  dropouts: number;
  gainReduction: GainReductionFrame;
}

export type Unit = "decibels" | "hertz" | "milliseconds" | "ratio" | "q" | "switch";
export type Scale = "linear" | "logarithmic";

export interface StageSpec {
  id: string;
  key: string;
  label: string;
  bypassable: boolean;
}

export interface ParamSpec {
  id: string;
  stage: string;
  key: string;
  label: string;
  unit: Unit;
  scale: Scale;
  min: number;
  max: number;
  default: number;
  step: number;
}

export interface ChainSchema {
  stages: StageSpec[];
  params: ParamSpec[];
}

export interface ParamValues {
  values: Record<string, number>;
  bypassed: string[];
}

export interface EngineApi {
  listDevices(): Promise<AudioDevices>;
  start(inputId: string, outputId: string): Promise<EngineState>;
  stop(): Promise<EngineState>;
  state(): Promise<EngineState>;
  schema(): Promise<ChainSchema>;
  params(): Promise<ParamValues>;
  setParam(id: string, value: number): Promise<void>;
  setBypass(stage: string, bypassed: boolean): Promise<void>;
  replaceParams(values: ParamValues): Promise<ParamValues>;
  onState(handler: (state: EngineState) => void): Promise<Unsubscribe>;
  onMeters(handler: (frame: MeterFrame) => void): Promise<Unsubscribe>;
}

export const STOPPED: EngineState = { running: false, input: null, output: null, sampleRate: null, error: null };

export function createEngineApi(call: Call, subscribe: Subscribe): EngineApi {
  return {
    listDevices: () => call<AudioDevices>("list_audio_devices"),
    start: (inputId, outputId) => call<EngineState>("start_engine", { inputId, outputId }),
    stop: () => call<EngineState>("stop_engine"),
    state: () => call<EngineState>("get_engine_state"),
    schema: () => call<ChainSchema>("get_schema"),
    params: () => call<ParamValues>("get_params"),
    setParam: (id, value) => call<void>("set_param", { id, value }),
    setBypass: (stage, bypassed) => call<void>("set_bypass", { stage, bypassed }),
    replaceParams: (values) => call<ParamValues>("set_params", { values }),
    onState: (handler) => subscribe<EngineState>("engine:state", handler),
    onMeters: (handler) => subscribe<MeterFrame>("engine:meters", handler),
  };
}
