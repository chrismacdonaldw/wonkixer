use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;
use wonkixer_dsp::units::MIN_DB;
use wonkixer_dsp::{BlockReport, StageId};

const FLOOR_DB: f32 = -100.0;
const CLIP_LEVEL: f32 = 0.999;

#[derive(Default)]
pub struct MeterStore {
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    input_block: AtomicU32,
    output_block: AtomicU32,
    ring_fill: AtomicU32,
    processing_latency: AtomicU32,
    conversion_latency: AtomicU32,
    dropouts: AtomicU32,
    reduction: [AtomicU32; StageId::COUNT],
    detector: [AtomicU32; StageId::COUNT],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    pub input_peak_db: f32,
    pub output_peak_db: f32,
    pub clipped: bool,
    pub latency_ms: f32,
    pub dropouts: u32,
    pub gain_reduction: BTreeMap<&'static str, f32>,
    pub detector_db: BTreeMap<&'static str, f32>,
}

impl MeterStore {
    pub fn record_input(&self, peak: f32, frames: usize) {
        self.input_peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.input_block.store(clamp_u32(frames), Ordering::Relaxed);
    }

    pub fn record_output(&self, peak: f32, frames: usize, ring_fill: usize) {
        self.output_peak
            .fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.output_block
            .store(clamp_u32(frames), Ordering::Relaxed);
        self.ring_fill
            .store(clamp_u32(ring_fill), Ordering::Relaxed);
    }

    pub fn record_report(&self, report: BlockReport) {
        for stage in StageId::DYNAMICS {
            let telemetry = report.get(stage);
            self.reduction[stage.index()]
                .fetch_max(telemetry.reduction_db.to_bits(), Ordering::Relaxed);
            let lifted = (telemetry.detector_db - MIN_DB).max(0.0);
            self.detector[stage.index()].fetch_max(lifted.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn record_dropout(&self) {
        self.dropouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_processing_latency(&self, frames: usize) {
        self.processing_latency
            .store(clamp_u32(frames), Ordering::Relaxed);
    }

    pub fn set_conversion_latency(&self, frames: usize) {
        self.conversion_latency
            .store(clamp_u32(frames), Ordering::Relaxed);
    }

    pub fn reset(&self) {
        for cell in [
            &self.input_peak,
            &self.output_peak,
            &self.input_block,
            &self.output_block,
            &self.ring_fill,
            &self.processing_latency,
            &self.conversion_latency,
            &self.dropouts,
        ] {
            cell.store(0, Ordering::Relaxed);
        }
        for cell in self.reduction.iter().chain(self.detector.iter()) {
            cell.store(0, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self, sample_rate: u32) -> MeterFrame {
        let input_peak = f32::from_bits(self.input_peak.swap(0, Ordering::Relaxed));
        let output_peak = f32::from_bits(self.output_peak.swap(0, Ordering::Relaxed));
        let buffered_frames = self.input_block.load(Ordering::Relaxed)
            + self.ring_fill.load(Ordering::Relaxed)
            + self.output_block.load(Ordering::Relaxed)
            + self.processing_latency.load(Ordering::Relaxed)
            + self.conversion_latency.load(Ordering::Relaxed);
        MeterFrame {
            input_peak_db: to_db(input_peak),
            output_peak_db: to_db(output_peak),
            clipped: output_peak >= CLIP_LEVEL,
            latency_ms: buffered_frames as f32 * 1000.0 / sample_rate as f32,
            dropouts: self.dropouts.load(Ordering::Relaxed),
            gain_reduction: StageId::DYNAMICS
                .iter()
                .map(|&stage| (stage.key(), take(&self.reduction[stage.index()])))
                .collect(),
            detector_db: StageId::DYNAMICS
                .iter()
                .map(|&stage| (stage.key(), take(&self.detector[stage.index()]) + MIN_DB))
                .collect(),
        }
    }
}

fn take(cell: &AtomicU32) -> f32 {
    f32::from_bits(cell.swap(0, Ordering::Relaxed))
}

fn to_db(peak: f32) -> f32 {
    if peak <= 0.0 {
        FLOOR_DB
    } else {
        (20.0 * peak.log10()).max(FLOOR_DB)
    }
}

fn clamp_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wonkixer_dsp::StageTelemetry;

    #[test]
    fn peaks_hold_the_maximum_until_read_then_reset() {
        let meters = MeterStore::default();
        meters.record_input(0.5, 480);
        meters.record_input(0.25, 480);
        meters.record_output(1.0, 480, 960);

        let first = meters.snapshot(48_000);
        let second = meters.snapshot(48_000);

        assert!((first.input_peak_db - -6.02).abs() < 0.01);
        assert_eq!(first.output_peak_db, 0.0);
        assert!(first.clipped);
        assert_eq!(second.input_peak_db, FLOOR_DB);
        assert!(!second.clipped);
    }

    #[test]
    fn latency_sums_the_buffered_frames_and_processing_delay() {
        let meters = MeterStore::default();
        meters.record_input(0.0, 480);
        meters.record_output(0.0, 480, 960);
        meters.set_processing_latency(240);

        let frame = meters.snapshot(48_000);

        assert!((frame.latency_ms - 45.0).abs() < 0.001);
    }

    #[test]
    fn latency_includes_sample_rate_conversion_until_reset() {
        let meters = MeterStore::default();
        meters.record_input(0.0, 480);
        meters.set_conversion_latency(96);
        assert!((meters.snapshot(48_000).latency_ms - 12.0).abs() < 0.001);

        meters.reset();
        assert_eq!(meters.snapshot(48_000).latency_ms, 0.0);
    }

    #[test]
    fn telemetry_holds_the_block_maximum_per_stage_until_read() {
        let meters = MeterStore::default();
        meters.record_report(
            BlockReport::NONE
                .with_stage(
                    StageId::Compressor,
                    StageTelemetry {
                        reduction_db: 6.0,
                        detector_db: -20.0,
                    },
                )
                .with_stage(
                    StageId::Limiter,
                    StageTelemetry {
                        reduction_db: 1.0,
                        detector_db: -3.0,
                    },
                ),
        );
        meters.record_report(BlockReport::NONE.with_stage(
            StageId::Compressor,
            StageTelemetry {
                reduction_db: 4.0,
                detector_db: -12.0,
            },
        ));

        let first = meters.snapshot(48_000);
        let second = meters.snapshot(48_000);

        assert_eq!(first.gain_reduction["compressor"], 6.0);
        assert_eq!(first.gain_reduction["limiter"], 1.0);
        assert_eq!(first.gain_reduction["expander"], 0.0);
        assert!((first.detector_db["compressor"] - -12.0).abs() < 1e-4);
        assert!((first.detector_db["limiter"] - -3.0).abs() < 1e-4);
        assert_eq!(first.detector_db["expander"], MIN_DB);
        assert_eq!(second.gain_reduction["compressor"], 0.0);
        assert_eq!(second.detector_db["compressor"], MIN_DB);
    }

    #[test]
    fn dropouts_accumulate_until_reset() {
        let meters = MeterStore::default();
        meters.record_dropout();
        meters.record_dropout();
        assert_eq!(meters.snapshot(48_000).dropouts, 2);

        meters.reset();
        assert_eq!(meters.snapshot(48_000).dropouts, 0);
    }
}
