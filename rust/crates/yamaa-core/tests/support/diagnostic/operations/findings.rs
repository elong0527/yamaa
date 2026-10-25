//! The readers of the findings fixtures, one module for each file:
//! aggregates, codelist binding, an expression's grammar, inheritance, ODM
//! sources and a project's runtime.
use super::{column_type, document, external, text, value, value_type, File};
use crate::support::diagnostic::json::Json;
use yamaa_core::diagnostic::Diagnostic;

mod aggregates;
mod codelist_binding;
mod grammar;
mod inheritance;
mod odm;
mod runtime;

pub(super) const FILES: &[File] = &[
    fixture!("aggregates.json"),
    fixture!("codelist_binding.json"),
    fixture!("grammar.json"),
    fixture!("inheritance.json"),
    fixture!("odm.json"),
    fixture!("runtime.json"),
];

type Reader = fn(&str, &Json) -> Option<Vec<Diagnostic>>;

/// Each module's readers, by the operations it names.
const READERS: [Reader; 6] = [
    aggregates::run,
    codelist_binding::run,
    grammar::run,
    inheritance::run,
    odm::run,
    runtime::run,
];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    READERS.iter().find_map(|run| run(operation, input))
}
