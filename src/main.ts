import { createTauriWindowControls } from "./tauri-window.ts";
import { initTitlebar } from "./titlebar.ts";

async function bootstrap(): Promise<void> {
  await initTitlebar(createTauriWindowControls());
}

void bootstrap();
