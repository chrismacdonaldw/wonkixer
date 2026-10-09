use crate::units::one_pole;

const SETTLE_TOLERANCE: f32 = 1e-4;

#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    value: f32,
    target: f32,
    coefficient: f32,
}

impl Smoother {
    pub fn new(value: f32, time_ms: f32, sample_rate: f32) -> Self {
        Self {
            value,
            target: value,
            coefficient: one_pole(time_ms, sample_rate),
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn snap(&mut self, value: f32) {
        self.value = value;
        self.target = value;
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn is_settled(&self) -> bool {
        self.value == self.target
    }

    pub fn advance(&mut self) -> f32 {
        if self.value != self.target {
            self.value = self.target + (self.value - self.target) * self.coefficient;
            let tolerance = SETTLE_TOLERANCE * self.target.abs().max(1.0);
            if (self.value - self.target).abs() <= tolerance {
                self.value = self.target;
            }
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_toward_the_target_and_eventually_settles() {
        let mut smoother = Smoother::new(0.0, 10.0, 48_000.0);
        smoother.set_target(1.0);

        let first = smoother.advance();
        assert!(first > 0.0 && first < 0.01);
        assert!(!smoother.is_settled());

        for _ in 0..48_000 {
            smoother.advance();
        }
        assert!(smoother.is_settled());
        assert_eq!(smoother.value(), 1.0);
    }

    #[test]
    fn snapping_skips_the_ramp() {
        let mut smoother = Smoother::new(0.0, 10.0, 48_000.0);
        smoother.snap(3.0);
        assert_eq!(smoother.advance(), 3.0);
        assert!(smoother.is_settled());
    }
}
