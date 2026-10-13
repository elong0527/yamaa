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
                    DateTimePrecision::Second => DateTimeCollected::Second,
                },
            }),
        }
    }
}
