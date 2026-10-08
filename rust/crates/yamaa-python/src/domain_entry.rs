//! Public-request argument/result conversion; the shared engine owns the lifecycle.
use pyo3::{exceptions::PyException, prelude::*, types::PyBytes};
#[cfg(unix)]
use std::sync::Mutex;
use yamaa_adapters::issue_rows::Issue;

pyo3::create_exception!(yamaa, DomainError, PyException);

#[pyclass(frozen, module = "yamaa._native", name = "_DomainResult")]
pub struct Domain {
    #[cfg(unix)]
    inner: Option<Mutex<yamaa_adapters::file_application::Domain>>,
    failures: Vec<Issue>,
}
#[pymethods]
impl Domain {
    fn output<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        #[cfg(unix)]
        if let Some(inner) = &self.inner {
            return inner
                .lock()
                .map(|inner| inner.output().map(|bytes| PyBytes::new(py, bytes)))
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                });
        }
        let _ = py;
        Ok(None)
    }
    fn issues(&self) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        #[cfg(unix)]
        if let Some(inner) = &self.inner {
            return inner
                .lock()
                .map(|inner| crate::specification_result::issue_rows(&inner.issues()))
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                });
        }
        Ok(crate::specification_result::issue_rows(&self.failures))
    }
    fn save(&self) -> PyResult<bool> {
        #[cfg(unix)]
        if let Some(inner) = &self.inner {
            return inner
                .lock()
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                })?
                .save()
                .map_err(|_| DomainError::new_err("cannot save a failed build"));
        }
        Err(DomainError::new_err("cannot save a failed build"))
    }
}

#[cfg(not(unix))]
fn unavailable() -> Vec<Issue> {
    vec![Issue::from_core(yamaa_core::application_issue::unsupported(
        "native_file_transport",
        None,
    ))
    .expect("application issue")]
}
#[pyfunction]
#[pyo3(signature=(specification,environment=None))]
pub fn _domain_file(py: Python<'_>, specification: &str, environment: Option<&str>) -> Domain {
    #[cfg(unix)]
    {
        let result = yamaa_adapters::file_application::domain(
            yamaa_engine::domain_entry::Request {
                specification,
                environment,
            },
            yamaa_adapters::specification_report::Identity {
                runtime: "python",
                runtime_version: py.version(),
                engine_version: yamaa_core::VERSION,
                example: "domain",
                specification,
                base_directory: ".",
            },
        );
        match result {
            Ok(inner) => Domain {
                inner: Some(Mutex::new(inner)),
                failures: Vec::new(),
            },
            Err(failures) => Domain {
                inner: None,
                failures,
            },
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (py, specification, environment);
        Domain {
            failures: unavailable(),
        }
    }
}
#[pyfunction]
#[pyo3(signature=(specification,environment=None))]
pub fn _check_file(
    py: Python<'_>,
    specification: &str,
    environment: Option<&str>,
) -> Vec<crate::specification_result::IssueRow> {
    #[cfg(unix)]
    {
        let issues = yamaa_adapters::file_application::check(
            yamaa_engine::domain_entry::Request {
                specification,
                environment,
            },
            yamaa_adapters::specification_report::Identity {
                runtime: "python",
                runtime_version: py.version(),
                engine_version: yamaa_core::VERSION,
                example: "check",
                specification,
                base_directory: ".",
            },
        );
        crate::specification_result::issue_rows(&issues)
    }
    #[cfg(not(unix))]
    {
        let _ = (py, specification, environment);
        crate::specification_result::issue_rows(&unavailable())
    }
}
