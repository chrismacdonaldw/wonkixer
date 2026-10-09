use std::sync::atomic::{AtomicU32, Ordering};

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StageId {
    Input,
    HighPass,
    NoiseSuppressor,
    Expander,
    DeEsser,
    Equalizer,
    Compressor,
    Limiter,
    Output,
}

impl StageId {
    pub const COUNT: usize = 9;
    pub const ALL: [StageId; Self::COUNT] = [
        StageId::Input,
        StageId::HighPass,
        StageId::NoiseSuppressor,
        StageId::Expander,
        StageId::DeEsser,
        StageId::Equalizer,
        StageId::Compressor,
        StageId::Limiter,
        StageId::Output,
    ];
    pub const DYNAMICS: [StageId; 4] = [
        StageId::Expander,
        StageId::DeEsser,
        StageId::Compressor,
        StageId::Limiter,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn spec(self) -> &'static StageSpec {
        &STAGES[self.index()]
    }

    pub fn key(self) -> &'static str {
        self.spec().key
    }

    pub fn from_key(key: &str) -> Option<Self> {
        STAGES
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| spec.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    Level,
    Filter,
    Cleanup,
    Dynamics,
    Sibilance,
    Eq,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageSpec {
    pub id: StageId,
    pub key: &'static str,
    pub label: &'static str,
    pub title: &'static str,
    pub family: Family,
    pub bypassable: bool,
    pub optional: bool,
    pub latency_ms: f32,
    pub blurb: &'static str,
}

const fn stage(
    id: StageId,
    key: &'static str,
    label: &'static str,
    title: &'static str,
    family: Family,
    blurb: &'static str,
) -> StageSpec {
    StageSpec {
        id,
        key,
        label,
        title,
        family,
        bypassable: true,
        optional: false,
        latency_ms: 0.0,
        blurb,
    }
}

const fn fixed(spec: StageSpec) -> StageSpec {
    StageSpec {
        bypassable: false,
        ..spec
    }
}

const fn optional(spec: StageSpec, latency_ms: f32) -> StageSpec {
    StageSpec {
        optional: true,
        latency_ms,
        ..spec
    }
}

const fn delayed(spec: StageSpec, latency_ms: f32) -> StageSpec {
    StageSpec { latency_ms, ..spec }
}

pub const STAGES: [StageSpec; StageId::COUNT] = [
    fixed(stage(
        StageId::Input,
        "input",
        "INPUT",
        "Input",
        Family::Level,
        "Trims the mic before anything else. Aim for peaks around -12 dBFS while you talk normally.",
    )),
    stage(
        StageId::HighPass,
        "hpf",
        "HIGH-PASS",
        "High-pass",
        Family::Filter,
        "Cuts rumble, breath pops and desk thumps below your voice. Raise it until your voice starts to thin, then back off a little.",
    ),
    optional(
        stage(
            StageId::NoiseSuppressor,
            "rnnoise",
            "NOISE SUPPRESSION",
            "Noise suppression",
            Family::Cleanup,
            "Removes steady background noise such as fans and hum, even while you talk. It works on 10 ms frames, so it adds about 10 ms of latency.",
        ),
        10.0,
    ),
    stage(
        StageId::Expander,
        "expander",
        "EXPANDER",
        "Expander",
        Family::Dynamics,
        "Turns room noise down in the gaps between phrases. Gentler than a gate, and it never fully mutes.",
    ),
    stage(
        StageId::DeEsser,
        "deesser",
        "DE-ESSER",
        "De-esser",
        Family::Sibilance,
        "Tames harsh s and sh sounds. Split band ducks only the highs, so the body of your voice is untouched.",
    ),
    stage(
        StageId::Equalizer,
        "eq",
        "EQ",
        "EQ",
        Family::Eq,
        "Shapes tone. A small cut near 250 Hz clears headset mud; a lift near 4 kHz adds clarity. Drag a numbered point, or type exact values below.",
    ),
    stage(
        StageId::Compressor,
        "compressor",
        "COMPRESSOR",
        "Compressor",
        Family::Dynamics,
        "Evens out loud and quiet words so people hear you at a steady level. Makeup brings the level back up afterwards.",
    ),
    delayed(
        stage(
            StageId::Limiter,
            "limiter",
            "LIMITER",
            "Limiter",
            Family::Dynamics,
            "A brickwall at the ceiling with 5 ms lookahead. Catches shouts and laughs before they clip.",
        ),
        5.0,
    ),
    fixed(stage(
        StageId::Output,
        "output",
        "OUTPUT",
        "Output",
        Family::Level,
        "Final trim into the virtual cable. A hard clip at 0 dBFS follows as a safety net.",
    )),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Unit {
    Decibels,
    Hertz,
    Milliseconds,
    Ratio,
    Q,
    Switch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    Linear,
    Logarithmic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    Main,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSpec {
    pub id: ParamId,
    pub stage: StageId,
    pub key: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    pub scale: Scale,
    pub tier: Tier,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step: f32,
    pub coarse: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamId {
    InputGain,
    HighPassCutoff,
    ExpanderThreshold,
    ExpanderRange,
    ExpanderRatio,
    ExpanderAttack,
    ExpanderHold,
    ExpanderRelease,
    ExpanderKnee,
    DeEsserThreshold,
    DeEsserFrequency,
    DeEsserSplitBand,
    DeEsserRatio,
    LowShelfFrequency,
    LowShelfGain,
    LowMidFrequency,
    LowMidGain,
    LowMidQ,
    PresenceFrequency,
    PresenceGain,
    PresenceQ,
    HighShelfFrequency,
    HighShelfGain,
    CompressorThreshold,
    CompressorRatio,
    CompressorMakeup,
    CompressorAttack,
    CompressorRelease,
    CompressorKnee,
    LimiterCeiling,
    LimiterRelease,
    OutputGain,
}

impl ParamId {
    pub const COUNT: usize = 32;
    pub const ALL: [ParamId; Self::COUNT] = [
        ParamId::InputGain,
        ParamId::HighPassCutoff,
        ParamId::ExpanderThreshold,
        ParamId::ExpanderRange,
        ParamId::ExpanderRatio,
        ParamId::ExpanderAttack,
        ParamId::ExpanderHold,
        ParamId::ExpanderRelease,
        ParamId::ExpanderKnee,
        ParamId::DeEsserThreshold,
        ParamId::DeEsserFrequency,
        ParamId::DeEsserSplitBand,
        ParamId::DeEsserRatio,
        ParamId::LowShelfFrequency,
        ParamId::LowShelfGain,
        ParamId::LowMidFrequency,
        ParamId::LowMidGain,
        ParamId::LowMidQ,
        ParamId::PresenceFrequency,
        ParamId::PresenceGain,
        ParamId::PresenceQ,
        ParamId::HighShelfFrequency,
        ParamId::HighShelfGain,
        ParamId::CompressorThreshold,
        ParamId::CompressorRatio,
        ParamId::CompressorMakeup,
        ParamId::CompressorAttack,
        ParamId::CompressorRelease,
        ParamId::CompressorKnee,
        ParamId::LimiterCeiling,
        ParamId::LimiterRelease,
        ParamId::OutputGain,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn spec(self) -> &'static ParamSpec {
        &SPECS[self.index()]
    }

    pub fn key(self) -> &'static str {
        self.spec().key
    }

    pub fn from_key(key: &str) -> Option<Self> {
        SPECS
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| spec.id)
    }

    pub fn clamp(self, value: f32) -> f32 {
        let spec = self.spec();
        if value.is_finite() {
            value.clamp(spec.min, spec.max)
        } else {
            spec.default
        }
    }
}

struct Range {
    unit: Unit,
    scale: Scale,
    min: f32,
    max: f32,
    step: f32,
    coarse: f32,
}

const FINE_DB: f32 = 0.1;
const FINE_Q: f32 = 0.01;

const fn db(min: f32, max: f32, coarse: f32) -> Range {
    Range {
        unit: Unit::Decibels,
        scale: Scale::Linear,
        min,
        max,
        step: if coarse < FINE_DB { coarse } else { FINE_DB },
        coarse,
    }
}

const fn hz(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Hertz,
        scale: Scale::Logarithmic,
        min,
        max,
        step: 1.0,
        coarse: 1.0,
    }
}

const fn ms(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Milliseconds,
        scale: Scale::Logarithmic,
        min,
        max,
        step: 0.1,
        coarse: 0.1,
    }
}

