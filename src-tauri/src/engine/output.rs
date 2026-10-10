use std::fmt;

use cpal::traits::{DeviceTrait, StreamTrait};

use super::priority::Promotion;
use super::wasapi::{self, Prepared, Render};

pub enum Output<'device> {
    LowLatency(Prepared),
    Shared {
        device: &'device cpal::Device,
        config: cpal::SupportedStreamConfig,
        refusal: String,
    },
}

pub enum OutputStream {
    LowLatency { _render: Render },
    Shared { _stream: cpal::Stream },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputMode {
    LowLatency { period: usize },
    Shared { period: usize, refusal: String },
}

impl OutputMode {
    #[cfg(test)]
    pub fn period(&self) -> usize {
        match self {
            Self::LowLatency { period } | Self::Shared { period, .. } => *period,
        }
    }
}

impl fmt::Display for OutputMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LowLatency { period } => {
                write!(formatter, "low-latency output, {period}-frame period")
            }
            Self::Shared { period, refusal } => write!(
                formatter,
                "shared output, {period}-frame period (no low-latency mode: {refusal})"
            ),
        }
    }
}

impl<'device> Output<'device> {
    pub fn prepare(device: &'device cpal::Device) -> Result<Self, String> {
        let refusal = match device.id() {
            Ok(id) => match wasapi::prepare(id.id()) {
                Ok(prepared) => return Ok(Self::LowLatency(prepared)),
                Err(refusal) => refusal,
            },
            Err(error) => format!("The output device has no id: {error}"),
        };
        Ok(Self::Shared {
            device,
            config: negotiated(device.default_output_config(), "output")?,
            refusal,
        })
    }

    pub fn mode(&self) -> OutputMode {
        match self {
            Self::LowLatency(prepared) => OutputMode::LowLatency {
                period: prepared.format().period,
            },
            Self::Shared {
                config, refusal, ..
            } => OutputMode::Shared {
                period: shared_period(config),
                refusal: refusal.clone(),
            },
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            Self::LowLatency(prepared) => prepared.format().sample_rate,
            Self::Shared { config, .. } => config.sample_rate(),
        }
    }

    pub fn channels(&self) -> u16 {
        match self {
            Self::LowLatency(prepared) => prepared.format().channels,
            Self::Shared { config, .. } => config.channels(),
        }
    }

    pub fn burst(&self) -> usize {
        match self {
            Self::LowLatency(prepared) => {
                let format = prepared.format();
                burst(format.period, format.default_period)
            }
            Self::Shared { config, .. } => {
                let period = shared_period(config);
                burst(period, period)
            }
        }
    }

    pub fn start(
        self,
        mut render: impl FnMut(&mut [f32], u128) + Send + 'static,
        on_error: impl FnMut(cpal::Error) + Send + 'static,
    ) -> Result<OutputStream, String> {
        match self {
            Self::LowLatency(prepared) => prepared
                .start(Box::new(render), Box::new(on_error))
                .map(|render| OutputStream::LowLatency { _render: render }),
            Self::Shared { device, config, .. } => {
                let mut promotion = Promotion::default();
                let stream = device
                    .build_output_stream(
                        config.config(),
                        move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                            promotion.once();
                            render(data, info.timestamp().playback.as_nanos());
                        },
                        on_error,
                        None,
                    )
                    .map_err(|error| format!("Could not open the output device: {error}"))?;
                stream
                    .play()
                    .map_err(|error| format!("Could not start the output device: {error}"))?;
                Ok(OutputStream::Shared { _stream: stream })
            }
        }
    }
}

fn burst(period: usize, default_period: usize) -> usize {
    if period < default_period {
        2 * period
    } else {
        period
    }
}

fn shared_period(config: &cpal::SupportedStreamConfig) -> usize {
    match *config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, .. } => min as usize,
        cpal::SupportedBufferSize::Unknown => (config.sample_rate() / 100) as usize,
    }
}

pub fn negotiated(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config(buffer_size: cpal::SupportedBufferSize) -> cpal::SupportedStreamConfig {
        cpal::SupportedStreamConfig::new(2, 48_000, buffer_size, cpal::SampleFormat::F32)
    }

    #[test]
    fn a_shared_stream_delivers_one_device_period_per_callback() {
        let period = config(cpal::SupportedBufferSize::Range { min: 480, max: 480 });

        assert_eq!(shared_period(&period), 480);
    }

    #[test]
    fn an_unknown_shared_period_is_taken_as_the_usual_ten_milliseconds() {
        let unknown = config(cpal::SupportedBufferSize::Unknown);

        assert_eq!(shared_period(&unknown), 480);
    }

    #[test]
    fn only_a_short_period_needs_room_for_two_periods_at_once() {
        assert_eq!(burst(128, 480), 256);
        assert_eq!(burst(480, 480), 480);
    }

    #[test]
    fn the_mode_names_the_period_and_why_low_latency_was_refused() {
        let shared = OutputMode::Shared {
            period: 480,
            refusal: "its shortest period is its default".to_owned(),
        };

        assert_eq!(OutputMode::LowLatency { period: 128 }.period(), 128);
        assert_eq!(
            shared.to_string(),
            "shared output, 480-frame period (no low-latency mode: its shortest period is its default)"
        );
    }

    #[test]
    fn only_float_devices_can_be_opened() {
        let integer = cpal::SupportedStreamConfig::new(
            2,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::I16,
        );

        assert!(negotiated(Ok(config(cpal::SupportedBufferSize::Unknown)), "output").is_ok());
        assert_eq!(
            negotiated(Ok(integer), "output"),
            Err("The output device uses i16 samples, which is not supported yet.".to_owned())
        );
    }
}
