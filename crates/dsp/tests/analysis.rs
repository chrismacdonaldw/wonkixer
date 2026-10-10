use std::f32::consts::TAU;

use wonkixer_dsp::analysis::{self, AnalysisError, Captures, Take, TakeRole, Tone};
use wonkixer_dsp::biquad::{Biquad, Coefficients};
use wonkixer_dsp::params::SPECS;
use wonkixer_dsp::{ParamId, ParamValues, StageId};

const RATE: u32 = 48_000;
const PITCH_HZ: f32 = 110.0;
const HARMONICS: u32 = 10;
const NOISE_PEAK: f32 = 0.0005;
const SYLLABLE_ON_MS: f32 = 150.0;
const SYLLABLE_OFF_MS: f32 = 100.0;
const RAMP_MS: f32 = 20.0;
const PAUSE_EVERY: usize = 10;
const PAUSE_MS: f32 = 600.0;
const SIBILANT_EVERY: usize = 4;
const SIBILANT_HZ: f32 = 7_000.0;
const SIBILANT_LEVEL: f32 = 0.6;
const LOUDNESS_CYCLE: [f32; 3] = [0.4, 1.0, 0.6];

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

fn samples(ms: f32) -> usize {
    (RATE as f32 * ms / 1000.0).round() as usize
}

fn silence(seconds: f32) -> Take {
    let mut noise = Noise(7);
    Take {
        sample_rate: RATE,
        samples: (0..(RATE as f32 * seconds) as usize)
            .map(|_| noise.next() * NOISE_PEAK)
            .collect(),
    }
}

fn harmonic(phase: f32) -> f32 {
    (1..=HARMONICS)
        .map(|k| (phase * k as f32).sin() / k as f32)
        .sum()
}

fn ramp(index: usize, len: usize) -> f32 {
    let edge = samples(RAMP_MS);
    let distance = index.min(len - 1 - index);
    if distance >= edge {
        1.0
    } else {
        0.5 - 0.5 * (TAU / 2.0 * distance as f32 / edge as f32).cos()
    }
}

fn sibilant_burst(len: usize, peak: f32, noise: &mut Noise) -> Vec<f32> {
    let mut filter = Biquad::new(Coefficients::band_pass(RATE as f32, SIBILANT_HZ, 2.0));
    let raw: Vec<f32> = (0..len).map(|_| filter.process(noise.next())).collect();
    let max = raw.iter().fold(0.0f32, |max, &x| max.max(x.abs()));
    raw.iter().map(|&x| x / max * peak).collect()
}

fn voice(seconds: f32, loudness: f32) -> Take {
    let norm = (0..1000)
        .map(|i| harmonic(TAU * i as f32 / 1000.0).abs())
        .fold(0.0f32, f32::max);
    let mut noise = Noise(11);
    let total = (RATE as f32 * seconds) as usize;
    let mut out = Vec::with_capacity(total);
    let mut phase = 0.0f32;
    let mut syllable = 0usize;
    while out.len() < total {
        if syllable > 0 && syllable.is_multiple_of(PAUSE_EVERY) {
            out.extend((0..samples(PAUSE_MS)).map(|_| noise.next() * NOISE_PEAK));
        }
        let amplitude = loudness * LOUDNESS_CYCLE[syllable % LOUDNESS_CYCLE.len()];
        let len = samples(SYLLABLE_ON_MS);
        if syllable % SIBILANT_EVERY == SIBILANT_EVERY - 1 {
            let burst = sibilant_burst(len, amplitude * SIBILANT_LEVEL, &mut noise);
            out.extend(
                burst
                    .iter()
                    .enumerate()
                    .map(|(i, &s)| s * ramp(i, len) + noise.next() * NOISE_PEAK),
            );
        } else {
            for i in 0..len {
                phase += TAU * PITCH_HZ / RATE as f32;
                let tone = harmonic(phase) / norm * amplitude * ramp(i, len);
                out.push((tone + noise.next() * NOISE_PEAK).clamp(-1.0, 1.0));
            }
        }
        out.extend((0..samples(SYLLABLE_OFF_MS)).map(|_| noise.next() * NOISE_PEAK));
        syllable += 1;
    }
    out.truncate(total);
    Take {
        sample_rate: RATE,
        samples: out,
    }
}

fn captures<'a>(silence: &'a Take, speech: &'a Take, loud: Option<&'a Take>) -> Captures<'a> {
    Captures {
        silence,
        speech,
        loud,
    }
}

#[test]
fn the_profile_matches_the_synthetic_voice() {
    let quiet = silence(3.0);
    let speech = voice(12.0, 0.25);
    let profile = analysis::analyze(captures(&quiet, &speech, None)).expect("profile");

    assert!(
        (-72.0..=-60.0).contains(&profile.noise_floor_db),
        "noise floor {}",
        profile.noise_floor_db
    );
    assert!(
        (-14.0..=-10.0).contains(&profile.speech.p90),
        "p90 {}",
        profile.speech.p90
    );
    assert!(profile.speech.p10 < profile.speech.p50 && profile.speech.p50 < profile.speech.p90);
    assert!(profile.speech_seconds > 5.0, "{}", profile.speech_seconds);
    let pitch = profile.pitch_low_hz.expect("pitch");
    assert!((100.0..=120.0).contains(&pitch), "pitch low {pitch}");
    let median = profile.pitch_median_hz.expect("median pitch");
    assert!((100.0..=120.0).contains(&median), "pitch median {median}");
    let sibilance = profile.sibilance.expect("sibilance");
    assert!(
        (5_500.0..=8_600.0).contains(&sibilance.frequency_hz),
        "sibilance at {}",
        sibilance.frequency_hz
    );
    assert!(sibilance.level_db > sibilance.vowel_level_db);
    assert_eq!(profile.spectrum.len(), 24);
    assert!(!profile.clipped);
    assert!(profile.loud.is_none());
}

