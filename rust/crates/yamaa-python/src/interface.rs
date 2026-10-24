//! The interface handshake of the Python binding, the gate that every native
//! function passes, and the census of the engine's live handles
//! (`rust/interface/README.md`, "Version and feature handshake").
//!
//! The generated `yamaa/__init__.py` calls `_interface_handshake` right after
//! importing this module, before any other native call. The binding sends
//! the request to the engine it links, and until a negotiation is accepted
//! every other module function refuses with a `RuntimeError`; a result
//! handle only comes from such a function, so its methods need no gate of
//! their own.
use pyo3::{
    exceptions::PyRuntimeError,
    prelude::*,
    types::{PyBool, PyDict, PyInt, PyList, PyString, PyTuple},
};
use yamaa_boundary::client::{Engine, Gate};
use yamaa_boundary::{Outcome, Value, DEPTH};

/// The gate, which the first accepted handshake opens with the engine's
/// table.
static GATE: Gate = Gate::new();

/// The engine, once the interface handshake succeeded; before, the binding's
/// ordinary boundary refusal, as a busy handle is refused.
pub(crate) fn gate() -> PyResult<Engine> {
    GATE.pass()
        .map_err(|closed| PyRuntimeError::new_err(closed.to_string()))
}

#[cfg(not(any(feature = "linked-engine", feature = "dynamic-engine")))]
compile_error!("yamaa-python links its engine, with the default linked-engine feature, or loads it, with dynamic-engine");

/// Negotiate `request` with the engine this binding links, or in the
/// test-only loaded mode with the engine library beside its own file.
fn negotiated(request: &Value) -> Outcome {
    #[cfg(feature = "dynamic-engine")]
    return GATE.negotiate_loaded(request);
    // SAFETY: the binding links the engine, whose negotiation entry this is.
    #[cfg(not(feature = "dynamic-engine"))]
    unsafe {
        GATE.negotiate(request, yamaa_engine::negotiate)
    }
}

/// Answer an interface request through the engine: the accepted record, or
/// the refusal envelope. The first acceptance opens the gate; a later call
/// answers its request and changes nothing.
#[pyfunction]
pub fn _interface_handshake<'py>(
    py: Python<'py>,
    request: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let outcome = negotiated(&value(request, 0));
    let record = PyDict::new(py);
    match outcome {
        Outcome::Accepted(accepted) => {
            record.set_item("accepted", true)?;
            record.set_item("version", accepted.version)?;
            record.set_item("enabled", accepted.enabled)?;
            record.set_item("supported", accepted.supported)?;
            record.set_item("core_version", accepted.core_version)?;
            record.set_item("language_versions", accepted.language_versions)?;
        }
        Outcome::Refused(refusal) => {
            record.set_item("accepted", false)?;
            record.set_item("requested_version", refusal.requested_version)?;
            record.set_item("supported_version", refusal.supported_version)?;
            record.set_item("reason", refusal.reason.as_str())?;
            record.set_item("missing_features", refusal.missing_features)?;
        }
    }
    Ok(record)
}

/// The engine's live handles by kind, in the census's order: the private
/// accessor of the handle lifetime suites.
#[pyfunction]
pub fn _live_handles(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let census = gate()?.census();
    let counts = PyDict::new(py);
    for (kind, count) in census.handles {
        counts.set_item(kind.as_str(), count)?;
    }
    Ok(counts)
}

/// A request at `depth` below its mapping, as the boundary reads it, never
/// deeper than [`DEPTH`]. An `int` within 64 bits that is not a `bool` is an
/// integer and a `str` text at any depth; a list or tuple above the items is
/// a sequence; and a dict is the request's mapping, where a key that is not a
/// `str` reads as the empty key, which names no field. Anything else, or
/// anything deeper, is no field of a request.
fn value(object: &Bound<'_, PyAny>, depth: usize) -> Value {
    if object.is_instance_of::<PyBool>() {
        Value::Other
    } else if let Ok(integer) = object.cast::<PyInt>() {
        integer
            .extract::<i64>()
            .map_or(Value::Other, Value::Integer)
    } else if let Ok(text) = object.cast::<PyString>() {
        text.to_str()
            .map_or(Value::Other, |text| Value::Text(text.to_owned()))
    } else if depth >= DEPTH {
        Value::Other
    } else if let Ok(list) = object.cast::<PyList>() {
        Value::List(list.iter().map(|item| value(&item, depth + 1)).collect())
    } else if let Ok(tuple) = object.cast::<PyTuple>() {
        Value::List(tuple.iter().map(|item| value(&item, depth + 1)).collect())
    } else if let (0, Ok(mapping)) = (depth, object.cast::<PyDict>()) {
        Value::Map(
            mapping
                .iter()
                .map(|(key, item)| (key_text(&key), value(&item, depth + 1)))
                .collect(),
        )
    } else {
        Value::Other
    }
}

/// A key of the request's mapping as text, or the empty key, which names no
/// field, when it is not a `str`.
fn key_text(key: &Bound<'_, PyAny>) -> String {
    key.cast::<PyString>()
        .ok()
        .and_then(|key| key.to_str().ok())
        .map_or_else(String::new, str::to_owned)
}
