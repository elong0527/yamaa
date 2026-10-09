//! Explicit Python callable capability on the calling interpreter thread.
use pyo3::{
    exceptions::{PyException, PyRuntimeError, PyTypeError, PyValueError},
    prelude::*,
    types::{PyBool, PyDate, PyDateTime, PyDict, PyFloat, PyInt, PyString},
    IntoPyObjectExt,
};
use yamaa_adapters::function_transport::{
    CallbackError, FunctionTransportError as Error, PreparedInvocation, MAX_ERROR_BYTES,
    MAX_RESULT_BYTES,
};
use yamaa_core::{
    table::ValueRef,
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::Value,
};
use yamaa_engine::function_invocation::{Argument, FunctionPort, HostError};

/// Execute one bounded request with an explicit callable; labels do not prove
/// artifact authorization. No GIL release, worker thread, discovery or retry.
#[pyfunction]
pub fn invoke_function(request: &str, callback: &Bound<'_, PyAny>) -> PyResult<String> {
    if !callback.is_callable() {
        return Err(PyTypeError::new_err("callback must be callable"));
    }
    let prepared = PreparedInvocation::parse(request).map_err(transport_error)?;
    if !prepared.host_names().all(host_name) {
        return Err(transport_error(Error::InvalidHostName));
    }
    let mut port = PythonPort {
        callback,
        interrupted: None,
    };
    let result = prepared.invoke(&mut port);
    if let Some(interrupted) = port.interrupted {
        return Err(interrupted);
    }
    result.map_err(transport_error)
}
/// Execute a typed dataset with an explicitly supplied, per-run snapshot of callables.
#[pyfunction]
pub fn execute_dataset_functions<'py>(
    py: Python<'py>,
    request: &str,
    source: &[u8],
    secondary: &Bound<'py, pyo3::types::PyList>,
    callbacks: &Bound<'py, pyo3::types::PyList>,
) -> PyResult<(Option<Bound<'py, pyo3::types::PyBytes>>, String)> {
    execute_dataset_bound(request, source, secondary, callbacks, None)
        .map(|result| crate::dataset_output(py, result))
}

/// Benchmark one execution with separate phase metadata; never retry calls to measure them.
/// This private instrumentation entrypoint is not an additional execution capability.
#[pyfunction]
pub fn _profile_dataset_functions<'py>(
    py: Python<'py>,
    request: &str,
    source: &[u8],
    secondary: &Bound<'py, pyo3::types::PyList>,
    callbacks: &Bound<'py, pyo3::types::PyList>,
) -> PyResult<(Option<Bound<'py, pyo3::types::PyBytes>>, String, String)> {
    let mut profile = yamaa_adapters::dataset_profile::DatasetProfile::new();
    let result = execute_dataset_bound(request, source, secondary, callbacks, Some(&mut profile))?;
    let (table, outcome) = crate::dataset_output(py, result);
    let metrics = profile
        .finish()
        .map_err(|_| PyRuntimeError::new_err("profile encoding failed"))?;
    Ok((table, outcome, metrics))
}

