//! Explicit Python callable capability on the calling interpreter thread.
use pyo3::{
    prelude::*,
    types::{PyBool, PyDate, PyDateTime, PyFloat, PyInt, PyString},
    IntoPyObjectExt,
};
use yamaa_boundary::client::{self, Failure, FailureKind, Scalar, ScalarRef};

/// A host failure the engine keeps, with its original Python object.
pub(super) type HostFailure = Failure<Py<PyAny>>;

/// Function argument encoding drops collected precision only at this boundary.
pub(super) fn host_argument<'py>(
    py: Python<'py>,
    value: ScalarRef<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    let outside = || pyo3::exceptions::PyValueError::new_err("a date outside the years 1 to 9999");
    match value {
        ScalarRef::Missing => Ok(py.None().into_bound(py)),
        ScalarRef::Text(s) => s.into_bound_py_any(py),
        ScalarRef::Int(n) => n.into_bound_py_any(py),
        ScalarRef::Float(n) => n.into_bound_py_any(py),
        ScalarRef::Bool(b) => b.into_bound_py_any(py),
        ScalarRef::Date(days) => {
            let (y, m, d) = client::civil_date(days).ok_or_else(outside)?;
            Ok(PyDate::new(py, y, m, d)?.into_any())
        }
        ScalarRef::DateTime(seconds) => {
            let (y, m, d, h, minute, s) = client::civil_datetime(seconds).ok_or_else(outside)?;
            Ok(PyDateTime::new(py, y, m, d, h, minute, s, 0, None)?.into_any())
        }
    }
}

/// Classify one Python failure, retaining the original exception object.
/// Exceptions stay raisable payloads; anything else is an interrupt.
pub(super) fn host_error(py: Python<'_>, error: PyErr) -> HostFailure {
    let payload = Some(error.value(py).clone().into_any().unbind());
    if error.is_instance_of::<pyo3::exceptions::PyException>(py) {
        let (class, message) = describe(py, &error);
        Failure {
            kind: FailureKind::Exception,
            class,
            message,
            returned: None,
            payload,
        }
    } else {
        crate::interrupt::remember(py, &error);
        Failure {
            payload,
            ..Failure::new(FailureKind::Interrupted, "")
        }
    }
}

/// Secondary __str__ failures cannot replace the original host exception class.
fn describe(py: Python<'_>, error: &PyErr) -> (String, String) {
    let class = error
        .get_type(py)
        .name()
        .ok()
        .and_then(|n| n.to_str().ok().map(str::to_owned))
        .unwrap_or_else(|| "Exception".into());
    let message = error
        .value(py)
        .str()
        .ok()
        .and_then(|s| s.to_str().ok().map(str::to_owned))
        .unwrap_or_else(|| "<exception message unavailable>".into());
    (class, message)
}

/// Reject a returned representation without converting it to another logical type.
fn rejected(reason: &str, returned: Option<String>) -> HostFailure {
    Failure {
        returned,
        ..Failure::new(FailureKind::Rejected, reason)
    }
}

fn internal() -> HostFailure {
    Failure::new(FailureKind::Internal, "function_boundary")
}

/// Admit exact numeric/temporal built-ins, compatible text subclasses, and None.
/// ABI3 date fields use safe attributes after exact built-in type admission.
pub(super) fn host_result(value: &Bound<'_, PyAny>) -> Result<Scalar, HostFailure> {
    if value.is_none() {
        return Ok(Scalar::Missing);
    }
    if value.is_exact_instance_of::<PyBool>() {
        return value.extract().map(Scalar::Bool).map_err(|_| internal());
    }
    if value.is_exact_instance_of::<PyInt>() {
        return value
            .extract()
            .map(Scalar::Int)
            .map_err(|_| rejected("a returned integer exceeds 64 bits", None));
    }
    if value.is_exact_instance_of::<PyFloat>() {
        // The engine turns a value that is not finite into a missing one.
        return value.extract().map(Scalar::Float).map_err(|_| internal());
    }
    if let Ok(text) = value.cast::<PyString>() {
        let text = text
            .to_cow()
            .map_err(|_| rejected("a returned string is not valid UTF-8", None))?;
        return Ok(Scalar::Text(text.into_owned()));
    }
    let is_date = value.is_exact_instance_of::<PyDate>();
    let is_datetime = value.is_exact_instance_of::<PyDateTime>();
    if is_date || is_datetime {
        let extract = || -> PyResult<Scalar> {
            let field = |name: &str| value.getattr(name)?.extract::<i64>();
            let date = (field("year")?, field("month")?, field("day")?);
            let days = client::epoch_days(date.0 as i32, date.1 as u8, date.2 as u8);
            if is_date {
                return Ok(Scalar::Date(days));
            }
            if !value.getattr("tzinfo")?.is_none() || field("microsecond")? != 0 {
                return Err(pyo3::exceptions::PyValueError::new_err("datetime"));
            }
            let time = field("hour")? * 3600 + field("minute")? * 60 + field("second")?;
            Ok(Scalar::DateTime(days * 86_400 + time))
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
        .and_then(|n| n.to_str().ok().map(str::to_owned));
    Err(rejected(
        "a binding returned a value of no scalar type",
        kind,
    ))
}
