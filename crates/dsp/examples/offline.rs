use std::env;
use std::error::Error;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use wonkixer_dsp::{BlockReport, Chain, ParamValues, StageId, presets};

const BLOCK: usize = 480;
const USAGE: &str = "usage: offline <input.wav> <output.wav> [preset name]";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or(USAGE)?;
    let output = args.next().ok_or(USAGE)?;
    let values = match args.next() {
        Some(name) => {
            presets::find(&name)
                .ok_or_else(|| format!("unknown preset: {name}"))?
                .values
        }
        None => ParamValues::default(),
    };

    let mut reader = WavReader::open(&input)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    let interleaved: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 * scale))
                .collect::<Result<_, _>>()?
        }
    };
    let mut mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    let mut chain = Chain::new(spec.sample_rate);
    chain.apply(&values);
    let mut peak = BlockReport::NONE;
    for block in mono.chunks_mut(BLOCK) {
        peak = peak.max(chain.process_block(block));
    }

    let mut writer = WavWriter::create(
        &output,
        WavSpec {
            channels: 1,
            sample_rate: spec.sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        },
    )?;
    for sample in &mono {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;

    println!("peak gain reduction");
    for stage in StageId::DYNAMICS {
        println!(
            "  {:<12} {:>5.1} dB",
            stage.key(),
            peak.get(stage).reduction_db
        );
    }
    Ok(())
}
