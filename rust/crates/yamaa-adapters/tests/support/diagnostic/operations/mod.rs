//! The readers of the operations the diagnostic cases name (#3691). Each
//! reads a case's `input` as the arguments of one adapters entry point,
//! refusing a member it does not take, calls it and returns every issue it
//! reports, in order, or none when it accepts the input. A reader asserts
//! nothing about what it returns: the replay compares that with the case's
//! authored rows. A reader may refuse an input whose setup fails, such as a
//! producer named under `built` that does not build.
//!
//! Each module owns its readers and the fixture files its cases are in.
use crate::support::Scratch;
use yamaa_core::{diagnostic::Diagnostic, json::Value};

/// A fixture file of `tests/fixtures/diagnostics/`, read at compile time, so
/// renaming or deleting it fails the build rather than dropping its cases.
pub struct File {
    pub name: &'static str,
    pub text: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        crate::support::diagnostic::operations::File {
            name: $name,
            text: include_str!(concat!("../../../fixtures/diagnostics/", $name)),
        }
    };
}

mod file_route;

/// What one operation reported, with the study directory its files were
/// written to, kept until the rows that name those files are compared.
pub struct Reported {
    pub issues: Vec<Diagnostic>,
    pub study: Option<Scratch>,
}

type Reader = fn(&str, &Value) -> Option<Reported>;

/// Each module's readers, by the operations it names, and its files.
const MODULES: [(Reader, &[File]); 1] = [(file_route::run, file_route::FILES)];

/// What `operation` reports for `input`.
pub fn run(operation: &str, input: &Value) -> Reported {
    MODULES
        .iter()
        .find_map(|(run, _)| run(operation, input))
        .unwrap_or_else(|| panic!("no reader for the operation {operation:?}"))
}

/// Every fixture file the modules read.
pub fn files() -> impl Iterator<Item = &'static File> {
    MODULES.iter().flat_map(|(_, files)| files.iter())
}
