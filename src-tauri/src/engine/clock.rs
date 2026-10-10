use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const UNKNOWN: i64 = i64::MIN;

pub struct CaptureClock {
    sample_rate: i128,
    origin: AtomicI64,
}

impl CaptureClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: i128::from(sample_rate),
            origin: AtomicI64::new(UNKNOWN),
        }
    }

    pub fn mark(&self, frame: u64, captured_ns: u128) {
        let origin = i128::try_from(captured_ns).unwrap_or(i128::MAX) - self.span(frame);
        let origin = i64::try_from(origin).unwrap_or(UNKNOWN);
        self.origin.store(origin, Ordering::Release);
    }

    pub fn delay(&self, frame: u64, playback_ns: u128) -> Option<Duration> {
        let origin = self.origin.load(Ordering::Acquire);
        if origin == UNKNOWN {
            return None;
        }
        let captured = i128::from(origin) + self.span(frame);
        let playback = i128::try_from(playback_ns).ok()?;
        u64::try_from(playback - captured)
            .ok()
            .map(Duration::from_nanos)
    }

    fn span(&self, frames: u64) -> i128 {
        i128::from(frames) * NANOS_PER_SECOND / self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const SECOND: u128 = 1_000_000_000;

    #[test]
    fn knows_nothing_before_the_first_capture() {
        let clock = CaptureClock::new(RATE);
        assert_eq!(clock.delay(0, 5 * SECOND), None);
    }

    #[test]
    fn interpolates_the_capture_time_of_any_frame_from_the_last_packet() {
        let clock = CaptureClock::new(RATE);
        clock.mark(48_000, 100 * SECOND);

        assert_eq!(
            clock.delay(48_000, 100 * SECOND + 30_000_000),
            Some(Duration::from_millis(30))
        );
        assert_eq!(
            clock.delay(48_480, 100 * SECOND + 30_000_000),
            Some(Duration::from_millis(20))
        );
        assert_eq!(
            clock.delay(47_520, 100 * SECOND + 30_000_000),
            Some(Duration::from_millis(40))
        );
    }

    #[test]
    fn a_newer_packet_replaces_the_reference_point() {
        let clock = CaptureClock::new(RATE);
        clock.mark(0, 10 * SECOND);
        clock.mark(480, 10 * SECOND + 10_000_200);

        assert_eq!(
            clock.delay(480, 10 * SECOND + 25_000_200),
            Some(Duration::from_millis(15))
        );
    }

    #[test]
    fn a_frame_played_before_it_was_captured_has_no_delay() {
        let clock = CaptureClock::new(RATE);
        clock.mark(4_800, 10 * SECOND);

        assert_eq!(clock.delay(9_600, 10 * SECOND), None);
    }
}
