use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::params::{ParamId, ParamValues, StageId};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParamDocument {
    pub values: BTreeMap<String, f32>,
    pub bypassed: Vec<String>,
}

impl From<&ParamValues> for ParamDocument {
    fn from(values: &ParamValues) -> Self {
        Self {
            values: values
                .entries()
                .map(|(id, value)| (id.key().to_owned(), value))
                .collect(),
            bypassed: values
                .bypassed_stages()
                .map(|stage| stage.key().to_owned())
                .collect(),
        }
    }
}

impl From<&ParamDocument> for ParamValues {
    fn from(document: &ParamDocument) -> Self {
        let mut values = ParamValues::default();
        for (key, value) in &document.values {
            if let Some(id) = ParamId::from_key(key) {
                values.set(id, *value);
            }
        }
        for key in &document.bypassed {
            if let Some(stage) = StageId::from_key(key) {
                values.set_bypassed(stage, true);
            }
        }
        values
    }
}

impl Serialize for ParamValues {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ParamDocument::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ParamValues {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ParamDocument::deserialize(deserializer).map(|document| ParamValues::from(&document))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_through_json_by_key() {
        let mut values = ParamValues::default();
        values.set(ParamId::CompressorRatio, 4.5);
        values.set_bypassed(StageId::Limiter, true);

        let json = serde_json::to_string(&values).expect("json");
        assert!(json.contains("\"compressor.ratio\":4.5"));
        assert!(json.contains("\"bypassed\":[\"limiter\"]"));

        let parsed: ParamValues = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, values);
    }

    #[test]
    fn unknown_keys_are_ignored_and_missing_keys_take_defaults() {
        let parsed: ParamValues = serde_json::from_str(
            r#"{"values":{"hpf.cutoff":120,"nope":1,"eq.lowMid.gain":-99},"bypassed":["ghost","eq"]}"#,
        )
        .expect("parse");

        assert_eq!(parsed.get(ParamId::HighPassCutoff), 120.0);
        assert_eq!(parsed.get(ParamId::LowMidGain), -12.0);
        assert_eq!(parsed.get(ParamId::CompressorRatio), 3.0);
        assert!(parsed.is_bypassed(StageId::Equalizer));
        assert!(!parsed.is_bypassed(StageId::Limiter));
    }

    #[test]
    fn an_empty_document_is_the_default_chain() {
        let parsed: ParamValues = serde_json::from_str("{}").expect("parse");
        assert_eq!(parsed, ParamValues::default());
    }
}
