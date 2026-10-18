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

    /// Return whole days since 1970-01-01, negative before it.
    pub fn epoch_days(self) -> i32 {
        yamaa_adapters::arrow_temporal::date_days(self.inner)
    }

    /// The date `days` whole days after 1970-01-01, when it lies within
    /// the civil years 1 to 9999.
    pub fn from_epoch_days(days: i64) -> Option<Self> {
        yamaa_adapters::arrow_temporal::date_from_days(
            days,
            yamaa_core::temporal::DatePrecision::Day,
        )
        .map(Self::from_core)
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

    /// Return whole seconds since 1970-01-01T00:00:00, counting the civil
    /// fields as zone-free time; negative before it.
    pub fn epoch_seconds(self) -> i64 {
        yamaa_adapters::arrow_temporal::datetime_seconds(self.inner)
    }

    /// The datetime `seconds` whole seconds after 1970-01-01T00:00:00, when
    /// it lies within the civil years 1 to 9999.
    pub fn from_epoch_seconds(seconds: i64) -> Option<Self> {
        yamaa_adapters::arrow_temporal::datetime_from_seconds(
            seconds,
            yamaa_core::temporal::DateTimePrecision::Second,
        )
        .map(Self::from_core)
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
    fn temporal_values_round_trip_through_epoch_counts() {
        let epoch = Date::new(1970, 1, 1).unwrap();
        assert_eq!(epoch.epoch_days(), 0);
        assert_eq!(Date::new(1969, 12, 31).unwrap().epoch_days(), -1);
        assert_eq!(Date::new(2024, 2, 29).unwrap().epoch_days(), 19782);
        for date in [
            Date::new(1, 1, 1).unwrap(),
            Date::new(1900, 3, 1).unwrap(),
            Date::new(2024, 2, 29).unwrap(),
            Date::new(9999, 12, 31).unwrap(),
        ] {
            assert_eq!(
                Date::from_epoch_days(i64::from(date.epoch_days())),
                Some(date)
            );
        }
        assert_eq!(Date::new(1, 1, 1).unwrap().epoch_days(), -719_162);
        assert!(Date::from_epoch_days(-719_163).is_none());
        assert_eq!(Date::new(9999, 12, 31).unwrap().epoch_days(), 2_932_896);
        assert!(Date::from_epoch_days(2_932_897).is_none());
        assert!(Date::from_epoch_days(i64::MIN).is_none());
        assert!(Date::from_epoch_days(i64::MAX).is_none());

        let before = DateTime::new(1969, 12, 31, 23, 59, 59).unwrap();
        assert_eq!(before.epoch_seconds(), -1);
        assert_eq!(DateTime::from_epoch_seconds(-1), Some(before));
        let leap = DateTime::new(2024, 2, 29, 12, 30, 45).unwrap();
        assert_eq!(leap.epoch_seconds(), 19782 * 86400 + 45045);
        assert_eq!(
            DateTime::from_epoch_seconds(leap.epoch_seconds()),
            Some(leap)
        );
        let last = DateTime::new(9999, 12, 31, 23, 59, 59).unwrap();
        assert_eq!(
            DateTime::from_epoch_seconds(last.epoch_seconds()),
            Some(last)
        );
        assert!(DateTime::from_epoch_seconds(last.epoch_seconds() + 1).is_none());
        assert!(DateTime::from_epoch_seconds(i64::MIN).is_none());
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
}
