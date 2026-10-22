//! A study directory run through the file route, as every host runs it.
use std::path::PathBuf;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::{
    diagnostic::Diagnostic, domain_entry::Request, json::Value, project_function::Language,
};

use super::Scratch;

/// The identity of a test run: the test runtime, the engine's version and
/// paths relative to the working directory, for the case `example`.
pub fn identity<'a>(example: &'a str, specification: &'a str) -> Identity<'a> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example,
        specification,
        base_directory: ".",
    }
}

/// The row an author writes for an issue: its phase, condition, rule, spec
/// paths and context. Its location and expression geometry are left out;
/// `super::rows` keeps them.
pub fn issue_row(issue: &Diagnostic) -> Value {
    yamaa_adapters::json!({
        "phase": issue.phase(),
        "condition": issue.condition(),
        "rule": issue.rule(),
        "spec_paths": issue.spec_paths,
        "context": issue.context.clone(),
    })
}

/// The issue rows of a run, whether it built or was refused.
pub fn issues(outcome: Result<file_application::Domain, Vec<Diagnostic>>) -> Vec<Value> {
    let issues = match outcome {
        Ok(domain) => domain.issues(),
        Err(issues) => issues,
    };
    issues.iter().map(issue_row).collect()
}

/// A scratch study whose files a test writes and whose specification it runs.
pub struct Study {
    directory: Scratch,
    example: &'static str,
}

impl Study {
    pub fn new(example: &'static str) -> Self {
        Self {
            directory: Scratch::new(example),
            example,
        }
    }

    /// Write `text` to `name` inside the study and return its path.
    pub fn write(&self, name: &str, text: impl AsRef<[u8]>) -> PathBuf {
        self.directory.write(name, text)
    }

    /// The path of `name` inside the study, as the engine receives it.
    pub fn path(&self, name: &str) -> String {
        self.directory.join(name).to_str().unwrap().to_owned()
    }

    /// Run the specification `name` for the Python host.
    pub fn run(&self, name: &str) -> Result<file_application::Domain, Vec<Diagnostic>> {
        let specification = self.path(name);
        file_application::domain(
            Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            identity(self.example, &specification),
        )
    }

    /// The bytes the run saved to `name`.
    pub fn saved(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.directory.join(name)).unwrap()
    }
}
