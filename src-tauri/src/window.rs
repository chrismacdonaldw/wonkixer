use tauri::{AppHandle, Manager, Runtime, Window, WindowEvent};

pub const MAIN: &str = "main";

pub fn reveal<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN) {
        window.unminimize().ok();
        window.show().ok();
        window.set_focus().ok();
    }
}

pub fn hide_instead_of_closing<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.label() == MAIN
    {
        api.prevent_close();
        window.hide().ok();
    }
}
