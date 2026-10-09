use ringbuf::traits::{Consumer, Observer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

const CAPACITY_SECONDS: u32 = 1;
const MARGIN_MS: u32 = 2;

pub fn ring(sample_rate: u32) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new((sample_rate * CAPACITY_SECONDS) as usize).split()
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
    use ringbuf::traits::Producer;

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
    fn passes_samples_through_inside_the_band() {
        let (_producer, mut outlet) = primed_outlet_with(BLOCK + MARGIN);
        let mut block = [0.0; BLOCK];

        assert_eq!(outlet.pull(&mut block), Pull::Ready);
        assert_eq!(outlet.fill(), MARGIN);
        assert_eq!(block[BLOCK - 1], (BLOCK - 1) as f32);
    }
}
