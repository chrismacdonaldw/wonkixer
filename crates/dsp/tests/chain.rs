use std::f32::consts::TAU;

use wonkixer_dsp::units::gain_to_db;
use wonkixer_dsp::{BlockReport, Chain, ParamId, ParamValues, StageId};

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

fn sine(frequency: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
    let count = (seconds * RATE as f32) as usize;
    (0..count)
        .map(|index| amplitude * (TAU * frequency * index as f32 / RATE as f32).sin())
        .collect()
}

fn peak_db(samples: &[f32]) -> f32 {
    gain_to_db(
        samples
            .iter()
            .fold(0.0f32, |peak, &sample| peak.max(sample.abs())),
    )
}

fn run(chain: &mut Chain, samples: &mut [f32]) -> BlockReport {
    samples
        .chunks_mut(BLOCK)
        .fold(BlockReport::NONE, |peak, block| {
            peak.max(chain.process_block(block))
        })
}

fn only(stage: StageId) -> ParamValues {
    let mut values = ParamValues::default();
    for other in StageId::ALL {
        values.set_bypassed(other, other != stage);
    }
    values
}

fn second_half(samples: &[f32]) -> &[f32] {
    &samples[samples.len() / 2..]
}

fn window(samples: &[f32], start_seconds: f32, length_seconds: f32) -> &[f32] {
    let start = (start_seconds * RATE as f32) as usize;
    let end = start + (length_seconds * RATE as f32) as usize;
    &samples[start..end]
}

fn render(values: &ParamValues, mut samples: Vec<f32>) -> Vec<f32> {
    let mut chain = Chain::new(RATE);
    chain.apply(values);
    run(&mut chain, &mut samples);
    samples
}

fn largest_step(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0f32, f32::max)
}

#[test]
fn high_pass_removes_rumble_and_keeps_voice() {
    let values = only(StageId::HighPass);
    let rumble = render(&values, sine(45.0, 1.0, 0.5));
    let voice = render(&values, sine(1_000.0, 1.0, 0.5));

    assert!(peak_db(second_half(&rumble)) < gain_to_db(0.5) - 11.0);
    assert!((peak_db(second_half(&voice)) - gain_to_db(0.5)).abs() < 0.1);
}

#[test]
fn compressor_reduces_by_the_static_curve_and_reports_it() {
    let mut values = only(StageId::Compressor);
    values.set(ParamId::CompressorThreshold, -18.0);
    values.set(ParamId::CompressorRatio, 3.0);
    values.set(ParamId::CompressorKnee, 0.0);
    values.set(ParamId::CompressorMakeup, 0.0);
    values.set(ParamId::CompressorAttack, 0.1);
    values.set(ParamId::CompressorRelease, 10.0);

    let mut chain = Chain::new(RATE);
    chain.apply(&values);
    let mut tone = sine(1_000.0, 1.0, 0.5);
    let report = run(&mut chain, &mut tone);
    let compressor = report.get(StageId::Compressor);

    assert!(
        (peak_db(second_half(&tone)) - -14.0).abs() < 0.5,
        "{}",
        peak_db(second_half(&tone))
    );
    assert!(
        (compressor.reduction_db - 8.0).abs() < 0.5,
        "{compressor:?}"
    );
    assert!(
        (compressor.detector_db - -6.02).abs() < 0.1,
        "{compressor:?}"
    );
    assert_eq!(report.get(StageId::Limiter).reduction_db, 0.0);
}

#[test]
fn expander_leaves_speech_alone_and_fades_the_gaps_after_the_hold() {
    let mut values = only(StageId::Expander);
    values.set(ParamId::ExpanderThreshold, -45.0);
    values.set(ParamId::ExpanderRatio, 8.0);
    values.set(ParamId::ExpanderRange, 30.0);
    values.set(ParamId::ExpanderHold, 80.0);
    values.set(ParamId::ExpanderRelease, 150.0);

    let mut signal = sine(1_000.0, 0.5, 0.1);
    signal.extend(sine(1_000.0, 1.0, 0.000_316));
    let out = render(&values, signal);

    assert!((peak_db(window(&out, 0.25, 0.2)) - -20.0).abs() < 0.2);
    assert!(peak_db(window(&out, 0.52, 0.02)) > -70.0 - 2.0);
    assert!(peak_db(window(&out, 1.3, 0.1)) < -90.0);
    assert!(peak_db(window(&out, 1.3, 0.1)) > -70.0 - 30.0 - 1.0);
}

