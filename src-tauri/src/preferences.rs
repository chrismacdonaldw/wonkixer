use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use wonkixer_dsp::ParamValues;

pub const VERSION: u8 = 1;
const DEFAULT_PRESET: &str = "Headset clean-up";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub version: u8,
    pub devices: Devices,
    pub auto_start: bool,
    pub session: ParamValues,
    pub active_preset: Option<PresetRef>,
    pub launch_at_login: bool,
    pub start_minimized: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Devices {
    pub input_id: Option<String>,
    pub output_id: Option<String>,
    pub input_name: Option<String>,
    pub output_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetRef {
    pub kind: PresetKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetKind {
    Builtin,
    User,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            devices: Devices::default(),
            auto_start: true,
            session: ParamValues::default(),
            active_preset: Some(PresetRef {
                kind: PresetKind::Builtin,
                name: DEFAULT_PRESET.to_owned(),
            }),
            launch_at_login: false,
            start_minimized: false,
        }
    }
}

pub struct PreferencesStore {
    path: PathBuf,
    current: Mutex<Preferences>,
}

impl PreferencesStore {
    pub fn load(path: PathBuf) -> Self {
        let current = read(&path).unwrap_or_default();
        Self {
            path,
            current: Mutex::new(current),
        }
    }

    pub fn current(&self) -> Result<Preferences, String> {
        self.current
            .lock()
            .map(|current| current.clone())
            .map_err(|_| UNAVAILABLE.to_owned())
    }

    pub fn save(&self, preferences: Preferences) -> Result<Preferences, String> {
        self.update(|current| {
            *current = Preferences {
                launch_at_login: current.launch_at_login,
                start_minimized: current.start_minimized,
                ..preferences
            }
        })
    }

    pub fn set_launch_at_login(&self, enabled: bool) -> Result<Preferences, String> {
        self.update(|current| current.launch_at_login = enabled)
    }

    pub fn set_start_minimized(&self, enabled: bool) -> Result<Preferences, String> {
        self.update(|current| current.start_minimized = enabled)
    }

    fn update(&self, change: impl FnOnce(&mut Preferences)) -> Result<Preferences, String> {
        let mut current = self.current.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        let mut next = current.clone();
        change(&mut next);
        next.version = VERSION;
        write_atomic(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

const UNAVAILABLE: &str = "Preferences are unavailable.";

fn read(path: &Path) -> Option<Preferences> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent folder", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let json = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not encode {}: {error}", path.display()))?;
    let staging = path.with_extension("json.tmp");
    fs::write(&staging, json)
        .map_err(|error| format!("Could not write {}: {error}", staging.display()))?;
    fs::rename(&staging, path)
        .map_err(|error| format!("Could not replace {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonkixer_dsp::{ParamId, StageId};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wonkixer-test-{}-{name}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        dir.join("preferences.json")
    }

    #[test]
    fn missing_file_yields_defaults_with_the_headset_preset_active() {
        let store = PreferencesStore::load(scratch("missing"));
        let current = store.current().expect("current");
        assert_eq!(current, Preferences::default());
        assert_eq!(
            current.active_preset.map(|preset| preset.name),
            Some(DEFAULT_PRESET.to_owned())
        );
    }

    #[test]
    fn saved_preferences_round_trip_through_disk() {
        let path = scratch("roundtrip");
        let store = PreferencesStore::load(path.clone());
        let mut preferences = Preferences::default();
        preferences.devices.input_id = Some("mic".to_owned());
        preferences.devices.input_name = Some("Microphone (HyperX)".to_owned());
        preferences.auto_start = false;
        preferences.session.set(ParamId::CompressorRatio, 5.0);
        preferences
            .session
            .set_bypassed(StageId::NoiseSuppressor, false);
        preferences.active_preset = None;

        store.save(preferences.clone()).expect("save");
        let reloaded = PreferencesStore::load(path).current().expect("current");

        assert_eq!(reloaded, preferences);
        assert!(!reloaded.session.is_bypassed(StageId::NoiseSuppressor));
    }

    #[test]
    fn saving_from_the_window_keeps_the_startup_behaviour() {
        let path = scratch("behaviour");
        let store = PreferencesStore::load(path.clone());
        store.set_launch_at_login(true).expect("launch at login");
        store.set_start_minimized(true).expect("start minimized");

        let from_window = Preferences {
            auto_start: false,
            ..Preferences::default()
        };
        let saved = store.save(from_window).expect("save");

        assert!(!saved.auto_start);
        assert!(saved.launch_at_login);
        assert!(saved.start_minimized);
        assert_eq!(
            PreferencesStore::load(path).current().expect("current"),
            saved
        );
    }

    #[test]
    fn older_files_without_startup_behaviour_default_to_off() {
        let path = scratch("older");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, br#"{ "version": 1, "autoStart": false }"#).expect("write");

        let current = PreferencesStore::load(path).current().expect("current");

        assert!(!current.auto_start);
        assert!(!current.launch_at_login);
        assert!(!current.start_minimized);
    }

    #[test]
    fn older_files_without_device_names_keep_their_ids() {
        let path = scratch("unnamed");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(
            &path,
            br#"{ "version": 1, "devices": { "inputId": "mic", "outputId": "cable" } }"#,
        )
        .expect("write");

        let devices = PreferencesStore::load(path)
            .current()
            .expect("current")
            .devices;

        assert_eq!(devices.input_id.as_deref(), Some("mic"));
        assert_eq!(devices.output_id.as_deref(), Some("cable"));
        assert_eq!(devices.input_name, None);
        assert_eq!(devices.output_name, None);
    }

    #[test]
    fn corrupt_files_fall_back_to_defaults() {
        let path = scratch("corrupt");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, b"{ not json").expect("write");
        let store = PreferencesStore::load(path);
        assert_eq!(store.current().expect("current"), Preferences::default());
    }
}
