use std::ops::RangeInclusive;

use crate::biquad::{Biquad, Coefficients};

use super::Take;
use super::frames;
use super::profile::Band;

pub const CENTRES_HZ: [f32; 24] = [
    63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1_000.0,
    1_250.0, 1_600.0, 2_000.0, 2_500.0, 3_150.0, 4_000.0, 5_000.0, 6_300.0, 8_000.0, 10_000.0,
    12_500.0,
];
const REFERENCE_DB: [f32; 24] = [
    38.6, 43.5, 54.4, 57.7, 56.8, 60.2, 60.3, 59.0, 62.1, 62.1, 60.5, 56.8, 53.7, 53.0, 52.0, 48.7,
    48.1, 46.8, 45.6, 44.6, 44.3, 43.7, 43.4, 41.3,
];
const THIRD_OCTAVE_Q: f32 = 4.318;
const ANCHOR_HZ: RangeInclusive<f32> = 400.0..=2_000.0;
const NYQUIST_GUARD: f32 = 0.4;
const FLOOR: f64 = 1e-12;

pub fn measure(take: &Take, active: &[bool]) -> Vec<Band> {
    let rate = take.sample_rate as f32;
    let usable: Vec<usize> = (0..CENTRES_HZ.len())
        .filter(|&index| CENTRES_HZ[index] < rate * NYQUIST_GUARD)
        .collect();
    let mut filters: Vec<Biquad> = usable
        .iter()
        .map(|&index| {
            Biquad::new(Coefficients::band_pass(
                rate,
                CENTRES_HZ[index],
                THIRD_OCTAVE_Q,
            ))
        })
        .collect();
    let mut energy = vec![0.0f64; usable.len()];
    let mut count = 0usize;

    let span = frames::span(take);
    for &sample in &take.samples[..span.start] {
        for filter in &mut filters {
            filter.process(sample);
        }
    }
    let len = frames::frame_len(take.sample_rate);
    for (frame, &is_active) in take.samples[span].chunks_exact(len).zip(active) {
        for &sample in frame {
            for (filter, total) in filters.iter_mut().zip(&mut energy) {
                let out = filter.process(sample);
                if is_active {
                    *total += f64::from(out * out);
                }
            }
        }
        if is_active {
            count += frame.len();
        }
    }
    if count == 0 {
        return Vec::new();
    }

    let bands = usable
        .iter()
        .zip(&energy)
        .map(|(&index, &total)| Band {
            frequency_hz: CENTRES_HZ[index],
            level_db: (10.0 * (total / count as f64 + FLOOR).log10()) as f32,
            reference_db: REFERENCE_DB[index],
        })
        .collect();
    anchored(bands)
}

fn anchored(mut bands: Vec<Band>) -> Vec<Band> {
    let (level_offset, reference_offset) = {
        let anchor: Vec<&Band> = bands
            .iter()
            .filter(|band| ANCHOR_HZ.contains(&band.frequency_hz))
            .collect();
        if anchor.is_empty() {
            return bands;
        }
        let count = anchor.len() as f32;
        (
            anchor.iter().map(|band| band.level_db).sum::<f32>() / count,
            anchor.iter().map(|band| band.reference_db).sum::<f32>() / count,
        )
    };
    for band in &mut bands {
        band.level_db -= level_offset;
        band.reference_db -= reference_offset;
    }
    bands
}
