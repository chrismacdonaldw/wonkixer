use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use wonkixer_dsp::{ParamValues, presets};

use crate::preferences::write_atomic;

const EXTENSION: &str = "json";
const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSummary {
    pub name: String,
    pub modified_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetCatalog {
    pub builtin: Vec<String>,
    pub user: Vec<PresetSummary>,
}

pub struct PresetStore {
    dir: PathBuf,
}

impl PresetStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn catalog(&self) -> Result<PresetCatalog, String> {
        let mut user = Vec::new();
        if self.dir.is_dir() {
            let entries = fs::read_dir(&self.dir)
                .map_err(|error| format!("Could not read the presets folder: {error}"))?;
            for entry in entries.flatten() {
                let path = entry.path();
                let is_preset = path.extension().and_then(|ext| ext.to_str()) == Some(EXTENSION);
                if let Some(name) = is_preset.then(|| stem(&path)).flatten() {
                    user.push(summary(name, &path));
                }
            }
        }
        user.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(PresetCatalog {
            builtin: presets::builtin()
                .into_iter()
                .map(|preset| preset.name.to_owned())
                .collect(),
            user,
        })
    }

    pub fn load(&self, name: &str) -> Result<ParamValues, String> {
        let path = self.path_for(name);
        let bytes = fs::read(&path).map_err(|_| format!("There is no preset named \"{name}\"."))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("The preset \"{name}\" could not be read: {error}"))
    }

    pub fn save(&self, name: &str, values: &ParamValues) -> Result<PresetSummary, String> {
        let name = validate_name(name)?;
        let path = self.path_for(&name);
        write_atomic(&path, values)?;
        Ok(summary(name, &path))
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<PresetSummary, String> {
        let to = validate_name(to)?;
        let source = self.path_for(from);
        if !source.is_file() {
            return Err(format!("There is no preset named \"{from}\"."));
        }
        let target = self.path_for(&to);
        if to != from && target.exists() {
            return Err(format!("You already have a preset named \"{to}\"."));
        }
        fs::rename(&source, &target)
            .map_err(|error| format!("Could not rename \"{from}\": {error}"))?;
        Ok(summary(to, &target))
    }

    pub fn delete(&self, name: &str) -> Result<(), String> {
        let path = self.path_for(name);
        if !path.is_file() {
            return Err(format!("There is no preset named \"{name}\"."));
        }
        trash::delete(&path).map_err(|error| format!("Could not delete \"{name}\": {error}"))
    }

    pub fn import(&self, source: &Path) -> Result<PresetSummary, String> {
        let bytes = fs::read(source)
            .map_err(|error| format!("Could not read {}: {error}", source.display()))?;
        let values: ParamValues = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{} is not a preset file: {error}", source.display()))?;
        let base = stem(source).unwrap_or_else(|| "Imported preset".to_owned());
        let name = self.unique_name(&sanitize(&base));
        self.save(&name, &values)
    }

    pub fn export(&self, values: &ParamValues, target: &Path) -> Result<(), String> {
        write_atomic(target, values)
    }

    fn unique_name(&self, base: &str) -> String {
        let mut candidate = base.to_owned();
        let mut counter = 2;
        while self.path_for(&candidate).exists() || presets::find(&candidate).is_some() {
            candidate = format!("{base} {counter}");
            counter += 1;
        }
        candidate
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.{EXTENSION}"))
    }
}

pub fn validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Give the preset a name.".to_owned());
    }
    if trimmed.contains(FORBIDDEN) {
        return Err("Names cannot contain \\ / : * ? \" < > |".to_owned());
    }
    if presets::find(trimmed).is_some() {
        return Err(format!(
            "\"{trimmed}\" is a built-in preset. Pick another name."
        ));
    }
    Ok(trimmed.to_owned())
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| if FORBIDDEN.contains(&ch) { ' ' } else { ch })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Imported preset".to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_owned)
}

fn summary(name: String, path: &Path) -> PresetSummary {
    let modified_ms = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis() as u64);
    PresetSummary { name, modified_ms }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonkixer_dsp::ParamId;

    fn store(name: &str) -> PresetStore {
        let dir =
            std::env::temp_dir().join(format!("wonkixer-presets-{}-{name}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        PresetStore::new(dir)
    }

    #[test]
    fn names_are_validated_against_the_file_system_and_builtins() {
        assert_eq!(validate_name("  Discord  "), Ok("Discord".to_owned()));
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("Broadcast").is_err());
    }

    #[test]
    fn save_list_load_rename_and_export_round_trip() {
        let store = store("roundtrip");
        let mut values = ParamValues::default();
        values.set(ParamId::CompressorRatio, 4.2);

        let saved = store.save("Discord", &values).expect("save");
        assert_eq!(saved.name, "Discord");
        let catalog = store.catalog().expect("catalog");
        assert_eq!(catalog.user.len(), 1);
        assert_eq!(catalog.builtin[0], "Flat");
        assert_eq!(store.load("Discord").expect("load"), values);

        let renamed = store.rename("Discord", "Stream").expect("rename");
        assert_eq!(renamed.name, "Stream");
        assert!(store.load("Discord").is_err());
        assert_eq!(store.load("Stream").expect("load"), values);

        let target = store.dir().join("exported").join("Stream copy.json");
        store.export(&values, &target).expect("export");
        let imported = store.import(&target).expect("import");
        assert_eq!(imported.name, "Stream copy");
        assert_eq!(store.load("Stream copy").expect("load"), values);
    }

    #[test]
    fn importing_a_clashing_name_picks_a_free_one() {
        let store = store("clash");
        let values = ParamValues::default();
        store.save("Mine", &values).expect("save");
        let source = store.dir().join("elsewhere").join("Mine.json");
        store.export(&values, &source).expect("export");
        assert_eq!(store.import(&source).expect("import").name, "Mine 2");
    }

    #[test]
    fn an_empty_folder_lists_only_builtins() {
        let catalog = store("empty").catalog().expect("catalog");
        assert!(catalog.user.is_empty());
        assert_eq!(catalog.builtin.len(), 5);
    }
}
