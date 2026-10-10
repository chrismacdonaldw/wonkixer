use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::dynamics::compressor_reduction;
use crate::params::{ParamId, ParamValues, StageId};

use super::profile::{Band, VoiceProfile};

const SPEECH_PEAK_TARGET_DB: f32 = -12.0;
const OUTPUT_PEAK_TARGET_DB: f32 = -8.0;
const LIMITER_MARGIN_DB: f32 = 1.0;
const PITCH_TO_CUTOFF: f32 = 0.75;
const CUTOFF_STEP_HZ: f32 = 5.0;
const CUTOFF_HZ: RangeInclusive<f32> = 50.0..=150.0;
const EXPANDER_ABOVE_NOISE_DB: f32 = 6.0;
const EXPANDER_BELOW_SPEECH_DB: f32 = 6.0;
const SUPPRESSION_NOISE_DB: f32 = -60.0;
const DEESSER_BELOW_SIBILANT_DB: f32 = 3.0;
const DEESSER_ABOVE_VOWEL_DB: f32 = 3.0;
const DEESSER_STEP_HZ: f32 = 50.0;
const STRONG_SIBILANCE_DB: f32 = 12.0;
const PEAK_REDUCTION_DB: f32 = 6.0;
const RATIO_PER_DB_OF_SPREAD: f32 = 0.25;
const RATIO: RangeInclusive<f32> = 2.0..=5.0;
const EQ_CORRECTION: f32 = 0.5;
const EQ_DEADBAND_DB: f32 = 1.5;
const EQ_CUT_LIMIT_DB: f32 = 3.0;
const EQ_BOOST_LIMIT_DB: f32 = 2.0;
const MUD_CORRECTION: f32 = 0.6;
const MUD_CUT_LIMIT_DB: f32 = 4.0;
const LOW_HZ: RangeInclusive<f32> = 63.0..=160.0;
const MUD_HZ: RangeInclusive<f32> = 200.0..=630.0;
const PRESENCE_HZ: RangeInclusive<f32> = 2_000.0..=5_000.0;
const AIR_HZ: RangeInclusive<f32> = 8_000.0..=12_500.0;
const EQ_GAINS: [ParamId; 4] = [
    ParamId::LowShelfGain,
    ParamId::LowMidGain,
    ParamId::PresenceGain,
    ParamId::HighShelfGain,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Flat,
    #[default]
    Neutral,
    Warm,
    Bright,
}

