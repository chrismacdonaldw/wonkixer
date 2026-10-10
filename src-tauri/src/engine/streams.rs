use std::sync::Arc;

use assert_no_alloc::{assert_no_alloc, permit_alloc};
use cpal::traits::{DeviceTrait, StreamTrait};
use wonkixer_dsp::Params;

use super::capture::{self, Sink, Tap};
use super::devices::Direction;
use super::meters::MeterStore;
use super::processor::Processor;
use super::transport::{self, Inlet, Outlet, Pull};

pub struct Streams {
    _input: cpal::Stream,
    _output: cpal::Stream,
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
    let output_config = negotiated(output.default_output_config(), "output")?;
    let sample_rate = output_config.sample_rate();
    let (producer, consumer) = transport::ring(sample_rate);
    let inlet = Inlet::new(producer, input_config.sample_rate(), sample_rate)?;
    let (tap, capture) = capture::tap(sample_rate);
    let processor = Processor::new(params, sample_rate);
    meters.set_processing_latency(processor.latency_samples());
    meters.set_conversion_latency(inlet.latency_frames());

    let input_stream = input
        .build_input_stream(
            input_config.config(),
            input_callback(inlet, input_config.channels(), Arc::clone(&meters)),
            error_callback(Direction::Input, Arc::clone(&meters), on_failure.clone()),
            None,
        )
        .map_err(|error| format!("Could not open the input device: {error}"))?;
    let output_stream = output
        .build_output_stream(
            output_config.config(),
            output_callback(
                Outlet::new(consumer, sample_rate),
                processor,
                output_config.channels(),
                sample_rate,
                Arc::clone(&meters),
                tap,
            ),
            error_callback(Direction::Output, meters, on_failure),
            None,
        )
        .map_err(|error| format!("Could not open the output device: {error}"))?;

    input_stream
        .play()
        .map_err(|error| format!("Could not start the input device: {error}"))?;
    output_stream
        .play()
        .map_err(|error| format!("Could not start the output device: {error}"))?;

    Ok(Opened {
        streams: Streams {
            _input: input_stream,
            _output: output_stream,
        },
        sample_rate,
        capture,
    })
}

fn negotiated(
    config: Result<cpal::SupportedStreamConfig, cpal::Error>,
    role: &str,
) -> Result<cpal::SupportedStreamConfig, String> {
    let config =
        config.map_err(|error| format!("Could not read the {role} device format: {error}"))?;
    if config.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!(
            "The {role} device uses {} samples, which is not supported yet.",
            config.sample_format()
        ));
    }
    if config.channels() == 0 {
        return Err(format!("The {role} device reports no audio channels."));
    }
    Ok(config)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Fault {
    Glitch,
    Ignorable,
    Fatal(Cause),
}

fn fault(error: &cpal::Error) -> Fault {
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
    meters: Arc<MeterStore>,
) -> impl FnMut(&[f32], &cpal::InputCallbackInfo) + Send + 'static {
    let channels = usize::from(channels);
    let scale = 1.0 / channels as f32;
    move |data, _| {
        assert_no_alloc(|| {
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
            meters.record_input(peak, inlet.chain_frames(data.len() / channels));
        })
    }
}

fn output_callback(
    mut outlet: Outlet,
    mut processor: Processor,
    channels: u16,
    sample_rate: u32,
    meters: Arc<MeterStore>,
    mut tap: Tap,
) -> impl FnMut(&mut [f32], &cpal::OutputCallbackInfo) + Send + 'static {
    let channels = usize::from(channels);
    let mut mono = vec![0.0f32; sample_rate as usize];
    let mut warmed_up = false;
    move |data, _| {
        assert_no_alloc(|| {
            if !warmed_up {
                permit_alloc(|| processor.warm_up());
                warmed_up = true;
            }
            let frames = (data.len() / channels).min(mono.len());
            let fill = outlet.fill();
            let block = &mut mono[..frames];
            if outlet.pull(block) == Pull::Underrun {
                meters.record_dropout();
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
    use std::sync::Mutex;
    use std::time::Duration;

    use cpal::traits::HostTrait;
    use wonkixer_dsp::ParamValues;

    use super::*;

    const OPT_IN: &str = "WONKIXER_AUDIO_TESTS";
    const CABLE_PREFIX: &str = "CABLE Input";

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

    #[test]
    fn passes_audio_between_real_devices_without_dropouts() {
        if std::env::var_os(OPT_IN).is_none() {
            return;
        }
        let host = cpal::default_host();
        let input = host.default_input_device().expect("a default input device");
        let output = host
            .output_devices()
            .expect("output devices")
            .find(|device| {
                super::super::devices::display_name(device)
                    .is_ok_and(|name| name.starts_with(CABLE_PREFIX))
            })
            .or_else(|| host.default_output_device())
            .expect("an output device");
        let meters = Arc::new(MeterStore::default());
        let params = Arc::new(Params::new(&ParamValues::default()));
        let (failures, on_failure) = failure_log();

        let opened = open(&input, &output, Arc::clone(&meters), params, on_failure)
            .expect("streams to open");
        let sample_rate = opened.sample_rate;
        std::thread::sleep(Duration::from_secs(3));
        let frame = meters.snapshot(sample_rate);

        assert!(failures.lock().expect("failure log").is_empty());
        assert_eq!(frame.dropouts, 0);
        assert!(
            frame.latency_ms > 0.0 && frame.latency_ms < 80.0,
            "{frame:?}"
        );
    }
}
