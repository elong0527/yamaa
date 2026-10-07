//! Independently authored PyArrow containers exercise the held-byte input codec.
use std::path::Path;
use yamaa_adapters::{
    arrow_table::TableLimits,
    parquet_source::{parse, CompressionLimits, Error, FramingLimits, Limits, MetadataLimits},
};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    temporal::{DatePrecision, DateTimePrecision},
    value::ColumnType,
};

fn limits() -> Limits {
    Limits {
        framing: FramingLimits {
            source_bytes: 1_048_576,
            metadata_bytes: 131_072,
            metadata_nodes: 8192,
            header_bytes: 8192,
            header_nodes: 8192,
            row_groups: 32,
            columns: 64,
            rows: 1000,
            cells: 10_000,
            pages: 1000,
            page_bytes: 1_048_576,
            decoded_bytes: 4_194_304,
        },
        compression: CompressionLimits {
            page_bytes: 1_048_576,
            window_log: 24,
        },
        metadata: MetadataLimits {
            bytes: 131_072,
            tables: 8192,
            depth: 64,
        },
        expanded_bytes: 4_194_304,
        retained_bytes: 4_194_304,
        array_elements: 10_000,
        batches: 1,
    }
}
fn table_limits() -> TableLimits {
    TableLimits {
        max_rows: 1000,
        max_columns: 64,
        max_batches: 1,
        max_cells: 10_000,
    }
}
fn content(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pq")
            .join(format!("{name}.parquet")),
    )
    .unwrap()
}

#[test]
fn independent_codecs_page_versions_and_encodings_preserve_exact_values() {
    let mut cases = vec!["delta".to_string()];
    for codec in ["none", "snappy", "gzip", "brotli", "lz4", "zstd"] {
        for version in [1, 2] {
            for dictionary in [0, 1] {
                cases.push(format!("{codec}-{version}-{dictionary}"));
            }
        }
    }
    for name in cases {
        let bytes = content(&name);
        let table = parse(&bytes, limits(), table_limits()).unwrap();
        drop(bytes);
        assert_eq!(table.row_count(), 3, "{name}");
        assert_eq!(
            table
                .schema()
                .columns()
                .iter()
                .map(|c| (c.name.as_str(), c.kind))
                .collect::<Vec<_>>(),
            [
                ("S", ColumnType::Str),
                ("I", ColumnType::Int),
                ("F", ColumnType::Float),
                ("D", ColumnType::Date),
                ("T", ColumnType::DateTime)
            ]
        );
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""), "{name}");
        assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str("β\0z"), "{name}");
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN), "{name}");
        assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Int(i64::MAX), "{name}");
        for column in 0..5 {
            assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing, "{name}");
        }
        for (row, bits) in [(0, (-0.0f64).to_bits()), (2, 1)] {
            let ValueRef::Float(value) = table.cell(row, 2).unwrap() else {
                panic!("{name}")
            };
            assert_eq!(value.get().to_bits(), bits, "{name}");
        }
        for (row, text) in [(0, "0001-01-01"), (2, "9999-12-31")] {
            let ValueRef::Date(value) = table.cell(row, 3).unwrap() else {
                panic!("{name}")
            };
            assert_eq!(value.to_string(), text, "{name}");
            assert_eq!(value.collected_precision(), DatePrecision::Day);
        }
        for (row, fields) in [
            (0, (1969, 12, 31, 23, 59, 59)),
            (2, (9999, 12, 31, 23, 59, 59)),
        ] {
            let ValueRef::DateTime(value) = table.cell(row, 4).unwrap() else {
                panic!("{name}")
            };
            assert_eq!(value.fields(), fields, "{name}");
            assert_eq!(value.collected_precision(), DateTimePrecision::Second);
        }
    }
}

#[test]
fn empty_tables_and_nonfinite_inputs_keep_the_closed_schema_and_normalized_nulls() {
    let empty = parse(&content("empty"), limits(), table_limits()).unwrap();
    assert_eq!(empty.row_count(), 0);
    assert_eq!(empty.schema().columns().len(), 5);
    let table = parse(&content("nonfinite"), limits(), table_limits()).unwrap();
    for row in 0..3 {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
    }
    let ValueRef::Float(value) = table.cell(3, 0).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
    // PyArrow restores the stored DATE as date32 even with an Arrow date64 hint.
    let date = parse(&content("date64"), limits(), table_limits()).unwrap();
    let ValueRef::Date(value) = date.cell(0, 0).unwrap() else {
        panic!()
    };
    assert_eq!(value.to_string(), "1970-01-01");
    assert_eq!(value.collected_precision(), DatePrecision::Day);
}

