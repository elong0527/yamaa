//! The readers of the operations the diagnostic cases name (#3691). Each
//! reads a case's `input` as the arguments of one adapters entry point,
//! refusing a member it does not take, calls it and returns every issue it
//! reports, in order, or none when it accepts the input. A reader asserts
//! nothing about what it returns: the replay compares that with the case's
//! authored rows. A reader may refuse an input whose setup fails, such as a
//! producer named under `built` that does not build.
//!
//! Each module owns its readers and the fixture files its cases are in.
use crate::support::diagnostic::replay::{field, items, object, text};
use crate::support::Scratch;
use yamaa_core::{diagnostic::Diagnostic, json::Value, project_function::Language};

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
mod project_scope;

/// What one operation reported, with the study directory its files were
/// written to, kept until the rows that name those files are compared.
pub struct Reported {
    pub issues: Vec<Diagnostic>,
    pub study: Option<Scratch>,
}

type Reader = fn(&str, &Value) -> Option<Reported>;

/// Each module's readers, by the operations it names, and its files.
const MODULES: [(Reader, &[File]); 2] = [
    (file_route::run, file_route::FILES),
    (project_scope::run, project_scope::FILES),
];

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

/// A fresh study directory holding the input's `files` (each relative path
/// and its text) and `directories` (empty ones), named canonically, as the
/// engine spells the paths it reports.
fn study(input: &Value) -> Scratch {
    let study = Scratch::canonical("diagnostics");
    for (name, file) in object(field(input, "files")) {
        study.write(name, text(file));
    }
    for name in input.get("directories").map_or(&[][..], items) {
        std::fs::create_dir_all(study.join(text(name))).unwrap();
    }
    study
}

/// The path of `name` in `study`, as a caller passes it, with forward
/// slashes.
fn path(study: &Scratch, name: &str) -> String {
    study.join(name).to_str().unwrap().replace('\\', "/")
}

/// The host language the input's `host` names, `python` unless written.
fn host(input: &Value) -> Language {
    match input.get("host").map(text) {
        None | Some("python") => Language::Python,
        Some("r") => Language::R,
        Some(other) => panic!("no host {other:?}"),
    }
}
