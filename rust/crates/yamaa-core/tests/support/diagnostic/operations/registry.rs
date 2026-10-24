//! Part C of #3691: the readers and fixture files of the records the
//! registry walk and the project environment still write in Rust.
use super::File;
use crate::support::diagnostic::json::Json;
use yamaa_core::diagnostic::Diagnostic;

pub(super) const FILES: &[File] = &[];

pub(super) fn run(_operation: &str, _input: &Json) -> Option<Vec<Diagnostic>> {
    None
}
