//! R host capabilities and rooted-result conversion for the shared engine.
use crate::domain_entry::State;
use extendr_api::prelude::*;
use std::rc::Rc;
use yamaa_adapters::{
    file_application::{self, RuntimePort},
    specification_report::Identity,
};
use yamaa_engine::application_observer::{Noop, Outcome, Phase, Scope};
pub(crate) struct Runtime<'request> {
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
                measurements: None,
                result: Some(result),
                project: None,
                _preparation: None,
                issues: Vec::new(),
            },
            Err(issues) => State {
                producer: None,
                measurements: None,
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
        self.plain_with_observer(prepared, Scope::new(&Noop))
    }
    fn plain_with_observer(
        &mut self,
        prepared: yamaa_adapters::file_preparation::FileSpecification,
        scope: Scope<'_>,
    ) -> std::result::Result<State, String> {
        Ok(self.plain_result(file_application::plain_with_observer(
            prepared,
            self.identity(),
            scope,
        )))
    }
    fn project(
        &mut self,
        project: Box<yamaa_adapters::file_project::FileProject>,
    ) -> std::result::Result<State, String> {
        self.project_with_observer(project, Scope::new(&Noop))
    }
    fn project_with_observer(
        &mut self,
        mut project: Box<yamaa_adapters::file_project::FileProject>,
        scope: Scope<'_>,
    ) -> std::result::Result<State, String> {
        let inner = project.build_with_observer(
            |run| {
                crate::project_activation::Port::new(
                    run.captured_environment()
                        .lock()
                        .map_or(&[], |lock| &lock.source.bytes),
                    self.verify.clone(),
                    self.resolve.clone(),
                )
            },
            scope,
        );
        let attempt = Rc::new(crate::file_project::platform::HeldAttempt {
            inner,
            run: project.retained_run(),
        });
        let fields = self.fields().map(str::to_owned);
        scope.phase(Phase::Output);
        let converted = (|| {
            let formatted = crate::project_result::platform::format(
                Rc::clone(&attempt),
                &fields,
                &self.formatter,
            );
            let result = match formatted.formatted {
                Some(result) => file_application::Domain::from_project(result, &project),
                None => Err(file_application::rejected("build", formatted.refusal)),
            };
            let mut state = self.plain_result(result);
            state.project = Some(attempt);
            Ok(state)
        })();
        scope.exit(
            Phase::Output,
            if converted.is_ok() {
                Outcome::Succeeded
            } else {
                Outcome::Failed
            },
        );
        converted
    }
    fn producer(
        &mut self,
        graph: Box<yamaa_adapters::file_graph::FileGraph>,
    ) -> std::result::Result<State, String> {
        self.producer_with_observer(graph, Scope::new(&Noop))
    }
    fn producer_with_observer(
        &mut self,
        graph: Box<yamaa_adapters::file_graph::FileGraph>,
        scope: Scope<'_>,
    ) -> std::result::Result<State, String> {
        let mut build = graph.into_build();
        let fields = self.fields();
        if fields
            .iter()
            .any(|text| text.len() > yamaa_adapters::host_limits::IDENTITY_BYTES)
        {
            return Err("producer identity byte limit".into());
        }
        let producer = crate::file_producer::platform::build_selected_with_observer(
            &mut build,
            fields.map(str::to_owned).to_vec(),
            self.verify.clone(),
            self.resolve.clone(),
            self.formatter.clone(),
            scope,
        )?;
        Ok(State {
            producer: Some(producer),
            measurements: None,
            result: None,
            project: None,
            _preparation: None,
            issues: Vec::new(),
        })
    }
}
