//! Schema factories owned by the engine integration suite.
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

#[allow(dead_code)]
pub mod producer;
#[allow(dead_code)]
pub mod producer_graph;
#[allow(dead_code)]
pub mod project_call_compiler;
pub mod project_codelist_compiler;

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

pub fn text_schema() -> TableSchema {
    table_schema(&[("ID", ColumnType::Str), ("TEXT", ColumnType::Str)])
}
