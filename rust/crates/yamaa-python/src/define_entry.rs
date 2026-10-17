//! Thin owned Define result transport. Shared phases own every file and finding.
use pyo3::{exceptions::PyException, prelude::*, types::PyBytes};
use std::sync::Mutex;
use yamaa::{CancellationToken, DefineRequest, Engine, EngineOptions, HostLanguage, SaveOutcome};
pyo3::create_exception!(yamaa, DefineError, PyException);

#[pyclass(frozen, module = "yamaa._native", name = "_DefineResult")]
pub struct Define {
    inner: Mutex<yamaa::DefineBuild>,
}
#[pymethods]
impl Define {
    fn issues<'py>(&self, py: Python<'py>) -> PyResult<Vec<crate::report_fields::IssueRow<'py>>> {
        {
            let result = self.inner.try_lock().map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
            })?;
            crate::report_fields::issue_rows(py, &result.report().diagnostics)
        }
    }
    fn artifacts<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>, String)>> {
        {
            self.inner
                .try_lock()
                .map(|result| {
                    result
                        .artifacts()
                        .into_iter()
                        .map(|artifact| {
                            (
                                artifact.path,
                                PyBytes::new(py, &artifact.bytes),
                                artifact.state,
                            )
                        })
                        .collect()
                })
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
                })
        }
    }
    fn capture_reads(&self) -> PyResult<usize> {
        {
            self.inner
                .try_lock()
                .map(|result| result.capture_reads())
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
                })
        }
    }
    fn save(&self) -> PyResult<bool> {
        {
            let mut result = self.inner.try_lock().map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
            })?;
            match result.save() {
                Ok(SaveOutcome::Saved) => Ok(true),
                Ok(_) => Ok(false),
                Err(_) => Err(DefineError::new_err(
                    result
                        .save_error_message()
                        .unwrap_or_else(|| "cannot save a failed Define result".into()),
                )),
            }
        }
    }
}
#[pyfunction]
#[pyo3(signature=(environment,standard,created,installed_version))]
pub fn _define_file(
    py: Python<'_>,
    environment: &str,
    standard: &str,
    created: Option<&str>,
    installed_version: &str,
) -> Define {
    let mut request = DefineRequest::new(environment, standard, installed_version);
    if let Some(created) = created {
        request = request.with_created(created);
    }
    let token = CancellationToken::new();
    let engine = Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "python".into(),
        py.version().into(),
    ));
    // Define composition is infallible at the boundary; failures are carried
    // by the result. Cancellation cannot trigger here with a fresh token.
    let define = engine
        .define(&request, &token)
        .expect("define composition with a fresh token");
    Define {
        inner: Mutex::new(define),
    }
}
