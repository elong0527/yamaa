//! R host capabilities and rooted-result conversion for the shared engine.
use crate::domain_entry::State;
use extendr_api::prelude::*;
use std::rc::Rc;
use yamaa_adapters::{
    file_application::{self, RuntimePort},
    specification_report::Identity,
};
pub struct Runtime<'request> {
    specification: &'request str,
    version: &'request str,
    verify: Function,
    resolve: Function,
    formatter: Function,
}
impl<'request> Runtime<'request> {
    pub fn new(
        specification: &'request str,
        version: &'request str,
        capabilities: List,
    ) -> std::result::Result<Self, String> {
        if capabilities.len() != 3 {
            return Err("invalid project host capabilities".into());
        }
        let verify = capabilities
            .elt(0)
            .map_err(|_| "missing lock capability")?
            .as_function()
            .ok_or("invalid lock capability")?;
        let resolve = capabilities
            .elt(1)
            .map_err(|_| "missing binding capability")?
            .as_function()
            .ok_or("invalid binding capability")?;
        let formatter = capabilities
            .elt(2)
            .map_err(|_| "missing condition capability")?
            .as_function()
            .ok_or("invalid condition capability")?;
        Ok(Self {
            specification,
            version,
            verify,
            resolve,
            formatter,
        })
    }
    fn identity(&self) -> Identity<'_> {
        Identity {
            runtime: "r",
            runtime_version: self.version,
            engine_version: yamaa_engine::host_values::VERSION,
            example: "domain",
            specification: self.specification,
            base_directory: ".",
        }
    }
    fn plain_result(
        &self,
        result: std::result::Result<file_application::Domain, file_application::Failure>,
    ) -> State {
        match result {
            Ok(result) => State {
                producer: None,
                result: Some(result),
                project: None,
                _preparation: None,
                issues: Vec::new(),
            },
            Err(issues) => State {
                producer: None,
                result: None,
                project: None,
                _preparation: None,
                issues,
            },
        }
    }
    fn fields(&self) -> [&str; 5] {
        [
            self.version,
            yamaa_engine::host_values::VERSION,
            "domain",
            self.specification,
            ".",
        ]
    }
}
impl RuntimePort for Runtime<'_> {
    type Result = State;
    type Error = String;
    fn plain(
        &mut self,
        prepared: yamaa_adapters::file_preparation::FileSpecification,
    ) -> std::result::Result<State, String> {
        Ok(self.plain_result(file_application::plain(prepared, self.identity())))
    }
    fn project(
        &mut self,
        mut project: Box<yamaa_adapters::file_project::FileProject>,
    ) -> std::result::Result<State, String> {
        let inner = project.build_with(|run| {
            crate::project_activation::Port::new(
                run.captured_environment()
                    .lock()
                    .map_or(&[], |lock| &lock.source.bytes),
                self.verify.clone(),
                self.resolve.clone(),
            )
        });
        let attempt = Rc::new(crate::file_project::platform::HeldAttempt {
            inner,
            run: project.retained_run(),
        });
        let fields = self.fields().map(str::to_owned);
        let formatted =
            crate::project_result::platform::format(Rc::clone(&attempt), &fields, &self.formatter);
        let result = match formatted.formatted {
            Some(result) => file_application::Domain::from_project(result, &project),
            None => Err(file_application::rejected("build", formatted.refusal)),
        };
        let mut state = self.plain_result(result);
        state.project = Some(attempt);
        Ok(state)
    }
    fn producer(
        &mut self,
        graph: Box<yamaa_adapters::file_graph::FileGraph>,
    ) -> std::result::Result<State, String> {
        let mut build = graph.into_build();
        let fields = self.fields();
        if fields
            .iter()
            .any(|text| text.len() > yamaa_adapters::host_limits::IDENTITY_BYTES)
        {
            return Err("producer identity byte limit".into());
        }
        let producer = crate::file_producer::platform::build_selected(
            &mut build,
            fields.map(str::to_owned).to_vec(),
            self.verify.clone(),
            self.resolve.clone(),
            self.formatter.clone(),
        )?;
        Ok(State {
            producer: Some(producer),
            result: None,
            project: None,
            _preparation: None,
            issues: Vec::new(),
        })
    }
}
