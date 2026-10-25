//! The readers of the operations the cases name (#3691, #3798). Each reads a
//! case's `input` as the arguments of one adapters entry point, refusing a
//! member it does not take, calls it and returns every issue it reports, in
//! order, or none when it accepts the input, with the output table and run
//! report it built and the files its save created, changed or removed. A
//! reader asserts nothing about what it returns: the replay compares that
//! with the case's authored rows.
//! A reader may refuse an input whose setup fails, such as a producer named
//! under `built` that does not build.
//!
//! Each module owns its readers and the fixture files its cases are in.
use crate::support::diagnostic::{
    form::{self, Files, Outcome},
    replay::{edit, items, object, text},
};
use crate::support::Scratch;
use yamaa_adapters::public_table::{PublicColumn, PublicTable};
use yamaa_core::{
    diagnostic::Diagnostic, json::Value, project_environment::Submission,
    project_function::Language, table::ValueRef, temporal::Precision, value::FiniteFloat,
};

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

mod define;
mod file_route;
mod host_functions;
mod odm;
mod project_scope;

/// What one operation reported, with the study directory its files were
/// written to, kept until the rows that name those files are compared.
pub struct Reported {
    pub issues: Vec<Diagnostic>,
    pub study: Option<Scratch>,
    /// The tables the operation built and the files its save created,
    /// changed or removed.
    pub outcome: Outcome,
}

type Reader = fn(&str, &Value) -> Option<Reported>;

/// Each module's readers, by the operations it names, and its files.
const MODULES: [(Reader, &[File]); 5] = [
    (define::run, define::FILES),
    (file_route::run, file_route::FILES),
    (host_functions::run, host_functions::FILES),
    (odm::run, odm::FILES),
    (project_scope::run, project_scope::FILES),
];

/// What `operation` reports for `input`.
pub fn run(operation: &str, input: &Value) -> Reported {
    MODULES
        .iter()
        .find_map(|(run, _)| run(operation, input))
        .unwrap_or_else(|| panic!("no reader for the operation {operation:?}"))
}

/// The calls the scripted host-functions port of `operation`, one of
/// [`host_functions`]'s, received for `input`, and where its run started
/// reading its study input, in order.
pub fn calls(operation: &str, input: &Value) -> Vec<String> {
    host_functions::logged(operation, input)
        .unwrap_or_else(|| panic!("{operation:?} runs no scripted host"))
        .1
}

/// Every fixture file the modules read.
pub fn files() -> impl Iterator<Item = &'static File> {
    MODULES.iter().flat_map(|(_, files)| files.iter())
}

/// The members of an input that stage its study directory, which each
/// reader's own members join.
const STUDY: [&str; 4] = ["benchmark", "edits", "files", "directories"];

/// A fresh study directory, named canonically, as the engine spells the
/// paths it reports, holding what the input stages: the files of the
/// benchmark `benchmark` names, each at its path in the benchmark
/// (`support::benchmarks::stage`), with the `edits` the input maps it to;
/// the input's `files` (each relative path and its text), none of them a
/// file the benchmark stages; and its `directories` (empty ones).
pub fn study(input: &Value) -> Scratch {
    let study = Scratch::canonical("diagnostics");
    let staged = input.get("benchmark").map_or_else(Vec::new, |name| {
        crate::support::benchmarks::stage(text(name), &study)
    });
    for (name, edits) in input.get("edits").map(object).into_iter().flatten() {
        assert!(
            staged.contains(name),
            "the edits of {name} name no file the benchmark stages"
        );
        let file = study.join(name);
        let edited = edit(std::fs::read_to_string(&file).unwrap(), items(edits), name);
        std::fs::write(file, edited).unwrap();
    }
    for (name, file) in input.get("files").map(object).into_iter().flatten() {
        assert!(
            !staged.contains(name),
            "{name} is a file the benchmark stages, which changes only by its edits"
        );
        study.write(name, text(file));
    }
    for name in input.get("directories").map_or(&[][..], items) {
        std::fs::create_dir_all(study.join(text(name))).unwrap();
    }
    study
}