#[test]
fn limiter_never_exceeds_the_ceiling_and_recovers() {
    let mut values = only(StageId::Limiter);
    values.set(ParamId::LimiterCeiling, -1.0);
    values.set(ParamId::LimiterRelease, 60.0);

    let mut signal = sine(1_000.0, 0.5, 2.0);
    signal.extend(sine(1_000.0, 0.5, 0.1));
    let mut chain = Chain::new(RATE);
    chain.apply(&values);
    let report = run(&mut chain, &mut signal);

    assert!(peak_db(&signal) <= -1.0 + 0.01, "{}", peak_db(&signal));
    assert!(
        report.get(StageId::Limiter).reduction_db > 6.0,
        "{report:?}"
    );
    assert!((peak_db(window(&signal, 0.8, 0.2)) - -20.0).abs() < 0.5);
    assert_eq!(chain.latency_samples(), 240);
}

#[test]
fn de_esser_ducks_sibilance_more_than_body() {
    let mut values = only(StageId::DeEsser);
    values.set(ParamId::DeEsserFrequency, 6_500.0);
    values.set(ParamId::DeEsserThreshold, -30.0);
    values.set(ParamId::DeEsserRatio, 4.0);

    let sibilance = render(&values, sine(7_000.0, 0.5, 0.316));
    let body = render(&values, sine(1_000.0, 0.5, 0.316));

    let sibilance_db = peak_db(second_half(&sibilance));
    let body_db = peak_db(second_half(&body));
    assert!(
        body_db - sibilance_db >= 10.0,
        "body {body_db} sibilance {sibilance_db}"
    );
    assert!((body_db - -10.0).abs() < 1.0, "{body_db}");
}

#[test]
fn noise_suppression_adds_a_frame_of_latency_only_while_in_the_chain() {
    let mut chain = Chain::new(RATE);
    assert_eq!(chain.latency_samples(), 240);

    let mut values = ParamValues::default();
    values.set_bypassed(StageId::NoiseSuppressor, false);
    chain.apply(&values);
    assert_eq!(chain.latency_samples(), 720);

    let mut tone = sine(300.0, 1.0, 0.3);
    run(&mut chain, &mut tone);
    assert!(tone.iter().all(|sample| sample.is_finite()));
    assert!(
        largest_step(&tone) < 0.1,
        "stepped by {}",
        largest_step(&tone)
    );
}

#[test]
fn silence_after_signal_never_produces_subnormals() {
    let mut signal = sine(1_000.0, 0.5, 0.5);
    signal.extend(std::iter::repeat_n(0.0, 3 * RATE as usize));
    let out = render(&ParamValues::default(), signal);

    assert!(out.iter().all(|sample| !sample.is_subnormal()));
    assert!(out.iter().all(|sample| sample.is_finite()));
}

#[test]
fn sweeping_every_parameter_is_finite_and_click_free() {
    for id in ParamId::ALL {
        let spec = id.spec();
        let mut chain = Chain::new(RATE);
        let mut values = ParamValues::default();
        let mut tone = sine(200.0, 1.0, 0.5);
        let blocks = tone.len() / BLOCK;
        for (index, block) in tone.chunks_mut(BLOCK).enumerate() {
            let position = index as f32 / blocks as f32;
            values.set(id, spec.min + (spec.max - spec.min) * position);
            chain.apply(&values);
            chain.process_block(block);
        }
        assert!(tone.iter().all(|sample| sample.is_finite()), "{}", spec.key);
        assert!(
            largest_step(&tone) < 0.1,
            "{} stepped by {}",
            spec.key,
            largest_step(&tone)
        );
    }
}

#[test]
fn toggling_bypass_crossfades_without_clicks() {
    let mut chain = Chain::new(RATE);
    let mut values = ParamValues::default();
    let mut tone = sine(300.0, 2.0, 0.5);
    for (index, block) in tone.chunks_mut(BLOCK).enumerate() {
        if index % 10 == 0 {
            let flip = index / 10 % 2 == 0;
            for stage in StageId::ALL {
                values.set_bypassed(stage, flip);
            }
            chain.apply(&values);
        }
        chain.process_block(block);
    }
    assert!(
        largest_step(&tone) < 0.1,
        "stepped by {}",
        largest_step(&tone)
    );
}
