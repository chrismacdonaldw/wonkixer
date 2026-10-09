mod commands;
mod engine;

use tauri::Manager;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(engine::Engine::spawn(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_devices,
            commands::start_engine,
            commands::stop_engine,
            commands::get_engine_state,
            commands::get_schema,
            commands::get_params,
            commands::set_param,
            commands::set_bypass,
            commands::set_params,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build wonkixer")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<engine::Engine>().shutdown();
            }
        });
}
