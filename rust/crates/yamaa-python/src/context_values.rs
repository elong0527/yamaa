//! Convert the engine's nested values to Python objects.
//!
//! Integer tokens become `int` (unbounded, so a token wider than 64 bits
//! keeps its exact value), other number tokens `float`, objects `dict` and
//! arrays `list`.
use pyo3::{
    prelude::*,
    types::{PyDict, PyFloat, PyInt, PyList},
    IntoPyObjectExt,
};
use yamaa_boundary::client::Node;

/// Build the owned Python value tree: integer tokens become `int` (unbounded,
/// so a token wider than 64 bits parses its exact spelling), other number
/// tokens `float`, objects `dict` and arrays `list`.
pub(crate) fn context_value<'py>(py: Python<'py>, value: &Node) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Node::Null => Ok(py.None().into_bound(py)),
        Node::Bool(value) => value.into_bound_py_any(py),
        Node::Int(value) => value.into_bound_py_any(py),
        Node::Number(token) => wide_number(py, token),
        Node::Float(value) => value.into_bound_py_any(py),
        Node::Text(value) => value.into_bound_py_any(py),
        Node::List(values) => {
            let list = PyList::empty(py);
            for value in values {
                list.append(context_value(py, value)?)?;
            }
            Ok(list.into_any())
        }
        Node::Map(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                dict.set_item(key, context_value(py, value)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

/// Parse an exact number spelling with the interpreter's own constructors, so
/// the host never narrows it. The interpreter's integer-string conversion
/// limit applies to a token of thousands of digits.
fn wide_number<'py>(py: Python<'py>, token: &str) -> PyResult<Bound<'py, PyAny>> {
    let integer = !token.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E'));
    if integer {
        py.get_type::<PyInt>().call1((token,))
    } else {
        py.get_type::<PyFloat>().call1((token,))
    }
}
