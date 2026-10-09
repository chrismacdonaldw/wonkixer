use super::{LEVEL_SMOOTH_MS, Stage};
use crate::dynamics::{GainSmoother, expander_reduction};
use crate::params::{ParamId, ParamValues};
use crate::smoother::Smoother;
use crate::units::{db_to_gain, gain_to_db, ms_to_samples};

pub struct Expander {
    sample_rate: f32,
    threshold: Smoother,
    ratio: Smoother,
    range: Smoother,
    knee: Smoother,
    smoother: GainSmoother,
    hold_samples: usize,
    hold_remaining: usize,
    reduction: f32,
}

impl Expander {
    pub fn new(sample_rate: f32) -> Self {
        let level = |id: ParamId| Smoother::new(id.spec().default, LEVEL_SMOOTH_MS, sample_rate);
        let hold_samples = ms_to_samples(ParamId::ExpanderHold.spec().default, sample_rate);
        Self {
            sample_rate,
            threshold: level(ParamId::ExpanderThreshold),
            ratio: level(ParamId::ExpanderRatio),
            range: level(ParamId::ExpanderRange),
            knee: level(ParamId::ExpanderKnee),
            smoother: GainSmoother::new(
                ParamId::ExpanderRelease.spec().default,
                ParamId::ExpanderAttack.spec().default,
                sample_rate,
            ),
            hold_samples,
            hold_remaining: hold_samples,
            reduction: 0.0,
        }
    }
}

impl Stage for Expander {
    fn apply(&mut self, values: &ParamValues) {
        self.threshold
            .set_target(values.get(ParamId::ExpanderThreshold));
        self.ratio.set_target(values.get(ParamId::ExpanderRatio));
        self.range.set_target(values.get(ParamId::ExpanderRange));
        self.knee.set_target(values.get(ParamId::ExpanderKnee));
        self.smoother.set_times(
            values.get(ParamId::ExpanderRelease),
            values.get(ParamId::ExpanderAttack),
            self.sample_rate,
        );
        self.hold_samples = ms_to_samples(values.get(ParamId::ExpanderHold), self.sample_rate);
    }

    fn process(&mut self, sample: f32) -> f32 {
        let desired = expander_reduction(
            gain_to_db(sample.abs()),
            self.threshold.advance(),
            self.ratio.advance(),
            self.knee.advance(),
            self.range.advance(),
        );
        let target = if desired > self.reduction {
            if self.hold_remaining > 0 {
                self.hold_remaining -= 1;
                self.reduction
            } else {
                desired
            }
        } else {
            self.hold_remaining = self.hold_samples;
            desired
        };
        self.reduction = self.smoother.follow(target);
        sample * db_to_gain(-self.reduction)
    }

    fn reduction_db(&self) -> f32 {
        self.reduction
    }

    fn reset(&mut self) {
        self.smoother.reset();
        self.reduction = 0.0;
        self.hold_remaining = self.hold_samples;
    }
}