/// Build and save, in order, each specification of `study` that `built`
/// names, for the `host`, with the project file `environment` names as each
/// request's environment, if any: each must build without an issue and save.
/// A reader whose input names `built` calls this before its own call, and so
/// does a sidecar that repeats that call.
pub fn build_first(study: &Scratch, built: &[Value], environment: Option<&str>, host: Language) {
    for name in built {
        let built = path(study, text(name));
        let mut domain = crate::support::study::domain(
            host,
            yamaa_core::domain_entry::Request {
                specification: &built,
                environment,
            },
            crate::support::study::identity("diagnostics", &built),
        )
        .unwrap_or_else(|issues| panic!("{built} is refused: {issues:?}"));
        assert!(domain.issues().is_empty(), "{built}: {:?}", domain.issues());
        assert!(domain.save().unwrap(), "{built} saves");
    }
}

/// The submission section `name` names: `sdtm`, `adam` or `send`.
pub fn section(name: &Value) -> Submission {
    match text(name) {
        "sdtm" => Submission::Sdtm,
        "adam" => Submission::Adam,
        "send" => Submission::Send,
        other => panic!("no section {other:?}"),
    }
}

/// The path of `name` in `study`, as a caller passes it, with forward
/// slashes.
fn path(study: &Scratch, name: &str) -> String {
    study.join(name).to_str().unwrap().replace('\\', "/")
}

/// Every file of `study` and its bytes, by its path there with forward
/// slashes.
pub fn contents(study: &Scratch) -> Files {
    state(study).files
}

/// A staged study's files and directories, including empty ones, so API
/// seams retain the original path-existence assertions too.
#[derive(Debug, PartialEq, Eq)]
pub struct StudyState {
    files: Files,
    directories: std::collections::BTreeSet<String>,
}

pub fn state(study: &Scratch) -> StudyState {
    fn walk(root: &std::path::Path, directory: &std::path::Path, state: &mut StudyState) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            if path.is_dir() {
                state.directories.insert(name);
                walk(root, &path, state);
            } else {
                state.files.insert(name, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut state = StudyState {
        files: Files::new(),
        directories: Default::default(),
    };
    walk(study, study, &mut state);
    state
}

/// The rows of a public table as a case writes them.
fn rows(table: &PublicTable) -> Vec<Value> {
    form::rows(table.names(), table.rows(), |row, column| {
        cell(table.column(column), row)
    })
}

/// The value at `row` of `column`, a date read from its days and a datetime
/// from its seconds.
fn cell(column: &PublicColumn, row: usize) -> ValueRef<'_> {
    use yamaa_adapters::epoch::{date_from_days, datetime_from_seconds};
    let missing = ValueRef::Missing;
    match column {
        PublicColumn::Str(values) => values.get(row).map_or(missing, ValueRef::Str),
        PublicColumn::Int(values) => values.get(row).map_or(missing, ValueRef::Int),
        PublicColumn::Float(values) => values.get(row).map_or(missing, |value| {
            ValueRef::Float(FiniteFloat::new(value).expect("a finite float"))
        }),
        PublicColumn::Date(values) => values.get(row).map_or(missing, |days| {
            ValueRef::Date(date_from_days(days.into(), Precision::Day).expect("a date"))
        }),
        PublicColumn::DateTime(values) => values.get(row).map_or(missing, |seconds| {
            ValueRef::DateTime(
                datetime_from_seconds(seconds, Precision::Second).expect("a datetime"),
            )
        }),
        PublicColumn::List(_) => panic!("an output or report column holds no list"),
    }
}

/// The host language the input's `host` names, `python` unless written.
fn host(input: &Value) -> Language {
    match input.get("host").map(text) {
        None | Some("python") => Language::Python,
        Some("r") => Language::R,
        Some(other) => panic!("no host {other:?}"),
    }
}
