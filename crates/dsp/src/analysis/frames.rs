use std::ops::Range;

use crate::units::gain_to_db;

use super::Take;

pub const FRAME_MS: f32 = 20.0;
const EDGE_MS: f32 = 250.0;
const ACTIVITY_ABOVE_NOISE_DB: f32 = 12.0;
const ACTIVITY_BELOW_PEAK_DB: f32 = 30.0;

pub fn frame_len(sample_rate: u32) -> usize {
    samples_for(sample_rate, FRAME_MS).max(1)
}

pub fn span(take: &Take) -> Range<usize> {
    let edge = samples_for(take.sample_rate, EDGE_MS);
    let len = take.samples.len();
    if len > 2 * edge {
        edge..len - edge
    } else {
        0..len
    }
}

pub fn seconds(frames: usize) -> f32 {
    frames as f32 * FRAME_MS / 1000.0
}

pub fn peaks_db(take: &Take) -> Vec<f32> {
    mapped_peaks_db(take, |sample| sample)
}

pub fn mapped_peaks_db(take: &Take, mut map: impl FnMut(f32) -> f32) -> Vec<f32> {
    let span = span(take);
    for &sample in &take.samples[..span.start] {
        map(sample);
    }
    let len = frame_len(take.sample_rate);
    let mut peaks = Vec::with_capacity(span.len() / len);
    for frame in take.samples[span].chunks_exact(len) {
        let peak = frame
            .iter()
            .fold(0.0f32, |peak, &sample| peak.max(map(sample).abs()));
        peaks.push(gain_to_db(peak));
    }
    peaks
}

pub fn sorted(mut values: Vec<f32>) -> Vec<f32> {
    values.sort_by(f32::total_cmp);
    values
}

pub fn percentile(sorted: &[f32], fraction: f32) -> f32 {
    let Some(&last) = sorted.last() else {
        return f32::NAN;
    };
    let position = fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f32;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let weight = position - lower as f32;
    let low = sorted[lower];
    let high = sorted.get(upper).copied().unwrap_or(last);
    low + (high - low) * weight
}

pub fn activity_threshold(noise_floor_db: f32, loud_reference_db: f32) -> f32 {
    (noise_floor_db + ACTIVITY_ABOVE_NOISE_DB).max(loud_reference_db - ACTIVITY_BELOW_PEAK_DB)
}

pub fn active_mask(peaks_db: &[f32], threshold_db: f32) -> Vec<bool> {
    peaks_db.iter().map(|&peak| peak > threshold_db).collect()
}

pub fn select(values: &[f32], mask: &[bool]) -> Vec<f32> {
    values
        .iter()
        .zip(mask)
        .filter(|(_, selected)| **selected)
        .map(|(&value, _)| value)
        .collect()
}

fn samples_for(sample_rate: u32, ms: f32) -> usize {
    (sample_rate as f32 * ms / 1000.0).round() as usize
}
