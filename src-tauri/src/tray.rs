use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager, Wry};

use crate::engine::{Engine, EngineState, STATE_EVENT};
use crate::preferences::PreferencesStore;
use crate::window;

const TRAY_ID: &str = "main";
const TOOLTIP: &str = "wonkixer";
const ICON: &[u8] = include_bytes!("../icons/32x32.png");
const SHOW_ID: &str = "show";
const ENGINE_ID: &str = "engine";
const QUIT_ID: &str = "quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineAction {
    Start,
    Stop,
}

impl EngineAction {
    fn for_state(state: &EngineState) -> Self {
        if state.running || state.waiting_for_devices {
            Self::Stop
        } else {
            Self::Start
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Start => "Start engine",
            Self::Stop => "Stop engine",
        }
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let engine_item = MenuItem::with_id(
        app,
        ENGINE_ID,
        EngineAction::Start.label(),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, SHOW_ID, "Show wonkixer", true, None::<&str>)?,
            &engine_item,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?,
        ],
    )?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON)?)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_icon_event)
        .build(app)?;
    follow_engine(app, engine_item);
    Ok(())
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        SHOW_ID => window::reveal(app),
        ENGINE_ID => toggle_engine(app),
        QUIT_ID => app.exit(0),
        _ => {}
    }
}

fn on_icon_event(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        window::reveal(tray.app_handle());
    }
}

fn follow_engine(app: &AppHandle, item: MenuItem<Wry>) {
    let handle = app.clone();
    app.listen(STATE_EVENT, move |event| {
        let Ok(state) = serde_json::from_str::<EngineState>(event.payload()) else {
            return;
        };
        let label = EngineAction::for_state(&state).label();
        let item = item.clone();
        handle
            .run_on_main_thread(move || {
                item.set_text(label).ok();
            })
            .ok();
    });
}

fn toggle_engine(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = app.state::<Engine>();
        let Ok(state) = engine.state().await else {
            return;
        };
        if EngineAction::for_state(&state) == EngineAction::Stop {
            engine.stop().await.ok();
            return;
        }
        let devices = app
            .state::<PreferencesStore>()
            .current()
            .map(|preferences| preferences.devices)
            .unwrap_or_default();
        match (devices.input_id, devices.output_id) {
            (Some(input_id), Some(output_id)) => {
                engine.start(input_id, output_id).await.ok();
            }
            _ => window::reveal(&app),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_to_stop_while_live_or_waiting_and_to_start_otherwise() {
        let live = EngineState {
            running: true,
            ..EngineState::default()
        };
        let waiting = EngineState {
            waiting_for_devices: true,
            error: Some("The input device is not connected.".to_owned()),
            ..EngineState::default()
        };
        let failed = EngineState {
            error: Some("Could not open the output device.".to_owned()),
            ..EngineState::default()
        };

        assert_eq!(EngineAction::for_state(&live), EngineAction::Stop);
        assert_eq!(EngineAction::for_state(&waiting), EngineAction::Stop);
        assert_eq!(EngineAction::for_state(&failed), EngineAction::Start);
        assert_eq!(
            EngineAction::for_state(&EngineState::default()),
            EngineAction::Start
        );
    }
}
