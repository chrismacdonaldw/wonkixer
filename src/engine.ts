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

export interface MeterFrame {
  inputPeakDb: number;
  outputPeakDb: number;
  clipped: boolean;
  latencyMs: number;
  dropouts: number;
}

export interface EngineApi {
  listDevices(): Promise<AudioDevices>;
  start(inputId: string, outputId: string): Promise<EngineState>;
  stop(): Promise<EngineState>;
  state(): Promise<EngineState>;
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
    onState: (handler) => subscribe<EngineState>("engine:state", handler),
    onMeters: (handler) => subscribe<MeterFrame>("engine:meters", handler),
  };
}
