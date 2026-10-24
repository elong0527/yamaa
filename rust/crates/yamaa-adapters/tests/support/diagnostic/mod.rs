//! The adapters' expected diagnostics written as data, and the one replay
//! that reads them (#3691): `replay` reads the cases of
//! `tests/fixtures/diagnostics/` and compares what each reports with its
//! authored rows, and `operations` holds the reader of each operation a case
//! names.
pub mod operations;
pub mod replay;
