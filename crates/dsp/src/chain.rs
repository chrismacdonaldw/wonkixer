use crate::params::{ParamId, ParamValues, StageId};
use crate::stages::{
    Bypassable, Compressor, DeEsser, Equalizer, Expander, Gain, HighPass, Limiter, NoiseSuppressor,
    Stage,
};
use crate::units::MIN_DB;

const ANTI_DENORMAL: f32 = 1e-18;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageTelemetry {
    pub reduction_db: f32,
    pub detector_db: f32,
}

impl StageTelemetry {
    pub const IDLE: Self = Self {
        reduction_db: 0.0,
        detector_db: MIN_DB,
    };

    fn max(self, other: Self) -> Self {
        Self {
            reduction_db: self.reduction_db.max(other.reduction_db),
            detector_db: self.detector_db.max(other.detector_db),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockReport {
    stages: [StageTelemetry; StageId::COUNT],
}

impl BlockReport {
    pub const NONE: Self = Self {
        stages: [StageTelemetry::IDLE; StageId::COUNT],
    };

    pub fn get(&self, stage: StageId) -> StageTelemetry {
        self.stages[stage.index()]
    }

    pub fn with_stage(mut self, stage: StageId, telemetry: StageTelemetry) -> Self {
        self.stages[stage.index()] = telemetry;
        self
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            stages: std::array::from_fn(|index| self.stages[index].max(other.stages[index])),
        }
    }
}

pub struct Chain {
    input: Gain,
    high_pass: Bypassable<HighPass>,
    noise: Bypassable<NoiseSuppressor>,
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
        let mut chain = Self {
            input: Gain::new(ParamId::InputGain, rate),
            high_pass: Bypassable::new(HighPass::new(rate), rate),
            noise: Bypassable::new(NoiseSuppressor::new(rate), rate),
            expander: Bypassable::new(Expander::new(rate), rate),
            deesser: Bypassable::new(DeEsser::new(rate), rate),
            equalizer: Bypassable::new(Equalizer::new(rate), rate),
            compressor: Bypassable::new(Compressor::new(rate), rate),
            limiter: Bypassable::new(Limiter::new(rate), rate),
            output: Gain::new(ParamId::OutputGain, rate),
            dither: ANTI_DENORMAL,
        };
        chain.apply(&ParamValues::default());
        chain
    }

    pub fn apply(&mut self, values: &ParamValues) {
        self.input.apply(values);
        self.output.apply(values);
        self.high_pass.apply(values);
        self.high_pass
            .set_bypassed(values.is_bypassed(StageId::HighPass));
        self.noise.apply(values);
        self.noise
            .set_bypassed(values.is_bypassed(StageId::NoiseSuppressor));
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
        signal = self.noise.process(signal);
        signal = self.expander.process(signal);
        signal = self.deesser.process(signal);
        signal = self.equalizer.process(signal);
        signal = self.compressor.process(signal);
        signal = self.limiter.process(signal);
        signal = self.output.process(signal);
        signal.clamp(-1.0, 1.0)
    }

    pub fn process_block(&mut self, block: &mut [f32]) -> BlockReport {
        let mut peak = BlockReport::NONE;
        for sample in block {
            *sample = self.process(*sample);
            peak = peak.max(self.telemetry());
        }
        peak
    }

    pub fn telemetry(&self) -> BlockReport {
        let mut report = BlockReport::NONE;
        report.stages[StageId::Expander.index()] = StageTelemetry {
            reduction_db: self.expander.reduction_db(),
            detector_db: self.expander.detector_db(),
        };
        report.stages[StageId::DeEsser.index()] = StageTelemetry {
            reduction_db: self.deesser.reduction_db(),
            detector_db: self.deesser.detector_db(),
        };
        report.stages[StageId::Compressor.index()] = StageTelemetry {
            reduction_db: self.compressor.reduction_db(),
            detector_db: self.compressor.detector_db(),
        };
        report.stages[StageId::Limiter.index()] = StageTelemetry {
            reduction_db: self.limiter.reduction_db(),
            detector_db: self.limiter.detector_db(),
        };
        report
    }

    pub fn latency_samples(&self) -> usize {
        let noise = if self.noise.is_bypassed() {
            0
        } else {
            self.noise.inner().latency_samples()
        };
        self.limiter.inner().latency_samples() + noise
    }

    pub fn reset(&mut self) {
        self.high_pass.reset();
        self.noise.reset();
        self.expander.reset();
        self.deesser.reset();
        self.equalizer.reset();
        self.compressor.reset();
        self.limiter.reset();
    }
}
