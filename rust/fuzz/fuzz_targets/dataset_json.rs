//! The Dataset-JSON transport writer. The engine writes Dataset-JSON and reads
//! none: untrusted bytes reach the writer as the published artifact that a
//! submission build reads back. Each input is such a CSV artifact, admitted by
//! the same CSV profile and declared typing as that build; every typed table
//! is rendered in both representations (JSON-4), and the output must parse
//! as JSON that holds every cell: the single-object form as one object, the
//! newline-delimited form as a first line and one JSON array per later line.
#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::Deserialize;
use serde_json::value::RawValue;
use yamaa_adapters::{
    csv_source,
    submission_json::{DatasetJsonColumn, DatasetJsonDocument, Layout},
};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    typed_csv::{PreparedTypes, TypedRows},
    value::ColumnType,
};

/// A header as composition produces one; its column names are the
/// artifact's own, so they cross the writer's escaping as well.
fn document(table: &TypedRows) -> DatasetJsonDocument {
    let columns = table
        .schema()
        .columns()
        .iter()
        .map(|column| DatasetJsonColumn {
            item_oid: format!("IT.RAW.{}", column.name),
            name: column.name.clone(),
            label: column.name.clone(),
            data_type: match column.kind {
                ColumnType::Int => "integer",
                ColumnType::Float => "float",
                ColumnType::Date => "date",
                ColumnType::DateTime => "datetime",
                ColumnType::Str => "string",
            }
            .into(),
            target_data_type: None,
            length: None,
            display_format: None,
            key_sequence: None,
        })
        .collect();
    DatasetJsonDocument {
        path: "datasets.RAW.transport".into(),
        created: "2026-01-01T00:00:00".into(),
        version: "1.1.0".into(),
        file_oid: "FUZZ.RAW".into(),
        originator: None,
        source_name: "yamaa".into(),
        source_version: "0".into(),
        study_oid: "STDY.FUZZ".into(),
        metadata_version_oid: "MDV.FUZZ".into(),
        metadata_ref: "define.xml".into(),
        item_group_oid: "IG.RAW".into(),
        name: "RAW".into(),
        label: "Raw".into(),
        columns,
    }
}

/// The parts of a rendered file that must hold the table. Cells stay raw
/// tokens: serde_json's default number parsing is not correctly rounded,
/// while the standard library's is.
#[derive(Deserialize)]
struct Rendered<'a> {
    records: usize,
    columns: Vec<RenderedColumn>,
    #[serde(borrow)]
    rows: Vec<Vec<&'a RawValue>>,
}

#[derive(Deserialize)]
struct RenderedColumn {
    name: String,
}

/// The first line of the newline-delimited form: the object without `rows`.
#[derive(Deserialize)]
struct Head {
    records: usize,
    columns: Vec<RenderedColumn>,
}

/// The rendered file read back by its layout: the single-object form is one
/// JSON text; the newline-delimited form is one JSON text per `U+000A`-ended
/// line, the first its header and each later one a row (JSON-13).
fn rendered(bytes: &[u8], layout: Layout) -> Rendered<'_> {
    match layout {
        Layout::Object => serde_json::from_slice(bytes).expect("Dataset-JSON output is JSON"),
        Layout::Lines => {
            let text = std::str::from_utf8(bytes).expect("UTF-8 output");
            let body = text.strip_suffix('\n').expect("a final newline");
            assert!(!body.contains('\r'), "no carriage return outside a string");
            let mut lines = body.split('\n');
            let head: Head = serde_json::from_str(lines.next().expect("a first line"))
                .expect("the first line is the header object");
            let rows = lines
                .map(|line| serde_json::from_str(line).expect("a row line is a JSON array"))
                .collect();
            Rendered {
                records: head.records,
                columns: head.columns,
                rows,
            }
        }
    }
}

/// The rendered file is JSON whose header and rows hold the table exactly.
fn check(rendered: &Rendered<'_>, table: &TypedRows) {
    let columns = table.schema().columns();
    assert_eq!(rendered.records, table.row_count());
    let names: Vec<_> = rendered.columns.iter().map(|column| &column.name).collect();
    let expected: Vec<_> = columns.iter().map(|column| &column.name).collect();
    assert_eq!(names, expected);
    assert_eq!(rendered.rows.len(), table.row_count());
    let text = |token: &str| serde_json::from_str::<String>(token).ok();
    for (row, cells) in rendered.rows.iter().enumerate() {
        assert_eq!(cells.len(), columns.len());
        for (column, cell) in cells.iter().enumerate() {
            let token = cell.get();
            let held = table.cell(row, column).expect("a held cell");
            let same = match held {
                ValueRef::Missing => token == "null",
                ValueRef::Str(value) => text(token).as_deref() == Some(value),
                ValueRef::Int(value) => token.parse::<i64>() == Ok(value),
                ValueRef::Float(value) => token.parse::<f64>() == Ok(value.get()),
                ValueRef::Date(value) => text(token) == Some(value.to_string()),
                ValueRef::DateTime(value) => text(token) == Some(value.to_string()),
                ValueRef::Bool(_) => false,
            };
            assert!(same, "row {row}, column {column}: {held:?} became {token}");
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(parsed) = csv_source::parse(data) else {
        return;
    };
    for kind in [
        None,
        Some(ColumnType::Int),
        Some(ColumnType::Float),
        Some(ColumnType::Date),
        Some(ColumnType::DateTime),
    ] {
        let declarations: Vec<(String, ColumnType)> = kind
            .map(|kind| {
                parsed
                    .names
                    .iter()
                    .map(|name| (name.clone(), kind))
                    .collect()
            })
            .unwrap_or_default();
        let Ok(table) =
            PreparedTypes::new(&declarations).and_then(|types| types.convert(parsed.clone()))
        else {
            continue;
        };
        for layout in [Layout::Object, Layout::Lines] {
            let bytes = yamaa_adapters::submission_json::render(&document(&table), &table, layout)
                .expect("an admitted table renders");
            check(&rendered(&bytes, layout), &table);
        }
    }
});
