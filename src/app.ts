import { initChainList } from "./chain-list.ts";
import type { EngineApi, ParamDocument, Preferences, PresetRef } from "./engine.ts";
import { initEnginePanel } from "./engine-panel.ts";
import { initFooter } from "./footer.ts";
import { connectMeters, createMeterFeed } from "./meters.ts";
import { createPreferencesSaver, persistedFieldsChanged, preferencesFrom, type Timers } from "./preferences.ts";
import { initPresetBar } from "./preset-bar.ts";
import { createSession, type Session } from "./session.ts";
import { completeDocument } from "./session-state.ts";
import { initStagePage } from "./stage-page.ts";

export interface AppOptions {
  timers?: Timers;
  now?: () => number;
}

export interface App {
  session: Session;
  flush(): Promise<void>;
}

export async function startApp(engine: EngineApi, doc: Document, options: AppOptions = {}): Promise<App> {
  const view = doc.defaultView;
  const timers: Timers = options.timers ?? {
    setTimeout: (handler, delay) => (view ?? globalThis).setTimeout(handler, delay) as unknown as number,
    clearTimeout: (handle) => (view ?? globalThis).clearTimeout(handle),
  };
  const schema = await engine.schema();
  const preferences = await engine.preferences().catch((): Preferences => ({
    version: 1,
    devices: { inputId: null, outputId: null },
    autoStart: false,
    session: { values: {}, bypassed: {} },
    activePreset: null,
  }));
  const document = await engine.replaceParams(completeDocument(schema, preferences.session));
  const catalog = await engine.listPresets().catch(() => ({ builtin: [], user: [] }));
  const activeDocument = await loadActive(engine, preferences.activePreset);
  const session = createSession(engine, schema, {
    document,
    activePreset: activeDocument ? preferences.activePreset : null,
    activeDocument,
    catalog,
  });

  const meters = createMeterFeed(options.now);
  await connectMeters(engine, meters);
  const saver = createPreferencesSaver((next) => engine.savePreferences(next), timers);

  let devices = preferences.devices;
  const schedule = (): void => saver.schedule(preferencesFrom(session.store.get(), devices, preferences.autoStart));

  initPresetBar(session, engine, doc);
  initChainList(session, meters, doc);
  initStagePage(session, meters.store, doc);
  initFooter(meters.store, doc);
  const panel = await initEnginePanel(engine, doc, {
    devices,
    autoStart: preferences.autoStart,
    onDevicesChange: (next) => {
      devices = next;
      schedule();
    },
  });
  devices = panel.devices();
  if (devices.inputId !== preferences.devices.inputId || devices.outputId !== preferences.devices.outputId) {
    schedule();
  }

  session.store.subscribe((state, previous) => {
    if (persistedFieldsChanged(state, previous)) {
      schedule();
    }
  });

  return { session, flush: () => saver.flush() };
}

async function loadActive(engine: EngineApi, ref: PresetRef | null): Promise<ParamDocument | null> {
  if (!ref) {
    return null;
  }
  try {
    return await engine.loadPreset(ref);
  } catch {
    return null;
  }
}
