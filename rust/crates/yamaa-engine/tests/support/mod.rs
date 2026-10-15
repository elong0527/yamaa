//! Finite factories and observers owned by the yamaa-engine integration suite.
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod producer;
pub mod producer_graph;
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
