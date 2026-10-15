//! Thin owned Define result transport. Shared phases own every file and finding.
use pyo3::{exceptions::PyException, prelude::*, types::PyBytes};
use std::sync::Mutex;
pyo3::create_exception!(yamaa, DefineError, PyException);

#[pyclass(frozen, module = "yamaa._native", name = "_DefineResult")]
pub struct Define {
    #[cfg(any(unix, windows))]
    inner: Mutex<yamaa_adapters::file_define::Define>,
}
#[pymethods]
impl Define {
    fn issues(&self) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        #[cfg(any(unix, windows))]
        {
            self.inner
                .try_lock()
                .map(|result| crate::specification_result::issue_rows(result.issues()))
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
                })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(Vec::new())
        }
    }
    fn artifacts<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>, String)>> {
        #[cfg(any(unix, windows))]
        {
            self.inner
                .try_lock()
                .map(|result| {
                    result
                        .artifacts()
                        .map(|(path, bytes, state)| {
                            (path.into(), PyBytes::new(py, bytes), format!("{state:?}"))
                        })
                        .collect()
                })
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
                })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = py;
            Ok(Vec::new())
        }
    }
    fn capture_reads(&self) -> PyResult<usize> {
        #[cfg(any(unix, windows))]
        {
            self.inner
                .try_lock()
                .map(|result| result.capture_reads())
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
                })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(0)
        }
    }
    fn save(&self) -> PyResult<bool> {
        #[cfg(any(unix, windows))]
        {
            let mut result = self.inner.try_lock().map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Define result already borrowed")
            })?;
            result.save().map_err(|_| {
                DefineError::new_err(match result.failure() {
                    Some(yamaa_adapters::file_define::Failure::Publication { error, .. }) => {
                        error.message()
                    }
                    _ => "cannot save a failed Define result".into(),
                })
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(DefineError::new_err("native file transport unavailable"))
        }
    }
}
#[pyfunction]
#[pyo3(signature=(environment,standard,created,installed_version))]
pub fn _define_file(
    environment: &str,
    standard: &str,
    created: Option<&str>,
    installed_version: &str,
) -> Define {
    #[cfg(any(unix, windows))]
    {
        Define {
            inner: Mutex::new(yamaa_adapters::file_define::Define::prepare(
                environment,
                standard,
                created,
                installed_version,
                yamaa_engine::host_values::Language::Python,
            )),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (environment, standard, created, installed_version);
        Define {}
    }
}
