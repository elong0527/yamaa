//! R host capabilities and rooted-result conversion for the shared engine.
use crate::domain_entry::State;
use extendr_api::prelude::*;
use std::rc::Rc;
use yamaa_adapters::{
    file_application::{self, RuntimePort},
    specification_report::Identity,
};
use yamaa_core::application_observer::{Outcome, Phase, Scope};
pub(crate) struct Runtime<'request> {
    specification: &'request str,
    version: &'request str,
    verify: Function,
    resolve: Function,
    formatter: Function,
    pub(crate) resolver: Option<std::sync::Arc<dyn yamaa_adapters::host_resources::Resolver>>,
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
            resolver: None,
        })
    }
    fn identity(&self) -> Identity<'_> {
        Identity {
            runtime: "r",
            runtime_version: self.version,
            engine_version: yamaa_core::VERSION,
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
            yamaa_core::VERSION,
            "domain",
            self.specification,
            ".",
        ]
    }
}
impl RuntimePort for Runtime<'_> {
    type Result = State;
    type Error = String;
    fn resolver(&self) -> Option<std::sync::Arc<dyn yamaa_adapters::host_resources::Resolver>> {
        self.resolver.clone()
    }
    fn plain(
        &mut self,
        prepared: yamaa_adapters::file_preparation::FileSpecification,
        scope: Scope<'_>,
    ) -> std::result::Result<State, String> {
        Ok(self.plain_result(file_application::plain(prepared, self.identity(), scope)))
    }
    fn project(
        &mut self,
        mut project: Box<yamaa_adapters::file_project::FileProject>,
        scope: Scope<'_>,
    ) -> std::result::Result<State, String> {
        let inner = project.build_with(
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
        let (state, outcome) = {
            let formatted = crate::project_result::platform::format(
                Rc::clone(&attempt),
                &fields,
                &self.formatter,
            );
            let result = match formatted {
                Ok(result) => file_application::Domain::from_project(result, &project),
                Err(refusal) => Err(refusal.issues()),
            };
            let outcome = if result.is_ok() {
                Outcome::Succeeded
            } else {
                Outcome::Failed
            };
            let mut state = self.plain_result(result);
            state.project = Some(attempt);
            (state, outcome)
        };
        scope.exit(Phase::Output, outcome);
        Ok(state)
    }
    fn producer(
        &mut self,
        graph: Box<yamaa_adapters::file_graph::FileGraph>,
        scope: Scope<'_>,
    ) -> std::result::Result<State, String> {
        let mut build = graph.into_build();
        let fields = self.fields();
        let producer = crate::file_producer::platform::build_selected(
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
