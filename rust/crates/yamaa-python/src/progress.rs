//! GC-visible ownership of progress callbacks and their original exceptions.
//!
//! The engine keeps only a weak reference. Each native result and streamed
//! table visits its strong Python owner; a foreign Arrow reader keeps that
//! owner alive until release. Tracebacks may therefore refer back to results
//! without hiding a cycle from Python's collector.
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyModule},
    PyTraverseError, PyVisit,
};
use std::sync::Arc;
use yamaa::host::functions::{FunctionError, HostPayload};

#[derive(Default)]
pub(crate) struct Causes(Option<Py<PyAny>>);

impl Causes {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self(self.0.as_ref().map(|owner| owner.clone_ref(py)))
    }

    pub(crate) fn traverse(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.0)
    }

    /// Completed calls no longer need their callback, but keep its causes.
    pub(crate) fn finished(&self, py: Python<'_>) -> PyResult<()> {
        if let Some(owner) = &self.0 {
            owner.bind(py).setattr("callback", py.None())?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct Progress {
    weak: Arc<Py<PyAny>>,
    thread: std::thread::ThreadId,
}

/// A failure's index in the GC-visible owner's list, never a hidden strong
/// reference to the exception (whose traceback can contain the result).
struct Cause {
    weak: Py<PyAny>,
    index: usize,
}

pub(crate) fn cause(py: Python<'_>, payload: &HostPayload) -> Option<Py<PyAny>> {
    payload
        .with::<Cause, _>(|cause| {
            cause
                .weak
                .bind(py)
                .call0()?
                .getattr("errors")?
                .get_item(cause.index)
                .map(Bound::unbind)
        })?
        .ok()
}

impl Progress {
    pub(crate) fn of(callable: Option<Bound<'_, PyAny>>) -> PyResult<(Option<Self>, Causes)> {
        let Some(callable) = callable else {
            return Ok((None, Causes::default()));
        };
        let py = callable.py();
        if !callable.is_callable() {
            return Err(crate::errors::message(
                py,
                false,
                "progress must be a callable or None",
            ));
        }
        let owner = PyModule::import(py, "yamaa._support")?
            .getattr("_ProgressCauses")?
            .call1((callable,))?;
        let weak = PyModule::import(py, "weakref")?
            .getattr("ref")?
            .call1((&owner,))?;
        Ok((
            Some(Self {
                weak: Arc::new(weak.unbind()),
                thread: std::thread::current().id(),
            }),
            Causes(Some(owner.unbind())),
        ))
    }

    pub(crate) fn call(&self, event: &yamaa::ProgressEvent) -> Result<(), FunctionError> {
        if std::thread::current().id() != self.thread {
            return Ok(());
        }
        Python::attach(|py| {
            let fields = PyDict::new(py);
            let owner = self.weak.bind(py).call0();
            let answer = fields
                .set_item("name", &event.name)
                .and_then(|()| fields.set_item("phase", event.phase.as_str()))
                .and_then(|()| fields.set_item("kind", event.kind.as_str()))
                .and_then(|()| fields.set_item("outcome", event.outcome.map(yamaa::Status::as_str)))
                .and_then(|()| fields.set_item("number", event.number))
                .and_then(|()| fields.set_item("rows", event.rows))
                .and_then(|()| {
                    owner
                        .as_ref()
                        .map_err(|e| e.clone_ref(py))?
                        .getattr("callback")?
                        .call1((fields,))
                });
            answer.map(drop).map_err(|error| {
                let mut failure = crate::function_callback::host_error(py, error);
                if let FunctionError::Exception { payload, .. } = &mut failure {
                    if let (Ok(owner), Some(kept)) = (owner, payload.as_ref()) {
                        let stored = kept.with::<Py<PyAny>, _>(|value| {
                            let errors = owner.getattr("errors")?.cast_into::<PyList>()?;
                            let index = errors.len();
                            errors.append(value.bind(py))?;
                            Ok::<_, PyErr>(Cause {
                                weak: self.weak.clone_ref(py),
                                index,
                            })
                        });
                        if let Some(Ok(cause)) = stored {
                            *payload = Some(HostPayload::new(cause));
                        }
                    }
                }
                failure
            })
        })
    }
}