#[test]
fn derived_settings_stay_in_range_and_follow_the_profile() {
    let quiet = silence(3.0);
    let speech = voice(12.0, 0.25);
    let profile = analysis::analyze(captures(&quiet, &speech, None)).expect("profile");
    let base = ParamValues::default();
    let calibration = analysis::derive(&profile, &base, Tone::Neutral);
    let values = &calibration.values;

    for spec in SPECS {
        let value = values.get(spec.id);
        assert!(
            (spec.min..=spec.max).contains(&value),
            "{} = {value} outside {}..{}",
            spec.key,
            spec.min,
            spec.max
        );
    }
    let trim = values.get(ParamId::InputGain);
    assert!(
        (trim - (-12.0 - profile.speech.p90)).abs() <= 0.3,
        "trim {trim}"
    );
    let expander = values.get(ParamId::ExpanderThreshold);
    assert!(
        expander > profile.noise_floor_db + trim,
        "expander {expander}"
    );
    assert!(expander < profile.speech.p10 + trim, "expander {expander}");
    assert!(values.is_bypassed(StageId::NoiseSuppressor));
    let cutoff = values.get(ParamId::HighPassCutoff);
    assert!((75.0..=90.0).contains(&cutoff), "cutoff {cutoff}");
    let frequency = values.get(ParamId::DeEsserFrequency);
    assert!(
        (5_500.0..=8_600.0).contains(&frequency),
        "de-esser {frequency}"
    );
    let ratio = values.get(ParamId::CompressorRatio);
    assert!((2.0..=5.0).contains(&ratio), "ratio {ratio}");
    let threshold = values.get(ParamId::CompressorThreshold);
    assert!(
        (-40.0..=-12.0).contains(&threshold),
        "threshold {threshold}"
    );
    assert_eq!(
        values.get(ParamId::CompressorAttack),
        base.get(ParamId::CompressorAttack)
    );
    for stage in [
        StageId::Input,
        StageId::Expander,
        StageId::Compressor,
        StageId::Output,
    ] {
        assert!(
            calibration.notes.iter().any(|note| note.stage == stage),
            "note for {stage:?}"
        );
    }
    for note in &calibration.notes {
        eprintln!("{}: {}", note.stage.key(), note.text);
    }
}

#[test]
fn flat_tone_zeroes_the_eq_and_a_loud_take_lowers_the_output() {
    let quiet = silence(3.0);
    let speech = voice(12.0, 0.25);
    let loud = voice(4.0, 0.95);
    let base = ParamValues::default();

    let without = analysis::analyze(captures(&quiet, &speech, None)).expect("profile");
    let flat = analysis::derive(&without, &base, Tone::Flat);
    for id in [
        ParamId::LowShelfGain,
        ParamId::LowMidGain,
        ParamId::PresenceGain,
        ParamId::HighShelfGain,
    ] {
        assert_eq!(flat.values.get(id), 0.0);
    }

    let with = analysis::analyze(captures(&quiet, &speech, Some(&loud))).expect("profile");
    assert!(with.loud.expect("loud stats").max > without.speech.max);
    let plain = analysis::derive(&without, &base, Tone::Neutral);
    let guarded = analysis::derive(&with, &base, Tone::Neutral);
    assert!(guarded.values.get(ParamId::OutputGain) <= plain.values.get(ParamId::OutputGain));
}

#[test]
fn bad_takes_are_reported_in_plain_words() {
    let quiet = silence(3.0);
    let speech = voice(12.0, 0.25);
    let short = silence(0.5);
    assert_eq!(
        analysis::analyze(captures(&short, &speech, None)),
        Err(AnalysisError::TooShort {
            role: TakeRole::Silence,
            seconds: analysis::MIN_SILENCE_SECONDS
        })
    );

    let mute = silence(6.0);
    assert_eq!(
        analysis::analyze(captures(&quiet, &mute, None)),
        Err(AnalysisError::NoSpeechDetected)
    );

    let talking = voice(3.0, 0.25);
    assert_eq!(
        analysis::analyze(captures(&talking, &speech, None)),
        Err(AnalysisError::SilenceNotQuiet)
    );

    let other_rate = Take {
        sample_rate: 44_100,
        samples: quiet.samples.clone(),
    };
    assert_eq!(
        analysis::analyze(captures(&other_rate, &speech, None)),
        Err(AnalysisError::SampleRateMismatch)
    );
    assert!(
        AnalysisError::NoSpeechDetected
            .to_string()
            .contains("No speech")
    );
}

#[test]
fn clipping_is_detected_and_called_out() {
    let quiet = silence(3.0);
    let speech = voice(12.0, 0.25);
    let hot = voice(4.0, 1.4);
    let profile = analysis::analyze(captures(&quiet, &speech, Some(&hot))).expect("profile");
    assert!(profile.clipped);
    let calibration = analysis::derive(&profile, &ParamValues::default(), Tone::Neutral);
    assert!(
        calibration
            .notes
            .iter()
            .any(|note| note.text.contains("clipped"))
    );
}
