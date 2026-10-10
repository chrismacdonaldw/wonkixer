use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::resampler::RateConverter;

const CAPACITY_SECONDS: u32 = 1;
const MARGIN_MS: u32 = 2;
const DEADBAND_MS: u32 = 1;
const SMOOTHING: f32 = 1.0 / 16.0;

pub fn ring(sample_rate: u32) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new((sample_rate * CAPACITY_SECONDS) as usize).split()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overflow;

pub struct Inlet {
    producer: HeapProd<f32>,
    converter: Option<RateConverter>,
    pushed: u64,
}

impl Inlet {
    pub fn new(producer: HeapProd<f32>, input_rate: u32, chain_rate: u32) -> Result<Self, String> {
        let converter = (input_rate != chain_rate)
            .then(|| RateConverter::new(input_rate, chain_rate))
            .transpose()?;
        Ok(Self {
            producer,
            converter,
            pushed: 0,
        })
    }

    pub fn pushed(&self) -> u64 {
        self.pushed
    }

    pub fn latency_frames(&self) -> usize {
        self.converter
            .as_ref()
            .map_or(0, RateConverter::latency_frames)
    }

    pub fn chain_frames(&self, input_frames: usize) -> usize {
        self.converter.as_ref().map_or(input_frames, |converter| {
            converter.output_frames(input_frames)
        })
    }

    pub fn push(&mut self, sample: f32) -> Result<(), Overflow> {
        let Some(converter) = &mut self.converter else {
            self.producer.try_push(sample).map_err(|_| Overflow)?;
            self.pushed += 1;
            return Ok(());
        };
        let Some(converted) = converter.push(sample) else {
            return Ok(());
        };
        let accepted = self.producer.push_slice(converted);
        self.pushed += accepted as u64;
        if accepted < converted.len() {
            return Err(Overflow);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    Priming,
    Ready,
    Underrun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub lag: usize,
    pub packet: usize,
}

pub struct Outlet {
    consumer: HeapCons<f32>,
    period: usize,
    margin: usize,
    deadband: usize,
    lag: f32,
    primed: bool,
    consumed: u64,
}

impl Outlet {
    pub fn new(consumer: HeapCons<f32>, sample_rate: u32, period: usize) -> Self {
        Self {
            consumer,
            period,
            margin: (sample_rate * MARGIN_MS / 1000) as usize,
            deadband: (sample_rate * DEADBAND_MS / 1000) as usize,
            lag: 0.0,
            primed: false,
            consumed: 0,
        }
    }

    pub fn fill(&self) -> usize {
        self.consumer.occupied_len()
    }

    pub fn position(&self) -> u64 {
        self.consumed
    }

    pub fn target(&self, block: usize, packet: usize) -> usize {
        packet + block.max(self.period) + self.margin
    }

    pub fn pull(&mut self, block: &mut [f32], timing: Option<Timing>) -> Pull {
        let mut fill = self.fill();
        let Timing { mut lag, packet } = self.trusted(timing).unwrap_or(Timing {
            lag: fill,
            packet: self.period,
        });
        let target = self.target(block.len(), packet);

        if !self.primed {
            if fill < block.len() || lag < target {
                block.fill(0.0);
                return Pull::Priming;
            }
            let backlog = self.consumer.skip((lag - target).min(fill - block.len()));
            self.consumed += backlog as u64;
            fill -= backlog;
            lag -= backlog;
            self.lag = lag as f32;
            self.primed = true;
        }

        if fill < block.len() {
            block.fill(0.0);
            self.primed = false;
            return Pull::Underrun;
        }

        self.lag += (lag as f32 - self.lag) * SMOOTHING;
        let taken = if self.lag > (target + self.deadband) as f32 && fill > block.len() {
            self.lag -= 1.0;
            self.consumer.skip(1) + self.consumer.pop_slice(block)
        } else if self.lag < target as f32 && block.len() > 1 {
            self.lag += 1.0;
            let last = block.len() - 1;
            let taken = self.consumer.pop_slice(&mut block[..last]);
            block[last] = block[last - 1];
            taken
        } else {
            self.consumer.pop_slice(block)
        };
        self.consumed += taken as u64;
        Pull::Ready
    }

    fn trusted(&self, timing: Option<Timing>) -> Option<Timing> {
        timing.filter(|timing| timing.lag <= self.consumer.capacity().get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const PERIOD: usize = 480;
    const MARGIN: usize = 96;
    const DEADBAND: usize = 48;
    const TARGET: usize = PERIOD + PERIOD + MARGIN;

    fn timing(lag: usize) -> Option<Timing> {
        Some(Timing {
            lag,
            packet: PERIOD,
        })
    }

    fn outlet_with(fill: usize) -> (HeapProd<f32>, Outlet) {
        let (mut producer, consumer) = ring(RATE);
        let samples: Vec<f32> = (0..fill).map(|index| index as f32).collect();
        producer.push_slice(&samples);
        (producer, Outlet::new(consumer, RATE, PERIOD))
    }

    fn running_outlet_with(fill: usize, lag: usize) -> (HeapProd<f32>, Outlet) {
        let (producer, mut outlet) = outlet_with(fill);
        outlet.primed = true;
        outlet.lag = lag as f32;
        (producer, outlet)
    }

    #[test]
    fn waits_until_the_oldest_frame_is_one_packet_and_one_block_old() {
        let (_producer, mut outlet) = outlet_with(PERIOD);
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(TARGET - 1)), Pull::Priming);
        assert!(block.iter().all(|&sample| sample == 0.0));
        assert_eq!(outlet.position(), 0);

        assert_eq!(outlet.pull(&mut block, timing(TARGET)), Pull::Ready);
        assert_eq!(block[PERIOD - 1], (PERIOD - 1) as f32);
        assert_eq!(outlet.position(), PERIOD as u64);
    }

    #[test]
    fn starts_at_the_target_by_dropping_what_built_up_while_priming() {
        let (_producer, mut outlet) = outlet_with(3 * PERIOD);
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(TARGET + 200)), Pull::Ready);
        assert_eq!(block[0], 200.0);
        assert_eq!(outlet.position(), (200 + PERIOD) as u64);
    }