#[test]
fn unsupported_stored_types_retain_independent_arrow_type_spelling() {
    for (name, expected) in [
        ("int32", "int32"),
        ("uint64", "uint64"),
        ("float32", "float"),
        ("binary", "binary"),
        ("milliseconds", "timestamp[ms]"),
        ("timezone", "timestamp[us, tz=UTC]"),
        ("list", "list<element: int64>"),
        ("fixed-list", "fixed_size_list<element: int64>[2]"),
        ("struct", "struct<item: int64>"),
        ("decimal", "decimal128(10, 2)"),
    ] {
        assert!(
            matches!(parse(&content(&format!("unsupported-{name}")), limits(), table_limits()),
                Err(Error::Unsupported {field, stored_type}) if field == "FIELD" && stored_type == expected),
            "{name}"
        );
    }
}

#[test]
fn nested_roots_name_their_own_field_after_flat_columns_and_preserve_name_precedence() {
    for (name, field, stored_type) in [
        ("mixed-struct", "S", "struct<left: int64, right: string>"),
        ("mixed-map", "M", "map<string, int64 ('M')>"),
    ] {
        assert!(
            matches!(parse(&content(name), limits(), table_limits()),
            Err(Error::Unsupported { field: actual, stored_type: actual_type })
            if actual == field && actual_type == stored_type),
            "{name}"
        );
    }
    assert!(matches!(
        parse(&content("mixed-empty"), limits(), table_limits()),
        Err(Error::EmptyName { field: 1 })
    ));
    assert!(
        matches!(parse(&content("mixed-duplicate"),limits(),table_limits()),Err(Error::DuplicateName{field}) if field=="I")
    );
}

#[test]
fn field_and_temporal_diagnostics_precede_invalid_text() {
    for name in ["boolean", "utf8-bool"] {
        assert!(
            matches!(parse(&content(name), limits(), table_limits()), Err(Error::Unsupported { stored_type, .. }) if stored_type == "bool"),
            "{name}"
        );
    }
    for name in ["fractional", "utf8-time"] {
        assert!(
            matches!(
                parse(&content(name), limits(), table_limits()),
                Err(Error::Value {
                    row: 1,
                    value: 1,
                    ..
                })
            ),
            "{name}"
        );
    }
    assert!(matches!(
        parse(&content("bad-date"), limits(), table_limits()),
        Err(Error::Value {
            row: 1,
            value: -719_163,
            ..
        })
    ));
    assert!(matches!(
        parse(&content("empty-name"), limits(), table_limits()),
        Err(Error::EmptyName { field: 1 })
    ));
    assert!(
        matches!(parse(&content("duplicate"), limits(), table_limits()), Err(Error::DuplicateName { field }) if field == "I")
    );
    assert!(matches!(
        parse(&content("utf8"), limits(), table_limits()),
        Err(Error::Malformed)
    ));
    let mut broken = content("boolean");
    broken.truncate(broken.len() - 1);
    assert!(matches!(
        parse(&broken, limits(), table_limits()),
        Err(Error::Malformed)
    ));
}

#[test]
fn source_and_expansion_policies_are_separate_from_language_findings() {
    let bytes = content("none-1-0");
    let mut small = limits();
    small.framing.source_bytes = bytes.len() - 1;
    assert!(matches!(
        parse(&bytes, small, table_limits()),
        Err(Error::Limit)
    ));
    small = limits();
    small.expanded_bytes = 1;
    assert!(matches!(
        parse(&bytes, small, table_limits()),
        Err(Error::Limit)
    ));
    small = limits();
    small.retained_bytes = 1;
    assert!(matches!(
        parse(&bytes, small, table_limits()),
        Err(Error::Limit)
    ));
    assert!(matches!(
        parse(
            &bytes,
            limits(),
            TableLimits {
                max_cells: 14,
                ..table_limits()
            }
        ),
        Err(Error::Limit)
    ));
    assert!(matches!(
        parse(
            &bytes,
            limits(),
            TableLimits {
                max_rows: 2,
                ..table_limits()
            }
        ),
        Err(Error::Limit)
    ));
}
