import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { describeError } from "./errors.ts";

export type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export type Unsubscribe = () => void;
export type Subscribe = <T>(event: string, handler: (payload: T) => void) => Promise<Unsubscribe>;

export const call: Call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new Error(describeError(error));
  }
};

export const subscribe: Subscribe = <T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe> =>
  listen<T>(event, (received) => handler(received.payload));
