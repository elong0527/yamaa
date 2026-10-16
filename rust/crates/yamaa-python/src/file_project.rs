//! The actual project attempt retained by a public file-domain result.
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

type NativeAttempt = yamaa_adapters::project_run::Attempt<
    yamaa_adapters::file_resources::Error,
    crate::project_activation::Error,
>;

pub(super) struct Attempt {
    pub(super) inner: Arc<Mutex<NativeAttempt>>,
    pub(super) run: Arc<yamaa_adapters::project_run::PreparedRun>,
}
impl Attempt {
    pub(super) fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        let state = self.inner.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "native project attempt is already borrowed or poisoned",
            )
        })?;
        Ok(crate::project_activation::host_failure_values(
            py,
            |visit| {
                yamaa_adapters::project_attempt::visit_host_failures(&state, |stage, error| {
                    visit(stage, error);
                });
            },
        ))
    }
}