/// Share complete admission, captured bindings and interruption propagation between both paths.
fn execute_dataset_bound(
    request: &str,
    source: &[u8],
    secondary: &Bound<'_, pyo3::types::PyList>,
    callbacks: &Bound<'_, pyo3::types::PyList>,
    mut profile: Option<&mut yamaa_adapters::dataset_profile::DatasetProfile>,
) -> PyResult<yamaa_adapters::dataset_transport::DatasetResponse> {
    use yamaa_adapters::dataset_transport::{PreparedDataset, MAX_SOURCES};
    let prepared = PreparedDataset::parse(request).map_err(crate::dataset_error)?;
    if let Some(profile) = profile.as_deref_mut() {
        profile.enter(yamaa_adapters::dataset_profile::ProfilePhase::HostBindings);
    }
    if callbacks.len() != prepared.function_signatures().len() {
        return Err(PyValueError::new_err(
            "callback count does not match dataset declarations",
        ));
    }
    if !prepared
        .function_signatures()
        .iter()
        .all(|s| s.parameters().iter().all(|p| host_name(&p.host_name)))
    {
        return Err(transport_error(Error::InvalidHostName));
    }
    let callbacks = callbacks
        .iter()
        .map(|callback| {
            if !callback.is_callable() {
                return Err(PyTypeError::new_err("callback must be callable"));
            }
            Ok(callback)
        })
        .collect::<PyResult<Vec<_>>>()?;
    if secondary.len() >= MAX_SOURCES {
        return Err(PyValueError::new_err("too many secondary dataset sources"));
    }
    let buffers = secondary
        .iter()
        .map(|value| value.cast_into::<pyo3::types::PyBytes>())
        .collect::<Result<Vec<_>, _>>()?;
    let slices = buffers
        .iter()
        .map(|bytes| bytes.as_bytes())
        .collect::<Vec<_>>();
    let mut bindings = PythonBindings {
        signatures: prepared.function_signatures(),
        callbacks,
        interrupted: None,
    };
    let result = if let Some(profile) = profile {
        prepared.execute_sources_functions_profiled(source, &slices, &mut bindings, profile)
    } else {
        prepared.execute_sources_functions(source, &slices, &mut bindings)
    };
    if let Some(interrupted) = bindings.interrupted {
        return Err(interrupted);
    }
    result.map_err(crate::dataset_error)
}

struct PythonBindings<'a, 'py> {
    signatures: &'a [yamaa_engine::function_invocation::InvocationPlan],
    callbacks: Vec<Bound<'py, PyAny>>,
    interrupted: Option<PyErr>,
}
impl yamaa_engine::dataset::FunctionBindings for PythonBindings<'_, '_> {
    type Error = CallbackError;
    /// Metadata is borrowed from the fully admitted request and remains stable for the run.
    fn signature(&self, slot: usize) -> Option<&yamaa_engine::function_invocation::InvocationPlan> {
        self.signatures.get(slot)
    }
    /// Reuse exact scalar encoding and original control-flow propagation without retries.
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<CallbackError>> {
        let mut port = PythonPort {
            callback: &self.callbacks[slot],
            interrupted: None,
        };
        let result = port.call(arguments);
        self.interrupted = port.interrupted;
        result
    }
}

/// Keep cancellation and unexpected internal failures separate from invalid inputs.
fn transport_error(error: Error) -> PyErr {
    if matches!(error, Error::Internal | Error::Interrupted) {
        PyRuntimeError::new_err(error.to_string())
    } else {
        PyValueError::new_err(error.to_string())
    }
}
/// Python hard keywords are excluded; soft keywords remain legal parameter names.
fn host_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(
            name,
            "False"
                | "None"
                | "True"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        )
}
struct PythonPort<'a, 'py> {
    callback: &'a Bound<'py, PyAny>,
    interrupted: Option<PyErr>,
}
impl FunctionPort for PythonPort<'_, '_> {
    type Error = CallbackError;
    /// Build owned Python scalars and call exactly once while the GIL stays held.
    fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
        let py = self.callback.py();
        let kwargs = PyDict::new(py);
        for argument in arguments {
            let value = host_argument(py, argument.value)
                .map_err(|_| HostError::Raised(CallbackError::Boundary(Error::Internal)))?;
            kwargs
                .set_item(argument.name, value)
                .map_err(|_| HostError::Raised(CallbackError::Boundary(Error::Internal)))?;
        }
        let returned = match self.callback.call((), Some(&kwargs)) {
            Ok(value) => value,
            Err(error) => {
                if !error.is_instance_of::<PyException>(py) {
                    self.interrupted = Some(error);
                    return Err(HostError::Raised(CallbackError::Boundary(
                        Error::Interrupted,
                    )));
                }
                return Err(HostError::Raised(exception(py, error)));
            }
        };
        host_result(&returned).map_err(HostError::InvalidResult)
    }
}
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
fn exception(py: Python<'_>, error: PyErr) -> CallbackError {
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
        return value
            .extract()
            .map(Value::float)
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
