//! Finite factories and observers owned by the yamaa-adapters integration suite.
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod ipc_snapshot;
#[path = "../../examples/support/odm_declaration.rs"]
pub mod odm_declaration;
pub mod project_call_compiler;
pub mod project_codelist_compiler;
pub mod project_schemas;
#[path = "../../examples/support/schema_declaration.rs"]
pub mod schema_declaration;
#[path = "../../examples/support/string_template_syntax.rs"]
pub mod string_template_syntax;

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
