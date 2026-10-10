use std::f32::consts::FRAC_1_SQRT_2;

use crate::biquad::{Biquad, Coefficients};
use crate::stages::deesser;
use crate::units::MIN_DB;

use super::Take;
use super::frames::{self, percentile};
use super::profile::Sibilance;

const SWEEP_START_HZ: f32 = 3_000.0;
const SWEEP_STEPS: usize = 11;
const SWEEP_OCTAVES_PER_STEP: f32 = 1.0 / 6.0;
const NYQUIST_GUARD: f32 = 0.45;
const HIGH_BAND_HZ: f32 = 4_000.0;
const LOW_BAND_HZ: f32 = 3_000.0;
const MIN_SIBILANT_FRAMES: usize = 8;
const SIBILANT_PERCENTILE: f32 = 0.5;
const VOWEL_PERCENTILE: f32 = 0.95;

pub fn measure(take: &Take, active: &[bool]) -> Option<Sibilance> {
    let rate = take.sample_rate as f32;
    let high = frames::mapped_peaks_db(
        take,
        cascade(Coefficients::high_pass(rate, HIGH_BAND_HZ, FRAC_1_SQRT_2)),
    );
    let low = frames::mapped_peaks_db(
        take,
        cascade(Coefficients::low_pass(rate, LOW_BAND_HZ, FRAC_1_SQRT_2)),
    );
    let sibilant: Vec<bool> = active
        .iter()
        .zip(&high)
        .zip(&low)
        .map(|((&on, &high), &low)| on && high > low)
        .collect();
    if sibilant.iter().filter(|&&flag| flag).count() < MIN_SIBILANT_FRAMES {
        return None;
    }
    let vowel: Vec<bool> = active
        .iter()
        .zip(&sibilant)
        .map(|(&on, &flag)| on && !flag)
        .collect();

    let (frequency_hz, _, band) = candidates(rate)
        .map(|frequency| {
            let band = frames::mapped_peaks_db(take, single(deesser::sidechain(rate, frequency)));
            (frequency, mean(&band, &sibilant), band)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))?;

    let sibilant_levels = frames::sorted(frames::select(&band, &sibilant));
    let vowel_levels = frames::sorted(frames::select(&band, &vowel));
    Some(Sibilance {
        frequency_hz,
        level_db: percentile(&sibilant_levels, SIBILANT_PERCENTILE),
        vowel_level_db: if vowel_levels.is_empty() {
            MIN_DB
        } else {
            percentile(&vowel_levels, VOWEL_PERCENTILE)
        },
    })
}

fn candidates(rate: f32) -> impl Iterator<Item = f32> {
    (0..SWEEP_STEPS)
        .map(|step| SWEEP_START_HZ * 2f32.powf(step as f32 * SWEEP_OCTAVES_PER_STEP))
        .filter(move |&frequency| frequency < rate * NYQUIST_GUARD)
}

fn cascade(coefficients: Coefficients) -> impl FnMut(f32) -> f32 {
    let mut filters = [Biquad::new(coefficients); 2];
    move |sample| {
        filters
            .iter_mut()
            .fold(sample, |signal, filter| filter.process(signal))
    }
}

fn single(coefficients: Coefficients) -> impl FnMut(f32) -> f32 {
    let mut filter = Biquad::new(coefficients);
    move |sample| filter.process(sample)
}

fn mean(values: &[f32], mask: &[bool]) -> f32 {
    let selected = frames::select(values, mask);
    selected.iter().sum::<f32>() / selected.len().max(1) as f32
}
