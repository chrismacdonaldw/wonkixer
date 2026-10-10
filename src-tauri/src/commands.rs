use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use wonkixer_dsp::analysis::{TakeRole, Tone};
use wonkixer_dsp::params::{self, Schema};
use wonkixer_dsp::{ParamId, ParamValues, StageId, presets};

use crate::calibration::{self, CalibrationResult, CalibrationSession, TakeSummary};
use crate::engine::devices::{self, AudioDevices, SetupStatus};
use crate::engine::{AuditionRequest, Engine, EngineState};
use crate::login;
use crate::preferences::{Preferences, PreferencesStore, PresetKind};
use crate::presets::{PresetCatalog, PresetStore, PresetSummary};

const PRESET_FILTER: &str = "wonkixer preset";
const CABLE_DOWNLOAD: &str = "https://vb-audio.com/Cable/";

#[tauri::command]
pub async fn list_audio_devices() -> Result<AudioDevices, String> {
    enumerate_devices().await
}

#[tauri::command]
pub async fn get_setup_status() -> Result<SetupStatus, String> {
    Ok(SetupStatus::detect(&enumerate_devices().await?))
}

#[tauri::command]
pub fn open_cable_download(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(CABLE_DOWNLOAD, None::<String>)
        .map_err(|error| format!("Could not open the VB-Cable download page: {error}"))
}

async fn enumerate_devices() -> Result<AudioDevices, String> {
    tauri::async_runtime::spawn_blocking(|| devices::enumerate(&cpal::default_host()))
        .await
        .map_err(|error| format!("Could not list audio devices: {error}"))?
}

#[tauri::command]
pub async fn start_engine(
    input_id: String,
    output_id: String,
    engine: State<'_, Engine>,
) -> Result<EngineState, String> {
    engine.start(input_id, output_id).await
}

#[tauri::command]
pub async fn stop_engine(engine: State<'_, Engine>) -> Result<EngineState, String> {
    engine.stop().await
}

#[tauri::command]
pub async fn get_engine_state(engine: State<'_, Engine>) -> Result<EngineState, String> {
    engine.state().await
}

#[tauri::command]
pub async fn set_engine_supervision(
    enabled: bool,
    engine: State<'_, Engine>,
) -> Result<EngineState, String> {
    engine.set_supervision(enabled).await
}

#[tauri::command]
pub fn get_schema() -> Schema {
    params::schema()
}

#[tauri::command]
pub fn get_params(engine: State<'_, Engine>) -> ParamValues {
    engine.params().snapshot()
}

#[tauri::command]
pub fn set_param(id: ParamId, value: f32, engine: State<'_, Engine>) {
    engine.params().set(id, value);
}

#[tauri::command]
pub fn set_bypass(stage: StageId, bypassed: bool, engine: State<'_, Engine>) {
    engine.params().set_bypassed(stage, bypassed);
}

#[tauri::command]
pub fn set_params(values: ParamValues, engine: State<'_, Engine>) -> ParamValues {
    engine.params().replace(&values);
    engine.params().snapshot()
}

#[tauri::command]
pub fn get_preferences(store: State<'_, PreferencesStore>) -> Result<Preferences, String> {
    store.current()
}

#[tauri::command]
pub fn save_preferences(
    preferences: Preferences,
    store: State<'_, PreferencesStore>,
) -> Result<Preferences, String> {
    store.save(preferences)
}

#[tauri::command]
pub fn set_launch_at_login(
    enabled: bool,
    app: AppHandle,
    store: State<'_, PreferencesStore>,
) -> Result<Preferences, String> {
    login::register(&app, enabled)?;
    store.set_launch_at_login(enabled)
}

#[tauri::command]
pub fn set_start_minimized(
    enabled: bool,
    store: State<'_, PreferencesStore>,
) -> Result<Preferences, String> {
    store.set_start_minimized(enabled)
}

#[tauri::command]
pub fn list_presets(store: State<'_, PresetStore>) -> Result<PresetCatalog, String> {
    store.catalog()
}

#[tauri::command]
pub fn load_preset(
    kind: PresetKind,
    name: String,
    store: State<'_, PresetStore>,
) -> Result<ParamValues, String> {
    match kind {
        PresetKind::Builtin => presets::find(&name)
            .map(|preset| preset.values)
            .ok_or_else(|| format!("There is no built-in preset named \"{name}\".")),
        PresetKind::User => store.load(&name),
    }
}

