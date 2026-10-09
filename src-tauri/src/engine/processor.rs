use std::sync::Arc;

use wonkixer_dsp::{Chain, GainReduction, ParamValues, Params};

pub struct Processor {
    params: Arc<Params>,
    values: ParamValues,
    seen_generation: u32,
    chain: Chain,
}

impl Processor {
    pub fn new(params: Arc<Params>, sample_rate: u32) -> Self {
        let values = params.snapshot();
        let mut chain = Chain::new(sample_rate);
        chain.apply(&values);
        Self {
            seen_generation: params.generation(),
            params,
            values,
            chain,
        }
    }

    pub fn latency_samples(&self) -> usize {
        self.chain.latency_samples()
    }

    pub fn process(&mut self, block: &mut [f32]) -> GainReduction {
        if self
            .params
            .load_if_changed(&mut self.seen_generation, &mut self.values)
        {
            self.chain.apply(&self.values);
        }
        self.chain.process_block(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonkixer_dsp::{ParamId, StageId};

    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;

    fn peak(block: &[f32]) -> f32 {
        block
            .iter()
            .fold(0.0f32, |peak, &sample| peak.max(sample.abs()))
    }

    fn tone() -> Vec<f32> {
        (0..BLOCK)
            .map(|index| 0.5 * (index as f32 * 0.1).sin())
            .collect()
    }

    #[test]
    fn picks_up_parameter_changes_between_blocks() {
        let mut flat = ParamValues::default();
        for stage in StageId::ALL {
            flat.set_bypassed(stage, true);
        }
        let params = Arc::new(Params::new(&flat));
        let mut processor = Processor::new(Arc::clone(&params), RATE);

        let mut block = tone();
        for _ in 0..10 {
            processor.process(&mut block);
            block = tone();
        }
        processor.process(&mut block);
        assert!((peak(&block) - 0.5).abs() < 0.01);

        params.set(ParamId::InputGain, -24.0);
        for _ in 0..10 {
            processor.process(&mut block);
            block = tone();
        }
        processor.process(&mut block);
        assert!(
            (peak(&block) - 0.5 * 0.0631).abs() < 0.005,
            "{}",
            peak(&block)
        );
    }

    #[test]
    fn reports_the_limiter_lookahead_as_latency() {
        let params = Arc::new(Params::new(&ParamValues::default()));
        let processor = Processor::new(params, RATE);
        assert_eq!(processor.latency_samples(), 240);
    }
}
