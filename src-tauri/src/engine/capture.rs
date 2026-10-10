use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use wonkixer_dsp::analysis::Take;

const CAPACITY_SECONDS: u32 = 2;
const ALREADY_RECORDING: &str = "A take is already being recorded.";
const NOT_RECORDING: &str = "No take is being recorded.";
const OVERFLOWED: &str = "The recording dropped audio. Record the take again.";

pub fn tap(sample_rate: u32) -> (Tap, Sink) {
    let (producer, consumer) =
        HeapRb::<f32>::new((sample_rate * CAPACITY_SECONDS) as usize).split();
    let armed = Arc::new(AtomicBool::new(false));
    let overflowed = Arc::new(AtomicBool::new(false));
    (
        Tap {
            producer,
            armed: Arc::clone(&armed),
            overflowed: Arc::clone(&overflowed),
        },
        Sink {
            consumer,
            armed,
            overflowed,
            sample_rate,
            recording: None,
        },
    )
}

pub struct Tap {
    producer: HeapProd<f32>,
    armed: Arc<AtomicBool>,
    overflowed: Arc<AtomicBool>,
}

impl Tap {
    pub fn record(&mut self, block: &[f32]) {
        if !self.armed.load(Ordering::Acquire) {
            return;
        }
        if self.producer.push_slice(block) < block.len() {
            self.overflowed.store(true, Ordering::Release);
        }
    }
}

pub struct Sink {
    consumer: HeapCons<f32>,
    armed: Arc<AtomicBool>,
    overflowed: Arc<AtomicBool>,
    sample_rate: u32,
    recording: Option<Recording>,
}

struct Recording {
    samples: Vec<f32>,
    limit: usize,
}

impl Sink {
    pub fn begin(&mut self, limit_seconds: u32) -> Result<(), String> {
        if self.recording.is_some() {
            return Err(ALREADY_RECORDING.to_owned());
        }
        self.consumer.clear();
        self.overflowed.store(false, Ordering::Release);
        let limit = (self.sample_rate * limit_seconds) as usize;
        self.recording = Some(Recording {
            samples: Vec::with_capacity(limit),
            limit,
        });
        self.armed.store(true, Ordering::Release);
        Ok(())
    }

    pub fn drain(&mut self) {
        if let Some(recording) = &mut self.recording {
            let room = recording.limit - recording.samples.len();
            recording
                .samples
                .extend(self.consumer.pop_iter().take(room));
            if recording.samples.len() >= recording.limit {
                self.armed.store(false, Ordering::Release);
            }
        }
        self.consumer.clear();
    }

    pub fn end(&mut self) -> Result<Take, String> {
        self.drain();
        self.armed.store(false, Ordering::Release);
        let recording = self
            .recording
            .take()
            .ok_or_else(|| NOT_RECORDING.to_owned())?;
        if self.overflowed.swap(false, Ordering::AcqRel) {
            return Err(OVERFLOWED.to_owned());
        }
        Ok(Take {
            sample_rate: self.sample_rate,
            samples: recording.samples,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 1_000;

    fn block(len: usize, value: f32) -> Vec<f32> {
        vec![value; len]
    }

    #[test]
    fn records_only_between_begin_and_end() {
        let (mut tap, mut sink) = tap(RATE);
        tap.record(&block(100, 0.1));
        sink.begin(1).expect("begin");
        tap.record(&block(100, 0.2));
        sink.drain();
        tap.record(&block(50, 0.3));
        let take = sink.end().expect("take");
        assert_eq!(take.sample_rate, RATE);
        assert_eq!(take.samples.len(), 150);
        assert_eq!(take.samples[0], 0.2);
        assert_eq!(take.samples[149], 0.3);
        tap.record(&block(100, 0.4));
        sink.drain();
        assert_eq!(sink.end(), Err(NOT_RECORDING.to_owned()));
    }

    #[test]
    fn stops_at_the_limit_and_refuses_a_second_take() {
        let (mut tap, mut sink) = tap(RATE);
        sink.begin(1).expect("begin");
        assert_eq!(sink.begin(1), Err(ALREADY_RECORDING.to_owned()));
        tap.record(&block(900, 0.5));
        sink.drain();
        tap.record(&block(900, 0.6));
        sink.drain();
        tap.record(&block(900, 0.7));
        let take = sink.end().expect("take");
        assert_eq!(take.samples.len(), 1_000);
        assert_eq!(take.samples[999], 0.6);
    }

    #[test]
    fn an_overflow_fails_the_take_instead_of_returning_a_gap() {
        let (mut tap, mut sink) = tap(RATE);
        sink.begin(5).expect("begin");
        tap.record(&block(2_500, 0.1));
        assert_eq!(sink.end(), Err(OVERFLOWED.to_owned()));
        sink.begin(5).expect("begin again");
        tap.record(&block(10, 0.1));
        assert_eq!(sink.end().expect("take").samples.len(), 10);
    }
}
