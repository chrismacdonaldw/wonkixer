use super::{LEVEL_SMOOTH_MS, Stage};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;
use crate::units::db_to_gain;

pub struct Gain {
    id: ParamId,
    gain: Smoother,
}

impl Gain {
    pub fn new(id: ParamId, sample_rate: f32) -> Self {
        Self {
            id,
            gain: Smoother::new(db_to_gain(id.spec().default), LEVEL_SMOOTH_MS, sample_rate),
        }
    }
}

impl Stage for Gain {
    fn apply(&mut self, values: &ParamValues) {
        self.gain.set_target(db_to_gain(values.get(self.id)));
    }

    fn process(&mut self, sample: f32) -> f32 {
        sample * self.gain.advance()
    }

    fn reset(&mut self) {}
}
