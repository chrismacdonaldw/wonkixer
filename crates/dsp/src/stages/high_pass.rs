use std::f32::consts::FRAC_1_SQRT_2;

use super::{FREQUENCY_SMOOTH_MS, Stage};
use crate::biquad::{Biquad, Coefficients};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;

const Q: f32 = FRAC_1_SQRT_2;

pub struct HighPass {
    sample_rate: f32,
    cutoff: Smoother,
    filter: Biquad,
}

impl HighPass {
    pub fn new(sample_rate: f32) -> Self {
        let cutoff = ParamId::HighPassCutoff.spec().default;
        Self {
            sample_rate,
            cutoff: Smoother::new(cutoff, FREQUENCY_SMOOTH_MS, sample_rate),
            filter: Biquad::new(Coefficients::high_pass(sample_rate, cutoff, Q)),
        }
    }
}

impl Stage for HighPass {
    fn apply(&mut self, values: &ParamValues) {
        self.cutoff.set_target(values.get(ParamId::HighPassCutoff));
    }

    fn process(&mut self, sample: f32) -> f32 {
        if !self.cutoff.is_settled() {
            let cutoff = self.cutoff.advance();
            self.filter
                .set(Coefficients::high_pass(self.sample_rate, cutoff, Q));
        }
        self.filter.process(sample)
    }

    fn reset(&mut self) {
        self.filter.reset();
    }
}
