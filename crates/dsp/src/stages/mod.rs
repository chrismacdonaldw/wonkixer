pub mod compressor;
pub mod deesser;
pub mod equalizer;
pub mod expander;
pub mod gain;
pub mod high_pass;
pub mod limiter;
pub mod noise_suppressor;

pub use compressor::Compressor;
pub use deesser::DeEsser;
pub use equalizer::Equalizer;
pub use expander::Expander;
pub use gain::Gain;
pub use high_pass::HighPass;
pub use limiter::Limiter;
pub use noise_suppressor::NoiseSuppressor;

use crate::params::ParamValues;
use crate::smoother::Smoother;
use crate::units::MIN_DB;

pub(crate) const LEVEL_SMOOTH_MS: f32 = 10.0;
pub(crate) const FREQUENCY_SMOOTH_MS: f32 = 30.0;
const BYPASS_FADE_MS: f32 = 5.0;

pub trait Stage {
    fn apply(&mut self, values: &ParamValues);
    fn process(&mut self, sample: f32) -> f32;
    fn reduction_db(&self) -> f32 {
        0.0
    }
    fn detector_db(&self) -> f32 {
        MIN_DB
    }
    fn reset(&mut self);
}

pub struct Bypassable<S> {
    stage: S,
    wet: Smoother,
}

impl<S: Stage> Bypassable<S> {
    pub fn new(stage: S, sample_rate: f32) -> Self {
        Self {
            stage,
            wet: Smoother::new(1.0, BYPASS_FADE_MS, sample_rate),
        }
    }

    pub fn set_bypassed(&mut self, bypassed: bool) {
        self.wet.set_target(if bypassed { 0.0 } else { 1.0 });
    }

    pub fn is_bypassed(&self) -> bool {
        self.wet.target() == 0.0
    }

    pub fn apply(&mut self, values: &ParamValues) {
        self.stage.apply(values);
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let wet = self.stage.process(sample);
        sample + (wet - sample) * self.wet.advance()
    }

    pub fn reduction_db(&self) -> f32 {
        self.stage.reduction_db() * self.wet.value()
    }

    pub fn detector_db(&self) -> f32 {
        self.stage.detector_db()
    }

    pub fn inner(&self) -> &S {
        &self.stage
    }

    pub fn reset(&mut self) {
        self.stage.reset();
    }
}
