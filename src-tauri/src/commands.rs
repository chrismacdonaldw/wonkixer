use tauri::State;
use wonkixer_dsp::params::{self, Schema};
use wonkixer_dsp::{ParamId, ParamValues, StageId};

use crate::engine::devices::{self, AudioDevices};
use crate::engine::{Engine, EngineState};

#[tauri::command]
pub async fn list_audio_devices() -> Result<AudioDevices, String> {
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
