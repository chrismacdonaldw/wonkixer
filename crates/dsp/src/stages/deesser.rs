use std::f32::consts::FRAC_1_SQRT_2;

use super::{FREQUENCY_SMOOTH_MS, LEVEL_SMOOTH_MS, Stage};
use crate::biquad::{Biquad, Coefficients};
use crate::dynamics::{GainSmoother, compressor_reduction};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;
use crate::units::{MIN_DB, db_to_gain, gain_to_db};

const SIDECHAIN_Q: f32 = 1.0;
const SPLIT_RATIO: f32 = 0.75;
const SPLIT_Q: f32 = FRAC_1_SQRT_2;
const KNEE_DB: f32 = 3.0;
const ATTACK_MS: f32 = 0.5;
const RELEASE_MS: f32 = 40.0;

pub fn sidechain(sample_rate: f32, frequency: f32) -> Coefficients {
    Coefficients::band_pass(sample_rate, frequency, SIDECHAIN_Q)
}

pub struct DeEsser {
    sample_rate: f32,
    frequency: Smoother,
    threshold: Smoother,
    ratio: Smoother,
    split_band: bool,
    sidechain: Biquad,
    split: [Biquad; 2],
    smoother: GainSmoother,
    reduction: f32,
    level_db: f32,
}

impl DeEsser {
    pub fn new(sample_rate: f32) -> Self {
        let frequency = ParamId::DeEsserFrequency.spec().default;
        let split = Biquad::new(Self::split_at(sample_rate, frequency));
        Self {
            sample_rate,
            frequency: Smoother::new(frequency, FREQUENCY_SMOOTH_MS, sample_rate),
            threshold: Smoother::new(
                ParamId::DeEsserThreshold.spec().default,
                LEVEL_SMOOTH_MS,
                sample_rate,
            ),
            ratio: Smoother::new(
                ParamId::DeEsserRatio.spec().default,
                LEVEL_SMOOTH_MS,
                sample_rate,
            ),
            split_band: ParamId::DeEsserSplitBand.spec().default >= 0.5,
            sidechain: Biquad::new(sidechain(sample_rate, frequency)),
            split: [split, split],
            smoother: GainSmoother::new(ATTACK_MS, RELEASE_MS, sample_rate),
            reduction: 0.0,
            level_db: MIN_DB,
        }
    }

    fn split_at(sample_rate: f32, frequency: f32) -> Coefficients {
        Coefficients::low_pass(sample_rate, frequency * SPLIT_RATIO, SPLIT_Q)
    }

    fn retune(&mut self, frequency: f32) {
        self.sidechain.set(sidechain(self.sample_rate, frequency));
        let split = Self::split_at(self.sample_rate, frequency);
        for stage in &mut self.split {
            stage.set(split);
        }
    }

    fn low_band(&mut self, sample: f32) -> f32 {
        self.split
            .iter_mut()
            .fold(sample, |signal, stage| stage.process(signal))
    }
}

impl Stage for DeEsser {
    fn apply(&mut self, values: &ParamValues) {
        self.frequency
            .set_target(values.get(ParamId::DeEsserFrequency));
        self.threshold
            .set_target(values.get(ParamId::DeEsserThreshold));
        self.ratio.set_target(values.get(ParamId::DeEsserRatio));
        self.split_band = values.is_on(ParamId::DeEsserSplitBand);
    }

    fn process(&mut self, sample: f32) -> f32 {
        if !self.frequency.is_settled() {
            let frequency = self.frequency.advance();
            self.retune(frequency);
        }
        let detected = self.sidechain.process(sample);
        self.level_db = gain_to_db(detected.abs());
        let desired = compressor_reduction(
            self.level_db,
            self.threshold.advance(),
            self.ratio.advance(),
            KNEE_DB,
        );
        self.reduction = self.smoother.follow(desired);
        let gain = db_to_gain(-self.reduction);
        let low = self.low_band(sample);
        if self.split_band {
            low + (sample - low) * gain
        } else {
            sample * gain
        }
    }

    fn reduction_db(&self) -> f32 {
        self.reduction
    }

    fn detector_db(&self) -> f32 {
        self.level_db
    }

    fn reset(&mut self) {
        self.sidechain.reset();
        for stage in &mut self.split {
            stage.reset();
        }
        self.smoother.reset();
        self.reduction = 0.0;
        self.level_db = MIN_DB;
    }
}
