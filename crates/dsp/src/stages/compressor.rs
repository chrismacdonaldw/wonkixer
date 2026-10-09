use super::{LEVEL_SMOOTH_MS, Stage};
use crate::dynamics::{GainSmoother, compressor_reduction};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;
use crate::units::{MIN_DB, db_to_gain, gain_to_db};

pub struct Compressor {
    sample_rate: f32,
    threshold: Smoother,
    ratio: Smoother,
    knee: Smoother,
    makeup: Smoother,
    smoother: GainSmoother,
    reduction: f32,
    level_db: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        let level = |id: ParamId| Smoother::new(id.spec().default, LEVEL_SMOOTH_MS, sample_rate);
        Self {
            sample_rate,
            threshold: level(ParamId::CompressorThreshold),
            ratio: level(ParamId::CompressorRatio),
            knee: level(ParamId::CompressorKnee),
            makeup: Smoother::new(
                db_to_gain(ParamId::CompressorMakeup.spec().default),
                LEVEL_SMOOTH_MS,
                sample_rate,
            ),
            smoother: GainSmoother::new(
                ParamId::CompressorAttack.spec().default,
                ParamId::CompressorRelease.spec().default,
                sample_rate,
            ),
            reduction: 0.0,
            level_db: MIN_DB,
        }
    }
}

impl Stage for Compressor {
    fn apply(&mut self, values: &ParamValues) {
        self.threshold
            .set_target(values.get(ParamId::CompressorThreshold));
        self.ratio.set_target(values.get(ParamId::CompressorRatio));
        self.knee.set_target(values.get(ParamId::CompressorKnee));
        self.makeup
            .set_target(db_to_gain(values.get(ParamId::CompressorMakeup)));
        self.smoother.set_times(
            values.get(ParamId::CompressorAttack),
            values.get(ParamId::CompressorRelease),
            self.sample_rate,
        );
    }

    fn process(&mut self, sample: f32) -> f32 {
        self.level_db = gain_to_db(sample.abs());
        let desired = compressor_reduction(
            self.level_db,
            self.threshold.advance(),
            self.ratio.advance(),
            self.knee.advance(),
        );
        self.reduction = self.smoother.follow(desired);
        sample * db_to_gain(-self.reduction) * self.makeup.advance()
    }

    fn reduction_db(&self) -> f32 {
        self.reduction
    }

    fn detector_db(&self) -> f32 {
        self.level_db
    }

    fn reset(&mut self) {
        self.smoother.reset();
        self.reduction = 0.0;
        self.level_db = MIN_DB;
    }
}
