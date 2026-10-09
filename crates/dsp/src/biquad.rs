use std::f32::consts::PI;

const MIN_Q: f32 = 0.05;
const MAX_FREQUENCY_RATIO: f32 = 0.49;
const SHELF_SLOPE: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Coefficients {
    pub const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    pub fn low_pass(sample_rate: f32, frequency: f32, q: f32) -> Self {
        let angle = Angle::new(sample_rate, frequency, q);
        let b1 = 1.0 - angle.cos;
        Self::normalized(
            b1 / 2.0,
            b1,
            b1 / 2.0,
            1.0 + angle.alpha,
            -2.0 * angle.cos,
            1.0 - angle.alpha,
        )
    }

    pub fn high_pass(sample_rate: f32, frequency: f32, q: f32) -> Self {
        let angle = Angle::new(sample_rate, frequency, q);
        let b0 = (1.0 + angle.cos) / 2.0;
        Self::normalized(
            b0,
            -(1.0 + angle.cos),
            b0,
            1.0 + angle.alpha,
            -2.0 * angle.cos,
            1.0 - angle.alpha,
        )
    }

    pub fn band_pass(sample_rate: f32, frequency: f32, q: f32) -> Self {
        let angle = Angle::new(sample_rate, frequency, q);
        Self::normalized(
            angle.alpha,
            0.0,
            -angle.alpha,
            1.0 + angle.alpha,
            -2.0 * angle.cos,
            1.0 - angle.alpha,
        )
    }

    pub fn peaking(sample_rate: f32, frequency: f32, q: f32, gain_db: f32) -> Self {
        let angle = Angle::new(sample_rate, frequency, q);
        let amplitude = amplitude(gain_db);
        Self::normalized(
            1.0 + angle.alpha * amplitude,
            -2.0 * angle.cos,
            1.0 - angle.alpha * amplitude,
            1.0 + angle.alpha / amplitude,
            -2.0 * angle.cos,
            1.0 - angle.alpha / amplitude,
        )
    }

    pub fn low_shelf(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let shelf = Shelf::new(sample_rate, frequency, gain_db);
        let (a, cos, root) = (shelf.amplitude, shelf.cos, shelf.root);
        Self::normalized(
            a * ((a + 1.0) - (a - 1.0) * cos + root),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - root),
            (a + 1.0) + (a - 1.0) * cos + root,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - root,
        )
    }

    pub fn high_shelf(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let shelf = Shelf::new(sample_rate, frequency, gain_db);
        let (a, cos, root) = (shelf.amplitude, shelf.cos, shelf.root);
        Self::normalized(
            a * ((a + 1.0) + (a - 1.0) * cos + root),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - root),
            (a + 1.0) - (a - 1.0) * cos + root,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - root,
        )
    }

    fn normalized(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    pub fn response_db(&self, sample_rate: f32, frequency: f32) -> f32 {
        let omega = 2.0 * PI * frequency / sample_rate;
        let (sin1, cos1) = omega.sin_cos();
        let (sin2, cos2) = (2.0 * omega).sin_cos();
        let numerator = (
            self.b0 + self.b1 * cos1 + self.b2 * cos2,
            -(self.b1 * sin1 + self.b2 * sin2),
        );
        let denominator = (
            1.0 + self.a1 * cos1 + self.a2 * cos2,
            -(self.a1 * sin1 + self.a2 * sin2),
        );
        let magnitude = (numerator.0.hypot(numerator.1)) / (denominator.0.hypot(denominator.1));
        20.0 * magnitude.log10()
    }
}

struct Angle {
    cos: f32,
    alpha: f32,
}

impl Angle {
    fn new(sample_rate: f32, frequency: f32, q: f32) -> Self {
        let omega = 2.0 * PI * clamp_frequency(sample_rate, frequency) / sample_rate;
        let (sin, cos) = omega.sin_cos();
        Self {
            cos,
            alpha: sin / (2.0 * q.max(MIN_Q)),
        }
    }
}

struct Shelf {
    amplitude: f32,
    cos: f32,
    root: f32,
}

impl Shelf {
    fn new(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let amplitude = amplitude(gain_db);
        let omega = 2.0 * PI * clamp_frequency(sample_rate, frequency) / sample_rate;
        let (sin, cos) = omega.sin_cos();
        let alpha =
            sin / 2.0 * ((amplitude + 1.0 / amplitude) * (1.0 / SHELF_SLOPE - 1.0) + 2.0).sqrt();
        Self {
            amplitude,
            cos,
            root: 2.0 * amplitude.sqrt() * alpha,
        }
    }
}

fn amplitude(gain_db: f32) -> f32 {
    10f32.powf(gain_db / 40.0)
}

fn clamp_frequency(sample_rate: f32, frequency: f32) -> f32 {
    frequency.clamp(1.0, sample_rate * MAX_FREQUENCY_RATIO)
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    coefficients: Coefficients,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(coefficients: Coefficients) -> Self {
        Self {
            coefficients,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn identity() -> Self {
        Self::new(Coefficients::IDENTITY)
    }

    pub fn set(&mut self, coefficients: Coefficients) {
        self.coefficients = coefficients;
    }

    pub fn coefficients(&self) -> Coefficients {
        self.coefficients
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let c = self.coefficients;
        let output = c.b0 * input + self.z1;
        self.z1 = c.b1 * input - c.a1 * output + self.z2;
        self.z2 = c.b2 * input - c.a2 * output;
        output
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;
    const BUTTERWORTH: f32 = std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn high_pass_rolls_off_below_cutoff_and_passes_above() {
        let filter = Coefficients::high_pass(RATE, 100.0, BUTTERWORTH);
        assert!((filter.response_db(RATE, 100.0) - -3.01).abs() < 0.1);
        assert!(filter.response_db(RATE, 50.0) < -11.0);
        assert!(filter.response_db(RATE, 1_000.0).abs() < 0.05);
    }

    #[test]
    fn peaking_boosts_at_center_and_is_flat_far_away() {
        let filter = Coefficients::peaking(RATE, 1_000.0, 1.0, 6.0);
        assert!((filter.response_db(RATE, 1_000.0) - 6.0).abs() < 0.05);
        assert!(filter.response_db(RATE, 20.0).abs() < 0.05);
        assert!(filter.response_db(RATE, 20_000.0).abs() < 0.1);
    }

    #[test]
    fn shelves_reach_their_gain_on_the_shelf_side() {
        let low = Coefficients::low_shelf(RATE, 200.0, -6.0);
        assert!((low.response_db(RATE, 20.0) - -6.0).abs() < 0.1);
        assert!(low.response_db(RATE, 5_000.0).abs() < 0.1);

        let high = Coefficients::high_shelf(RATE, 8_000.0, 3.0);
        assert!((high.response_db(RATE, 20_000.0) - 3.0).abs() < 0.2);
        assert!(high.response_db(RATE, 100.0).abs() < 0.05);
    }

    #[test]
    fn band_pass_peaks_at_unity_on_center() {
        let filter = Coefficients::band_pass(RATE, 6_500.0, 1.0);
        assert!(filter.response_db(RATE, 6_500.0).abs() < 0.05);
        assert!(filter.response_db(RATE, 500.0) < -20.0);
    }

    #[test]
    fn identity_passes_samples_unchanged() {
        let mut filter = Biquad::identity();
        assert_eq!(filter.process(0.25), 0.25);
        assert_eq!(filter.process(-0.5), -0.5);
    }
}
