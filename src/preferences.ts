import type { DevicePreferences, Preferences } from "./engine.ts";
import type { SessionState } from "./session-state.ts";
import { currentDocument } from "./session-state.ts";

export const SAVE_DELAY_MS = 300;
export const PREFERENCES_VERSION = 1;

export interface Timers {
  setTimeout(handler: () => void, delayMs: number): number;
  clearTimeout(handle: number): void;
}

export interface PreferencesSaver {
  schedule(preferences: Preferences): void;
  flush(): Promise<void>;
}

export function createPreferencesSaver(
  save: (preferences: Preferences) => Promise<unknown>,
  timers: Timers,
  delayMs: number = SAVE_DELAY_MS,
): PreferencesSaver {
  let timer: number | null = null;
  let generation = 0;
  let pending: Preferences | null = null;
  let inFlight: Promise<void> = Promise.resolve();

  const commit = (): Promise<void> => {
    const preferences = pending;
    pending = null;
    if (!preferences) {
      return inFlight;
    }
    const current = ++generation;
    inFlight = inFlight.then(() => save(preferences)).then(
      () => undefined,
      () => undefined,
    );
    return inFlight.then(() => {
      if (current !== generation && pending) {
        return commit();
      }
    });
  };

  return {
    schedule(preferences) {
      pending = preferences;
      if (timer !== null) {
        timers.clearTimeout(timer);
      }
      timer = timers.setTimeout(() => {
        timer = null;
        void commit();
      }, delayMs);
    },
    async flush() {
      if (timer !== null) {
        timers.clearTimeout(timer);
        timer = null;
      }
      await commit();
    },
  };
}

export function preferencesFrom(state: SessionState, devices: DevicePreferences, autoStart: boolean): Preferences {
  return {
    version: PREFERENCES_VERSION,
    devices,
    autoStart,
    session: currentDocument(state),
    activePreset: state.activePreset,
  };
}

export function persistedFieldsChanged(next: SessionState, previous: SessionState): boolean {
  return (
    next.values !== previous.values ||
    next.bypassed !== previous.bypassed ||
    next.added !== previous.added ||
    next.activePreset !== previous.activePreset
  );
}