#[tauri::command]
pub fn save_user_preset(
    name: String,
    values: ParamValues,
    store: State<'_, PresetStore>,
) -> Result<PresetSummary, String> {
    store.save(&name, &values)
}

#[tauri::command]
pub fn rename_user_preset(
    from: String,
    to: String,
    store: State<'_, PresetStore>,
) -> Result<PresetSummary, String> {
    store.rename(&from, &to)
}

#[tauri::command]
pub fn delete_user_preset(name: String, store: State<'_, PresetStore>) -> Result<(), String> {
    store.delete(&name)
}

#[tauri::command]
pub async fn import_preset(
    app: AppHandle,
    store: State<'_, PresetStore>,
) -> Result<Option<PresetSummary>, String> {
    let dir = store.dir().to_path_buf();
    let picked = pick_file(app, dir).await?;
    picked.map(|path| store.import(&path)).transpose()
}

#[tauri::command]
pub async fn export_preset(
    app: AppHandle,
    name: String,
    values: ParamValues,
    store: State<'_, PresetStore>,
) -> Result<Option<String>, String> {
    let suggested = format!("{}.json", name.trim());
    let Some(target) = pick_save_target(app, suggested).await? else {
        return Ok(None);
    };
    store.export(&values, &target)?;
    Ok(Some(target.display().to_string()))
}

#[tauri::command]
pub fn open_presets_folder(app: AppHandle, store: State<'_, PresetStore>) -> Result<(), String> {
    std::fs::create_dir_all(store.dir())
        .map_err(|error| format!("Could not create the presets folder: {error}"))?;
    app.opener()
        .open_path(store.dir().display().to_string(), None::<String>)
        .map_err(|error| format!("Could not open the presets folder: {error}"))
}

async fn pick_file(app: AppHandle, start_in: PathBuf) -> Result<Option<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(PRESET_FILTER, &["json"])
            .set_directory(start_in)
            .blocking_pick_file()
            .and_then(|file| file.as_path().map(|path| path.to_path_buf()))
    })
    .await
    .map_err(|error| format!("The file picker failed: {error}"))
}

async fn pick_save_target(app: AppHandle, file_name: String) -> Result<Option<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(PRESET_FILTER, &["json"])
            .set_file_name(file_name)
            .blocking_save_file()
            .and_then(|file| file.as_path().map(|path| path.to_path_buf()))
    })
    .await
    .map_err(|error| format!("The save dialog failed: {error}"))
}

#[tauri::command]
pub async fn begin_take(engine: State<'_, Engine>) -> Result<(), String> {
    engine.begin_capture(calibration::MAX_TAKE_SECONDS).await
}

#[tauri::command]
pub async fn end_take(
    role: TakeRole,
    engine: State<'_, Engine>,
    session: State<'_, CalibrationSession>,
) -> Result<TakeSummary, String> {
    let take = engine.end_capture().await?;
    Ok(session.store(role, take))
}

#[tauri::command]
pub async fn start_audition(
    role: TakeRole,
    output_id: Option<String>,
    engine: State<'_, Engine>,
    session: State<'_, CalibrationSession>,
) -> Result<EngineState, String> {
    let clip = session.clip(role);
    engine
        .start_audition(AuditionRequest {
            role,
            clip,
            output_id,
        })
        .await
}

#[tauri::command]
pub async fn stop_audition(engine: State<'_, Engine>) -> Result<EngineState, String> {
    engine.stop_audition().await
}

#[tauri::command]
pub fn list_takes(session: State<'_, CalibrationSession>) -> Vec<TakeSummary> {
    session.summaries()
}

#[tauri::command]
pub fn discard_takes(session: State<'_, CalibrationSession>) {
    session.discard();
}

#[tauri::command]
pub async fn analyze_takes(
    tone: Tone,
    engine: State<'_, Engine>,
    session: State<'_, CalibrationSession>,
) -> Result<CalibrationResult, String> {
    let takes = session.takes();
    let base = engine.params().snapshot();
    tauri::async_runtime::spawn_blocking(move || calibration::calibrate(&takes, &base, tone))
        .await
        .map_err(|error| format!("Calibration failed: {error}"))?
}
