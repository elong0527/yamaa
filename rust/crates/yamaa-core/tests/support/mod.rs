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
pub mod specification;

/// A CSV text table, authored: the header's names and each record's fields,
/// `None` for an empty one, as the adapters' parser admits a source.
pub fn text_table(
    names: &[&str],
    records: &[&[Option<&str>]],
) -> yamaa_core::csv_source::CsvSource {
    yamaa_core::csv_source::CsvSource {
        names: names.iter().map(|name| (*name).into()).collect(),
        records: records
            .iter()
            .map(|record| record.iter().map(|field| field.map(String::from)).collect())
            .collect(),
    }
}

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

pub mod ports;

pub mod document;

pub mod diagnostic;

pub mod dataset;

pub mod resolved;
