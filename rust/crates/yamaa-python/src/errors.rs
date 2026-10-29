//! Native refusal classes shared by module calls and Table methods.
use pyo3::{exceptions::PyException, prelude::*, types::PyList};

use crate::arrow_bridge::Table;

pyo3::create_exception!(yamaa, Error, PyException);
pyo3::create_exception!(yamaa, StateError, Error);

/// Build the same error with or without the generated layer loaded. The layer
/// installs its declared constructor and pickle methods on these native types;
/// a raw native import still receives the message, issues and paths.
fn make(
    py: Python<'_>,
    state: bool,
    message: String,
    issues: &yamaa::Table,
    paths: Vec<String>,
) -> PyResult<PyErr> {
    let table = Py::new(py, Table::from(issues))?;
    let paths = PyList::new(py, paths)?;
    let class = if state {
        py.get_type::<StateError>()
    } else {
        py.get_type::<Error>()
    };
    let value = class.call1((&message, &table, &paths))?;
    value.setattr("args", (message,))?;
    value.setattr("__suppress_context__", true)?;
    value.setattr("issues", table)?;
    value.setattr("paths", paths)?;
    Ok(PyErr::from_value(value))
}

pub(crate) fn message(py: Python<'_>, state: bool, message: impl Into<String>) -> PyErr {
    make(
        py,
        state,
        message.into(),
        &yamaa::diagnostic::issues(&[]),
        Vec::new(),
    )
    .unwrap_or_else(|error| error)
}

pub(crate) fn failure(py: Python<'_>, failure: yamaa::Failure) -> PyErr {
    let cause = failure.cause.as_ref().and_then(|payload| {
        crate::progress::cause(py, payload)
            .or_else(|| payload.with::<Py<PyAny>, _>(|value| value.clone_ref(py)))
    });
    let state = matches!(
        failure.kind,
        yamaa::FailureKind::Stream { .. } | yamaa::FailureKind::Cancelled
    );
    let error = make(py, state, failure.message, &failure.issues, failure.paths)
        .unwrap_or_else(|error| error);
    if let Some(cause) = cause {
        error.set_cause(py, Some(PyErr::from_value(cause.into_bound(py))));
    }
    error
}

#[pyfunction]
pub fn _empty_issues() -> PyResult<Table> {
    crate::interface::gate()?;
    Ok(Table::from(&yamaa::diagnostic::issues(&[])))
}
