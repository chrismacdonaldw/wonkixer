mod calibration;
mod commands;
mod engine;
mod login;
mod preferences;
mod presets;
mod tray;
mod window;

use tauri::Manager;

use preferences::PreferencesStore;
use presets::PresetStore;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            window::reveal(app)
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(window::hide_instead_of_closing)
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let preferences = PreferencesStore::load(data_dir.join("preferences.json"));
            let startup = preferences.current()?;
            app.manage(preferences);
            app.manage(PresetStore::new(data_dir.join("presets")));
            app.manage(calibration::CalibrationSession::default());
            app.manage(engine::Engine::spawn(app.handle().clone()));
            tray::install(app.handle())?;
            login::reconcile(app.handle(), startup.launch_at_login).ok();
            if !startup.start_minimized {
                window::reveal(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_devices,
            commands::start_engine,
            commands::stop_engine,
            commands::get_engine_state,
            commands::set_engine_supervision,
            commands::get_setup_status,
            commands::get_schema,
            commands::get_params,
            commands::set_param,
            commands::set_bypass,
            commands::set_params,
            commands::get_preferences,
            commands::save_preferences,
            commands::set_launch_at_login,
            commands::set_start_minimized,
            commands::list_presets,
            commands::load_preset,
            commands::save_user_preset,
            commands::rename_user_preset,
            commands::delete_user_preset,
            commands::import_preset,
            commands::export_preset,
            commands::open_presets_folder,
            commands::begin_take,
            commands::end_take,
            commands::list_takes,
            commands::discard_takes,
            commands::analyze_takes,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build wonkixer")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<engine::Engine>().shutdown();
            }
        });
}
