use std::collections::VecDeque;

pub struct DelayLine {
    buffer: Vec<f32>,
    index: usize,
}

impl DelayLine {
    pub fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn push(&mut self, sample: f32) -> f32 {
        let delayed = self.buffer[self.index];
        self.buffer[self.index] = sample;
        self.index = (self.index + 1) % self.buffer.len();
        delayed
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }
}

pub struct SlidingMax {
    window: u64,
    count: u64,
    candidates: VecDeque<(u64, f32)>,
}

impl SlidingMax {
    pub fn new(window: usize) -> Self {
        let window = window.max(1);
        Self {
            window: window as u64,
            count: 0,
            candidates: VecDeque::with_capacity(window + 1),
        }
    }

    pub fn push(&mut self, value: f32) -> f32 {
        self.count += 1;
        while self
            .candidates
            .back()
            .is_some_and(|&(_, candidate)| candidate <= value)
        {
            self.candidates.pop_back();
        }
        self.candidates.push_back((self.count, value));
        while self
            .candidates
            .front()
            .is_some_and(|&(index, _)| index + self.window <= self.count)
        {
            self.candidates.pop_front();
        }
        self.candidates.front().map_or(value, |&(_, max)| max)
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.candidates.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_line_returns_samples_after_its_length() {
        let mut delay = DelayLine::new(3);
        assert_eq!(delay.push(1.0), 0.0);
        assert_eq!(delay.push(2.0), 0.0);
        assert_eq!(delay.push(3.0), 0.0);
        assert_eq!(delay.push(4.0), 1.0);
        assert_eq!(delay.push(5.0), 2.0);
    }

    #[test]
    fn sliding_max_tracks_the_window_and_forgets_old_peaks() {
        let mut max = SlidingMax::new(3);
        assert_eq!(max.push(1.0), 1.0);
        assert_eq!(max.push(5.0), 5.0);
        assert_eq!(max.push(2.0), 5.0);
        assert_eq!(max.push(1.0), 5.0);
        assert_eq!(max.push(1.0), 2.0);
        assert_eq!(max.push(0.5), 1.0);
    }

    #[test]
    fn sliding_max_never_grows_past_its_window() {
        let mut max = SlidingMax::new(4);
        for value in (0..1000).rev() {
            max.push(value as f32);
            assert!(max.candidates.len() <= 4);
        }
    }
}
