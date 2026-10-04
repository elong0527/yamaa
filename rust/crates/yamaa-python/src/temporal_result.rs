//! Closed owned result bridge for the optional Python facade's known temporals.
use pyo3::{
    prelude::*,
    types::{PyInt, PyString, PyTuple},
};
use yamaa_adapters::function_transport::CallbackError;
use yamaa_core::{
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::Value,
};

/// An owned temporal or an invalid representation, never a generic object box.
/// No public constructor, subclassing, fields or mutation are exposed to Python.
#[pyclass(frozen, module = "yamaa_native", name = "_TemporalResult")]
pub struct TemporalResult {
    value: Option<Temporal>,
}

/// The carrier cannot hold other core values, even within this binding crate.
#[derive(Clone, Copy)]
enum Temporal {
    Date(Date),
    DateTime(DateTime),
}

impl TemporalResult {
    /// Reject during result admission, rather than misclassifying a facade
    /// validation error as a callback-raised REQ-0701 condition.
    pub fn value(&self) -> Result<Value, CallbackError> {
        match self.value {
            Some(Temporal::Date(value)) => Ok(Value::Date(value)),
            Some(Temporal::DateTime(value)) => Ok(Value::DateTime(value)),
            None => Err(CallbackError::Rejected {
                reason: "a returned temporal representation is invalid".into(),
                returned: None,
            }),
        }
    }
}

/// Build only validated core dates/datetimes. Invalid representation stays data
/// until the invocation result check; no Python scalar coercion is performed.
#[pyfunction]
pub fn _temporal_result(
    kind: &Bound<'_, PyAny>,
    fields: &Bound<'_, PyAny>,
    precision: &Bound<'_, PyAny>,
) -> TemporalResult {
    TemporalResult {
        value: decode(kind, fields, precision),
    }
}

/// Exact built-in storage prevents bool/int coercion and arbitrary object hooks.
fn decode(
    kind: &Bound<'_, PyAny>,
    fields: &Bound<'_, PyAny>,
    precision: &Bound<'_, PyAny>,
) -> Option<Temporal> {
    if !kind.is_exact_instance_of::<PyString>()
        || !precision.is_exact_instance_of::<PyString>()
        || !fields.is_exact_instance_of::<PyTuple>()
    {
        return None;
    }
    let kind = kind.cast::<PyString>().ok()?.to_str().ok()?;
    let precision = precision.cast::<PyString>().ok()?.to_str().ok()?;
    let fields = fields.cast::<PyTuple>().ok()?;
    let expected = match kind {
        "date" => 3,
        "datetime" => 6,
        _ => return None,
    };
    if fields.len() != expected {
        return None;
    }
    let mut parts = Vec::with_capacity(expected);
    for part in fields.iter() {
        if !part.is_exact_instance_of::<PyInt>() {
            return None;
        }
        parts.push(part.extract::<u16>().ok()?);
    }
    let date_precision = if kind == "date" {
        match precision {
            "year" => DatePrecision::Year,
            "month" => DatePrecision::Month,
            "day" => DatePrecision::Day,
            _ => return None,
        }
    } else {
        DatePrecision::Day
    };
    let date = Date::new(
        parts[0],
        parts[1].try_into().ok()?,
        parts[2].try_into().ok()?,
        date_precision,
    )
    .ok()?;
    if kind == "date" {
        return Some(Temporal::Date(date));
    }
    let precision = match precision {
        "day" => DateTimePrecision::Day,
        "second" => DateTimePrecision::Second,
        _ => return None,
    };
    Some(Temporal::DateTime(
        DateTime::new(
            date,
            parts[3].try_into().ok()?,
            parts[4].try_into().ok()?,
            parts[5].try_into().ok()?,
            precision,
        )
        .ok()?,
    ))
}