impl Tone {
    fn label(self) -> &'static str {
        match self {
            Tone::Flat => "Flat",
            Tone::Neutral => "Neutral",
            Tone::Warm => "Warm",
            Tone::Bright => "Bright",
        }
    }

    fn offsets(self) -> Offsets {
        match self {
            Tone::Flat | Tone::Neutral => Offsets::NONE,
            Tone::Warm => Offsets {
                low: 1.0,
                presence: -1.0,
                air: -1.0,
            },
            Tone::Bright => Offsets {
                low: -1.0,
                presence: 1.0,
                air: 1.5,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct Offsets {
    low: f32,
    presence: f32,
    air: f32,
}

impl Offsets {
    const NONE: Self = Self {
        low: 0.0,
        presence: 0.0,
        air: 0.0,
    };
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub stage: StageId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    pub values: ParamValues,
    pub notes: Vec<Note>,
}

pub fn derive(profile: &VoiceProfile, base: &ParamValues, tone: Tone) -> Calibration {
    let mut plan = Plan {
        profile,
        values: *base,
        notes: Vec::new(),
        trim: 0.0,
    };
    plan.input();
    plan.high_pass();
    plan.noise_and_expander();
    plan.de_esser();
    plan.equalizer(tone);
    let curve = plan.compressor();
    plan.output(&curve);
    Calibration {
        values: plan.values,
        notes: plan.notes,
    }
}

struct Plan<'a> {
    profile: &'a VoiceProfile,
    values: ParamValues,
    notes: Vec<Note>,
    trim: f32,
}

struct Curve {
    threshold: f32,
    ratio: f32,
    knee: f32,
    makeup: f32,
}

impl Curve {
    fn output(&self, level_db: f32) -> f32 {
        level_db - compressor_reduction(level_db, self.threshold, self.ratio, self.knee)
            + self.makeup
    }
}

impl Plan<'_> {
    fn set(&mut self, id: ParamId, value: f32) -> f32 {
        let snapped = snap(id, value);
        self.values.set(id, snapped);
        snapped
    }

    fn note(&mut self, stage: StageId, text: impl Into<String>) {
        self.notes.push(Note {
            stage,
            text: text.into(),
        });
    }

    fn trimmed(&self, level_db: f32) -> f32 {
        level_db + self.trim
    }

    fn input(&mut self) {
        let peak = self.profile.speech.p90;
        self.trim = self.set(ParamId::InputGain, SPEECH_PEAK_TARGET_DB - peak);
        self.note(
            StageId::Input,
            format!(
                "Normal speech peaked at {peak:.0} dBFS, so the input is trimmed by {:+.1} dB to sit at {SPEECH_PEAK_TARGET_DB:.0} dBFS.",
                self.trim
            ),
        );
        if self.profile.clipped {
            self.note(
                StageId::Input,
                "The microphone clipped during a take. Lower its level in Windows Sound settings and calibrate again.",
            );
        }
    }

    fn high_pass(&mut self) {
        match self.profile.pitch_low_hz {
            Some(pitch) => {
                let wanted = round_to(pitch * PITCH_TO_CUTOFF, CUTOFF_STEP_HZ)
                    .clamp(*CUTOFF_HZ.start(), *CUTOFF_HZ.end());
                let cutoff = self.set(ParamId::HighPassCutoff, wanted);
                self.note(
                    StageId::HighPass,
                    format!(
                        "Your lowest pitch is around {pitch:.0} Hz, so the high-pass starts at {cutoff:.0} Hz."
                    ),
                );
            }
            None => {
                let cutoff = self.values.get(ParamId::HighPassCutoff);
                self.note(
                    StageId::HighPass,
                    format!(
                        "Pitch could not be measured, so the high-pass stays at {cutoff:.0} Hz."
                    ),
                );
            }
        }
    }

    fn noise_and_expander(&mut self) {
        let noise = self.trimmed(self.profile.noise_floor_db);
        let quiet = self.trimmed(self.profile.speech.p10);
        let floor = noise + EXPANDER_ABOVE_NOISE_DB;
        let ceiling = quiet - EXPANDER_BELOW_SPEECH_DB;
        let crowded = floor > ceiling;
        let wanted = if crowded {
            (floor + ceiling) / 2.0
        } else {
            floor
        };
        let threshold = self.set(ParamId::ExpanderThreshold, wanted);
        let suppress = crowded || noise > SUPPRESSION_NOISE_DB;
        self.values
            .set_bypassed(StageId::NoiseSuppressor, !suppress);
        self.note(
            StageId::Expander,
            format!(
                "Room noise peaks at {noise:.0} dBFS and your quietest words at {quiet:.0} dBFS, so the expander opens at {threshold:.1} dB."
            ),
        );
        self.note(
            StageId::NoiseSuppressor,
            if crowded {
                "Quiet words sit close to the noise, so noise suppression is on to help the expander."
            } else if suppress {
                "The noise floor is high enough to hear, so noise suppression is on."
            } else {
                "The noise floor is low, so noise suppression stays off."
            },
        );
    }

    fn de_esser(&mut self) {
        let Some(sibilance) = self.profile.sibilance else {
            self.note(
                StageId::DeEsser,
                "No strong S sounds were found, so the de-esser keeps its current settings.",
            );
            return;
        };
        let frequency = self.set(
            ParamId::DeEsserFrequency,
            round_to(sibilance.frequency_hz, DEESSER_STEP_HZ),
        );
        let sibilant = self.trimmed(sibilance.level_db);
        let vowel = self.trimmed(sibilance.vowel_level_db);
        let threshold = self.set(
            ParamId::DeEsserThreshold,
            (sibilant - DEESSER_BELOW_SIBILANT_DB).max(vowel + DEESSER_ABOVE_VOWEL_DB),
        );
        self.note(
            StageId::DeEsser,
            format!(
                "Your S sounds centre on {:.1} kHz and peak at {sibilant:.0} dB in that band, so the de-esser listens there and acts above {threshold:.1} dB.",
                frequency / 1000.0
            ),
        );
    }

    fn equalizer(&mut self, tone: Tone) {
        if tone == Tone::Flat {
            for id in EQ_GAINS {
                self.set(id, 0.0);
            }
            self.note(StageId::Equalizer, "The EQ is left flat.");
            return;
        }
        let profile = self.profile;
        let spectrum = profile.spectrum.as_slice();
        let offsets = tone.offsets();

        let low = correction(region_mean(spectrum, LOW_HZ)).min(EQ_BOOST_LIMIT_DB) + offsets.low;
        let low_gain = self.set(ParamId::LowShelfGain, low);

        let mud_gain = match region_extreme(spectrum, MUD_HZ, 1.0) {
            Some(band) if band.excess_db() > EQ_DEADBAND_DB => {
                self.set(ParamId::LowMidFrequency, band.frequency_hz);
                self.set(
                    ParamId::LowMidGain,
                    -(band.excess_db() * MUD_CORRECTION).min(MUD_CUT_LIMIT_DB),
                )
            }
            _ => self.set(ParamId::LowMidGain, 0.0),
        };

        let presence_excess = region_mean(spectrum, PRESENCE_HZ);
        if let Some(band) = region_extreme(spectrum, PRESENCE_HZ, presence_excess) {
            self.set(ParamId::PresenceFrequency, band.frequency_hz);
        }
        let presence_gain = self.set(
            ParamId::PresenceGain,
            correction(presence_excess) + offsets.presence,
        );

        let strong_sibilance = profile.sibilance.is_some_and(|sibilance| {
            sibilance.level_db - sibilance.vowel_level_db > STRONG_SIBILANCE_DB
        });
        let air_limit = if strong_sibilance {
            0.0
        } else {
            EQ_BOOST_LIMIT_DB
        };
        let air_gain = self.set(
            ParamId::HighShelfGain,
            correction(region_mean(spectrum, AIR_HZ)).min(air_limit) + offsets.air,
        );

        let mud_hz = self.values.get(ParamId::LowMidFrequency);
        self.note(
            StageId::Equalizer,
            format!(
                "{} tone: low shelf {low_gain:+.1} dB, {mud_hz:.0} Hz {mud_gain:+.1} dB, presence {presence_gain:+.1} dB, high shelf {air_gain:+.1} dB.",
                tone.label()
            ),
        );
    }

    fn compressor(&mut self) -> Curve {
        let quiet = self.trimmed(self.profile.speech.p10);
        let typical = self.trimmed(self.profile.speech.p50);
        let loud = self.trimmed(self.profile.speech.p90);
        let spread = (loud - quiet).max(0.0);
        let ratio = self.set(
            ParamId::CompressorRatio,
            (spread * RATIO_PER_DB_OF_SPREAD).clamp(*RATIO.start(), *RATIO.end()),
        );
        let threshold = self.set(
            ParamId::CompressorThreshold,
            loud - PEAK_REDUCTION_DB / (1.0 - 1.0 / ratio),
        );
        let knee = self.values.get(ParamId::CompressorKnee);
        let reduction = |level: f32| compressor_reduction(level, threshold, ratio, knee);
        let average = 0.25 * reduction(quiet) + 0.5 * reduction(typical) + 0.25 * reduction(loud);
        let makeup = self.set(ParamId::CompressorMakeup, average);
        self.note(
            StageId::Compressor,
            format!(
                "Your words span {spread:.0} dB from quiet to loud, so the compressor works at {ratio:.1}:1 above {threshold:.1} dB and adds {makeup:.1} dB back."
            ),
        );
        Curve {
            threshold,
            ratio,
            knee,
            makeup,
        }
    }

    fn output(&mut self, curve: &Curve) {
        let predicted = curve.output(self.trimmed(self.profile.speech.p90));
        let mut gain = OUTPUT_PEAK_TARGET_DB - predicted;
        if let Some(loud) = self.profile.loud {
            let ceiling = self.values.get(ParamId::LimiterCeiling) - LIMITER_MARGIN_DB;
            let peak = curve.output(self.trimmed(loud.max)) + gain;
            if peak > ceiling {
                gain -= peak - ceiling;
            }
        }
        let gain = self.set(ParamId::OutputGain, gain);
        self.note(
            StageId::Output,
            format!(
                "Output trimmed by {gain:+.1} dB so normal speech peaks near {OUTPUT_PEAK_TARGET_DB:.0} dBFS and shouting stays under the limiter."
            ),
        );
    }
}

fn snap(id: ParamId, value: f32) -> f32 {
    let step = id.spec().coarse;
    id.clamp((value / step).round() * step)
}

fn round_to(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

fn correction(excess_db: f32) -> f32 {
    if excess_db.abs() < EQ_DEADBAND_DB {
        0.0
    } else {
        (-excess_db * EQ_CORRECTION).clamp(-EQ_CUT_LIMIT_DB, EQ_CUT_LIMIT_DB)
    }
}

fn in_region(spectrum: &[Band], hz: RangeInclusive<f32>) -> impl Iterator<Item = &Band> {
    spectrum
        .iter()
        .filter(move |band| hz.contains(&band.frequency_hz))
}

fn region_mean(spectrum: &[Band], hz: RangeInclusive<f32>) -> f32 {
    let (sum, count) = in_region(spectrum, hz).fold((0.0f32, 0usize), |(sum, count), band| {
        (sum + band.excess_db(), count + 1)
    });
    if count == 0 { 0.0 } else { sum / count as f32 }
}

fn region_extreme(spectrum: &[Band], hz: RangeInclusive<f32>, direction: f32) -> Option<&Band> {
    let bands = in_region(spectrum, hz);
    if direction >= 0.0 {
        bands.max_by(|a, b| a.excess_db().total_cmp(&b.excess_db()))
    } else {
        bands.min_by(|a, b| a.excess_db().total_cmp(&b.excess_db()))
    }
}
