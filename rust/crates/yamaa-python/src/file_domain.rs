//! Host activation and result conversion for the engine-selected capabilities.
use crate::domain_entry::Domain;
use pyo3::{prelude::*, types::PyTuple};
use std::sync::Mutex;
use yamaa_adapters::{
    file_application::{self, RuntimePort},
    specification_report::Identity,
};
pub struct Runtime<'py, 'request> {
    pub py: Python<'py>,
    pub specification: &'request str,
}
impl Runtime<'_, '_> {
    fn identity(&self) -> Identity<'_> {
        Identity {
            runtime: "python",
            runtime_version: self.py.version(),
            engine_version: yamaa_engine::host_values::VERSION,
            example: "domain",
            specification: self.specification,
            base_directory: ".",
        }
    }
    fn plain_result(
        &self,
        result: Result<file_application::Domain, file_application::Failure>,
    ) -> Domain {
        match result {
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
        }
    }
}
impl RuntimePort for Runtime<'_, '_> {
    type Result = Domain;
    type Error = PyErr;
    fn plain(
        &mut self,
        prepared: yamaa_adapters::file_preparation::FileSpecification,
    ) -> PyResult<Domain> {
        Ok(self.plain_result(file_application::plain(prepared, self.identity())))
    }
    fn project(
        &mut self,
        mut project: Box<yamaa_adapters::file_project::FileProject>,
    ) -> PyResult<Domain> {
        let py = self.py;
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
                yamaa_engine::host_values::VERSION,
                "domain",
                self.specification,
                ".",
            ],
        )?;
        let result = crate::project_result::prepare(py, &attempt, &metadata)?;
        result.propagate_interrupt(py)?;
        let (attempt, result) = result.into_application(&project);
        let mut result = self.plain_result(result);
        result.project = Some(attempt);
        Ok(result)
    }
    fn producer(&mut self, graph: Box<yamaa_adapters::file_graph::FileGraph>) -> PyResult<Domain> {
        let py = self.py;
        let mut build = graph.into_build();
        let metadata = PyTuple::new(
            py,
            [
                py.version(),
                yamaa_engine::host_values::VERSION,
                "domain",
                self.specification,
                ".",
            ],
        )?;
        let fields = std::sync::Arc::new(crate::specification_result::fields(&metadata)?);
        let attempt = crate::file_producer::build_selected(
            py,
            &mut build,
            fields,
            yamaa_engine::capacity::REPORT_ENCODED_BYTES,
        );
        let producer = attempt.formatted(py, yamaa_engine::capacity::REPORT_ENCODED_BYTES, true)?;
        Ok(Domain {
            producer: Some(producer),
            inner: None,
            project: None,
            _preparation: None,
            failures: Vec::new(),
        })
    }
}
