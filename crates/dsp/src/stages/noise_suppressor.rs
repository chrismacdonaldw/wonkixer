use nnnoiseless::DenoiseState;

use super::Stage;
use crate::params::ParamValues;

pub const FRAME: usize = DenoiseState::FRAME_SIZE;
pub const NATIVE_SAMPLE_RATE: f32 = 48_000.0;
const PCM_SCALE: f32 = 32_768.0;
const WARM_UP_LEVEL: f32 = 1_000.0;
const WARM_UP_FRAMES: usize = 3;

pub struct NoiseSuppressor {
    state: Box<DenoiseState<'static>>,
    input: [f32; FRAME],
    output: [f32; FRAME],
    index: usize,
    supported: bool,
}

impl NoiseSuppressor {
    pub fn new(sample_rate: f32) -> Self {
        let mut stage = Self {
            state: DenoiseState::new(),
            input: [0.0; FRAME],
            output: [0.0; FRAME],
            index: 0,
            supported: sample_rate == NATIVE_SAMPLE_RATE,
        };
        stage.warm_up();
        stage
    }

    pub fn warm_up(&mut self) {
        for (index, sample) in self.input.iter_mut().enumerate() {
            *sample = WARM_UP_LEVEL * ((index % 7) as f32 - 3.0);
        }
        for _ in 0..WARM_UP_FRAMES {
            self.state.process_frame(&mut self.output, &self.input);
        }
        self.input.fill(0.0);
        self.output.fill(0.0);
    }

    pub fn latency_samples(&self) -> usize {
        if self.supported { FRAME } else { 0 }
    }
}

impl Stage for NoiseSuppressor {
    fn apply(&mut self, _values: &ParamValues) {}

    fn process(&mut self, sample: f32) -> f32 {
        if !self.supported {
            return sample;
        }
        self.input[self.index] = sample * PCM_SCALE;
        let delayed = self.output[self.index] / PCM_SCALE;
        self.index += 1;
        if self.index == FRAME {
            self.state.process_frame(&mut self.output, &self.input);
            self.index = 0;
        }
        delayed
    }

    fn reset(&mut self) {
        self.input.fill(0.0);
        self.output.fill(0.0);
        self.index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_by_one_frame_at_the_native_rate() {
        let mut stage = NoiseSuppressor::new(NATIVE_SAMPLE_RATE);
        assert_eq!(stage.latency_samples(), FRAME);
        for _ in 0..FRAME {
            assert_eq!(stage.process(0.25), 0.0);
        }
        let mut peak = 0.0f32;
        for _ in 0..FRAME * 4 {
            peak = peak.max(stage.process(0.0).abs());
        }
        assert!(peak.is_finite());
    }

    #[test]
    fn passes_through_untouched_at_other_rates() {
        let mut stage = NoiseSuppressor::new(44_100.0);
        assert_eq!(stage.latency_samples(), 0);
        assert_eq!(stage.process(0.3), 0.3);
    }
}
