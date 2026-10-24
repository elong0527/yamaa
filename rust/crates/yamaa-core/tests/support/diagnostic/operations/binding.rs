//! Part A2 of #3691: the readers and fixture files of the preflight and
//! binding tables, which are still written in Rust.
use super::File;
use crate::support::diagnostic::json::Json;
use yamaa_core::diagnostic::Diagnostic;

pub(super) const FILES: &[File] = &[];

pub(super) fn run(_operation: &str, _input: &Json) -> Option<Vec<Diagnostic>> {
    None
}
