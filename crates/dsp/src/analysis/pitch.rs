use std::f32::consts::FRAC_1_SQRT_2;

use crate::biquad::{Biquad, Coefficients};

use super::{Take, frames};

const TARGET_RATE: f32 = 8_000.0;
const LOW_PASS_HZ: f32 = 1_000.0;
const WINDOW_MS: f32 = 40.0;
const MIN_HZ: f32 = 60.0;
const MAX_HZ: f32 = 400.0;
const MIN_CLARITY: f32 = 0.75;
const OCTAVE_TOLERANCE: f32 = 0.9;
const TINY: f64 = 1e-12;

pub fn track(take: &Take, active: &[bool]) -> Vec<f32> {
    let factor = ((take.sample_rate as f32 / TARGET_RATE).round() as usize).max(1);
    let rate = take.sample_rate as f32 / factor as f32;
    let signal = decimate(take, factor);
    let hop = frames::frame_len(take.sample_rate) / factor;
    let window = (rate * WINDOW_MS / 1000.0).round() as usize;
    let min_lag = (rate / MAX_HZ).round() as usize;
    let max_lag = (rate / MIN_HZ).round() as usize;
    let offset = frames::span(take).start / factor;

    let mut voiced = Vec::new();
    for (index, pair) in active.windows(2).enumerate() {
        if !(pair[0] && pair[1]) {
            continue;
        }
        let start = offset + index * hop;
        let Some(segment) = signal.get(start..start + window + max_lag) else {
            break;
        };
        if let Some(lag) = period(segment, window, min_lag, max_lag) {
            voiced.push(rate / lag);
        }
    }
    voiced
}

fn decimate(take: &Take, factor: usize) -> Vec<f32> {
    let coefficients = Coefficients::low_pass(take.sample_rate as f32, LOW_PASS_HZ, FRAC_1_SQRT_2);
    let mut filters = [Biquad::new(coefficients); 2];
    let mut decimated = Vec::with_capacity(take.samples.len() / factor + 1);
    for (index, &sample) in take.samples.iter().enumerate() {
        let filtered = filters
            .iter_mut()
            .fold(sample, |signal, filter| filter.process(signal));
        if index % factor == 0 {
            decimated.push(filtered);
        }
    }
    decimated
}

fn period(segment: &[f32], window: usize, min_lag: usize, max_lag: usize) -> Option<f32> {
    let prefix = prefix_energy(segment);
    let energy = |from: usize| prefix[from + window] - prefix[from];
    let base = energy(0);
    if base <= TINY {
        return None;
    }
    let reference = &segment[..window];
    let scores: Vec<f32> = (min_lag..=max_lag)
        .map(|lag| {
            let dot: f32 = reference
                .iter()
                .zip(&segment[lag..lag + window])
                .map(|(a, b)| a * b)
                .sum();
            (f64::from(dot) / (base * energy(lag)).sqrt().max(TINY)) as f32
        })
        .collect();
    let best = scores.iter().copied().fold(f32::MIN, f32::max);
    if best < MIN_CLARITY {
        return None;
    }
    let floor = best * OCTAVE_TOLERANCE;
    (0..scores.len())
        .find(|&index| scores[index] >= floor && is_peak(&scores, index))
        .map(|index| (min_lag + index) as f32)
}

fn prefix_energy(segment: &[f32]) -> Vec<f64> {
    let mut prefix = Vec::with_capacity(segment.len() + 1);
    let mut total = 0.0f64;
    prefix.push(total);
    for &sample in segment {
        total += f64::from(sample) * f64::from(sample);
        prefix.push(total);
    }
    prefix
}

fn is_peak(scores: &[f32], index: usize) -> bool {
    let left = index
        .checked_sub(1)
        .is_none_or(|previous| scores[previous] <= scores[index]);
    let right = scores
        .get(index + 1)
        .is_none_or(|&next| next <= scores[index]);
    left && right
}
