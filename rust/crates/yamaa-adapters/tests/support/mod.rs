//! Finite factories and observers owned by the yamaa-adapters integration suite.
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod aggregate_syntax;
pub mod ipc_snapshot;
pub mod numeric_syntax;
pub mod odm_declaration;
pub mod predicate_syntax;
pub mod project_call_compiler;
pub mod project_codelist_compiler;
pub mod project_schemas;
pub mod schema_declaration;
pub mod string_template_syntax;
pub mod syntax_spans;

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
