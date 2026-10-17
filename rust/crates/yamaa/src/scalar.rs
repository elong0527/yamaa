//! Facade-owned scalar values exchanged with project functions.
//
//! This is the public form of `yamaa_core::value::Value` (owned) and
//! `yamaa_core::table::ValueRef` (borrowed). There is no boolean column type
//! in the public schema vocabulary, but booleans remain valid function
//! scalars. Nonfinite binary64 values normalize to missing at the boundary,
//! exactly as the engine rule does. Temporal values carry whole days and
//! whole seconds; collected precision is dropped only here.

/// An owned facade scalar, the public form of the core runtime value.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Missing,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl Scalar {
    /// Normalize a host binary64 value to a finite float or the missing value.
    pub fn float(value: f64) -> Self {
        if value.is_finite() {
            Self::Float(value)
        } else {
            Self::Missing
        }
    }

    pub(crate) fn from_core(value: yamaa_core::value::Value) -> Self {
        use yamaa_core::value::Value;
        match value {
            Value::Missing => Self::Missing,
            Value::Str(value) => Self::Str(value),
            Value::Int(value) => Self::Int(value),
            Value::Float(value) => Self::Float(value.get()),
            Value::Bool(value) => Self::Bool(value),
            Value::Date(value) => Self::Date(Date::from_core(value)),
            Value::DateTime(value) => Self::DateTime(DateTime::from_core(value)),
        }
    }

    pub(crate) fn into_core(self) -> yamaa_core::value::Value {
        use yamaa_core::value::Value;
        match self {
            Self::Missing => Value::Missing,
            Self::Str(value) => Value::Str(value),
            Self::Int(value) => Value::Int(value),
            Self::Float(value) => Value::float(value),
            Self::Bool(value) => Value::Bool(value),
            Self::Date(value) => Value::Date(value.into_core()),
            Self::DateTime(value) => Value::DateTime(value.into_core()),
        }
    }
}

/// A borrowed facade scalar view, the public form of the core cell view.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalarRef<'a> {
    Missing,
    Str(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl<'a> ScalarRef<'a> {
    pub(crate) fn from_core(value: yamaa_core::table::ValueRef<'a>) -> Self {
        use yamaa_core::table::ValueRef;
        match value {
            ValueRef::Missing => Self::Missing,
            ValueRef::Str(value) => Self::Str(value),
            ValueRef::Int(value) => Self::Int(value),
            ValueRef::Float(value) => Self::Float(value.get()),
            ValueRef::Bool(value) => Self::Bool(value),
            ValueRef::Date(value) => Self::Date(Date::from_core(value)),
            ValueRef::DateTime(value) => Self::DateTime(DateTime::from_core(value)),
        }
    }

    /// Borrow an owned facade scalar while retaining exact representation.
    /// Hosts use this view to feed owned batch values back into invocation.
    pub fn from_owned(value: &'a Scalar) -> Self {
        match value {
            Scalar::Missing => Self::Missing,
            Scalar::Str(value) => Self::Str(value),
            Scalar::Int(value) => Self::Int(*value),
            Scalar::Float(value) => Self::Float(*value),
            Scalar::Bool(value) => Self::Bool(*value),
            Scalar::Date(value) => Self::Date(*value),
            Scalar::DateTime(value) => Self::DateTime(*value),
        }
    }
}

/// A facade civil date with day precision, the public form of the core date.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    inner: yamaa_core::temporal::Date,
}

impl Date {
    /// Validate complete civil fields with day precision.
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        yamaa_core::temporal::Date::new(year, month, day, yamaa_core::temporal::DatePrecision::Day)
            .ok()
            .map(|inner| Self { inner })
    }

    /// Return civil fields in most-significant-first chronological order.
    pub fn fields(self) -> (u16, u8, u8) {
        self.inner.fields()
    }

    pub(crate) fn from_core(value: yamaa_core::temporal::Date) -> Self {
        Self { inner: value }
    }

    pub(crate) fn into_core(self) -> yamaa_core::temporal::Date {
        self.inner
    }
}

/// A facade zone-free datetime with whole-second precision.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    inner: yamaa_core::temporal::DateTime,
}

