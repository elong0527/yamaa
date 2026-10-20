//! Finite factories and observers owned by the yamaa-core integration suite.
use yamaa_core::schema as document_schema;
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

pub mod producer;
pub mod project_call_compiler;
pub mod project_codelist_compiler;
pub mod project_environment;
pub mod roots;

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

pub fn int_schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap()
}
pub fn id_source_schema() -> TableSchema {
    int_schema(&["ID"])
}
pub fn id_value_schema() -> TableSchema {
    int_schema(&["ID", "V"])
}

pub fn text_schema() -> TableSchema {
    table_schema(&[("ID", ColumnType::Str), ("TEXT", ColumnType::Str)])
}

pub mod ports;

pub mod document;

pub mod diagnostic;

pub mod dataset;

pub mod resolved;
