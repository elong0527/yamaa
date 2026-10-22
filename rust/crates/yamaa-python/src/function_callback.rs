//! Explicit Python callable capability on the calling interpreter thread.
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyBool, PyDate, PyDateTime, PyFloat, PyInt, PyString},
    IntoPyObjectExt,
};
use yamaa::{Date, DateTime, EncodedScalar, FunctionError, HostPayload, Scalar, ScalarRef};

/// Function argument encoding drops collected precision only at this boundary.
pub(super) fn host_argument<'py>(
    py: Python<'py>,
    value: ScalarRef<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    match value {
        ScalarRef::Missing => Ok(py.None().into_bound(py)),
        ScalarRef::Str(s) => s.into_bound_py_any(py),
        ScalarRef::Int(n) => n.into_bound_py_any(py),
        ScalarRef::Float(n) => n.into_bound_py_any(py),
        ScalarRef::Bool(b) => b.into_bound_py_any(py),
        ScalarRef::Date(d) => {
            let (y, m, d) = d.fields();
            Ok(PyDate::new(py, i32::from(y), m, d)?.into_any())
        }
        ScalarRef::DateTime(d) => {
            let (y, m, d, h, minute, s) = d.fields();
            Ok(PyDateTime::new(py, i32::from(y), m, d, h, minute, s, 0, None)?.into_any())
        }
        // The facade enum is non-exhaustive; uncovered variants cannot occur.
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "unsupported scalar argument",
        )),
    }
}

/// Classify one Python failure, retaining the original exception object.
/// Exceptions stay raisable payloads; anything else is an interrupt.
pub(super) fn host_error(py: Python<'_>, error: PyErr) -> FunctionError {
    if error.is_instance_of::<pyo3::exceptions::PyException>(py) {
        let (class, message) = describe(py, &error);
        FunctionError::Exception {
            class,
            message,
            payload: Some(HostPayload::new(
                error.value(py).clone().into_any().unbind(),
            )),
        }
    } else {
        crate::interrupt::remember(py, &error);
        FunctionError::Interrupted {
            payload: Some(HostPayload::new(
                error.value(py).clone().into_any().unbind(),
            )),
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
fn rejected(reason: &str, returned: Option<String>) -> FunctionError {
    FunctionError::Rejected {
        reason: reason.into(),
        returned,
    }
}

/// Admit exact numeric/temporal built-ins, compatible text subclasses, and None.
/// ABI3 date fields use safe attributes after exact built-in type admission.
pub(super) fn host_result(value: &Bound<'_, PyAny>) -> Result<Scalar, FunctionError> {
    if value.is_none() {
        return Ok(Scalar::Missing);
    }
    if value.is_exact_instance_of::<PyBool>() {
        return value
            .extract()
            .map(Scalar::Bool)
            .map_err(|_| FunctionError::Internal {
                code: "function_boundary",
            });
    }
    if value.is_exact_instance_of::<PyInt>() {
        return value
            .extract()
            .map(Scalar::Int)
            .map_err(|_| rejected("a returned integer exceeds 64 bits", None));
    }
    if value.is_exact_instance_of::<PyFloat>() {
        let number: f64 = value.extract().map_err(|_| FunctionError::Internal {
            code: "function_boundary",
        })?;
        return EncodedScalar::decode(2, &number.to_le_bytes()).map_err(|_| {
            FunctionError::Internal {
                code: "function_boundary",
            }
        });
    }
    if let Ok(text) = value.cast::<PyString>() {
        let text = text
            .to_cow()
            .map_err(|_| rejected("a returned string is not valid UTF-8", None))?;
        return Ok(Scalar::Str(text.into_owned()));
    }
    let is_date = value.is_exact_instance_of::<PyDate>();
    let is_datetime = value.is_exact_instance_of::<PyDateTime>();
    if is_date || is_datetime {
        let extract = || -> PyResult<Scalar> {
            let year = value.getattr("year")?.extract()?;
            let month = value.getattr("month")?.extract()?;
            let day = value.getattr("day")?.extract()?;
            let date = Date::new(year, month, day).ok_or_else(|| PyValueError::new_err("date"))?;
            if is_date {
                return Ok(Scalar::Date(date));
            }
            if !value.getattr("tzinfo")?.is_none()
                || value.getattr("microsecond")?.extract::<u32>()? != 0
            {
                return Err(PyValueError::new_err("datetime"));
            }
            let hour = value.getattr("hour")?.extract()?;
            let minute = value.getattr("minute")?.extract()?;
            let second = value.getattr("second")?.extract()?;
            DateTime::new(
                date.fields().0,
                date.fields().1,
                date.fields().2,
                hour,
                minute,
                second,
            )
            .map(Scalar::DateTime)
            .ok_or_else(|| PyValueError::new_err("datetime"))
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
