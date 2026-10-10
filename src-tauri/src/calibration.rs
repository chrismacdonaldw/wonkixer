use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use wonkixer_dsp::ParamValues;
use wonkixer_dsp::analysis::{
    self, Calibration, Captures, Note, Take, TakeRole, Tone, VoiceProfile,
};

use crate::engine::Clip;

pub const MAX_TAKE_SECONDS: u32 = 60;

#[derive(Default)]
pub struct CalibrationSession {
    takes: Mutex<BTreeMap<TakeRole, Take>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeSummary {
    pub role: TakeRole,
    pub seconds: f32,
    pub peak_db: f32,
    pub clipped: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationResult {
    pub profile: VoiceProfile,
    pub values: ParamValues,
    pub notes: Vec<Note>,
}

impl CalibrationSession {
    pub fn store(&self, role: TakeRole, take: Take) -> TakeSummary {
        let summary = summarize(role, &take);
        self.lock().insert(role, take);
        summary
    }

    pub fn summaries(&self) -> Vec<TakeSummary> {
        self.lock()
            .iter()
            .map(|(&role, take)| summarize(role, take))
            .collect()
    }

    pub fn discard(&self) {
        self.lock().clear();
    }

    pub fn takes(&self) -> BTreeMap<TakeRole, Take> {
        self.lock().clone()
    }

    pub fn clip(&self, role: TakeRole) -> Option<Clip> {
        self.lock()
            .get(&role)
            .filter(|take| !take.samples.is_empty())
            .map(|take| Clip::new(&take.samples, take.sample_rate))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<TakeRole, Take>> {
        self.takes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn calibrate(
    takes: &BTreeMap<TakeRole, Take>,
    base: &ParamValues,
    tone: Tone,
) -> Result<CalibrationResult, String> {
    let silence = takes
        .get(&TakeRole::Silence)
        .ok_or("Record the silence take first.")?;
    let speech = takes
        .get(&TakeRole::Speech)
        .ok_or("Record the speech take first.")?;
    let profile = analysis::analyze(Captures {
        silence,
        speech,
        loud: takes.get(&TakeRole::Loud),
    })
    .map_err(|error| error.to_string())?;
    let Calibration { values, notes } = analysis::derive(&profile, base, tone);
    Ok(CalibrationResult {
        profile,
        values,
        notes,
    })
}

fn summarize(role: TakeRole, take: &Take) -> TakeSummary {
    TakeSummary {
        role,
        seconds: take.seconds(),
        peak_db: take.peak_db(),
        clipped: take.is_clipped(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(seconds: f32, level: f32) -> Take {
        Take {
            sample_rate: 48_000,
            samples: (0..(48_000.0 * seconds) as usize)
                .map(|index| if index % 2 == 0 { level } else { -level })
                .collect(),
        }
    }

    #[test]
    fn takes_are_summarised_and_replaced_per_role() {
        let session = CalibrationSession::default();
        let summary = session.store(TakeRole::Silence, take(2.0, 0.001));
        assert_eq!(summary.role, TakeRole::Silence);
        assert!((summary.seconds - 2.0).abs() < 0.01);
        assert!((summary.peak_db + 60.0).abs() < 0.1);
        assert!(!summary.clipped);

        session.store(TakeRole::Silence, take(3.0, 1.0));
        let summaries = session.summaries();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].clipped);
        session.discard();
        assert!(session.summaries().is_empty());
    }

    #[test]
    fn only_a_recorded_take_can_be_auditioned() {
        let session = CalibrationSession::default();
        assert!(session.clip(TakeRole::Speech).is_none());

        session.store(TakeRole::Speech, take(0.0, 0.1));
        assert!(session.clip(TakeRole::Speech).is_none());

        session.store(TakeRole::Speech, take(1.0, 0.1));
        assert!(session.clip(TakeRole::Speech).is_some());
        assert!(session.clip(TakeRole::Loud).is_none());
    }

    #[test]
    fn calibration_needs_the_silence_and_speech_takes() {
        let mut takes = BTreeMap::new();
        let base = ParamValues::default();
        assert_eq!(
            calibrate(&takes, &base, Tone::Neutral).unwrap_err(),
            "Record the silence take first."
        );
        takes.insert(TakeRole::Silence, take(2.0, 0.001));
        assert_eq!(
            calibrate(&takes, &base, Tone::Neutral).unwrap_err(),
            "Record the speech take first."
        );
        takes.insert(TakeRole::Speech, take(5.0, 0.001));
        assert!(
            calibrate(&takes, &base, Tone::Neutral)
                .unwrap_err()
                .contains("No speech")
        );
    }
}
