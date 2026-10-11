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
    _preparation: Option<yamaa_adapters::file_workflow::SelectionError>,
    #[cfg(any(unix, windows))]
    producer: Option<crate::file_producer::Report>,
    failures: Vec<Issue>,
}
#[pymethods]
impl Domain {
    /// The public facade propagates control flow from this retained owner.
    fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return producer.propagate_domain_interrupt(py);
        }
        let _ = py;
        Ok(())
    }
    /// Private qualification borrows the actual retained whole attempt.
    fn producer_attempt(&self) -> Option<crate::file_producer::Attempt> {
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return Some(producer.domain_attempt());
        }
        None
    }
    fn producer_projection_failure(&self) -> Option<String> {
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return producer.domain_projection_failure();
        }
        None
    }
    fn preparation_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>)>> {
        #[cfg(any(unix, windows))]
        if let Some(error) = &self._preparation {
            let sources = error.retained_sources(65_536, 16_777_216).map_err(|_| {
                pyo3::exceptions::PyValueError::new_err(
                    "source evidence limit; original preparation remains retained",
                )
            })?;
            crate::file_producer::source_budget(
                sources
                    .iter()
                    .map(|(name, bytes)| (name.as_str(), bytes.as_ref())),
            )?;
            return Ok(sources
                .into_iter()
                .map(|(name, bytes)| (name, PyBytes::new(py, &bytes)))
                .collect());
        }
        let _ = py;
        Ok(Vec::new())
    }
    /// Qualification can inspect original host failures even if report projection refuses.
    fn project_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        #[cfg(any(unix, windows))]
        if let Some(project) = &self.project {
            return project.host_failures(py);
        }
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return producer.domain_failures(py);
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
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return Ok(producer.domain_observations());
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
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return Ok(producer
                .domain_output()
                .map(|bytes| PyBytes::new(py, bytes)));
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
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return Ok(crate::specification_result::issue_rows(
                &producer.domain_issues(),
            ));
        }
        Ok(crate::specification_result::issue_rows(&self.failures))
    }
    fn log<'py>(&self, py: Python<'py>, warning: bool) -> PyResult<Option<Bound<'py, PyBytes>>> {
        #[cfg(any(unix, windows))]
        if let Some(inner) = &self.inner {
            return inner
                .try_lock()
                .map(|inner| {
                    (if warning {
                        inner.warning_log()
                    } else {
                        inner.verification_log()
                    })
                    .map(|bytes| PyBytes::new(py, bytes))
                })
                .map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                });
        }
        let _ = (py, warning);
        Ok(None)
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
        #[cfg(any(unix, windows))]
        if self
            .producer
            .as_ref()
            .is_some_and(|result| result.domain_output().is_some())
        {
            return Err(DomainError::new_err(
                "producer workflow publication is not supported",
            ));
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
            let selected = match yamaa_adapters::file_workflow::select(
                specification,
                environment,
                yamaa_core::project_function::Language::Python,
            ) {
                Ok(selected) => selected,
                Err(error) => {
                    return Ok(Domain {
                        producer: None,
                        inner: None,
                        project: None,
                        failures: yamaa_adapters::project_application::selection_failure(
                            &error,
                            yamaa_core::project_function::Language::Python,
                        ),
                        _preparation: Some(error),
                    })
                }
            };
            let mut project = match selected {
                yamaa_adapters::file_workflow::Selected::Ordinary(project) => project,
                yamaa_adapters::file_workflow::Selected::Producer(graph) => {
                    let mut build = graph.into_build();
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
                    let fields =
                        std::sync::Arc::new(crate::specification_result::fields(&metadata)?);
                    let attempt =
                        crate::file_producer::build_selected(py, &mut build, fields, 16_777_216);
                    let producer = attempt.formatted(py, 16_777_216, true)?;
                    return Ok(Domain {
                        producer: Some(producer),
                        inner: None,
                        project: None,
                        _preparation: None,
                        failures: Vec::new(),
                    });
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
                    producer: None,
                    inner: Some(Mutex::new(inner)),
                    project: Some(attempt),
                    _preparation: None,
                    failures: Vec::new(),
                },
                Err(failures) => Domain {
                    producer: None,
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
                producer: None,
                inner: Some(Mutex::new(inner)),
                project: None,
                _preparation: None,
                failures: Vec::new(),
            },
            Err(failures) => Domain {
                producer: None,
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
