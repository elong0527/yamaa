//! Lossless values in retained report observations; no request protocol.
use serde::{Deserialize, Serialize};

use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
use yamaa_core::value::Value;

/// Represent retained scalar facts without host numeric coercion.
#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub(crate) struct ScalarValue(Scalar);

impl ScalarValue {
    /// Decode to a normalized core value without exposing wire internals.
    pub(crate) fn into_core(self) -> Result<Value, Error> {
        self.0.into_core()
    }

    /// Encode a core value using the same lossless scalar representation.
    pub(crate) fn from_core(value: Value) -> Self {
        Self(Scalar::from_core(value))
    }

    /// Project to the owned JSON tree directly, in the same shape the serde
    /// representation produces, without a text round trip.
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
                            Json::String(
                                match temporal.precision {
                                    DateCollected::Year => "year",
                                    DateCollected::Month => "month",
                                    DateCollected::Day => "day",
                                }
                                .into(),
                            ),
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
                            Json::String(
                                match temporal.precision {
                                    DateTimeCollected::Day => "day",
                                    DateTimeCollected::Hour => "hour",
                                    DateTimeCollected::Minute => "minute",
                                    DateTimeCollected::Second => "second",
                                }
                                .into(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        };
        Json::Object([(key.into(), value)].into_iter().collect())
    }

    /// Admit an owned JSON tree in the serde representation's shape directly,
    /// without a text round trip.
    pub(crate) fn from_json(value: &yamaa_core::json::Value) -> Result<Self, Error> {
        use yamaa_core::json::Value as Json;
        let invalid = Error::InvalidScalar;
        let object = value.as_object().ok_or(invalid)?;
        if object.len() != 1 {
            return Err(invalid);
        }
        let (key, payload) = object.iter().next().unwrap();
        let temporal = |payload: &Json| -> Result<(String, String), Error> {
            let fields = payload.as_object().ok_or(invalid)?;
            if fields.len() != 2 {
                return Err(invalid);
            }
            let text = fields
                .get("text")
                .and_then(Json::as_str)
                .ok_or(invalid)?
                .to_owned();
            let precision = fields
                .get("precision")
                .and_then(Json::as_str)
                .ok_or(invalid)?
                .to_owned();
            Ok((text, precision))
        };
        let scalar = match (key.as_str(), payload) {
            ("missing", Json::Null) => Scalar::Missing(()),
            ("int", Json::String(text)) => Scalar::Int(text.clone()),
            ("float", Json::String(text)) => Scalar::Float(text.clone()),
            ("str", Json::String(text)) => Scalar::Str(text.clone()),
            ("bool", Json::Bool(value)) => Scalar::Bool(*value),
            ("date", payload) => {
                let (text, precision) = temporal(payload)?;
                Scalar::Date(Temporal {
                    text,
                    precision: match precision.as_str() {
                        "year" => DateCollected::Year,
                        "month" => DateCollected::Month,
                        "day" => DateCollected::Day,
                        _ => return Err(invalid),
                    },
                })
            }
            ("datetime", payload) => {
                let (text, precision) = temporal(payload)?;
                Scalar::Datetime(Temporal {
                    text,
                    precision: match precision.as_str() {
                        "day" => DateTimeCollected::Day,
                        "hour" => DateTimeCollected::Hour,
                        "minute" => DateTimeCollected::Minute,
                        "second" => DateTimeCollected::Second,
                        _ => return Err(invalid),
                    },
                })
            }
            _ => return Err(invalid),
        };
        Ok(Self(scalar))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Error {
    InvalidScalar,
}
/// Decimal integer strings avoid R/JSON number narrowing; float strings carry bits.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Scalar {
    Missing(()),
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    Date(Temporal<DateCollected>),
    Datetime(Temporal<DateTimeCollected>),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Temporal<P> {
    text: String,
    precision: P,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum DateCollected {
    Year,
    Month,
    Day,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum DateTimeCollected {
    Day,
    Hour,
    Minute,
    Second,
}

impl Scalar {
    /// Validate transport spelling, construct core values and normalize nonfinite floats.
    fn into_core(self) -> Result<Value, Error> {
        let invalid = Error::InvalidScalar;
        Ok(match self {
            Self::Missing(()) => Value::Missing,
            Self::Int(text) => {
                let value = text.parse::<i64>().map_err(|_| invalid)?;
                if value.to_string() != text {
                    return Err(invalid);
                }
                Value::Int(value)
            }
            Self::Float(text) => {
                if text.len() != 16
                    || !text
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(invalid);
                }
                let bits = u64::from_str_radix(&text, 16).map_err(|_| invalid)?;
                Value::float(f64::from_bits(bits))
            }
            Self::Str(text) => Value::Str(text),
            Self::Bool(value) => Value::Bool(value),
            Self::Date(Temporal { text, precision }) => {
                let date: Date = text.parse().map_err(|_| invalid)?;
                let (year, month, day) = date.fields();
                let precision = match precision {
                    DateCollected::Year => DatePrecision::Year,
                    DateCollected::Month => DatePrecision::Month,
                    DateCollected::Day => DatePrecision::Day,
                };
                Value::Date(Date::new(year, month, day, precision).map_err(|_| invalid)?)
            }
            Self::Datetime(Temporal { text, precision }) => {
                let date: DateTime = text.parse().map_err(|_| invalid)?;
                if date.to_string() != text {
                    return Err(invalid);
                }
                let (year, month, day, hour, minute, second) = date.fields();
                let precision = match precision {
                    DateTimeCollected::Day => DateTimePrecision::Day,
                    DateTimeCollected::Hour => DateTimePrecision::Hour,
                    DateTimeCollected::Minute => DateTimePrecision::Minute,
                    DateTimeCollected::Second => DateTimePrecision::Second,
                };
                let date = Date::new(year, month, day, DatePrecision::Day).map_err(|_| invalid)?;
                Value::DateTime(
                    DateTime::new(date, hour, minute, second, precision).map_err(|_| invalid)?,
                )
            }
        })
    }

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
                precision: match value.collected_precision() {
                    DatePrecision::Year => DateCollected::Year,
                    DatePrecision::Month => DateCollected::Month,
                    DatePrecision::Day => DateCollected::Day,
                },
            }),
            Value::DateTime(value) => Self::Datetime(Temporal {
                text: value.to_string(),
                precision: match value.collected_precision() {
                    DateTimePrecision::Day => DateTimeCollected::Day,
                    DateTimePrecision::Hour => DateTimeCollected::Hour,
                    DateTimePrecision::Minute => DateTimeCollected::Minute,
                    DateTimePrecision::Second => DateTimeCollected::Second,
                },
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_and_minute_datetime_precision_survive_the_json_projection() {
        let day = Date::new(2025, 3, 4, DatePrecision::Day).unwrap();
        for (value, precision) in [
            (
                DateTime::new(day, 8, 59, 59, DateTimePrecision::Hour).unwrap(),
                "hour",
            ),
            (
                DateTime::new(day, 8, 30, 0, DateTimePrecision::Minute).unwrap(),
                "minute",
            ),
        ] {
            let json = ScalarValue::from_core(Value::DateTime(value)).to_json();
            assert_eq!(json["datetime"]["precision"].as_str(), Some(precision));
            let Value::DateTime(decoded) =
                ScalarValue::from_json(&json).unwrap().into_core().unwrap()
            else {
                panic!("datetime required")
            };
            assert_eq!(decoded.fields(), value.fields());
            assert_eq!(decoded.collected_precision(), value.collected_precision());
        }
    }
}
