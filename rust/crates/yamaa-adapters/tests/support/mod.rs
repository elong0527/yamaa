//! Finite factories and observers owned by the yamaa-adapters integration suite.
use yamaa_core::schema as document_schema;
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod aggregate_syntax;
pub mod benchmarks;
pub mod ipc_snapshot;
pub mod numeric_syntax;
pub mod odm_declaration;
#[cfg(feature = "odm")]
pub mod odm_files;
#[cfg(feature = "parquet")]
pub mod parquet_files;
pub mod predicate_syntax;
pub mod project_call_compiler;
pub mod project_codelist_compiler;
pub mod project_schemas;
pub mod roots;
pub mod sas7bdat;
pub mod schema_declaration;
pub mod scratch;
pub mod string_template_syntax;
pub mod syntax_spans;
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

pub mod document;

pub mod specification_run;

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

/// The report rows of a native preparation refusal's language findings.
pub fn preparation_findings(
    error: yamaa_adapters::file_preparation::Error,
) -> yamaa_core::json::Value {
    use yamaa_adapters::{file_preparation::Error, specification_diagnostics};
    findings(match error {
        Error::Preparation(error) => {
            specification_diagnostics::inheritance_failure_ref(&error).expect("semantic report")
        }
        Error::ResourceFinding { diagnostic, .. }
        | Error::PreparationFinding { diagnostic, .. } => {
            specification_diagnostics::Rejection::Invalid(vec![*diagnostic])
        }
        Error::Compile(error) => specification_diagnostics::failure(&error, None),
        error => panic!("unexpected native rejection: {error:?}"),
    })
}

/// The message of the resource error a native preparation refusal retains.
pub fn retained_resource_message(error: yamaa_adapters::file_preparation::Error) -> String {
    use yamaa_adapters::{file_preparation::Error, specification_diagnostics};
    match error {
        Error::Resource(error) => error.message().into(),
        Error::Preparation(error) => {
            match specification_diagnostics::inheritance_failure_ref(&error) {
                Err(resource) => resource.message().into(),
                Ok(rejection) => panic!("expected a retained resource error: {rejection:?}"),
            }
        }
        error => panic!("expected a retained resource error: {error:?}"),
    }
}

/// The host that each runtime name in these tests stands for.
pub fn host(runtime: &str) -> yamaa_core::project_function::Language {
    match runtime {
        "python" => yamaa_core::project_function::Language::Python,
        "r" => yamaa_core::project_function::Language::R,
        other => panic!("no host for test runtime {other}"),
    }
}
