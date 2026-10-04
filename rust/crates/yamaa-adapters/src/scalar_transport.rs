//! Lossless, versioned scalar transport for installed-host boundary qualification.
//! JSON belongs in adapters; neither core values nor engine APIs depend on serde.

use serde::{Deserialize, Serialize};
use std::{fmt, panic::catch_unwind};
use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
use yamaa_core::value::Value;

/// Bound bytes before JSON parsing; this is transport policy, not a language limit.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const PROTOCOL: &str = "scalar/1";

/// Stable boundary categories; messages deliberately do not reproduce input data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    RequestLimit,
    InvalidEnvelope,
    UnsupportedProtocol,
    InvalidScalar,
    Internal,
}

impl fmt::Display for TransportError {
    /// Supply a stable host error message without input or panic payload disclosure.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RequestLimit => "scalar transport request exceeds byte limit",
            Self::InvalidEnvelope => "invalid scalar transport envelope",
            Self::UnsupportedProtocol => "unsupported scalar transport protocol",
            Self::InvalidScalar => "invalid scalar transport value",
            Self::Internal => "internal scalar transport failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    protocol: String,
    value: Scalar,
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
    fn into_core(self) -> Result<Value, TransportError> {
        let invalid = TransportError::InvalidScalar;
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

/// Contain unwind panics without translating them into ordinary language conditions.
/// Process aborts and allocator exhaustion are not recoverable by this boundary.
fn contain(
    operation: impl FnOnce() -> Result<String, TransportError> + std::panic::UnwindSafe,
) -> Result<String, TransportError> {
    catch_unwind(operation).unwrap_or(Err(TransportError::Internal))
}

/// Decode one strict envelope, construct a core value and return owned JSON text.
/// This prototype proves scalar transport only; it evaluates no specifications,
/// invokes no callbacks and does not claim Arrow ownership or dataset support.
pub fn scalar_round_trip(request: &str) -> Result<String, TransportError> {
    contain(|| {
        if request.len() > MAX_REQUEST_BYTES {
            return Err(TransportError::RequestLimit);
        }
        let envelope: Envelope =
            serde_json::from_str(request).map_err(|_| TransportError::InvalidEnvelope)?;
        if envelope.protocol != PROTOCOL {
            return Err(TransportError::UnsupportedProtocol);
        }
        let value = envelope.value.into_core()?;
        serde_json::to_string(&Envelope {
            protocol: PROTOCOL.into(),
            value: Scalar::from_core(value),
        })
        .map_err(|_| TransportError::Internal)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A panic payload is not returned as data or exposed as a host error message.
    #[test]
    fn panic_is_a_distinct_boundary_failure() {
        assert_eq!(
            contain(|| panic!("private payload")),
            Err(TransportError::Internal)
        );
    }

    /// Test decoded core fields independently of the inverse transport implementation.
    #[test]
    fn core_values_are_lossless_and_not_json_number_coercions() {
        assert_eq!(
            Scalar::Int("-9223372036854775808".into())
                .into_core()
                .unwrap(),
            Value::Int(i64::MIN)
        );
        assert_eq!(
            Scalar::Int("9223372036854775807".into())
                .into_core()
                .unwrap(),
            Value::Int(i64::MAX)
        );
        assert_eq!(
            Scalar::Int("9007199254740993".into()).into_core().unwrap(),
            Value::Int(9_007_199_254_740_993)
        );
        let Value::Float(value) = Scalar::Float("8000000000000000".into())
            .into_core()
            .unwrap()
        else {
            panic!("finite float")
        };
        assert_eq!(value.get().to_bits(), (-0.0_f64).to_bits());
        let Value::Date(value) = Scalar::Date(Temporal {
            text: "2024-02-29".into(),
            precision: DateCollected::Year,
        })
        .into_core()
        .unwrap() else {
            panic!("civil date")
        };
        assert_eq!(value.fields(), (2024, 2, 29));
        assert_eq!(value.collected_precision(), DatePrecision::Year);
        let Value::DateTime(value) = Scalar::Datetime(Temporal {
            text: "2024-02-29T23:59:59".into(),
            precision: DateTimeCollected::Day,
        })
        .into_core()
        .unwrap() else {
            panic!("civil datetime")
        };
        assert_eq!(value.fields(), (2024, 2, 29, 23, 59, 59));
        assert_eq!(value.collected_precision(), DateTimePrecision::Day);
    }
}
