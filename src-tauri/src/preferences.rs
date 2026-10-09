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
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Devices {
    pub input_id: Option<String>,
    pub output_id: Option<String>,
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

    pub fn save(&self, mut preferences: Preferences) -> Result<Preferences, String> {
        preferences.version = VERSION;
        write_atomic(&self.path, &preferences)?;
        let mut current = self.current.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        *current = preferences.clone();
        Ok(preferences)
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
    fn corrupt_files_fall_back_to_defaults() {
        let path = scratch("corrupt");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, b"{ not json").expect("write");
        let store = PreferencesStore::load(path);
        assert_eq!(store.current().expect("current"), Preferences::default());
    }
}