const fn ratio(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Ratio,
        scale: Scale::Linear,
        min,
        max,
        step: 0.1,
        coarse: 0.1,
    }
}

const fn q(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Q,
        scale: Scale::Logarithmic,
        min,
        max,
        step: FINE_Q,
        coarse: 0.05,
    }
}

const fn switch() -> Range {
    Range {
        unit: Unit::Switch,
        scale: Scale::Linear,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        coarse: 1.0,
    }
}

const fn param(
    id: ParamId,
    stage: StageId,
    key: &'static str,
    label: &'static str,
    tier: Tier,
    range: Range,
    default: f32,
) -> ParamSpec {
    ParamSpec {
        id,
        stage,
        key,
        label,
        unit: range.unit,
        scale: range.scale,
        tier,
        min: range.min,
        max: range.max,
        default,
        step: range.step,
        coarse: range.coarse,
    }
}

use Tier::{Detail, Main};

pub const SPECS: [ParamSpec; ParamId::COUNT] = [
    param(
        ParamId::InputGain,
        StageId::Input,
        "input.gain",
        "Gain",
        Main,
        db(-24.0, 24.0, 0.5),
        0.0,
    ),
    param(
        ParamId::HighPassCutoff,
        StageId::HighPass,
        "hpf.cutoff",
        "Cutoff",
        Main,
        hz(40.0, 300.0),
        90.0,
    ),
    param(
        ParamId::ExpanderThreshold,
        StageId::Expander,
        "expander.threshold",
        "Threshold",
        Main,
        db(-80.0, -20.0, 0.5),
        -45.0,
    ),
    param(
        ParamId::ExpanderRange,
        StageId::Expander,
        "expander.range",
        "Range",
        Main,
        db(0.0, 60.0, 1.0),
        30.0,
    ),
    param(
        ParamId::ExpanderRatio,
        StageId::Expander,
        "expander.ratio",
        "Ratio",
        Main,
        ratio(1.0, 8.0),
        2.5,
    ),
    param(
        ParamId::ExpanderAttack,
        StageId::Expander,
        "expander.attack",
        "Attack",
        Detail,
        ms(0.1, 50.0),
        2.0,
    ),
    param(
        ParamId::ExpanderHold,
        StageId::Expander,
        "expander.hold",
        "Hold",
        Detail,
        ms(0.0, 500.0),
        80.0,
    ),
    param(
        ParamId::ExpanderRelease,
        StageId::Expander,
        "expander.release",
        "Release",
        Detail,
        ms(10.0, 1000.0),
        150.0,
    ),
    param(
        ParamId::ExpanderKnee,
        StageId::Expander,
        "expander.knee",
        "Knee",
        Detail,
        db(0.0, 12.0, 0.5),
        6.0,
    ),
    param(
        ParamId::DeEsserThreshold,
        StageId::DeEsser,
        "deesser.threshold",
        "Threshold",
        Main,
        db(-60.0, 0.0, 0.5),
        -30.0,
    ),
    param(
        ParamId::DeEsserFrequency,
        StageId::DeEsser,
        "deesser.frequency",
        "Frequency",
        Main,
        hz(3000.0, 10000.0),
        6500.0,
    ),
    param(
        ParamId::DeEsserSplitBand,
        StageId::DeEsser,
        "deesser.splitBand",
        "Mode",
        Main,
        switch(),
        1.0,
    ),
    param(
        ParamId::DeEsserRatio,
        StageId::DeEsser,
        "deesser.ratio",
        "Ratio",
        Detail,
        ratio(1.0, 10.0),
        4.0,
    ),
    param(
        ParamId::LowShelfFrequency,
        StageId::Equalizer,
        "eq.lowShelf.frequency",
        "Low shelf",
        Main,
        hz(40.0, 400.0),
        120.0,
    ),
    param(
        ParamId::LowShelfGain,
        StageId::Equalizer,
        "eq.lowShelf.gain",
        "Low shelf gain",
        Main,
        db(-12.0, 12.0, 0.5),
        0.0,
    ),
    param(
        ParamId::LowMidFrequency,
        StageId::Equalizer,
        "eq.lowMid.frequency",
        "Low mid",
        Main,
        hz(100.0, 1000.0),
        250.0,
    ),
    param(
        ParamId::LowMidGain,
        StageId::Equalizer,
        "eq.lowMid.gain",
        "Low mid gain",
        Main,
        db(-12.0, 12.0, 0.5),
        -2.5,
    ),
    param(
        ParamId::LowMidQ,
        StageId::Equalizer,
        "eq.lowMid.q",
        "Low mid Q",
        Main,
        q(0.3, 4.0),
        1.0,
    ),
    param(
        ParamId::PresenceFrequency,
        StageId::Equalizer,
        "eq.presence.frequency",
        "Presence",
        Main,
        hz(1500.0, 8000.0),
        4000.0,
    ),
    param(
        ParamId::PresenceGain,
        StageId::Equalizer,
        "eq.presence.gain",
        "Presence gain",
        Main,
        db(-12.0, 12.0, 0.5),
        2.5,
    ),
    param(
        ParamId::PresenceQ,
        StageId::Equalizer,
        "eq.presence.q",
        "Presence Q",
        Main,
        q(0.3, 4.0),
        0.8,
    ),
    param(
        ParamId::HighShelfFrequency,
        StageId::Equalizer,
        "eq.highShelf.frequency",
        "High shelf",
        Main,
        hz(4000.0, 16000.0),
        10000.0,
    ),
    param(
        ParamId::HighShelfGain,
        StageId::Equalizer,
        "eq.highShelf.gain",
        "High shelf gain",
        Main,
        db(-12.0, 12.0, 0.5),
        1.0,
    ),
    param(
        ParamId::CompressorThreshold,
        StageId::Compressor,
        "compressor.threshold",
        "Threshold",
        Main,
        db(-60.0, 0.0, 0.5),
        -18.0,
    ),
    param(
        ParamId::CompressorRatio,
        StageId::Compressor,
        "compressor.ratio",
        "Ratio",
        Main,
        ratio(1.0, 20.0),
        3.0,
    ),
    param(
        ParamId::CompressorMakeup,
        StageId::Compressor,
        "compressor.makeup",
        "Makeup",
        Main,
        db(0.0, 24.0, 0.5),
        6.0,
    ),
    param(
        ParamId::CompressorAttack,
        StageId::Compressor,
        "compressor.attack",
        "Attack",
        Detail,
        ms(0.1, 100.0),
        8.0,
    ),
    param(
        ParamId::CompressorRelease,
        StageId::Compressor,
        "compressor.release",
        "Release",
        Detail,
        ms(10.0, 1000.0),
        120.0,
    ),
    param(
        ParamId::CompressorKnee,
        StageId::Compressor,
        "compressor.knee",
        "Knee",
        Detail,
        db(0.0, 12.0, 0.5),
        6.0,
    ),
    param(
        ParamId::LimiterCeiling,
        StageId::Limiter,
        "limiter.ceiling",
        "Ceiling",
        Main,
        db(-6.0, 0.0, 0.1),
        -1.0,
    ),
    param(
        ParamId::LimiterRelease,
        StageId::Limiter,
        "limiter.release",
        "Release",
        Detail,
        ms(10.0, 500.0),
        60.0,
    ),
    param(
        ParamId::OutputGain,
        StageId::Output,
        "output.gain",
        "Gain",
        Main,
        db(-24.0, 12.0, 0.5),
        0.0,
    ),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamValues {
    values: [f32; ParamId::COUNT],
    bypassed: [bool; StageId::COUNT],
}

impl Default for ParamValues {
    fn default() -> Self {
        Self {
            values: std::array::from_fn(|index| SPECS[index].default),
            bypassed: std::array::from_fn(|index| STAGES[index].optional),
        }
    }
}

impl ParamValues {
    pub fn get(&self, id: ParamId) -> f32 {
        self.values[id.index()]
    }

    pub fn set(&mut self, id: ParamId, value: f32) {
        self.values[id.index()] = id.clamp(value);
    }

    pub fn is_on(&self, id: ParamId) -> bool {
        self.get(id) >= 0.5
    }

    pub fn is_bypassed(&self, stage: StageId) -> bool {
        self.bypassed[stage.index()]
    }

    pub fn set_bypassed(&mut self, stage: StageId, bypassed: bool) {
        self.bypassed[stage.index()] = bypassed && stage.spec().bypassable;
    }

    pub fn entries(&self) -> impl Iterator<Item = (ParamId, f32)> + '_ {
        ParamId::ALL.iter().map(move |&id| (id, self.get(id)))
    }

    pub fn bypasses(&self) -> impl Iterator<Item = (StageId, bool)> + '_ {
        StageId::ALL
            .iter()
            .copied()
            .filter(|stage| stage.spec().bypassable)
            .map(move |stage| (stage, self.is_bypassed(stage)))
    }
}

