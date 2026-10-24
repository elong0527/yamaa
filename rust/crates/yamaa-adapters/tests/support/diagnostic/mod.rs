//! The adapters' expected outcomes written as data, and the one replay that
//! reads them (#3691, #3798): `replay` reads the cases of
//! `tests/fixtures/diagnostics/` and compares what each reports with its
//! authored rows, tables and saved files, `form` holds the case form it
//! shares with core's replay, and `operations` holds the reader of each
//! operation a case names.
pub mod form;
pub mod operations;
pub mod replay;
