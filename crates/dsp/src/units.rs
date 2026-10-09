pub const MIN_DB: f32 = -120.0;

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db * 0.05)
}

pub fn gain_to_db(gain: f32) -> f32 {
    if gain > 0.0 {
        (20.0 * gain.log10()).max(MIN_DB)
    } else {
        MIN_DB
    }
}

pub fn one_pole(time_ms: f32, sample_rate: f32) -> f32 {
    if time_ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (time_ms * 0.001 * sample_rate)).exp()
    }
}

pub fn ms_to_samples(time_ms: f32, sample_rate: f32) -> usize {
    (time_ms * 0.001 * sample_rate).round().max(0.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_round_trip_through_linear_gain() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0206) - 0.5).abs() < 1e-4);
        assert!((gain_to_db(2.0) - 6.0206).abs() < 1e-3);
        assert_eq!(gain_to_db(0.0), MIN_DB);
    }

    #[test]
    fn one_pole_is_instant_at_zero_and_slower_for_longer_times() {
        assert_eq!(one_pole(0.0, 48_000.0), 0.0);
        assert!(one_pole(10.0, 48_000.0) < one_pole(100.0, 48_000.0));
        assert_eq!(ms_to_samples(5.0, 48_000.0), 240);
    }
}
