//! Explicit Python callable capability on the calling interpreter thread.
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyBool, PyDate, PyDateTime, PyFloat, PyInt, PyString},
    IntoPyObjectExt,
};
use yamaa_adapters::callback::{
    BoundaryError as Error, CallbackError, MAX_ERROR_BYTES, MAX_RESULT_BYTES,
};
use yamaa_engine::host_values::{
    Date, DatePrecision, DateTime, DateTimePrecision, Value, ValueRef,
};

/// Function argument encoding drops collected precision only at this boundary.
pub(super) fn host_argument<'py>(
    py: Python<'py>,
    value: ValueRef<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    match value {
        ValueRef::Missing => Ok(py.None().into_bound(py)),
        ValueRef::Str(s) => s.into_bound_py_any(py),
        ValueRef::Int(n) => n.into_bound_py_any(py),
        ValueRef::Float(n) => n.get().into_bound_py_any(py),
        ValueRef::Bool(b) => b.into_bound_py_any(py),
        ValueRef::Date(d) => {
            let (y, m, d) = d.fields();
            Ok(PyDate::new(py, i32::from(y), m, d)?.into_any())
        }
        ValueRef::DateTime(d) => {
            let (y, m, d, h, minute, s) = d.fields();
            Ok(PyDateTime::new(py, i32::from(y), m, d, h, minute, s, 0, None)?.into_any())
        }
    }
}
/// Retain a bounded UTF-8 prefix with explicit truncation, never split a codepoint.
fn bounded(text: &str) -> (String, bool) {
    let mut end = text.len().min(MAX_ERROR_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].into(), end != text.len())
}
/// Secondary __str__ failures cannot replace the original host exception class.
pub(super) fn exception(py: Python<'_>, error: PyErr) -> CallbackError {
    let (class, class_truncated) = error
        .get_type(py)
        .name()
        .ok()
        .and_then(|n| n.to_str().ok().map(bounded))
        .unwrap_or_else(|| ("Exception".into(), false));
    let (message, truncated) = error
        .value(py)
        .str()
        .ok()
        .and_then(|s| s.to_str().ok().map(bounded))
        .unwrap_or_else(|| ("<exception message unavailable>".into(), false));
    CallbackError::Exception {
        class,
        message,
        truncated: truncated || class_truncated,
    }
}
/// Reject a returned representation without converting it to another logical type.
fn rejected(reason: &str, returned: Option<String>) -> CallbackError {
    CallbackError::Rejected {
        reason: reason.into(),
        returned,
    }
}
/// Admit exact numeric/temporal built-ins, compatible text subclasses, and None.
/// ABI3 date fields use safe attributes after exact built-in type admission.
pub(super) fn host_result(value: &Bound<'_, PyAny>) -> Result<Value, CallbackError> {
    if let Ok(temporal) = value.extract::<PyRef<'_, crate::temporal_result::TemporalResult>>() {
        return temporal.value();
    }
    if value.is_none() {
        return Ok(Value::Missing);
    }
    if value.is_exact_instance_of::<PyBool>() {
        return Ok(Value::Bool(
            value
                .extract()
                .map_err(|_| CallbackError::Boundary(Error::Internal))?,
        ));
    }
    if value.is_exact_instance_of::<PyInt>() {
        return value
            .extract()
            .map(Value::Int)
            .map_err(|_| rejected("a returned integer exceeds 64 bits", None));
    }
    if value.is_exact_instance_of::<PyFloat>() {
        let number: f64 = value
            .extract()
            .map_err(|_| CallbackError::Boundary(Error::Internal))?;
        return yamaa_adapters::scalar_bytes::decode(2, &number.to_le_bytes())
            .map_err(|_| CallbackError::Boundary(Error::Internal));
    }
    if let Ok(text) = value.cast::<PyString>() {
        let text = text
            .to_cow()
            .map_err(|_| rejected("a returned string is not valid UTF-8", None))?;
        if text.len() > MAX_RESULT_BYTES {
            return Err(CallbackError::Boundary(Error::OutputLimit));
        }
        return Ok(Value::Str(text.into_owned()));
    }
    let is_date = value.is_exact_instance_of::<PyDate>();
    let is_datetime = value.is_exact_instance_of::<PyDateTime>();
    if is_date || is_datetime {
        let extract = || -> PyResult<Value> {
            let year = value.getattr("year")?.extract()?;
            let month = value.getattr("month")?.extract()?;
            let day = value.getattr("day")?.extract()?;
            let date = Date::new(year, month, day, DatePrecision::Day)
                .map_err(|_| PyValueError::new_err("date"))?;
            if is_date {
                return Ok(Value::Date(date));
            }
            if !value.getattr("tzinfo")?.is_none()
                || value.getattr("microsecond")?.extract::<u32>()? != 0
            {
                return Err(PyValueError::new_err("datetime"));
            }
            let hour = value.getattr("hour")?.extract()?;
            let minute = value.getattr("minute")?.extract()?;
            let second = value.getattr("second")?.extract()?;
            Ok(Value::DateTime(
                DateTime::new(date, hour, minute, second, DateTimePrecision::Second)
                    .map_err(|_| PyValueError::new_err("datetime"))?,
            ))
        };
        return extract().map_err(|_| {
            rejected(
                "a returned datetime carries a zone or a fraction of a second",
                None,
            )
        });
    }
    let kind = value
        .get_type()
        .name()
        .ok()
        .and_then(|n| n.to_str().ok().map(|s| bounded(s).0));
    Err(rejected(
        "a binding returned a value of no scalar type",
        kind,
    ))
}
