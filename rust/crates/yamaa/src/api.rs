//! The host API both bindings expose: operations on a specification or a
//! project path. Hosts keep the paths in their own lazy objects; every call
//! here rereads the files, and every result is a fixed snapshot that writes
//! nothing until `save`.

use crate::cancel::CancellationToken;
use crate::diagnostic::{context_value_from_core, ContextValue, Report};
use crate::engine::{Build, Engine, Request, SaveOutcome};
use crate::error::Error;
use crate::table::Table;

/// A refused operation: a message for people and the issues that explain it.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Failure {
    pub message: String,
    pub issues: Table,
}

impl Failure {
    pub fn new(message: impl Into<String>, report: &Report) -> Self {
        Self {
            message: message.into(),
            issues: report.table(),
        }
    }

    pub fn from_error(context: &str, error: &Error) -> Self {
        match error {
            Error::Rejected(report) => Self::new(format!("{context}; see issues"), report),
            other => Self::new(format!("{context}: {other}"), &Report::default()),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// One specification run: its output dataset, issues and run report.
pub struct Run {
    build: Build,
}

impl Run {
    /// The output dataset, missing when the run failed.
    pub fn output(&self) -> Option<&Table> {
        self.build.output()
    }

    /// What is wrong, one row per finding; empty when nothing is.
    pub fn issues(&self) -> Table {
        self.build.report().table()
    }

    /// What the run read, counted and checked; missing when the run never
    /// began or, in a producer graph, when the entry never ran.
    pub fn report(&self) -> Option<&Table> {
        self.build.run_report()
    }

    /// Write the declared report file and the output dataset. A failed run
    /// writes only its report file and fails with the run's issues.
    pub fn save(&mut self) -> Result<(), Failure> {
        match self.build.save() {
            Ok(SaveOutcome::Saved) => Ok(()),
            Ok(_) => Err(Failure::new(
                "nothing was saved; see issues",
                self.build.report(),
            )),
            Err(error) => Err(Failure::from_error("cannot save a failed run", &error)),
        }
    }

    /// Borrow the underlying build for qualification evidence.
    #[doc(hidden)]
    pub fn build(&self) -> &Build {
        &self.build
    }
}

fn request<'a>(specification: &'a str, project: Option<&'a str>) -> Request<'a> {
    match project {
        Some(project) => Request::new(specification).with_environment(project),
        None => Request::new(specification),
    }
}

impl Engine {
    /// Check one specification without reading study data or running project
    /// code, with the project's codelists and functions when it has one.
    pub fn spec_check(
        &self,
        specification: &str,
        project: Option<&str>,
        token: &CancellationToken,
    ) -> Table {
        self.check(&request(specification, project), token).table()
    }

    /// Build one specification in memory; nothing is written until `save`.
    pub fn spec_run(
        &self,
        specification: &str,
        project: Option<&str>,
        token: &CancellationToken,
    ) -> Result<Run, Error> {
        self.build(&request(specification, project), token)
            .map(|build| Run { build })
    }

    /// The specification after its layers are composed, as one structured
    /// tree: the normalized model, each field's origin and each resource.
    pub fn spec_resolve(
        &self,
        specification: &str,
        project: Option<&str>,
    ) -> Result<ContextValue, Failure> {
        let resolved = self
            .resolve_specification(&request(specification, project))
            .map_err(|error| Failure::from_error("cannot resolve the specification", &error))?;
        let text = resolved
            .to_json()
            .map_err(|error| Failure::from_error("cannot resolve the specification", &error))?;
        let value = yamaa_adapters::json_wire::decode(&text).map_err(|_| {
            Failure::new(
                "cannot resolve the specification: invalid resolved facts",
                &Report::default(),
            )
        })?;
        Ok(context_value_from_core(&value))
    }
}
