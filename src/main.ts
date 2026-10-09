import { createEngineApi } from "./engine.ts";
import { initEnginePanel } from "./engine-panel.ts";
import { call, subscribe } from "./ipc.ts";
import { initMeters } from "./meters.ts";
import { createTauriWindowControls } from "./tauri-window.ts";
import { initTitlebar } from "./titlebar.ts";

async function bootstrap(): Promise<void> {
  const engine = createEngineApi(call, subscribe);
  await initTitlebar(createTauriWindowControls());
  await Promise.all([initEnginePanel(engine), initMeters(engine)]);
}

void bootstrap();