pub struct Params {
    values: [AtomicU32; ParamId::COUNT],
    bypassed: AtomicU32,
    generation: AtomicU32,
}

impl Params {
    pub fn new(initial: &ParamValues) -> Self {
        Self {
            values: std::array::from_fn(|index| AtomicU32::new(initial.values[index].to_bits())),
            bypassed: AtomicU32::new(bypass_bits(initial)),
            generation: AtomicU32::new(0),
        }
    }

    pub fn set(&self, id: ParamId, value: f32) {
        self.values[id.index()].store(id.clamp(value).to_bits(), Ordering::Relaxed);
        self.bump();
    }

    pub fn set_bypassed(&self, stage: StageId, bypassed: bool) {
        let bit = 1u32 << stage.index();
        if bypassed && stage.spec().bypassable {
            self.bypassed.fetch_or(bit, Ordering::Relaxed);
        } else {
            self.bypassed.fetch_and(!bit, Ordering::Relaxed);
        }
        self.bump();
    }

    pub fn replace(&self, values: &ParamValues) {
        for (id, value) in values.entries() {
            self.values[id.index()].store(value.to_bits(), Ordering::Relaxed);
        }
        self.bypassed.store(bypass_bits(values), Ordering::Relaxed);
        self.bump();
    }

    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> ParamValues {
        let bits = self.bypassed.load(Ordering::Relaxed);
        ParamValues {
            values: std::array::from_fn(|index| {
                f32::from_bits(self.values[index].load(Ordering::Relaxed))
            }),
            bypassed: std::array::from_fn(|index| bits & (1u32 << index) != 0),
        }
    }

