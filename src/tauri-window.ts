import { getCurrentWindow } from "@tauri-apps/api/window";
import type { WindowControls } from "./window-controls.ts";

export function createTauriWindowControls(): WindowControls {
  const current = getCurrentWindow();
  return {
    minimize: () => current.minimize(),
    toggleMaximize: () => current.toggleMaximize(),
    close: () => current.close(),
    isMaximized: () => current.isMaximized(),
    startDragging: () => current.startDragging(),
    startResizeDragging: (direction) => current.startResizeDragging(direction),
    onResized: (handler) => current.onResized(handler),
  };
}

export async function onWindowClose(flush: () => Promise<void>): Promise<void> {
  const current = getCurrentWindow();
  let flushed = false;
  await current.onCloseRequested(async (event) => {
    if (flushed) {
      return;
    }
    event.preventDefault();
    await flush();
    flushed = true;
    await current.close();
  });
}
