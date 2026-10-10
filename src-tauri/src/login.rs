use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

pub fn register<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let changed = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    changed.map_err(|error| format!("Could not change launch at login: {error}"))
}

pub fn reconcile<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    let registered = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| format!("Could not read the launch at login setting: {error}"))?;
    if registered == enabled {
        return Ok(());
    }
    register(app, enabled)
}
