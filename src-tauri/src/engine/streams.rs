use std::sync::Arc;

use assert_no_alloc::assert_no_alloc;
use cpal::traits::{DeviceTrait, StreamTrait};
use ringbuf::HeapProd;
use ringbuf::traits::Producer;

use super::meters::MeterStore;
use super::transport::{self, Outlet, Pull};

pub struct Streams {
    _input: cpal::Stream,
    _output: cpal::Stream,
}

pub fn open(
    input: &cpal::Device,
    output: &cpal::Device,
    meters: Arc<MeterStore>,
    on_failure: impl Fn(String) + Clone + Send + 'static,
) -> Result<(Streams, u32), String> {
    let input_config = negotiated(input.default_input_config(), "input")?;
    let output_config = negotiated(output.default_output_config(), "output")?;
    if input_config.sample_rate() != output_config.sample_rate() {
        return Err(format!(
            "The input runs at {} Hz but the output runs at {} Hz. Set both devices to the same rate in Windows Sound settings.",
            input_config.sample_rate(),
            output_config.sample_rate()
        ));
    }
    let sample_rate = input_config.sample_rate();
    let (producer, consumer) = transport::ring(sample_rate);

    let input_stream = input
        .build_input_stream(
            input_config.config(),
            input_callback(producer, input_config.channels(), Arc::clone(&meters)),
            failure_callback(on_failure.clone()),
            None,
        )
        .map_err(|error| format!("Could not open the input device: {error}"))?;
    let output_stream = output
        .build_output_stream(
            output_config.config(),
            output_callback(
                Outlet::new(consumer, sample_rate),
                output_config.channels(),
                sample_rate,
                meters,
            ),
            failure_callback(on_failure),
            None,
        )
        .map_err(|error| format!("Could not open the output device: {error}"))?;

    input_stream
        .play()
        .map_err(|error| format!("Could not start the input device: {error}"))?;
    output_stream
        .play()
        .map_err(|error| format!("Could not start the output device: {error}"))?;

    Ok((
        Streams {
            _input: input_stream,
            _output: output_stream,
        },
        sample_rate,
    ))
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

fn failure_callback(
    on_failure: impl Fn(String) + Send + 'static,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| on_failure(error.to_string())
}

fn input_callback(
    mut producer: HeapProd<f32>,
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
                overflowed |= producer.try_push(sample).is_err();
            }
            if overflowed {
                meters.record_dropout();
            }
            meters.record_input(peak, data.len() / channels);
        })
    }
}

fn output_callback(
    mut outlet: Outlet,
    channels: u16,
    sample_rate: u32,
    meters: Arc<MeterStore>,
) -> impl FnMut(&mut [f32], &cpal::OutputCallbackInfo) + Send + 'static {
    let channels = usize::from(channels);
    let mut mono = vec![0.0f32; sample_rate as usize];
    move |data, _| {
        assert_no_alloc(|| {
            let frames = (data.len() / channels).min(mono.len());
            let fill = outlet.fill();
            let block = &mut mono[..frames];
            if outlet.pull(block) == Pull::Underrun {
                meters.record_dropout();
            }
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

    use super::*;

    const OPT_IN: &str = "WONKIXER_AUDIO_TESTS";
    const CABLE_PREFIX: &str = "CABLE Input";

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
        let failures = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&failures);

        let (_streams, sample_rate) = open(&input, &output, Arc::clone(&meters), move |message| {
            sink.lock().expect("failure log").push(message);
        })
        .expect("streams to open");
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
