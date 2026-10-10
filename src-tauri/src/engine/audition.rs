use std::sync::Arc;

use assert_no_alloc::{assert_no_alloc, permit_alloc};
use cpal::traits::{DeviceTrait, StreamTrait};
use wonkixer_dsp::Params;

use super::output::negotiated;
use super::processor::Processor;
use super::resampler;
use super::streams::{Fault, fault};
use super::wasapi;

const FADE_MS: u32 = 15;

#[derive(Clone, Debug)]
pub struct Clip {
    samples: Arc<[f32]>,
    sample_rate: u32,
}

impl Clip {
    pub fn new(samples: &[f32], sample_rate: u32) -> Self {
        let mut faded = samples.to_vec();
        let fade = ((sample_rate * FADE_MS / 1000) as usize).min(faded.len() / 2);
        let (head, rest) = faded.split_at_mut(fade);
        let tail_start = rest.len() - fade;
        for (index, (first, last)) in head
            .iter_mut()
            .zip(rest[tail_start..].iter_mut().rev())
            .enumerate()
        {
            let gain = index as f32 / fade as f32;
            *first *= gain;
            *last *= gain;
        }
        Self {
            samples: faded.into(),
            sample_rate,
        }
    }

    fn at_rate(&self, sample_rate: u32) -> Result<Self, String> {
        if sample_rate == self.sample_rate {
            return Ok(self.clone());
        }
        let converted = resampler::convert(&self.samples, self.sample_rate, sample_rate)?;
        Ok(Self {
            samples: converted.into(),
            sample_rate,
        })
    }
}

struct Loop {
    samples: Arc<[f32]>,
    position: usize,
}

impl Loop {
    fn new(clip: Clip) -> Self {
        Self {
            samples: clip.samples,
            position: 0,
        }
    }

    fn fill(&mut self, block: &mut [f32]) {
        if self.samples.is_empty() {
            block.fill(0.0);
            return;
        }
        let mut written = 0;
        while written < block.len() {
            let source = &self.samples[self.position..];
            let count = source.len().min(block.len() - written);
            block[written..written + count].copy_from_slice(&source[..count]);
            written += count;
            self.position = (self.position + count) % self.samples.len();
        }
    }
}

pub struct Audition {
    _stream: cpal::Stream,
}

pub fn open(
    device: &cpal::Device,
    clip: &Clip,
    params: Arc<Params>,
    on_failure: impl Fn() + Send + 'static,
) -> Result<Audition, String> {
    let config = negotiated(device.default_output_config(), "playback")?;
    let sample_rate = config.sample_rate();
    let clip = clip.at_rate(sample_rate)?;
    let mut render = render(
        Loop::new(clip),
        Processor::new(params, sample_rate),
        config.channels(),
        sample_rate as usize,
    );
    let mut promoted = false;
    let stream = device
        .build_output_stream(
            config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if !promoted {
                    wasapi::promote_current_thread();
                    promoted = true;
                }
                render(data);
            },
            move |error| {
                if let Fault::Fatal(_) = fault(&error) {
                    on_failure();
                }
            },
            None,
        )
        .map_err(|error| format!("Could not open the playback device: {error}"))?;
    stream
        .play()
        .map_err(|error| format!("Could not start the playback device: {error}"))?;
    Ok(Audition { _stream: stream })
}

