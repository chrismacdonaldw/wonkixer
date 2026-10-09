use crate::units::one_pole;

pub fn compressor_reduction(level_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let overshoot = level_db - threshold_db;
    let slope = 1.0 - 1.0 / ratio.max(1.0);
    knee(overshoot, knee_db) * slope
}

pub fn expander_reduction(
    level_db: f32,
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    range_db: f32,
) -> f32 {
    let undershoot = threshold_db - level_db;
    let slope = ratio.max(1.0) - 1.0;
    (knee(undershoot, knee_db) * slope).min(range_db.max(0.0))
}

pub fn limiter_reduction(level_db: f32, ceiling_db: f32) -> f32 {
    (level_db - ceiling_db).max(0.0)
}

fn knee(excess: f32, width_db: f32) -> f32 {
    if width_db <= 0.0 || 2.0 * excess.abs() > width_db {
        excess.max(0.0)
    } else {
        let half = width_db / 2.0;
        (excess + half) * (excess + half) / (2.0 * width_db)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GainSmoother {
    rise: f32,
    fall: f32,
    value: f32,
}

impl GainSmoother {
    pub fn new(rise_ms: f32, fall_ms: f32, sample_rate: f32) -> Self {
        Self {
            rise: one_pole(rise_ms, sample_rate),
            fall: one_pole(fall_ms, sample_rate),
            value: 0.0,
        }
    }

    pub fn set_times(&mut self, rise_ms: f32, fall_ms: f32, sample_rate: f32) {
        self.rise = one_pole(rise_ms, sample_rate);
        self.fall = one_pole(fall_ms, sample_rate);
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn follow(&mut self, target: f32) -> f32 {
        let coefficient = if target > self.value {
            self.rise
        } else {
            self.fall
        };
        self.value = target + (self.value - target) * coefficient;
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressor_curve_is_flat_below_soft_above_and_continuous_at_the_knee() {
        assert_eq!(compressor_reduction(-40.0, -20.0, 4.0, 6.0), 0.0);
        assert_eq!(compressor_reduction(-23.0, -20.0, 4.0, 6.0), 0.0);
        assert!((compressor_reduction(-20.0, -20.0, 4.0, 6.0) - 0.5625).abs() < 1e-4);
        assert!((compressor_reduction(-17.0, -20.0, 4.0, 6.0) - 2.25).abs() < 1e-4);
        assert!((compressor_reduction(0.0, -20.0, 4.0, 6.0) - 15.0).abs() < 1e-4);
        assert_eq!(compressor_reduction(0.0, -20.0, 1.0, 6.0), 0.0);
    }

    #[test]
    fn hard_knee_compressor_matches_the_straight_line() {
        assert_eq!(compressor_reduction(-10.0, -20.0, 2.0, 0.0), 5.0);
        assert_eq!(compressor_reduction(-20.0, -20.0, 2.0, 0.0), 0.0);
    }

    #[test]
    fn expander_curve_is_unity_above_threshold_and_capped_by_range() {
        assert_eq!(expander_reduction(-10.0, -40.0, 2.5, 6.0, 30.0), 0.0);
        assert!((expander_reduction(-50.0, -40.0, 2.5, 6.0, 30.0) - 15.0).abs() < 1e-4);
        assert_eq!(expander_reduction(-100.0, -40.0, 2.5, 6.0, 30.0), 30.0);
        assert_eq!(expander_reduction(-100.0, -40.0, 1.0, 6.0, 30.0), 0.0);
    }

    #[test]
    fn limiter_curve_only_acts_above_the_ceiling() {
        assert_eq!(limiter_reduction(-3.0, -1.0), 0.0);
        assert_eq!(limiter_reduction(2.0, -1.0), 3.0);
    }

    #[test]
    fn gain_smoother_uses_rise_time_going_up_and_fall_time_going_down() {
        let mut smoother = GainSmoother::new(1.0, 100.0, 48_000.0);
        let up = smoother.follow(10.0);
        assert!(up > 0.1 && up < 0.3);
        for _ in 0..480 {
            smoother.follow(10.0);
        }
        assert!(smoother.value() > 9.99);
        let down = smoother.follow(0.0);
        assert!(down > 9.99 - 0.01 && down < 10.0);
    }
}
