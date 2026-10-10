//! Public-request argument/result conversion; the shared engine owns the lifecycle.
use pyo3::{
    exceptions::PyException,
    prelude::*,
    types::{PyBytes, PyTuple},
};
#[cfg(any(unix, windows))]
use std::sync::Mutex;
use yamaa_adapters::issue_rows::Issue;

pyo3::create_exception!(yamaa, DomainError, PyException);

#[pyclass(frozen, module = "yamaa._native", name = "_DomainResult")]
pub struct Domain {
    #[cfg(any(unix, windows))]
    inner: Option<Mutex<yamaa_adapters::file_application::Domain>>,
    #[cfg(any(unix, windows))]
    project: Option<crate::file_project::Attempt>,
    #[cfg(any(unix, windows))]
    _preparation: Option<yamaa_adapters::project_application::Error>,
    failures: Vec<Issue>,
}
#[pymethods]
impl Domain {
    /// Qualification can inspect original host failures even if report projection refuses.
    fn project_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        #[cfg(any(unix, windows))]
        if let Some(project) = &self.project {
            return project.host_failures(py);
        }
        let _ = py;
        Ok(Vec::new())
    }
    /// Private installed-run evidence; unsupported preparation has no build report.
    fn observations(&self) -> PyResult<Option<String>> {
        #[cfg(any(unix, windows))]
        if let Some(inner) = &self.inner {
            return inner
                .try_lock()
                .map(|inner| Some(inner.observations().to_string()))
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                });
        }
        Ok(None)
    }
    fn output<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        #[cfg(any(unix, windows))]
        if let Some(inner) = &self.inner {
            return inner
                .try_lock()
                .map(|inner| inner.output().map(|bytes| PyBytes::new(py, bytes)))
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                });
        }
        let _ = py;
        Ok(None)
    }
    fn issues(&self) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        #[cfg(any(unix, windows))]
        if let Some(inner) = &self.inner {
            return inner
                .try_lock()
                .map(|inner| crate::specification_result::issue_rows(&inner.issues()))
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                });
        }
        Ok(crate::specification_result::issue_rows(&self.failures))
    }
    fn save(&self) -> PyResult<bool> {
        #[cfg(any(unix, windows))]
        if let Some(inner) = &self.inner {
            return inner
                .try_lock()
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                })?
                .save()
                .map_err(|_| DomainError::new_err("cannot save a failed build"));
        }
        Err(DomainError::new_err("cannot save a failed build"))
    }
}

#[cfg(not(any(unix, windows)))]
fn unavailable() -> Vec<Issue> {
    vec![Issue::from_core(yamaa_core::application_issue::unsupported(
        "native_file_transport",
        None,
    ))
    .expect("application issue")]
}
#[pyfunction]
#[pyo3(signature=(specification,environment=None))]
pub fn _domain_file(
    py: Python<'_>,
    specification: &str,
    environment: Option<&str>,
) -> PyResult<Domain> {
    #[cfg(any(unix, windows))]
    {
        if let Some(environment) = environment {
            let mut project = match yamaa_adapters::project_application::prepare(
                specification,
                environment,
                yamaa_core::project_function::Language::Python,
            ) {
                Ok(project) => project,
                Err(error) => {
                    return Ok(Domain {
                        inner: None,
                        project: None,
                        failures: yamaa_adapters::project_application::failure(
                            &error,
                            yamaa_core::project_function::Language::Python,
                        ),
                        _preparation: Some(error),
                    })
                }
            };
            let inner = project.build_with(|run| {
                crate::project_activation::Port::new(
                    py,
                    run.captured_environment()
                        .lock()
                        .map_or(&[], |lock| &lock.source.bytes),
                )
            });
            let attempt = crate::file_project::Attempt {
                inner: std::sync::Arc::new(Mutex::new(inner)),
                run: project.retained_run(),
            };
            let metadata = PyTuple::new(
                py,
                [
                    py.version(),
                    yamaa_core::VERSION,
                    "domain",
                    specification,
                    ".",
                ],
            )?;
            let result = crate::project_result::prepare(py, &attempt, &metadata)?;
            result.propagate_interrupt(py)?;
            let (attempt, result) = result.into_application(&project);
            return Ok(match result {
                Ok(inner) => Domain {
                    inner: Some(Mutex::new(inner)),
                    project: Some(attempt),
                    _preparation: None,
                    failures: Vec::new(),
                },
                Err(failures) => Domain {
                    inner: None,
                    project: Some(attempt),
                    _preparation: None,
                    failures,
                },
            });
        }
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
        Ok(match result {
            Ok(inner) => Domain {
                inner: Some(Mutex::new(inner)),
                project: None,
                _preparation: None,
                failures: Vec::new(),
            },
            Err(failures) => Domain {
                inner: None,
                project: None,
                _preparation: None,
                failures,
            },
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (py, specification, environment);
        Ok(Domain {
            failures: unavailable(),
        })
    }
}
#[pyfunction]
#[pyo3(signature=(specification,environment=None))]
pub fn _check_file(
    py: Python<'_>,
    specification: &str,
    environment: Option<&str>,
) -> Vec<crate::specification_result::IssueRow> {
    #[cfg(any(unix, windows))]
    {
        if let Some(environment) = environment {
            return crate::specification_result::issue_rows(
                &yamaa_adapters::project_application::check(
                    specification,
                    environment,
                    yamaa_core::project_function::Language::Python,
                ),
            );
        }
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
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (py, specification, environment);
        crate::specification_result::issue_rows(&unavailable())
    }
}

/// Repository qualification may statically inspect either host's metadata.
/// This grants no activation or callable-resolution authority.
#[pyfunction]
pub fn _check_project_metadata(
    specification: &str,
    environment: &str,
    language: &str,
) -> PyResult<Vec<crate::specification_result::IssueRow>> {
    let host = match language {
        "python" => yamaa_core::project_function::Language::Python,
        "r" => yamaa_core::project_function::Language::R,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid project language",
            ))
        }
    };
    #[cfg(any(unix, windows))]
    {
        Ok(crate::specification_result::issue_rows(
            &yamaa_adapters::project_application::check(specification, environment, host),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (specification, environment, host);
        Ok(crate::specification_result::issue_rows(&unavailable()))
    }
}