fn render(
    mut source: Loop,
    mut processor: Processor,
    channels: u16,
    capacity: usize,
) -> impl FnMut(&mut [f32]) + Send + 'static {
    let channels = usize::from(channels);
    let mut mono = vec![0.0f32; capacity];
    let mut warmed_up = false;
    move |data| {
        assert_no_alloc(|| {
            if !warmed_up {
                permit_alloc(|| processor.warm_up());
                warmed_up = true;
            }
            for chunk in data.chunks_mut(mono.len() * channels) {
                let block = &mut mono[..chunk.len() / channels];
                source.fill(block);
                processor.process(block);
                for (frame, &sample) in chunk.chunks_exact_mut(channels).zip(block.iter()) {
                    frame.fill(sample);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use wonkixer_dsp::{ParamId, ParamValues, StageId};

    use super::*;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;

    fn looped(samples: &[f32]) -> Loop {
        Loop {
            samples: samples.into(),
            position: 0,
        }
    }

    fn filled(source: &mut Loop, len: usize) -> Vec<f32> {
        let mut block = vec![-1.0; len];
        source.fill(&mut block);
        block
    }

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0.0f32, |peak, &sample| peak.max(sample.abs()))
    }

    fn tone(seconds: usize) -> Vec<f32> {
        (0..RATE as usize * seconds)
            .map(|index| 0.5 * (TAU * 500.0 * index as f32 / RATE as f32).sin())
            .collect()
    }

    fn flat() -> ParamValues {
        let mut values = ParamValues::default();
        for stage in StageId::ALL {
            values.set_bypassed(stage, true);
        }
        values
    }

    #[test]
    fn the_loop_wraps_to_the_start_across_block_boundaries() {
        let mut source = looped(&[0.0, 1.0, 2.0, 3.0, 4.0]);

        assert_eq!(filled(&mut source, 3), [0.0, 1.0, 2.0]);
        assert_eq!(filled(&mut source, 3), [3.0, 4.0, 0.0]);
        assert_eq!(filled(&mut source, 3), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_block_longer_than_the_take_repeats_it() {
        let mut source = looped(&[0.0, 1.0, 2.0]);

        assert_eq!(filled(&mut source, 7), [0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0]);
        assert_eq!(filled(&mut source, 2), [1.0, 2.0]);
    }

    #[test]
    fn an_empty_take_plays_silence() {
        assert_eq!(filled(&mut looped(&[]), 4), [0.0; 4]);
    }

    #[test]
    fn a_take_at_the_device_rate_is_shared_without_converting() {
        let clip = Clip::new(&[0.5; 4_800], RATE);
        let ready = clip.at_rate(RATE).expect("clip");

        assert!(Arc::ptr_eq(&clip.samples, &ready.samples));
    }

    #[test]
    fn a_take_at_another_rate_is_converted_once_before_playing() {
        let clip = Clip::new(&vec![0.0; 44_100], 44_100);
        let ready = clip.at_rate(RATE).expect("clip");

        assert_eq!(ready.sample_rate, RATE);
        assert_eq!(ready.samples.len(), RATE as usize);
    }

    #[test]
    fn both_ends_of_the_take_fade_so_the_loop_point_does_not_click() {
        let clip = Clip::new(&[1.0; 4_800], RATE);
        let fade = (RATE * FADE_MS / 1000) as usize;

        assert_eq!(clip.samples[0], 0.0);
        assert_eq!(clip.samples[4_799], 0.0);
        assert!((clip.samples[fade / 2] - 0.5).abs() < 0.01);
        assert!(
            clip.samples[fade..4_800 - fade]
                .iter()
                .all(|&sample| sample == 1.0)
        );
        assert_eq!(Clip::new(&[1.0; 3], RATE).samples[..], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn renders_the_loop_to_every_channel_through_the_shared_settings_without_allocating() {
        let params = Arc::new(Params::new(&flat()));
        let processor = Processor::new(Arc::clone(&params), RATE);
        let mut render = render(
            Loop::new(Clip::new(&tone(1), RATE)),
            processor,
            2,
            BLOCK / 4,
        );
        std::thread::spawn(move || {
            let mut data = vec![0.0f32; 2 * BLOCK];
            for _ in 0..20 {
                render(&mut data);
            }
            assert!(data.chunks_exact(2).all(|frame| frame[0] == frame[1]));
            assert!((peak(&data) - 0.5).abs() < 0.01, "{}", peak(&data));

            params.set(ParamId::InputGain, -24.0);
            for _ in 0..20 {
                render(&mut data);
            }
            assert!(
                (peak(&data) - 0.5 * 0.0631).abs() < 0.005,
                "{}",
                peak(&data)
            );
        })
        .join()
        .expect("join");
    }
}
