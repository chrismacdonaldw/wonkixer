use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
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

pub fn display_name(device: &cpal::Device) -> Result<String, String> {
    device
        .description()
        .map(|description| description.name().to_owned())
        .map_err(|error| format!("Could not read the audio device details: {error}"))
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
