use std::sync::atomic::{AtomicU32, Ordering};

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StageId {
    Input,
    HighPass,
    Expander,
    DeEsser,
    Equalizer,
    Compressor,
    Limiter,
    Output,
}

impl StageId {
    pub const COUNT: usize = 8;
    pub const ALL: [StageId; Self::COUNT] = [
        StageId::Input,
        StageId::HighPass,
        StageId::Expander,
        StageId::DeEsser,
        StageId::Equalizer,
        StageId::Compressor,
        StageId::Limiter,
        StageId::Output,
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageSpec {
    pub id: StageId,
    pub key: &'static str,
    pub label: &'static str,
    pub bypassable: bool,
}

const fn stage(id: StageId, key: &'static str, label: &'static str, bypassable: bool) -> StageSpec {
    StageSpec {
        id,
        key,
        label,
        bypassable,
    }
}

pub const STAGES: [StageSpec; StageId::COUNT] = [
    stage(StageId::Input, "input", "INPUT", false),
    stage(StageId::HighPass, "hpf", "HIGH-PASS", true),
    stage(StageId::Expander, "expander", "EXPANDER", true),
    stage(StageId::DeEsser, "deesser", "DE-ESSER", true),
    stage(StageId::Equalizer, "eq", "EQ", true),
    stage(StageId::Compressor, "compressor", "COMPRESSOR", true),
    stage(StageId::Limiter, "limiter", "LIMITER", true),
    stage(StageId::Output, "output", "OUTPUT", false),
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSpec {
    pub id: ParamId,
    pub stage: StageId,
    pub key: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    pub scale: Scale,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamId {
    InputGain,
    HighPassCutoff,
    ExpanderThreshold,
    ExpanderRatio,
    ExpanderRange,
    ExpanderAttack,
    ExpanderHold,
    ExpanderRelease,
    ExpanderKnee,
    DeEsserFrequency,
    DeEsserThreshold,
    DeEsserRatio,
    DeEsserSplitBand,
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
    CompressorKnee,
    CompressorAttack,
    CompressorRelease,
    CompressorMakeup,
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
        ParamId::ExpanderRatio,
        ParamId::ExpanderRange,
        ParamId::ExpanderAttack,
        ParamId::ExpanderHold,
        ParamId::ExpanderRelease,
        ParamId::ExpanderKnee,
        ParamId::DeEsserFrequency,
        ParamId::DeEsserThreshold,
        ParamId::DeEsserRatio,
        ParamId::DeEsserSplitBand,
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
        ParamId::CompressorKnee,
        ParamId::CompressorAttack,
        ParamId::CompressorRelease,
        ParamId::CompressorMakeup,
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
}

const fn db(min: f32, max: f32, step: f32) -> Range {
    Range {
        unit: Unit::Decibels,
        scale: Scale::Linear,
        min,
        max,
        step,
    }
}

const fn hz(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Hertz,
        scale: Scale::Logarithmic,
        min,
        max,
        step: 1.0,
    }
}

const fn ms(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Milliseconds,
        scale: Scale::Logarithmic,
        min,
        max,
        step: 0.1,
    }
}

const fn ratio(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Ratio,
        scale: Scale::Linear,
        min,
        max,
        step: 0.1,
    }
}

const fn q(min: f32, max: f32) -> Range {
    Range {
        unit: Unit::Q,
        scale: Scale::Logarithmic,
        min,
        max,
        step: 0.05,
    }
}

const fn switch() -> Range {
    Range {
        unit: Unit::Switch,
        scale: Scale::Linear,
        min: 0.0,
        max: 1.0,
        step: 1.0,
    }
}

const fn param(
    id: ParamId,
    stage: StageId,
    key: &'static str,
    label: &'static str,
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
        min: range.min,
        max: range.max,
        default,
        step: range.step,
    }
}

