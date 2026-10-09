use super::{LEVEL_SMOOTH_MS, Stage};
use crate::delay::{DelayLine, SlidingMax};
use crate::dynamics::{GainSmoother, limiter_reduction};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;
use crate::units::{MIN_DB, db_to_gain, gain_to_db, ms_to_samples};

pub const LOOKAHEAD_MS: f32 = 5.0;
const ATTACK_DIVISOR: f32 = 5.0;

pub struct Limiter {
    sample_rate: f32,
    ceiling: Smoother,
    delay: DelayLine,
    window: SlidingMax,
    smoother: GainSmoother,
    reduction: f32,
    level_db: f32,
}

impl Limiter {
    pub fn new(sample_rate: f32) -> Self {
        let lookahead = ms_to_samples(LOOKAHEAD_MS, sample_rate);
        Self {
            sample_rate,
            ceiling: Smoother::new(
                ParamId::LimiterCeiling.spec().default,
                LEVEL_SMOOTH_MS,
                sample_rate,
            ),
            delay: DelayLine::new(lookahead),
            window: SlidingMax::new(lookahead + 1),
            smoother: GainSmoother::new(
                LOOKAHEAD_MS / ATTACK_DIVISOR,
                ParamId::LimiterRelease.spec().default,
                sample_rate,
            ),
            reduction: 0.0,
            level_db: MIN_DB,
        }
    }

    pub fn latency_samples(&self) -> usize {
        self.delay.len()
    }
}

impl Stage for Limiter {
    fn apply(&mut self, values: &ParamValues) {
        self.ceiling.set_target(values.get(ParamId::LimiterCeiling));
        self.smoother.set_times(
            LOOKAHEAD_MS / ATTACK_DIVISOR,
            values.get(ParamId::LimiterRelease),
            self.sample_rate,
        );
    }

    fn process(&mut self, sample: f32) -> f32 {
        let peak = self.window.push(sample.abs());
        let delayed = self.delay.push(sample);
        let ceiling_db = self.ceiling.advance();
        self.level_db = gain_to_db(peak);
        let desired = limiter_reduction(self.level_db, ceiling_db);
        self.reduction = self.smoother.follow(desired);
        let ceiling = db_to_gain(ceiling_db);
        (delayed * db_to_gain(-self.reduction)).clamp(-ceiling, ceiling)
    }

    fn reduction_db(&self) -> f32 {
        self.reduction
    }

    fn detector_db(&self) -> f32 {
        self.level_db
    }

    fn reset(&mut self) {
        self.delay.reset();
        self.window.reset();
        self.smoother.reset();
        self.reduction = 0.0;
        self.level_db = MIN_DB;
    }
}