impl DateTime {
    /// Validate complete civil fields with whole-second precision.
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Option<Self> {
        Date::new(year, month, day).and_then(|date| {
            yamaa_core::temporal::DateTime::new(
                date.inner,
                hour,
                minute,
                second,
                yamaa_core::temporal::DateTimePrecision::Second,
            )
            .ok()
            .map(|inner| Self { inner })
        })
    }

    /// Return civil fields in most-significant-first chronological order.
    pub fn fields(self) -> (u16, u8, u8, u8, u8, u8) {
        self.inner.fields()
    }

    pub(crate) fn from_core(value: yamaa_core::temporal::DateTime) -> Self {
        Self { inner: value }
    }

    pub(crate) fn into_core(self) -> yamaa_core::temporal::DateTime {
        self.inner
    }
}

/// A byte-carried host scalar: tag 0 missing, 1 int, 2 float, 3 str, 4 bool.
/// Tags 5/6 carry whole epoch days/seconds as exact binary64 little-endian
/// bytes. Integers use canonical ASCII decimal; float bits use eight
/// little-endian bytes. This is the public form of the adapter scalar bytes.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedScalar {
    pub tag: i32,
    pub payload: Vec<u8>,
}

impl EncodedScalar {
    /// Admit a byte payload with no locale or sentinel assumption.
    pub fn decode(tag: i32, bytes: &[u8]) -> Result<Scalar, ScalarDecodeError> {
        let value = yamaa_adapters::scalar_bytes::decode(tag, bytes)
            .map_err(|reason| ScalarDecodeError { reason })?;
        Ok(Scalar::from_core(value))
    }

    /// Encode a normalized host value; temporal precision is dropped only here.
    pub fn encode(value: Scalar) -> Self {
        let encoded = yamaa_adapters::scalar_bytes::encode(value.into_core());
        Self {
            tag: encoded.tag,
            payload: encoded.payload,
        }
    }
}

/// A scalar byte-payload admission failure, carrying its stable reason.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalarDecodeError {
    reason: &'static str,
}

impl ScalarDecodeError {
    /// Return the stable admission reason without host text.
    pub fn reason(self) -> &'static str {
        self.reason
    }
}

impl std::fmt::Display for ScalarDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason)
    }
}

impl std::error::Error for ScalarDecodeError {}

/// Convert an integer to binary64 only when the value is exactly retained.
pub fn exact_float(value: &Scalar) -> Result<Scalar, ScalarDecodeError> {
    let core = value.clone().into_core();
    let converted = yamaa_adapters::scalar_bytes::exact_float(&core)
        .map_err(|reason| ScalarDecodeError { reason })?;
    Ok(Scalar::from_core(converted))
}

/// Admit a closed infix/unary spelling before host operands are converted.
pub fn scalar_operator_code(name: &str, unary: bool) -> Option<i32> {
    yamaa_adapters::scalar_bytes::operator_code(name, unary)
}

/// Apply a closed scalar operator using shared numeric comparison semantics.
/// Codes: 1 +, 2 -, 3 *, 4 /, 5 MOD; 6 unary+, 7 unary-; 8 ==, 9 !=,
/// 10 <, 11 <=, 12 >, 13 >=. No vector recycling or implicit conversion.
pub fn apply_scalar_operator(
    code: i32,
    left: &Scalar,
    right: Option<&Scalar>,
) -> Result<Scalar, ScalarOperatorError> {
    let left = left.clone().into_core();
    let right = right.map(|value| value.clone().into_core());
    let applied = yamaa_adapters::scalar_bytes::operate(code, &left, right.as_ref())
        .map_err(|message| ScalarOperatorError { message })?;
    Ok(Scalar::from_core(applied))
}

/// A scalar operator failure, carrying its owned diagnostic message.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarOperatorError {
    message: String,
}

