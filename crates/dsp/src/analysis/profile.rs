use serde::Serialize;

use super::frames::percentile;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelStats {
    pub p10: f32,
    pub p50: f32,
    pub p90: f32,
    pub max: f32,
}

impl LevelStats {
    pub fn of(sorted_db: &[f32]) -> Option<Self> {
        let max = *sorted_db.last()?;
        Some(Self {
            p10: percentile(sorted_db, 0.1),
            p50: percentile(sorted_db, 0.5),
            p90: percentile(sorted_db, 0.9),
            max,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sibilance {
    pub frequency_hz: f32,
    pub level_db: f32,
    pub vowel_level_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub frequency_hz: f32,
    pub level_db: f32,
    pub reference_db: f32,
}

impl Band {
    pub fn excess_db(&self) -> f32 {
        self.level_db - self.reference_db
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceProfile {
    pub sample_rate: u32,
    pub noise_floor_db: f32,
    pub speech: LevelStats,
    pub speech_seconds: f32,
    pub loud: Option<LevelStats>,
    pub pitch_low_hz: Option<f32>,
    pub pitch_median_hz: Option<f32>,
    pub sibilance: Option<Sibilance>,
    pub spectrum: Vec<Band>,
    pub clipped: bool,
}
