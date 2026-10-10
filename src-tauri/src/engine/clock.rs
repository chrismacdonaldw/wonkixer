use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use super::transport::Timing;

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const UNKNOWN: i64 = i64::MIN;
const STEP_TOLERANCE_NS: u128 = 1_500_000;
const SETTLE_LIMIT: u32 = 5;

pub struct CaptureClock {
    sample_rate: i128,
    origin: AtomicI64,
    packet: AtomicUsize,
}

impl CaptureClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: i128::from(sample_rate),
            origin: AtomicI64::new(UNKNOWN),
            packet: AtomicUsize::new(0),
        }
    }

    pub fn mark(&self, frame: u64, frames: usize, captured_ns: u128) {
        let origin = i128::try_from(captured_ns).unwrap_or(i128::MAX) - self.span(frame);
        let origin = i64::try_from(origin).unwrap_or(UNKNOWN);
        self.packet.store(frames, Ordering::Relaxed);
        self.origin.store(origin, Ordering::Release);
    }

    pub fn delay(&self, frame: u64, instant_ns: u128) -> Option<Duration> {
        let origin = self.origin.load(Ordering::Acquire);
        if origin == UNKNOWN {
            return None;
        }
        let captured = i128::from(origin) + self.span(frame);
        let instant = i128::try_from(instant_ns).ok()?;
        u64::try_from(instant - captured)
            .ok()
            .map(Duration::from_nanos)
    }

    pub fn timing(&self, frame: u64, instant_ns: u128) -> Option<Timing> {
        let delay = self.delay(frame, instant_ns)?;
        let lag = delay.as_nanos() * self.sample_rate as u128 / NANOS_PER_SECOND as u128;
        Some(Timing {
            lag: usize::try_from(lag).ok()?,
            packet: self.packet.load(Ordering::Relaxed),
        })
    }

    fn span(&self, frames: u64) -> i128 {
        i128::from(frames) * NANOS_PER_SECOND / self.sample_rate
    }
}

#[derive(Debug)]
pub struct Settling {
    sample_rate: u128,
    expected: Option<u128>,
    seen: u32,
    settled: bool,
}

impl Settling {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: u128::from(sample_rate).max(1),
            expected: None,
            seen: 0,
            settled: false,
        }
    }

    pub fn admit(&mut self, captured_ns: u128, frames: usize) -> bool {
        if self.settled {
            return true;
        }
        let in_step = self
            .expected
            .is_some_and(|expected| expected.abs_diff(captured_ns) <= STEP_TOLERANCE_NS);
        self.seen += 1;
        self.expected =
            Some(captured_ns + frames as u128 * NANOS_PER_SECOND as u128 / self.sample_rate);
        self.settled = in_step || self.seen > SETTLE_LIMIT;
        self.settled
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ticker {
    frequency: u128,
}

impl Ticker {
    pub fn new() -> Self {
        let mut frequency = 0i64;
        unsafe { QueryPerformanceFrequency(&mut frequency) }.ok();
        Self {
            frequency: u128::try_from(frequency).unwrap_or(0).max(1),
        }
    }

    pub fn now_ns(&self) -> u128 {
        let mut counter = 0i64;
        unsafe { QueryPerformanceCounter(&mut counter) }.ok();
        u128::try_from(counter).unwrap_or(0) * NANOS_PER_SECOND as u128 / self.frequency
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
        clock.mark(48_000, 480, 100 * SECOND);

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
        clock.mark(0, 480, 10 * SECOND);
        clock.mark(480, 480, 10 * SECOND + 10_000_200);

        assert_eq!(
            clock.delay(480, 10 * SECOND + 25_000_200),
            Some(Duration::from_millis(15))
        );
    }

    #[test]
    fn timing_counts_the_lag_in_frames_and_reports_the_latest_packet() {
        let clock = CaptureClock::new(RATE);
        clock.mark(0, 960, 10 * SECOND);
        clock.mark(960, 480, 10 * SECOND + 20_000_000);

        assert_eq!(
            clock.timing(960, 10 * SECOND + 32_000_000),
            Some(Timing {
                lag: 576,
                packet: 480,
            })
        );
    }

    const PACKET: usize = 480;
    const PACKET_NS: u128 = 10_000_000;

    #[test]
    fn the_first_capture_packet_is_never_trusted() {
        let mut settling = Settling::new(RATE);

        assert!(!settling.admit(SECOND, PACKET));
    }

    #[test]
    fn a_packet_out_of_step_with_the_one_before_is_dropped_too() {
        let mut settling = Settling::new(RATE);
        settling.admit(SECOND, PACKET);

        assert!(!settling.admit(SECOND + PACKET_NS + 3_260_000, PACKET));
        assert!(settling.admit(SECOND + 2 * PACKET_NS + 3_260_000, PACKET));
    }

    #[test]
    fn packets_in_step_within_the_usual_jitter_are_admitted() {
        let mut settling = Settling::new(RATE);
        settling.admit(SECOND, PACKET);

        assert!(settling.admit(SECOND + PACKET_NS + 740_000, PACKET));
    }

    #[test]
    fn gives_up_waiting_for_steady_timestamps_after_a_few_packets() {
        let mut settling = Settling::new(RATE);
        let admitted: Vec<bool> = (0..7u128)
            .map(|index| settling.admit(SECOND + index * 2 * PACKET_NS, PACKET))
            .collect();

        assert_eq!(admitted, [false, false, false, false, false, true, true]);
    }

    #[test]
    fn once_settled_every_packet_is_admitted() {
        let mut settling = Settling::new(RATE);
        settling.admit(SECOND, PACKET);
        settling.admit(SECOND + PACKET_NS, PACKET);

        assert!(settling.admit(SECOND + 5 * PACKET_NS, PACKET));
    }

    #[test]
    fn the_ticker_runs_forward_on_the_stream_clock() {
        let ticker = Ticker::new();
        let before = ticker.now_ns();
        std::thread::sleep(Duration::from_millis(5));
        let elapsed = ticker.now_ns() - before;

        assert!((5_000_000..1_000_000_000).contains(&elapsed), "{elapsed}");
    }

    #[test]
    fn a_frame_played_before_it_was_captured_has_no_delay() {
        let clock = CaptureClock::new(RATE);
        clock.mark(4_800, 480, 10 * SECOND);

        assert_eq!(clock.delay(9_600, 10 * SECOND), None);
    }
}