impl ScalarOperatorError {
    /// Return the owned operator failure message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ScalarOperatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ScalarOperatorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_normalizes_nonfinite_to_missing() {
        assert_eq!(Scalar::float(1.5), Scalar::Float(1.5));
        assert_eq!(Scalar::float(f64::NAN), Scalar::Missing);
        assert_eq!(Scalar::float(f64::INFINITY), Scalar::Missing);
        assert_eq!(Scalar::float(-0.0), Scalar::Float(-0.0));
    }

    #[test]
    fn scalar_round_trips_through_core() {
        let values = vec![
            Scalar::Missing,
            Scalar::Str("text".into()),
            Scalar::Int(-42),
            Scalar::Float(2.5),
            Scalar::Bool(true),
            Scalar::Date(Date::new(2024, 2, 29).unwrap()),
            Scalar::DateTime(DateTime::new(2024, 1, 31, 23, 59, 58).unwrap()),
        ];
        for value in values {
            let round = Scalar::from_core(value.clone().into_core());
            assert_eq!(round, value);
        }
        assert_eq!(
            Scalar::from_core(Scalar::Float(f64::NAN).into_core()),
            Scalar::Missing
        );
    }

    #[test]
    fn scalar_ref_views_borrow_owned_values() {
        let owned = Scalar::Str("borrowed".into());
        assert_eq!(ScalarRef::from_owned(&owned), ScalarRef::Str("borrowed"));
        let core = yamaa_core::value::Value::Int(7);
        assert_eq!(
            ScalarRef::from_core(yamaa_core::table::ValueRef::from(&core)),
            ScalarRef::Int(7)
        );
    }

    #[test]
    fn dates_validate_civil_fields() {
        assert!(Date::new(2024, 2, 29).is_some());
        assert!(Date::new(2023, 2, 29).is_none());
        assert!(Date::new(2024, 13, 1).is_none());
        assert!(Date::new(0, 1, 1).is_none());
        assert_eq!(Date::new(2024, 2, 29).unwrap().fields(), (2024, 2, 29));
        assert!(DateTime::new(2024, 1, 1, 24, 0, 0).is_none());
        assert_eq!(
            DateTime::new(2024, 1, 1, 0, 0, 0).unwrap().fields(),
            (2024, 1, 1, 0, 0, 0)
        );
    }

    #[test]
    fn encoded_scalars_round_trip() {
        let values = vec![
            Scalar::Missing,
            Scalar::Int(-9223372036854775808),
            Scalar::Float(0.5),
            Scalar::Str("héllo".into()),
            Scalar::Bool(false),
            Scalar::Date(Date::new(2024, 2, 29).unwrap()),
            Scalar::DateTime(DateTime::new(2024, 1, 1, 12, 30, 45).unwrap()),
        ];
        for value in values {
            let encoded = EncodedScalar::encode(value.clone());
            assert_eq!(
                EncodedScalar::decode(encoded.tag, &encoded.payload).unwrap(),
                value
            );
        }
    }

    #[test]
    fn encoded_scalar_rejections_carry_stable_reasons() {
        assert_eq!(
            EncodedScalar::decode(0, b"nope").unwrap_err().reason(),
            "invalid scalar payload"
        );
        assert_eq!(
            EncodedScalar::decode(1, b"01").unwrap_err().reason(),
            "invalid integer scalar"
        );
        assert_eq!(
            EncodedScalar::decode(9, b"").unwrap_err().reason(),
            "invalid scalar payload"
        );
        assert_eq!(
            EncodedScalar::decode(2, &0.0f64.to_le_bytes()).unwrap(),
            Scalar::Float(0.0)
        );
        assert_eq!(
            EncodedScalar::decode(2, &f64::NAN.to_le_bytes()).unwrap(),
            Scalar::Missing
        );
    }

    #[test]
    fn scalar_operators_match_shared_semantics() {
        assert_eq!(scalar_operator_code("+", false), Some(1));
        assert_eq!(scalar_operator_code("-", true), Some(7));
        assert_eq!(scalar_operator_code("%", false), None);
        let one = Scalar::Int(1);
        let two = Scalar::Int(2);
        assert_eq!(
            apply_scalar_operator(1, &one, Some(&two)).unwrap(),
            Scalar::Int(3)
        );
        assert!(apply_scalar_operator(6, &one, Some(&two)).is_err());
        assert_eq!(exact_float(&Scalar::Int(3)).unwrap(), Scalar::Float(3.0));
        assert!(exact_float(&Scalar::Int(i64::MAX)).is_err());
        assert!(exact_float(&Scalar::Str("3".into())).is_err());
    }
}
