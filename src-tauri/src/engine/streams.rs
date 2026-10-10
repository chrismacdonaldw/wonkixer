use std::sync::Arc;

use assert_no_alloc::{assert_no_alloc, permit_alloc};
use cpal::traits::{DeviceTrait, StreamTrait};
use wonkixer_dsp::Params;

use super::capture::{self, Sink, Tap};
use super::clock::{CaptureClock, Ticker};
use super::devices::Direction;
use super::meters::MeterStore;
use super::output::{Output, OutputStream, negotiated};
use super::processor::Processor;
use super::transport::{self, Inlet, Outlet, Pull};
use super::wasapi;

pub struct Streams {
    _input: cpal::Stream,
    _output: OutputStream,
}

impl Streams {
    #[cfg(test)]
    fn low_latency(&self) -> bool {
        matches!(self._output, OutputStream::LowLatency { .. })
    }
}

pub struct Opened {
    pub streams: Streams,
    pub sample_rate: u32,
    pub capture: Sink,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Disconnected,
    ServiceStopped,
    Reset,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamFailure {
    pub direction: Direction,
    pub cause: Cause,
}

impl StreamFailure {
    pub fn describe(&self, device: &str) -> String {
        let role = self.direction.noun();
        match &self.cause {
            Cause::Disconnected => format!("The {role} \"{device}\" was disconnected."),
            Cause::ServiceStopped => "The Windows audio service stopped.".to_owned(),
            Cause::Reset => format!(
                "Windows reset the {role} \"{device}\", usually because its format changed in Sound settings."
            ),
            Cause::Failed(detail) => format!("The {role} \"{device}\" stopped: {detail}"),
        }
    }
}

pub fn open(
    input: &cpal::Device,
    output: &cpal::Device,
    meters: Arc<MeterStore>,
    params: Arc<Params>,
    on_failure: impl Fn(StreamFailure) + Clone + Send + 'static,
) -> Result<Opened, String> {
    let input_config = negotiated(input.default_input_config(), "input")?;
    let output = Output::prepare(output)?;
    let sample_rate = output.sample_rate();
    let (producer, consumer) = transport::ring(sample_rate);
    let inlet = Inlet::new(producer, input_config.sample_rate(), sample_rate)?;
    let (tap, capture) = capture::tap(sample_rate);
    let clock = Arc::new(CaptureClock::new(sample_rate));
    let processor = Processor::new(params, sample_rate);
    meters.set_processing_latency(processor.latency_samples());
    meters.set_conversion_latency(inlet.latency_frames());

    let input_stream = input
        .build_input_stream(
            input_config.config(),
            input_callback(
                inlet,
                input_config.channels(),
                Arc::clone(&clock),
                Arc::clone(&meters),
            ),
            error_callback(Direction::Input, Arc::clone(&meters), on_failure.clone()),
            None,
        )
        .map_err(|error| format!("Could not open the input device: {error}"))?;
    let render = output_callback(
        Outlet::new(consumer, sample_rate, output.burst()),
        processor,
        output.channels(),
        sample_rate,
        clock,
        Arc::clone(&meters),
        tap,
    );
    let output_stream = output.start(
        render,
        error_callback(Direction::Output, meters, on_failure),
    )?;

    input_stream
        .play()
        .map_err(|error| format!("Could not start the input device: {error}"))?;

    Ok(Opened {
        streams: Streams {
            _input: input_stream,
            _output: output_stream,
        },
        sample_rate,
        capture,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Glitch,
    Ignorable,
    Fatal(Cause),
}

pub fn fault(error: &cpal::Error) -> Fault {
    match error.kind() {
        cpal::ErrorKind::Xrun => Fault::Glitch,
        cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied => Fault::Ignorable,
        cpal::ErrorKind::DeviceNotAvailable => Fault::Fatal(Cause::Disconnected),
        cpal::ErrorKind::HostUnavailable => Fault::Fatal(Cause::ServiceStopped),
        cpal::ErrorKind::StreamInvalidated => Fault::Fatal(Cause::Reset),
        _ => Fault::Fatal(Cause::Failed(error.to_string())),
    }
}

fn error_callback(
    direction: Direction,
    meters: Arc<MeterStore>,
    on_failure: impl Fn(StreamFailure) + Send + 'static,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| match fault(&error) {
        Fault::Glitch => meters.record_dropout(),
        Fault::Ignorable => {}
        Fault::Fatal(cause) => on_failure(StreamFailure { direction, cause }),
    }
}

fn input_callback(
    mut inlet: Inlet,
    channels: u16,
    clock: Arc<CaptureClock>,
    meters: Arc<MeterStore>,
) -> impl FnMut(&[f32], &cpal::InputCallbackInfo) + Send + 'static {
    let channels = usize::from(channels);
    let scale = 1.0 / channels as f32;
    let mut pre_roll = true;
    move |data, info| {
        if pre_roll {
            wasapi::promote_current_thread();
            pre_roll = false;
            return;
        }
        assert_no_alloc(|| {
            let first = inlet.pushed();
            let mut peak = 0.0f32;
            let mut overflowed = false;
            for frame in data.chunks_exact(channels) {
                let sample = frame.iter().sum::<f32>() * scale;
                peak = peak.max(sample.abs());
                overflowed |= inlet.push(sample).is_err();
            }
            if overflowed {
                meters.record_dropout();
            }
            let frames = inlet.chain_frames(data.len() / channels);
            clock.mark(first, frames, info.timestamp().capture.as_nanos());
            meters.record_input(peak, frames);
        })
    }
}

fn output_callback(
    mut outlet: Outlet,
    mut processor: Processor,
    channels: u16,
    sample_rate: u32,
    clock: Arc<CaptureClock>,
    meters: Arc<MeterStore>,
    mut tap: Tap,
) -> impl FnMut(&mut [f32], u128) + Send + 'static {
    let channels = usize::from(channels);
    let ticker = Ticker::new();
    let mut mono = vec![0.0f32; sample_rate as usize];
    let mut warmed_up = false;
    move |data, playback| {
        assert_no_alloc(|| {
            if !warmed_up {
                permit_alloc(|| processor.warm_up());
                warmed_up = true;
            }
            let frames = (data.len() / channels).min(mono.len());
            let fill = outlet.fill();
            let head = outlet.position();
            let block = &mut mono[..frames];
            match outlet.pull(block, clock.timing(head, ticker.now_ns())) {
                Pull::Underrun => meters.record_dropout(),
                Pull::Ready => {
                    meters.mark_flowing();
                    if let Some(delay) = clock.delay(head, playback) {
                        meters.record_transport_delay(delay);
                    }
                }
                Pull::Priming => {}
            }
            tap.record(block);
            meters.record_report(processor.process(block));
            meters.set_processing_latency(processor.latency_samples());
            let mut peak = 0.0f32;
            for (frame, &sample) in data.chunks_exact_mut(channels).zip(block.iter()) {
                peak = peak.max(sample.abs());
                frame.fill(sample);
            }
            meters.record_output(peak, frames, fill);
        })
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use cpal::traits::HostTrait;
    use wonkixer_dsp::ParamValues;

    use super::super::audition::{self, Clip};
    use super::super::devices;
    use super::*;

    const OPT_IN: &str = "WONKIXER_AUDIO_TESTS";
    const SOAK: &str = "WONKIXER_SOAK_SECONDS";
    const MIN_SOAK_SECONDS: u64 = 10;
    const SETTLE_SECONDS: u64 = 1;
    const LOW_LATENCY_BOUND_MS: f32 = 45.0;
    const SHARED_BOUND_MS: f32 = 80.0;
    const AUDITION_FROM_SECOND: u64 = 3;
    const AUDITION_SECONDS: u64 = 4;
    const TAKE_RATE: u32 = 48_000;

    fn failure_log() -> (
        Arc<Mutex<Vec<StreamFailure>>>,
        impl Fn(StreamFailure) + Clone + Send + 'static,
    ) {
        let failures = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&failures);
        (failures, move |failure| {
            sink.lock().expect("failure log").push(failure);
        })
    }

