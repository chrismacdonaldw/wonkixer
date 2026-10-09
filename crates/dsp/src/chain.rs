use crate::params::{ParamId, ParamValues, StageId};
use crate::stages::{
    Bypassable, Compressor, DeEsser, Equalizer, Expander, Gain, HighPass, Limiter, Stage,
};

const ANTI_DENORMAL: f32 = 1e-18;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GainReduction {
    pub expander: f32,
    pub deesser: f32,
    pub compressor: f32,
    pub limiter: f32,
}

impl GainReduction {
    pub const NONE: Self = Self {
        expander: 0.0,
        deesser: 0.0,
        compressor: 0.0,
        limiter: 0.0,
    };

    pub fn max(self, other: Self) -> Self {
        Self {
            expander: self.expander.max(other.expander),
            deesser: self.deesser.max(other.deesser),
            compressor: self.compressor.max(other.compressor),
            limiter: self.limiter.max(other.limiter),
        }
    }
}

pub struct Chain {
    input: Gain,
    high_pass: Bypassable<HighPass>,
    expander: Bypassable<Expander>,
    deesser: Bypassable<DeEsser>,
    equalizer: Bypassable<Equalizer>,
    compressor: Bypassable<Compressor>,
    limiter: Bypassable<Limiter>,
    output: Gain,
    dither: f32,
}

impl Chain {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        Self {
            input: Gain::new(ParamId::InputGain, rate),
            high_pass: Bypassable::new(HighPass::new(rate), rate),
            expander: Bypassable::new(Expander::new(rate), rate),
            deesser: Bypassable::new(DeEsser::new(rate), rate),
            equalizer: Bypassable::new(Equalizer::new(rate), rate),
            compressor: Bypassable::new(Compressor::new(rate), rate),
            limiter: Bypassable::new(Limiter::new(rate), rate),
            output: Gain::new(ParamId::OutputGain, rate),
            dither: ANTI_DENORMAL,
        }
    }

    pub fn apply(&mut self, values: &ParamValues) {
        self.input.apply(values);
        self.output.apply(values);
        self.high_pass.apply(values);
        self.high_pass
            .set_bypassed(values.is_bypassed(StageId::HighPass));
        self.expander.apply(values);
        self.expander
            .set_bypassed(values.is_bypassed(StageId::Expander));
        self.deesser.apply(values);
        self.deesser
            .set_bypassed(values.is_bypassed(StageId::DeEsser));
        self.equalizer.apply(values);
        self.equalizer
            .set_bypassed(values.is_bypassed(StageId::Equalizer));
        self.compressor.apply(values);
        self.compressor
            .set_bypassed(values.is_bypassed(StageId::Compressor));
        self.limiter.apply(values);
        self.limiter
            .set_bypassed(values.is_bypassed(StageId::Limiter));
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        self.dither = -self.dither;
        let mut signal = self.input.process(sample + self.dither);
        signal = self.high_pass.process(signal);
        signal = self.expander.process(signal);
        signal = self.deesser.process(signal);
        signal = self.equalizer.process(signal);
        signal = self.compressor.process(signal);
        signal = self.limiter.process(signal);
        signal = self.output.process(signal);
        signal.clamp(-1.0, 1.0)
    }

    pub fn process_block(&mut self, block: &mut [f32]) -> GainReduction {
        let mut peak = GainReduction::NONE;
        for sample in block {
            *sample = self.process(*sample);
            peak = peak.max(self.reduction());
        }
        peak
    }

    pub fn reduction(&self) -> GainReduction {
        GainReduction {
            expander: self.expander.reduction_db(),
            deesser: self.deesser.reduction_db(),
            compressor: self.compressor.reduction_db(),
            limiter: self.limiter.reduction_db(),
        }
    }

    pub fn latency_samples(&self) -> usize {
        self.limiter.inner().latency_samples()
    }

    pub fn reset(&mut self) {
        self.high_pass.reset();
        self.expander.reset();
        self.deesser.reset();
        self.equalizer.reset();
        self.compressor.reset();
        self.limiter.reset();
    }
}