    #[test]
    fn keeps_a_whole_block_when_dropping_the_backlog() {
        let (_producer, mut outlet) = outlet_with(PERIOD + 10);
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(TARGET + 200)), Pull::Ready);
        assert_eq!(block[0], 10.0);
        assert_eq!(outlet.fill(), 0);
    }

    #[test]
    fn cannot_start_before_a_whole_block_is_buffered() {
        let (_producer, mut outlet) = outlet_with(PERIOD - 1);
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(2 * TARGET)), Pull::Priming);
    }

    #[test]
    fn an_oversized_first_block_raises_the_target_for_that_block_only() {
        let (_producer, mut outlet) = outlet_with(2 * PERIOD);
        let mut first = [1.0; 2 * PERIOD];
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut first, timing(TARGET)), Pull::Priming);
        assert_eq!(outlet.target(2 * PERIOD, PERIOD), 3 * PERIOD + MARGIN);
        assert_eq!(outlet.pull(&mut block, timing(TARGET)), Pull::Ready);
    }

    #[test]
    fn a_short_block_still_leaves_room_for_a_whole_period() {
        let (_producer, outlet) = outlet_with(0);

        assert_eq!(outlet.target(128, PERIOD), TARGET);
        assert_eq!(outlet.target(128, 128), 128 + PERIOD + MARGIN);
    }

    #[test]
    fn without_timing_the_ring_level_stands_in_for_the_lag() {
        let (mut producer, mut outlet) = outlet_with(TARGET - 1);
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, None), Pull::Priming);
        producer.push_slice(&[0.5]);
        assert_eq!(outlet.pull(&mut block, None), Pull::Ready);
    }

    #[test]
    fn timing_longer_than_the_ring_is_not_trusted() {
        let (_producer, mut outlet) = outlet_with(PERIOD);
        let mut block = [1.0; PERIOD];

        assert_eq!(
            outlet.pull(&mut block, timing(RATE as usize + 1)),
            Pull::Priming
        );
    }

    #[test]
    fn reports_an_underrun_and_reprimes_when_starved() {
        let (_producer, mut outlet) = running_outlet_with(PERIOD - 1, TARGET);
        let mut block = [1.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(TARGET)), Pull::Underrun);
        assert!(block.iter().all(|&sample| sample == 0.0));
        assert_eq!(outlet.pull(&mut block, timing(TARGET)), Pull::Priming);
    }

    #[test]
    fn repeats_a_sample_when_the_lag_falls_below_the_target() {
        let (_producer, mut outlet) = running_outlet_with(2 * PERIOD, TARGET - 1);
        let mut block = [0.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(TARGET - 1)), Pull::Ready);
        assert_eq!(outlet.position(), (PERIOD - 1) as u64);
        assert_eq!(block[PERIOD - 1], block[PERIOD - 2]);
    }

    #[test]
    fn drops_a_sample_when_the_lag_rises_past_the_deadband() {
        let lag = TARGET + DEADBAND + 1;
        let (_producer, mut outlet) = running_outlet_with(2 * PERIOD, lag);
        let mut block = [0.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(lag)), Pull::Ready);
        assert_eq!(outlet.position(), (PERIOD + 1) as u64);
        assert_eq!(block[0], 1.0);
    }

    #[test]
    fn never_drops_the_last_buffered_sample() {
        let lag = TARGET + DEADBAND + 1;
        let (_producer, mut outlet) = running_outlet_with(PERIOD, lag);
        let mut block = [0.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(lag)), Pull::Ready);
        assert_eq!(outlet.position(), PERIOD as u64);
        assert_eq!(block[0], 0.0);
    }

    #[test]
    fn passes_samples_through_inside_the_band() {
        let lag = TARGET + DEADBAND;
        let (_producer, mut outlet) = running_outlet_with(2 * PERIOD, lag);
        let mut block = [0.0; PERIOD];

        assert_eq!(outlet.pull(&mut block, timing(lag)), Pull::Ready);
        assert_eq!(outlet.fill(), PERIOD);
        assert_eq!(block[PERIOD - 1], (PERIOD - 1) as f32);
    }

    #[test]
    fn smooths_out_a_single_late_timestamp() {
        let (_producer, mut outlet) = running_outlet_with(2 * PERIOD, TARGET);
        let mut block = [0.0; PERIOD];

        assert_eq!(
            outlet.pull(&mut block, timing(TARGET + 10 * DEADBAND)),
            Pull::Ready
        );
        assert_eq!(outlet.position(), PERIOD as u64);
    }

    #[test]
    fn an_inlet_at_the_chain_rate_passes_samples_straight_through() {
        let (producer, consumer) = ring(RATE);
        let mut inlet = Inlet::new(producer, RATE, RATE).expect("inlet");
        for index in 0..PERIOD {
            inlet.push(index as f32).expect("room");
        }
        assert_eq!(inlet.latency_frames(), 0);
        assert_eq!(inlet.chain_frames(PERIOD), PERIOD);
        assert_eq!(inlet.pushed(), PERIOD as u64);
        assert_eq!(consumer.occupied_len(), PERIOD);
    }

    #[test]
    fn an_inlet_below_the_chain_rate_converts_into_the_ring() {
        let (producer, consumer) = ring(RATE);
        let mut inlet = Inlet::new(producer, 44_100, RATE).expect("inlet");
        for _ in 0..44_100 {
            inlet.push(0.0).expect("room");
        }
        assert!(inlet.latency_frames() > 0);
        assert_eq!(inlet.chain_frames(441), 480);
        assert_eq!(inlet.pushed(), consumer.occupied_len() as u64);
        assert!(consumer.occupied_len().abs_diff(RATE as usize) <= inlet.latency_frames());
    }

    #[test]
    fn an_inlet_reports_overflow_when_the_ring_is_full() {
        let (producer, _consumer) = ring(RATE);
        let mut inlet = Inlet::new(producer, RATE, RATE).expect("inlet");
        for _ in 0..RATE {
            inlet.push(0.0).expect("room");
        }
        assert_eq!(inlet.push(0.0), Err(Overflow));
        assert_eq!(inlet.pushed(), u64::from(RATE));
    }
}