    #[test]
    fn glitches_count_as_dropouts_instead_of_stopping_the_engine() {
        let meters = Arc::new(MeterStore::default());
        meters.mark_flowing();
        let (failures, on_failure) = failure_log();
        let mut callback = error_callback(Direction::Input, Arc::clone(&meters), on_failure);

        callback(cpal::Error::new(cpal::ErrorKind::Xrun));
        callback(cpal::Error::new(cpal::ErrorKind::DeviceChanged));
        callback(cpal::Error::new(cpal::ErrorKind::RealtimeDenied));
        callback(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));

        assert_eq!(meters.snapshot(48_000).dropouts, 1);
        assert_eq!(
            *failures.lock().expect("failure log"),
            vec![StreamFailure {
                direction: Direction::Input,
                cause: Cause::Disconnected,
            }]
        );
    }

    #[test]
    fn fatal_errors_are_classified_by_what_happened_to_the_device() {
        let cause = |kind| match fault(&cpal::Error::new(kind)) {
            Fault::Fatal(cause) => cause,
            other => panic!("{kind:?} should be fatal, got {other:?}"),
        };

        assert_eq!(
            cause(cpal::ErrorKind::DeviceNotAvailable),
            Cause::Disconnected
        );
        assert_eq!(
            cause(cpal::ErrorKind::HostUnavailable),
            Cause::ServiceStopped
        );
        assert_eq!(cause(cpal::ErrorKind::StreamInvalidated), Cause::Reset);
        assert!(matches!(
            cause(cpal::ErrorKind::BackendError),
            Cause::Failed(_)
        ));
    }

    #[test]
    fn a_lost_device_is_described_in_plain_words() {
        let lost = StreamFailure {
            direction: Direction::Input,
            cause: Cause::Disconnected,
        };

        assert_eq!(
            lost.describe("Microphone (HyperX Cloud Alpha Wireless)"),
            "The input device \"Microphone (HyperX Cloud Alpha Wireless)\" was disconnected."
        );
    }

    fn test_input(host: &cpal::Host) -> cpal::Device {
        let usable = |device: &cpal::Device| {
            devices::display_name(device).is_ok_and(|name| !name.starts_with(devices::CABLE_OUTPUT))
        };
        host.default_input_device()
            .filter(usable)
            .or_else(|| host.input_devices().ok()?.find(usable))
            .expect("an input device other than the cable")
    }

    fn test_output(host: &cpal::Host) -> cpal::Device {
        host.output_devices()
            .expect("output devices")
            .find(|device| {
                devices::display_name(device)
                    .is_ok_and(|name| name.starts_with(devices::CABLE_INPUT))
            })
            .or_else(|| host.default_output_device())
            .expect("an output device")
    }

    fn test_playback(host: &cpal::Host) -> cpal::Device {
        let usable = |device: &cpal::Device| {
            devices::display_name(device).is_ok_and(|name| !name.starts_with(devices::CABLE_INPUT))
        };
        host.default_output_device()
            .filter(usable)
            .or_else(|| host.output_devices().ok()?.find(usable))
            .expect("a playback device other than the cable")
    }

    fn quiet_take() -> Clip {
        let samples: Vec<f32> = (0..2 * TAKE_RATE)
            .map(|index| {
                let time = index as f32 / TAKE_RATE as f32;
                let gate = if ((time * 4.0) as u32).is_multiple_of(2) {
                    0.05
                } else {
                    0.0
                };
                gate * (TAU * 330.0 * time).sin()
            })
            .collect();
        Clip::new(&samples, TAKE_RATE)
    }

    #[test]
    fn passes_audio_between_real_devices_without_dropouts() {
        if std::env::var_os(OPT_IN).is_none() {
            return;
        }
        let seconds = std::env::var(SOAK)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(MIN_SOAK_SECONDS)
            .max(MIN_SOAK_SECONDS);
        let host = cpal::default_host();
        let input = test_input(&host);
        let output = test_output(&host);
        let playback = test_playback(&host);
        let meters = Arc::new(MeterStore::default());
        let params = Arc::new(Params::new(&ParamValues::default()));
        let (failures, on_failure) = failure_log();
        let audition_lost = Arc::new(AtomicBool::new(false));

        let opened = open(
            &input,
            &output,
            Arc::clone(&meters),
            Arc::clone(&params),
            on_failure,
        )
        .expect("streams to open");
        let sample_rate = opened.sample_rate;
        let low_latency = opened.streams.low_latency();
        let mut worst = 0.0f32;
        let mut audition = None;
        for second in 1..=seconds {
            if second == AUDITION_FROM_SECOND {
                let lost = Arc::clone(&audition_lost);
                audition = Some(
                    audition::open(&playback, &quiet_take(), Arc::clone(&params), move || {
                        lost.store(true, Ordering::Relaxed);
                    })
                    .expect("the audition to open"),
                );
            } else if second == AUDITION_FROM_SECOND + AUDITION_SECONDS {
                audition = None;
            }
            std::thread::sleep(Duration::from_secs(1));
            let frame = meters.snapshot(sample_rate);
            eprintln!(
                "{second:>3} s: {:.1} ms, {} dropouts{}",
                frame.latency_ms,
                frame.dropouts,
                if audition.is_some() {
                    ", auditioning"
                } else {
                    ""
                }
            );
            if second > SETTLE_SECONDS {
                worst = worst.max(frame.latency_ms);
            }
        }
        let frame = meters.snapshot(sample_rate);
        eprintln!(
            "{} -> {} ({}): worst {worst:.1} ms after settling, {} dropouts in {seconds} s, \
             auditioned to {} for {AUDITION_SECONDS} s",
            devices::display_name(&input).unwrap_or_default(),
            devices::display_name(&output).unwrap_or_default(),
            if low_latency {
                "low-latency output"
            } else {
                "shared output"
            },
            frame.dropouts,
            devices::display_name(&playback).unwrap_or_default(),
        );

        assert!(failures.lock().expect("failure log").is_empty());
        assert!(!audition_lost.load(Ordering::Relaxed));
        assert_eq!(frame.dropouts, 0);
        let bound = if low_latency {
            LOW_LATENCY_BOUND_MS
        } else {
            SHARED_BOUND_MS
        };
        assert!(worst > 0.0 && worst < bound, "{worst} ms");
    }
}
