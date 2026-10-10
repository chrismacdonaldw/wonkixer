use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

pub const CABLE_INPUT: &str = "CABLE Input";
pub const CABLE_OUTPUT: &str = "CABLE Output";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub cable_installed: bool,
    pub cable_input_id: Option<String>,
    pub cable_output_id: Option<String>,
    pub inputs: usize,
    pub outputs: usize,
}

impl SetupStatus {
    pub fn detect(devices: &AudioDevices) -> Self {
        let cable_input_id = first_named(&devices.outputs, CABLE_INPUT);
        let cable_output_id = first_named(&devices.inputs, CABLE_OUTPUT);
        Self {
            cable_installed: cable_input_id.is_some() && cable_output_id.is_some(),
            cable_input_id,
            cable_output_id,
            inputs: devices.inputs.len(),
            outputs: devices.outputs.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

impl Direction {
    pub fn noun(self) -> &'static str {
        match self {
            Self::Input => "input device",
            Self::Output => "output device",
        }
    }
}

pub fn enumerate(host: &cpal::Host) -> Result<AudioDevices, String> {
    Ok(AudioDevices {
        inputs: describe_all(
            host.input_devices(),
            host.default_input_device(),
            Direction::Input,
        )?,
        outputs: describe_all(
            host.output_devices(),
            host.default_output_device(),
            Direction::Output,
        )?,
    })
}

pub fn find(host: &cpal::Host, id: &str) -> Result<cpal::Device, String> {
    let device_id: cpal::DeviceId = id
        .parse()
        .map_err(|_| format!("Unrecognised audio device id: {id}"))?;
    host.device_by_id(&device_id)
        .ok_or_else(|| format!("The audio device is no longer available: {id}"))
}

pub fn is_connected(host: &cpal::Host, id: &str) -> bool {
    find(host, id).is_ok()
}

pub fn display_name(device: &cpal::Device) -> Result<String, String> {
    device
        .description()
        .map(|description| description.name().to_owned())
        .map_err(|error| format!("Could not read the audio device details: {error}"))
}

fn first_named(devices: &[AudioDevice], prefix: &str) -> Option<String> {
    devices
        .iter()
        .find(|device| device.name.starts_with(prefix))
        .map(|device| device.id.clone())
}

fn describe_all<I>(
    devices: Result<I, cpal::Error>,
    default: Option<cpal::Device>,
    direction: Direction,
) -> Result<Vec<AudioDevice>, String>
where
    I: Iterator<Item = cpal::Device>,
{
    let default_id = default.and_then(|device| device.id().ok());
    let devices = devices.map_err(|error| format!("Could not list audio devices: {error}"))?;
    Ok(devices
        .filter_map(|device| describe(&device, direction, default_id.as_ref()))
        .collect())
}

fn describe(
    device: &cpal::Device,
    direction: Direction,
    default_id: Option<&cpal::DeviceId>,
) -> Option<AudioDevice> {
    let id = device.id().ok()?;
    let name = display_name(device).ok()?;
    let config = default_config(device, direction).ok()?;
    Some(AudioDevice {
        is_default: Some(&id) == default_id,
        id: id.to_string(),
        name,
        sample_rate: config.sample_rate(),
        channels: config.channels(),
    })
}

fn default_config(
    device: &cpal::Device,
    direction: Direction,
) -> Result<cpal::SupportedStreamConfig, cpal::Error> {
    match direction {
        Direction::Input => device.default_input_config(),
        Direction::Output => device.default_output_config(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str) -> AudioDevice {
        AudioDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            is_default: false,
            sample_rate: 48_000,
            channels: 2,
        }
    }

    #[test]
    fn finds_both_cable_endpoints_by_name() {
        let status = SetupStatus::detect(&AudioDevices {
            inputs: vec![
                device("mic", "Microphone (HyperX Cloud Alpha Wireless)"),
                device("cable-out", "CABLE Output (VB-Audio Virtual Cable)"),
            ],
            outputs: vec![
                device("phones", "Headphones (HyperX Cloud Alpha Wireless)"),
                device("cable-in", "CABLE Input (VB-Audio Virtual Cable)"),
            ],
        });

        assert_eq!(
            status,
            SetupStatus {
                cable_installed: true,
                cable_input_id: Some("cable-in".to_owned()),
                cable_output_id: Some("cable-out".to_owned()),
                inputs: 2,
                outputs: 2,
            }
        );
    }

    #[test]
    fn a_missing_capture_endpoint_means_the_cable_is_not_installed() {
        let status = SetupStatus::detect(&AudioDevices {
            inputs: vec![device("mic", "Microphone (USB)")],
            outputs: vec![device("cable-in", "CABLE Input (VB-Audio Virtual Cable)")],
        });

        assert!(!status.cable_installed);
        assert_eq!(status.cable_input_id.as_deref(), Some("cable-in"));
        assert_eq!(status.cable_output_id, None);
    }

    #[test]
    fn the_endpoint_direction_matters() {
        let status = SetupStatus::detect(&AudioDevices {
            inputs: vec![device("wrong-way", "CABLE Input (VB-Audio Virtual Cable)")],
            outputs: Vec::new(),
        });

        assert_eq!(status.cable_input_id, None);
        assert_eq!(status.inputs, 1);
        assert_eq!(status.outputs, 0);
    }
}
