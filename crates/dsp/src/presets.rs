use crate::params::{ParamId, ParamValues, StageId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub values: ParamValues,
}

pub const FLAT: &str = "Flat";
pub const HEADSET_CLEANUP: &str = "Headset clean-up";
pub const PODCAST: &str = "Podcast";
pub const BROADCAST: &str = "Broadcast";
pub const LATE_NIGHT: &str = "Late night";

pub fn builtin() -> Vec<Preset> {
    vec![
        Preset {
            name: FLAT,
            values: flat(),
        },
        Preset {
            name: HEADSET_CLEANUP,
            values: ParamValues::default(),
        },
        Preset {
            name: PODCAST,
            values: tuned(&[
                (ParamId::ExpanderThreshold, -50.0),
                (ParamId::ExpanderRatio, 2.0),
                (ParamId::DeEsserThreshold, -26.0),
                (ParamId::PresenceGain, 1.5),
                (ParamId::CompressorThreshold, -20.0),
                (ParamId::CompressorRatio, 2.5),
                (ParamId::CompressorMakeup, 5.0),
            ]),
        },
        Preset {
            name: BROADCAST,
            values: tuned(&[
                (ParamId::HighPassCutoff, 100.0),
                (ParamId::PresenceGain, 3.5),
                (ParamId::CompressorThreshold, -16.0),
                (ParamId::CompressorRatio, 4.0),
                (ParamId::CompressorMakeup, 8.0),
                (ParamId::LimiterCeiling, -0.5),
            ]),
        },
        Preset {
            name: LATE_NIGHT,
            values: tuned(&[
                (ParamId::InputGain, 6.0),
                (ParamId::ExpanderThreshold, -55.0),
                (ParamId::CompressorThreshold, -24.0),
                (ParamId::CompressorRatio, 4.0),
                (ParamId::CompressorMakeup, 10.0),
            ]),
        },
    ]
}

pub fn find(name: &str) -> Option<Preset> {
    builtin().into_iter().find(|preset| preset.name == name)
}

fn flat() -> ParamValues {
    let mut values = ParamValues::default();
    for stage in StageId::ALL {
        values.set_bypassed(stage, true);
    }
    values
}

fn tuned(overrides: &[(ParamId, f32)]) -> ParamValues {
    let mut values = ParamValues::default();
    for &(id, value) in overrides {
        values.set(id, value);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_findable() {
        let presets = builtin();
        let mut names: Vec<&str> = presets.iter().map(|preset| preset.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), presets.len());
        assert_eq!(find(BROADCAST).map(|preset| preset.name), Some(BROADCAST));
        assert!(find("nope").is_none());
    }

    #[test]
    fn flat_bypasses_every_effect_but_keeps_the_gains() {
        let flat = find(FLAT).expect("flat preset").values;
        for stage in StageId::ALL {
            assert_eq!(flat.is_bypassed(stage), stage.spec().bypassable);
        }
        assert_eq!(flat.get(ParamId::InputGain), 0.0);
    }

    #[test]
    fn tuned_presets_stay_inside_the_parameter_ranges() {
        for preset in builtin() {
            for (id, value) in preset.values.entries() {
                assert_eq!(value, id.clamp(value), "{} in {}", id.key(), preset.name);
            }
        }
    }
}