pub const SPECS: [ParamSpec; ParamId::COUNT] = [
    param(
        ParamId::InputGain,
        StageId::Input,
        "input.gain",
        "Gain",
        db(-24.0, 24.0, 0.5),
        0.0,
    ),
    param(
        ParamId::HighPassCutoff,
        StageId::HighPass,
        "hpf.cutoff",
        "Cutoff",
        hz(40.0, 300.0),
        90.0,
    ),
    param(
        ParamId::ExpanderThreshold,
        StageId::Expander,
        "expander.threshold",
        "Threshold",
        db(-80.0, -20.0, 0.5),
        -45.0,
    ),
    param(
        ParamId::ExpanderRatio,
        StageId::Expander,
        "expander.ratio",
        "Ratio",
        ratio(1.0, 8.0),
        2.5,
    ),
    param(
        ParamId::ExpanderRange,
        StageId::Expander,
        "expander.range",
        "Range",
        db(0.0, 60.0, 1.0),
        30.0,
    ),
    param(
        ParamId::ExpanderAttack,
        StageId::Expander,
        "expander.attack",
        "Attack",
        ms(0.1, 50.0),
        2.0,
    ),
    param(
        ParamId::ExpanderHold,
        StageId::Expander,
        "expander.hold",
        "Hold",
        ms(0.0, 500.0),
        80.0,
    ),
    param(
        ParamId::ExpanderRelease,
        StageId::Expander,
        "expander.release",
        "Release",
        ms(10.0, 1000.0),
        150.0,
    ),
    param(
        ParamId::ExpanderKnee,
        StageId::Expander,
        "expander.knee",
        "Knee",
        db(0.0, 12.0, 0.5),
        6.0,
    ),
    param(
        ParamId::DeEsserFrequency,
        StageId::DeEsser,
        "deesser.frequency",
        "Frequency",
        hz(3000.0, 10000.0),
        6500.0,
    ),
    param(
        ParamId::DeEsserThreshold,
        StageId::DeEsser,
        "deesser.threshold",
        "Threshold",
        db(-60.0, 0.0, 0.5),
        -30.0,
    ),
    param(
        ParamId::DeEsserRatio,
        StageId::DeEsser,
        "deesser.ratio",
        "Ratio",
        ratio(1.0, 10.0),
        4.0,
    ),
    param(
        ParamId::DeEsserSplitBand,
        StageId::DeEsser,
        "deesser.splitBand",
        "Split band",
        switch(),
        1.0,
    ),
    param(
        ParamId::LowShelfFrequency,
        StageId::Equalizer,
        "eq.lowShelf.frequency",
        "Low shelf",
        hz(40.0, 400.0),
        120.0,
    ),
    param(
        ParamId::LowShelfGain,
        StageId::Equalizer,
        "eq.lowShelf.gain",
        "Low shelf gain",
        db(-12.0, 12.0, 0.5),
        0.0,
    ),
    param(
        ParamId::LowMidFrequency,
        StageId::Equalizer,
        "eq.lowMid.frequency",
        "Low mid",
        hz(100.0, 1000.0),
        250.0,
    ),
    param(
        ParamId::LowMidGain,
        StageId::Equalizer,
        "eq.lowMid.gain",
        "Low mid gain",
        db(-12.0, 12.0, 0.5),
        -2.5,
    ),
    param(
        ParamId::LowMidQ,
        StageId::Equalizer,
        "eq.lowMid.q",
        "Low mid Q",
        q(0.3, 4.0),
        1.0,
    ),
    param(
        ParamId::PresenceFrequency,
        StageId::Equalizer,
        "eq.presence.frequency",
        "Presence",
        hz(1500.0, 8000.0),
        4000.0,
    ),
    param(
        ParamId::PresenceGain,
        StageId::Equalizer,
        "eq.presence.gain",
        "Presence gain",
        db(-12.0, 12.0, 0.5),
        2.5,
    ),
    param(
        ParamId::PresenceQ,
        StageId::Equalizer,
        "eq.presence.q",
        "Presence Q",
        q(0.3, 4.0),
        0.8,
    ),
    param(
        ParamId::HighShelfFrequency,
        StageId::Equalizer,
        "eq.highShelf.frequency",
        "High shelf",
        hz(4000.0, 16000.0),
        10000.0,
    ),
    param(
        ParamId::HighShelfGain,
        StageId::Equalizer,
        "eq.highShelf.gain",
        "High shelf gain",
        db(-12.0, 12.0, 0.5),
        1.0,
    ),
    param(
        ParamId::CompressorThreshold,
        StageId::Compressor,
        "compressor.threshold",
        "Threshold",
        db(-60.0, 0.0, 0.5),
        -18.0,
    ),
    param(
        ParamId::CompressorRatio,
        StageId::Compressor,
        "compressor.ratio",
        "Ratio",
        ratio(1.0, 20.0),
        3.0,
    ),
    param(
        ParamId::CompressorKnee,
        StageId::Compressor,
        "compressor.knee",
        "Knee",
        db(0.0, 12.0, 0.5),
        6.0,
    ),
    param(
        ParamId::CompressorAttack,
        StageId::Compressor,
        "compressor.attack",
        "Attack",
        ms(0.1, 100.0),
        8.0,
    ),
    param(
        ParamId::CompressorRelease,
        StageId::Compressor,
        "compressor.release",
        "Release",
        ms(10.0, 1000.0),
        120.0,
    ),
    param(
        ParamId::CompressorMakeup,
        StageId::Compressor,
        "compressor.makeup",
        "Makeup",
        db(0.0, 24.0, 0.5),
        6.0,
    ),
    param(
        ParamId::LimiterCeiling,
        StageId::Limiter,
        "limiter.ceiling",
        "Ceiling",
        db(-6.0, 0.0, 0.1),
        -1.0,
    ),
    param(
        ParamId::LimiterRelease,
        StageId::Limiter,
        "limiter.release",
        "Release",
        ms(10.0, 500.0),
        60.0,
    ),
    param(
        ParamId::OutputGain,
        StageId::Output,
        "output.gain",
        "Gain",
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
            bypassed: [false; StageId::COUNT],
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

    pub fn bypassed_stages(&self) -> impl Iterator<Item = StageId> + '_ {
        StageId::ALL
            .iter()
            .copied()
            .filter(move |&stage| self.is_bypassed(stage))
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
        .bypassed_stages()
        .fold(0, |bits, stage| bits | (1u32 << stage.index()))
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
            assert!(spec.step > 0.0, "{}", spec.key);
            assert!(keys.insert(spec.key), "duplicate key {}", spec.key);
            if spec.scale == Scale::Logarithmic {
                assert!(
                    spec.min > 0.0 || spec.unit == Unit::Milliseconds,
                    "{}",
                    spec.key
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
    fn shared_params_round_trip_through_atomics_with_a_generation() {
        let params = Params::new(&ParamValues::default());
        let mut seen = params.generation();
        let mut local = ParamValues::default();
        assert!(!params.load_if_changed(&mut seen, &mut local));

        params.set(ParamId::CompressorRatio, 4.0);
        params.set_bypassed(StageId::Limiter, true);
        assert!(params.load_if_changed(&mut seen, &mut local));
        assert_eq!(local.get(ParamId::CompressorRatio), 4.0);
        assert!(local.is_bypassed(StageId::Limiter));
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
        let stage: StageId = serde_json::from_str("\"deesser\"").expect("stage");
        assert_eq!(stage, StageId::DeEsser);
    }
}