    pub fn load_if_changed(&self, seen_generation: &mut u32, into: &mut ParamValues) -> bool {
        let generation = self.generation();
        if generation == *seen_generation {
            return false;
        }
        *seen_generation = generation;
        *into = self.snapshot();
        true
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }
}

fn bypass_bits(values: &ParamValues) -> u32 {
    values
        .bypasses()
        .filter(|&(_, bypassed)| bypassed)
        .fold(0, |bits, (stage, _)| bits | (1u32 << stage.index()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub stages: &'static [StageSpec],
    pub params: &'static [ParamSpec],
}

pub fn schema() -> Schema {
    Schema {
        stages: &STAGES,
        params: &SPECS,
    }
}

impl Serialize for ParamId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for ParamId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(KeyVisitor {
            kind: "parameter",
            lookup: ParamId::from_key,
        })
    }
}

impl Serialize for StageId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for StageId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(KeyVisitor {
            kind: "stage",
            lookup: StageId::from_key,
        })
    }
}

struct KeyVisitor<T> {
    kind: &'static str,
    lookup: fn(&str) -> Option<T>,
}

impl<T> Visitor<'_> for KeyVisitor<T> {
    type Value = T;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "a known {} key", self.kind)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
        (self.lookup)(value).ok_or_else(|| E::custom(format!("unknown {} key: {value}", self.kind)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_line_up_with_their_ids() {
        for (index, id) in ParamId::ALL.iter().enumerate() {
            assert_eq!(id.index(), index);
            assert_eq!(SPECS[index].id, *id);
        }
        for (index, id) in StageId::ALL.iter().enumerate() {
            assert_eq!(id.index(), index);
            assert_eq!(STAGES[index].id, *id);
        }
    }

    #[test]
    fn every_spec_is_self_consistent_and_keys_are_unique() {
        let mut keys = std::collections::HashSet::new();
        for spec in SPECS {
            assert!(spec.min < spec.max, "{}", spec.key);
            assert!(
                spec.default >= spec.min && spec.default <= spec.max,
                "{}",
                spec.key
            );
            assert!(spec.step > 0.0 && spec.step <= spec.coarse, "{}", spec.key);
            assert!(keys.insert(spec.key), "duplicate key {}", spec.key);
            if spec.scale == Scale::Logarithmic {
                assert!(
                    spec.min > 0.0 || spec.unit == Unit::Milliseconds,
                    "{}",
                    spec.key
                );
            }
        }
        let mut stage_keys = std::collections::HashSet::new();
        for spec in STAGES {
            assert!(stage_keys.insert(spec.key), "duplicate stage {}", spec.key);
            assert!(!spec.optional || spec.bypassable, "{}", spec.key);
        }
    }

    #[test]
    fn main_settings_come_before_detail_settings_within_a_stage() {
        for stage in StageId::ALL {
            let tiers: Vec<Tier> = SPECS
                .iter()
                .filter(|spec| spec.stage == stage)
                .map(|spec| spec.tier)
                .collect();
            let first_detail = tiers.iter().position(|&tier| tier == Tier::Detail);
            if let Some(position) = first_detail {
                assert!(
                    tiers[position..].iter().all(|&tier| tier == Tier::Detail),
                    "{}",
                    stage.key()
                );
            }
        }
    }

    #[test]
    fn values_clamp_to_the_spec_and_reject_non_finite_numbers() {
        let mut values = ParamValues::default();
        values.set(ParamId::HighPassCutoff, 10_000.0);
        assert_eq!(values.get(ParamId::HighPassCutoff), 300.0);
        values.set(ParamId::HighPassCutoff, f32::NAN);
        assert_eq!(values.get(ParamId::HighPassCutoff), 90.0);
        values.set_bypassed(StageId::Input, true);
        assert!(!values.is_bypassed(StageId::Input));
        values.set_bypassed(StageId::Expander, true);
        assert!(values.is_bypassed(StageId::Expander));
    }

    #[test]
    fn optional_stages_start_bypassed_and_core_stages_start_on() {
        let values = ParamValues::default();
        assert!(values.is_bypassed(StageId::NoiseSuppressor));
        assert!(!values.is_bypassed(StageId::Compressor));
        assert!(!values.is_bypassed(StageId::Input));
    }

    #[test]
    fn shared_params_round_trip_through_atomics_with_a_generation() {
        let params = Params::new(&ParamValues::default());
        let mut seen = params.generation();
        let mut local = ParamValues::default();
        assert!(!params.load_if_changed(&mut seen, &mut local));

        params.set(ParamId::CompressorRatio, 4.0);
        params.set_bypassed(StageId::Limiter, true);
        params.set_bypassed(StageId::NoiseSuppressor, false);
        assert!(params.load_if_changed(&mut seen, &mut local));
        assert_eq!(local.get(ParamId::CompressorRatio), 4.0);
        assert!(local.is_bypassed(StageId::Limiter));
        assert!(!local.is_bypassed(StageId::NoiseSuppressor));
        assert!(!params.load_if_changed(&mut seen, &mut local));

        params.set_bypassed(StageId::Limiter, false);
        assert!(!params.snapshot().is_bypassed(StageId::Limiter));
    }

    #[test]
    fn ids_serialize_as_their_keys() {
        let json = serde_json::to_string(&ParamId::LowMidQ).expect("json");
        assert_eq!(json, "\"eq.lowMid.q\"");
        let parsed: ParamId = serde_json::from_str("\"limiter.ceiling\"").expect("parse");
        assert_eq!(parsed, ParamId::LimiterCeiling);
        assert!(serde_json::from_str::<ParamId>("\"nope\"").is_err());
        let stage: StageId = serde_json::from_str("\"rnnoise\"").expect("stage");
        assert_eq!(stage, StageId::NoiseSuppressor);
    }
}
