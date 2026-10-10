import { startApp } from "./app.ts";
import { createEngineApi } from "./engine.ts";
import { call, subscribe } from "./ipc.ts";
import { createTauriWindowControls, onWindowClose } from "./tauri-window.ts";
import { initTitlebar } from "./titlebar.ts";

async function bootstrap(): Promise<void> {
  const engine = createEngineApi(call, subscribe);
  await initTitlebar(createTauriWindowControls());
  const app = await startApp(engine, document);
  await onWindowClose(() => app.flush());
}

void bootstrap();
