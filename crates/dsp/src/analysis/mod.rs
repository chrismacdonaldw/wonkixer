mod derive;
mod frames;
mod pitch;
mod profile;
mod sibilance;
mod spectrum;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use derive::{Calibration, Note, Tone, derive};
pub use profile::{Band, LevelStats, Sibilance, VoiceProfile};

use crate::units::gain_to_db;
use frames::percentile;

pub const MIN_SILENCE_SECONDS: f32 = 1.5;
pub const MIN_SPEECH_SECONDS: f32 = 4.0;
pub const MIN_LOUD_SECONDS: f32 = 2.0;
const MIN_ACTIVE_SPEECH_SECONDS: f32 = 2.0;
const MIN_SPEECH_PEAK_DB: f32 = -45.0;
const NOISE_PERCENTILE: f32 = 0.95;
const LOUD_PERCENTILE: f32 = 0.98;
const QUIET_SILENCE_MARGIN_DB: f32 = 15.0;
const MIN_VOICED_FRAMES: usize = 20;
const CLIP_LEVEL: f32 = 0.985;
const CLIP_SAMPLES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TakeRole {
    Silence,
    Speech,
    Loud,
}

impl TakeRole {
    pub fn label(self) -> &'static str {
        match self {
            TakeRole::Silence => "silence",
            TakeRole::Speech => "speech",
            TakeRole::Loud => "loud",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Take {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl Take {
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    pub fn peak_db(&self) -> f32 {
        gain_to_db(
            self.samples
                .iter()
                .fold(0.0f32, |peak, sample| peak.max(sample.abs())),
        )
    }

    pub fn is_clipped(&self) -> bool {
        self.samples
            .iter()
            .filter(|sample| sample.abs() >= CLIP_LEVEL)
            .count()
            >= CLIP_SAMPLES
    }
}

#[derive(Clone, Copy)]
pub struct Captures<'a> {
    pub silence: &'a Take,
    pub speech: &'a Take,
    pub loud: Option<&'a Take>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisError {
    TooShort { role: TakeRole, seconds: f32 },
    SampleRateMismatch,
    NoSpeechDetected,
    SilenceNotQuiet,
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalysisError::TooShort { role, seconds } => write!(
                f,
                "The {} take needs at least {seconds:.0} seconds.",
                role.label()
            ),
            AnalysisError::SampleRateMismatch => write!(
                f,
                "The takes were recorded at different sample rates. Record them again."
            ),
            AnalysisError::NoSpeechDetected => write!(
                f,
                "No speech was found in the speech take. Speak at a normal volume and try again."
            ),
            AnalysisError::SilenceNotQuiet => write!(
                f,
                "The silence take was not quiet. Stay silent while it records."
            ),
        }
    }
}

impl std::error::Error for AnalysisError {}

pub fn analyze(captures: Captures) -> Result<VoiceProfile, AnalysisError> {
    let sample_rate = captures.speech.sample_rate;
    let same_rate = captures.silence.sample_rate == sample_rate
        && captures
            .loud
            .is_none_or(|take| take.sample_rate == sample_rate);
    if !same_rate {
        return Err(AnalysisError::SampleRateMismatch);
    }
    require(captures.silence, TakeRole::Silence, MIN_SILENCE_SECONDS)?;
    require(captures.speech, TakeRole::Speech, MIN_SPEECH_SECONDS)?;
    if let Some(loud) = captures.loud {
        require(loud, TakeRole::Loud, MIN_LOUD_SECONDS)?;
    }

    let noise_floor_db = percentile(
        &frames::sorted(frames::peaks_db(captures.silence)),
        NOISE_PERCENTILE,
    );
    let speech_peaks = frames::peaks_db(captures.speech);
    let loud_reference_db = percentile(&frames::sorted(speech_peaks.clone()), LOUD_PERCENTILE);
    if loud_reference_db < MIN_SPEECH_PEAK_DB {
        return Err(AnalysisError::NoSpeechDetected);
    }
    if noise_floor_db > loud_reference_db - QUIET_SILENCE_MARGIN_DB {
        return Err(AnalysisError::SilenceNotQuiet);
    }

    let threshold_db = frames::activity_threshold(noise_floor_db, loud_reference_db);
    let active = frames::active_mask(&speech_peaks, threshold_db);
    let active_peaks = frames::sorted(frames::select(&speech_peaks, &active));
    let speech_seconds = frames::seconds(active_peaks.len());
    let speech = LevelStats::of(&active_peaks)
        .filter(|_| speech_seconds >= MIN_ACTIVE_SPEECH_SECONDS)
        .ok_or(AnalysisError::NoSpeechDetected)?;

    let loud = captures.loud.and_then(|take| {
        let peaks = frames::peaks_db(take);
        let mask = frames::active_mask(&peaks, threshold_db);
        LevelStats::of(&frames::sorted(frames::select(&peaks, &mask)))
    });

    let voiced = frames::sorted(pitch::track(captures.speech, &active));
    let (pitch_low_hz, pitch_median_hz) = if voiced.len() >= MIN_VOICED_FRAMES {
        (
            Some(percentile(&voiced, 0.1)),
            Some(percentile(&voiced, 0.5)),
        )
    } else {
        (None, None)
    };

    Ok(VoiceProfile {
        sample_rate,
        noise_floor_db,
        speech,
        speech_seconds,
        loud,
        pitch_low_hz,
        pitch_median_hz,
        sibilance: sibilance::measure(captures.speech, &active),
        spectrum: spectrum::measure(captures.speech, &active),
        clipped: [Some(captures.silence), Some(captures.speech), captures.loud]
            .into_iter()
            .flatten()
            .any(Take::is_clipped),
    })
}

fn require(take: &Take, role: TakeRole, seconds: f32) -> Result<(), AnalysisError> {
    if take.seconds() + f32::EPSILON < seconds {
        Err(AnalysisError::TooShort { role, seconds })
    } else {
        Ok(())
    }
}
