//! Public-request argument/result conversion; the shared engine owns the lifecycle.
use pyo3::{exceptions::PyException, prelude::*, types::PyBytes};
#[cfg(any(unix, windows))]
use std::sync::Mutex;
use yamaa_adapters::issue_rows::Issue;

pyo3::create_exception!(yamaa, DomainError, PyException);

#[pyclass(frozen, module = "yamaa._native", name = "_DomainResult")]
pub struct Domain {
    pub(crate) measurements:
        Option<std::sync::Arc<yamaa_adapters::application_measurements::Measurements>>,
    #[cfg(any(unix, windows))]
    pub(crate) inner: Option<Mutex<yamaa_adapters::file_application::Domain>>,
    #[cfg(any(unix, windows))]
    pub(crate) project: Option<crate::file_project::Attempt>,
    #[cfg(any(unix, windows))]
    pub(crate) _preparation: Option<yamaa_adapters::file_workflow::SelectionError>,
    #[cfg(any(unix, windows))]
    pub(crate) producer: Option<crate::file_producer::Report>,
    pub(crate) failures: Vec<Issue>,
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
            let sources = error
                .retained_sources(
                    yamaa_adapters::host_limits::RECORDS,
                    yamaa_adapters::host_limits::REPLY_BYTES,
                )
                .map_err(|_| {
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
    #[pyo3(signature=(include_measurements=false))]
    fn observations(&self, include_measurements: bool) -> PyResult<Option<String>> {
        let observed = (|| {
            #[cfg(any(unix, windows))]
            if let Some(inner) = &self.inner {
                return inner
                    .try_lock()
                    .map(|inner| Some(inner.observations_json()))
                    .map_err(|_| {
                        pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
                    });
            }
            #[cfg(any(unix, windows))]
            if let Some(producer) = &self.producer {
                return producer.domain_observations();
            }
            Ok(None)
        })()?;
        Ok(match (include_measurements, self.measurements.as_ref()) {
            (true, Some(measurements)) => Some(measurements.envelope(observed)),
            _ => observed,
        })
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
            return producer.domain_output(py);
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
                &producer.domain_issues()?,
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
        #[cfg(any(unix, windows))]
        if let Some(producer) = &self.producer {
            return producer.domain_log(py, warning);
        }
        let _ = (py, warning);
        Ok(None)
    }
    fn save(&self) -> PyResult<bool> {
        if let Some(measurements) = &self.measurements {
            measurements.save_entered();
        }
        let returned = (|| {
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
            if let Some(producer) = &self.producer {
                return producer.domain_save();
            }
            Err(DomainError::new_err("cannot save a failed build"))
        })();
        if let Some(measurements) = &self.measurements {
            measurements.save_completed(match &returned {
                Ok(true) => "succeeded",
                Ok(false) => "refused",
                Err(_) => "failed",
            });
        }
        returned
    }
}

impl Domain {
    fn measurement_accepted(&self) -> bool {
        #[cfg(any(unix, windows))]
        {
            if let Some(inner) = &self.inner {
                return inner
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .output()
                    .is_some();
            }
            if let Some(producer) = &self.producer {
                return producer.domain_accepted();
            }
        }
        false
    }
}

#[cfg(not(any(unix, windows)))]
fn unavailable() -> Vec<Issue> {
    vec![
        Issue::from_core(yamaa_engine::host_values::unavailable_file_transport())
            .expect("application issue"),
    ]
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
        let mut ports = yamaa_adapters::file_application::Application {
            host: yamaa_engine::host_values::Language::Python,
            runtime: crate::file_domain::Runtime { py, specification },
        };
        let measurements =
            std::sync::Arc::new(yamaa_adapters::application_measurements::Measurements::new());
        let returned = match yamaa_engine::domain_entry::domain_with_observer(
            yamaa_engine::domain_entry::Request {
                specification,
                environment,
            },
            &mut ports,
            yamaa_engine::application_observer::Scope::new(measurements.as_ref()),
        ) {
            Ok(result) => Ok(result),
            Err(yamaa_adapters::file_application::ApplicationError::Runtime(error)) => Err(error),
            Err(yamaa_adapters::file_application::ApplicationError::Preparation(error)) => {
                Ok(Domain {
                    producer: None,
                    measurements: None,
                    inner: None,
                    project: None,
                    failures: yamaa_adapters::project_application::selection_failure(
                        &error, ports.host,
                    ),
                    _preparation: Some(error),
                })
            }
        };
        match returned {
            Ok(mut result) => {
                measurements.complete(result.measurement_accepted());
                result.measurements = Some(measurements);
                Ok(result)
            }
            Err(error) => {
                measurements.complete(false);
                Err(error)
            }
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (py, specification, environment);
        Ok(Domain {
            measurements: None,
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
        let issues = yamaa_adapters::file_application::check(
            yamaa_engine::domain_entry::Request {
                specification,
                environment,
            },
            yamaa_adapters::specification_report::Identity {
                runtime: "python",
                runtime_version: py.version(),
                engine_version: yamaa_engine::host_values::VERSION,
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
        "python" => yamaa_engine::host_values::Language::Python,
        "r" => yamaa_engine::host_values::Language::R,
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
