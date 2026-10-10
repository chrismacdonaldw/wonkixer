use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::resampler::RateConverter;

const CAPACITY_SECONDS: u32 = 1;
const MARGIN_MS: u32 = 2;

pub fn ring(sample_rate: u32) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new((sample_rate * CAPACITY_SECONDS) as usize).split()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overflow;

pub struct Inlet {
    producer: HeapProd<f32>,
    converter: Option<RateConverter>,
}

impl Inlet {
    pub fn new(producer: HeapProd<f32>, input_rate: u32, chain_rate: u32) -> Result<Self, String> {
        let converter = (input_rate != chain_rate)
            .then(|| RateConverter::new(input_rate, chain_rate))
            .transpose()?;
        Ok(Self {
            producer,
            converter,
        })
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
            return self.producer.try_push(sample).map_err(|_| Overflow);
        };
        match converter.push(sample) {
            Some(converted) if self.producer.push_slice(converted) < converted.len() => {
                Err(Overflow)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    Priming,
    Ready,
    Underrun,
}

pub struct Outlet {
    consumer: HeapCons<f32>,
    margin: usize,
    block: usize,
    primed: bool,
}

impl Outlet {
    pub fn new(consumer: HeapCons<f32>, sample_rate: u32) -> Self {
        Self {
            consumer,
            margin: (sample_rate * MARGIN_MS / 1000) as usize,
            block: 0,
            primed: false,
        }
    }

    pub fn fill(&self) -> usize {
        self.consumer.occupied_len()
    }

    pub fn pull(&mut self, block: &mut [f32]) -> Pull {
        self.block = self.block.max(block.len());
        let fill = self.fill();

        if !self.primed {
            if fill < self.prime_level() {
                block.fill(0.0);
                return Pull::Priming;
            }
            self.primed = true;
        }

        if fill < block.len() {
            block.fill(0.0);
            self.primed = false;
            return Pull::Underrun;
        }

        if fill > self.high_water() {
            self.consumer.skip(1);
            self.consumer.pop_slice(block);
        } else if fill < self.low_water() && block.len() > 1 {
            let last = block.len() - 1;
            self.consumer.pop_slice(&mut block[..last]);
            block[last] = block[last - 1];
        } else {
            self.consumer.pop_slice(block);
        }
        Pull::Ready
    }

    fn prime_level(&self) -> usize {
        2 * self.block
    }

    fn low_water(&self) -> usize {
        self.block + self.margin
    }

    fn high_water(&self) -> usize {
        2 * self.block + self.margin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;
    const MARGIN: usize = 96;

    fn outlet_with(fill: usize) -> (HeapProd<f32>, Outlet) {
        let (mut producer, consumer) = ring(RATE);
        let samples: Vec<f32> = (0..fill).map(|index| index as f32).collect();
        producer.push_slice(&samples);
        (producer, Outlet::new(consumer, RATE))
    }

    fn primed_outlet_with(fill: usize) -> (HeapProd<f32>, Outlet) {
        let (producer, mut outlet) = outlet_with(fill);
        outlet.primed = true;
        outlet.block = BLOCK;
        (producer, outlet)
    }

    #[test]
    fn stays_silent_until_two_blocks_are_buffered() {
        let (mut producer, mut outlet) = outlet_with(BLOCK);
        let mut block = [1.0; BLOCK];

        assert_eq!(outlet.pull(&mut block), Pull::Priming);
        assert!(block.iter().all(|&sample| sample == 0.0));

        producer.push_slice(&[0.5; BLOCK]);
        assert_eq!(outlet.pull(&mut block), Pull::Ready);
        assert_eq!(block[0], 0.0);
        assert_eq!(outlet.fill(), BLOCK);
    }

    #[test]
    fn reports_an_underrun_and_reprimes_when_starved() {
        let (_producer, mut outlet) = primed_outlet_with(BLOCK - 1);
        let mut block = [1.0; BLOCK];

        assert_eq!(outlet.pull(&mut block), Pull::Underrun);
        assert!(block.iter().all(|&sample| sample == 0.0));
        assert_eq!(outlet.pull(&mut block), Pull::Priming);
    }

    #[test]
    fn repeats_a_sample_when_running_low() {
        let (_producer, mut outlet) = primed_outlet_with(BLOCK + MARGIN - 1);
        let mut block = [0.0; BLOCK];

        assert_eq!(outlet.pull(&mut block), Pull::Ready);
        assert_eq!(outlet.fill(), MARGIN);
        assert_eq!(block[BLOCK - 1], block[BLOCK - 2]);
    }

    #[test]
    fn drops_a_sample_when_running_high() {
        let fill = 2 * BLOCK + MARGIN + 1;
        let (_producer, mut outlet) = primed_outlet_with(fill);
        let mut block = [0.0; BLOCK];

        assert_eq!(outlet.pull(&mut block), Pull::Ready);
        assert_eq!(outlet.fill(), fill - BLOCK - 1);
        assert_eq!(block[0], 1.0);
    }

    #[test]
    fn an_inlet_at_the_chain_rate_passes_samples_straight_through() {
        let (producer, consumer) = ring(RATE);
        let mut inlet = Inlet::new(producer, RATE, RATE).expect("inlet");
        for index in 0..BLOCK {
            inlet.push(index as f32).expect("room");
        }
        assert_eq!(inlet.latency_frames(), 0);
        assert_eq!(inlet.chain_frames(BLOCK), BLOCK);
        assert_eq!(consumer.occupied_len(), BLOCK);
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
    }

    #[test]
    fn passes_samples_through_inside_the_band() {
        let (_producer, mut outlet) = primed_outlet_with(BLOCK + MARGIN);
        let mut block = [0.0; BLOCK];

        assert_eq!(outlet.pull(&mut block), Pull::Ready);
        assert_eq!(outlet.fill(), MARGIN);
        assert_eq!(block[BLOCK - 1], (BLOCK - 1) as f32);
    }
}
