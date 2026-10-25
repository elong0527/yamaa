//! Finite factories and observers owned by the yamaa-adapters integration suite.
use yamaa_core::schema as document_schema;
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod document;
pub mod inheritance;
#[cfg(feature = "odm")]
pub mod odm_files;
#[cfg(feature = "parquet")]
pub mod parquet_files;
pub mod roots;
pub mod sas7bdat;
pub mod scratch;
pub mod xport;

/// Construct an ordered schema independently of plan compilation.
pub fn table_schema(columns: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|&(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    )
    .unwrap()
}

/// Diagnostics as the run report renders them, for comparison with authored
/// rows.
pub fn rows(issues: &[yamaa_core::diagnostic::Diagnostic]) -> Vec<yamaa_core::json::Value> {
    issues
        .iter()
        .map(yamaa_adapters::report_storage::diagnostic_value)
        .collect()
}

/// The report rows of a refusal's language findings, as authored fixtures
/// record them; any other refusal fails the test.
pub fn findings(
    rejection: yamaa_adapters::specification_diagnostics::Rejection,
) -> yamaa_core::json::Value {
    match rejection {
        yamaa_adapters::specification_diagnostics::Rejection::Invalid(findings) => {
            yamaa_core::json::Value::Array(rows(&findings))
        }
        other => panic!("expected language findings: {other:?}"),
    }
}

pub use scratch::Scratch;

/// Replace exactly one occurrence, so an edit can never silently miss.
#[track_caller]
pub fn edit(document: &str, original: &str, replacement: &str) -> String {
    assert_eq!(
        document.matches(original).count(),
        1,
        "expected exactly one {original:?}"
    );
    document.replacen(original, replacement, 1)
}

/// Exact-once edits on any text, as `edit` makes them: `body.edit(from, to)`
/// fails the test when `from` occurs other than once.
pub trait Edit {
    fn edit(&self, from: &str, to: &str) -> String;
}

impl Edit for str {
    #[track_caller]
    fn edit(&self, from: &str, to: &str) -> String {
        edit(self, from, to)
    }
}
