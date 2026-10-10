use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, Resampler, SincInterpolationParameters, WindowFunction};

const CHUNK_FRAMES: usize = 64;
const SINC_LEN: usize = 64;
const FIXED_RATIO: f64 = 1.0;
const MONO: usize = 1;

pub struct RateConverter {
    resampler: Async<f32>,
    chunk: Vec<f32>,
    filled: usize,
    converted: Vec<f32>,
}

impl RateConverter {
    pub fn new(from_rate: u32, to_rate: u32) -> Result<Self, String> {
        let ratio = f64::from(to_rate) / f64::from(from_rate);
        let filter = SincInterpolationParameters::new(SINC_LEN, WindowFunction::BlackmanHarris2);
        let resampler = Async::<f32>::new_sinc(
            ratio,
            FIXED_RATIO,
            &filter,
            CHUNK_FRAMES,
            MONO,
            FixedAsync::Input,
        )
        .map_err(|error| {
            format!("Could not convert the input from {from_rate} Hz to {to_rate} Hz: {error}")
        })?;
        let converted = vec![0.0; resampler.output_frames_max()];
        Ok(Self {
            resampler,
            chunk: vec![0.0; CHUNK_FRAMES],
            filled: 0,
            converted,
        })
    }

    pub fn latency_frames(&self) -> usize {
        self.resampler.output_delay() + self.output_frames(CHUNK_FRAMES)
    }

    pub fn output_frames(&self, input_frames: usize) -> usize {
        (input_frames as f64 * self.resampler.resample_ratio()).round() as usize
    }

    pub fn push(&mut self, sample: f32) -> Option<&[f32]> {
        self.chunk[self.filled] = sample;
        self.filled += 1;
        if self.filled < CHUNK_FRAMES {
            return None;
        }
        self.filled = 0;
        let produced = self.convert_chunk();
        Some(&self.converted[..produced])
    }

    fn convert_chunk(&mut self) -> usize {
        let capacity = self.converted.len();
        let (Ok(input), Ok(mut output)) = (
            InterleavedSlice::new(&self.chunk[..], MONO, CHUNK_FRAMES),
            InterleavedSlice::new_mut(&mut self.converted[..], MONO, capacity),
        ) else {
            return 0;
        };
        self.resampler
            .process_into_buffer(&input, &mut output, None)
            .map_or(0, |(_, produced)| produced)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use assert_no_alloc::assert_no_alloc;

    use super::*;

    const SOURCE_RATE: u32 = 44_100;
    const TARGET_RATE: u32 = 48_000;
    const TONE_HZ: f32 = 1_000.0;
    const AMPLITUDE: f32 = 0.5;

    fn tone(rate: u32, seconds: u32) -> Vec<f32> {
        (0..rate * seconds)
            .map(|index| AMPLITUDE * (TAU * TONE_HZ * index as f32 / rate as f32).sin())
            .collect()
    }

    fn convert(converter: &mut RateConverter, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(input.len() * 2);
        for &sample in input {
            if let Some(converted) = converter.push(sample) {
                output.extend_from_slice(converted);
            }
        }
        output
    }

    fn rising_crossings(signal: &[f32]) -> Vec<f32> {
        signal
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0)
            .map(|(index, pair)| index as f32 + pair[0] / (pair[0] - pair[1]))
            .collect()
    }

    fn rms(signal: &[f32]) -> f32 {
        (signal.iter().map(|sample| sample * sample).sum::<f32>() / signal.len() as f32).sqrt()
    }

    #[test]
    fn converts_a_44_1_khz_tone_to_48_khz_keeping_pitch_and_level() {
        let mut converter = RateConverter::new(SOURCE_RATE, TARGET_RATE).expect("converter");
        let output = convert(&mut converter, &tone(SOURCE_RATE, 2));

        let expected_len = 2 * TARGET_RATE as usize;
        assert!(
            output.len().abs_diff(expected_len) <= converter.latency_frames(),
            "{} frames",
            output.len()
        );

        let settled = &output[TARGET_RATE as usize / 10..output.len() - TARGET_RATE as usize / 10];
        let crossings = rising_crossings(settled);
        let cycles = (crossings.len() - 1) as f32;
        let span = crossings[crossings.len() - 1] - crossings[0];
        let frequency = cycles * TARGET_RATE as f32 / span;
        assert!((frequency - TONE_HZ).abs() < 0.5, "{frequency} Hz");

        let level_db = 20.0 * (rms(settled) / (AMPLITUDE / 2.0f32.sqrt())).log10();
        assert!(level_db.abs() < 0.1, "{level_db} dB");
    }

    #[test]
    fn reports_its_filter_delay_and_chunk_as_latency() {
        let converter = RateConverter::new(SOURCE_RATE, TARGET_RATE).expect("converter");
        let latency = converter.latency_frames();
        assert!(latency > CHUNK_FRAMES, "{latency}");
        assert!(latency < TARGET_RATE as usize * 3 / 1000, "{latency}");
    }

    #[test]
    fn converting_never_allocates_after_setup() {
        let mut converter = RateConverter::new(SOURCE_RATE, TARGET_RATE).expect("converter");
        let input = tone(SOURCE_RATE, 1);
        std::thread::spawn(move || {
            let produced = assert_no_alloc(|| {
                let mut produced = 0;
                for &sample in &input {
                    if let Some(converted) = converter.push(sample) {
                        produced += converted.len();
                    }
                }
                produced
            });
            assert!(produced > 0);
        })
        .join()
        .expect("join");
    }
}
