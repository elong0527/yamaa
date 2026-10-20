//! Lossless values in retained report observations; no request protocol.
use yamaa_core::temporal::Precision;
use yamaa_core::value::Value;

/// Represent retained scalar facts without host numeric coercion.
pub(crate) struct ScalarValue(Scalar);

impl ScalarValue {
    /// Encode a core value using the lossless scalar representation.
    pub(crate) fn from_core(value: Value) -> Self {
        Self(Scalar::from_core(value))
    }

    /// Project to the owned JSON tree: one key naming the type, holding the
    /// value's exact text, its bits or its temporal text and precision.
    pub(crate) fn to_json(&self) -> yamaa_core::json::Value {
        use yamaa_core::json::Value as Json;
        let (key, value) = match &self.0 {
            Scalar::Missing(()) => ("missing", Json::Null),
            Scalar::Int(text) => ("int", Json::String(text.clone())),
            Scalar::Float(text) => ("float", Json::String(text.clone())),
            Scalar::Str(text) => ("str", Json::String(text.clone())),
            Scalar::Bool(value) => ("bool", Json::Bool(*value)),
            Scalar::Date(temporal) => (
                "date",
                Json::Object(
                    [
                        ("text".into(), Json::String(temporal.text.clone())),
                        (
                            "precision".into(),
                            Json::String(temporal.precision.word().into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            Scalar::Datetime(temporal) => (
                "datetime",
                Json::Object(
                    [
                        ("text".into(), Json::String(temporal.text.clone())),
                        (
                            "precision".into(),
                            Json::String(temporal.precision.word().into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        };
        Json::Object([(key.into(), value)].into_iter().collect())
    }
}

/// Decimal integer strings avoid R/JSON number narrowing; float strings carry bits.
enum Scalar {
    Missing(()),
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    Date(Temporal),
    Datetime(Temporal),
}

struct Temporal {
    text: String,
    precision: Precision,
}

impl Scalar {
    /// Encode an actual normalized core value while retaining temporal metadata.
    fn from_core(value: Value) -> Self {
        match value {
            Value::Missing => Self::Missing(()),
            Value::Int(value) => Self::Int(value.to_string()),
            Value::Float(value) => Self::Float(format!("{:016x}", value.get().to_bits())),
            Value::Str(value) => Self::Str(value),
            Value::Bool(value) => Self::Bool(value),
            Value::Date(value) => Self::Date(Temporal {
                text: value.to_string(),
                precision: value.collected_precision(),
            }),
            Value::DateTime(value) => Self::Datetime(Temporal {
                text: value.to_string(),
                precision: value.collected_precision(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::temporal::{Date, DateTime};

    #[test]
    fn hour_and_minute_datetime_precision_survive_the_json_projection() {
        let day = Date::new(2025, 3, 4, Precision::Day).unwrap();
        for (value, precision) in [
            (
                DateTime::new(day, 8, 59, 59, Precision::Hour).unwrap(),
                "hour",
            ),
            (
                DateTime::new(day, 8, 30, 0, Precision::Minute).unwrap(),
                "minute",
            ),
        ] {
            let json = ScalarValue::from_core(Value::DateTime(value)).to_json();
            assert_eq!(json["datetime"]["precision"].as_str(), Some(precision));
            assert_eq!(
                json["datetime"]["text"].as_str(),
                Some(value.to_string().as_str())
            );
        }
    }
}
