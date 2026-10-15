//! Independently authored PyArrow containers exercise the held-byte input codec.
use std::path::Path;
use yamaa_adapters::parquet_source::{parse, Error};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    temporal::{DatePrecision, DateTimePrecision},
    value::ColumnType,
};

fn content(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pq")
            .join(format!("{name}.parquet")),
    )
    .unwrap()
}

#[test]
fn odm_metadata_admission_ignores_vendor_types_but_checks_every_physical_chunk() {
    use yamaa_adapters::parquet_source::parse_odm;
    use yamaa_core::odm_source::{SchemaError, FIELDS};
    let bytes = content("odm-vendor");
    let table = parse_odm(&bytes).unwrap();
    assert_eq!(
        table
            .schema()
            .columns()
            .iter()
            .map(|column| (column.name.as_str(), column.kind))
            .collect::<Vec<_>>(),
        FIELDS.map(|name| (name, ColumnType::Str))
    );
    let expected = [
        Some("S"),
        Some("M"),
        Some("P"),
        Some("E"),
        None,
        Some("F"),
        None,
        Some("G"),
        None,
        Some("IT.TEST.OID"),
        Some("Y"),
    ];
    for (column, expected) in expected.into_iter().enumerate() {
        assert_eq!(
            table.cell(0, column).unwrap(),
            expected.map_or(ValueRef::Missing, ValueRef::Str)
        );
    }
    // The ordinary closed profile retains its own independently qualified
    // unsupported vendor behavior; only the authored ODM interpretation projects.
    assert!(parse(&bytes).is_err());
    assert!(matches!(
        parse_odm(&content("odm-vendor-malformed")),
        Err(Error::Malformed)
    ));
    let expected = [
        (
            "odm-wrong-type",
            vec![SchemaError::Type {
                field: "Value",
                stored_field: "Value".into(),
                kind: Some(ColumnType::Int),
            }],
        ),
        (
            "odm-ambiguous",
            vec![SchemaError::Ambiguous {
                field: "StudyOID",
                stored: vec!["StudyOID".into(), "studyoid".into()],
            }],
        ),
        (
            "odm-missing",
            vec![SchemaError::Missing {
                fields: vec!["FormOID", "FormRepeatKey"],
            }],
        ),
    ];
    for (name, expected) in expected {
        let Error::OdmFields(actual) = parse_odm(&content(name)).unwrap_err() else {
            panic!("typed ODM field finding: {name}")
        };
        assert_eq!(actual, expected, "{name}");
    }
    let empty = parse_odm(&content("odm-empty")).unwrap();
    assert_eq!(empty.row_count(), 0);
    assert_eq!(empty.schema(), table.schema());
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
        let table = parse(&bytes).unwrap();
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
    let empty = parse(&content("empty")).unwrap();
    assert_eq!(empty.row_count(), 0);
    assert_eq!(empty.schema().columns().len(), 5);
    let table = parse(&content("nonfinite")).unwrap();
    for row in 0..3 {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
    }
    let ValueRef::Float(value) = table.cell(3, 0).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
    // PyArrow restores the stored DATE as date32 even with an Arrow date64 hint.
    let date = parse(&content("date64")).unwrap();
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
            matches!(parse(&content(&format!("unsupported-{name}"))),
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
            matches!(parse(&content(name)),
            Err(Error::Unsupported { field: actual, stored_type: actual_type })
            if actual == field && actual_type == stored_type),
            "{name}"
        );
    }
    assert!(matches!(
        parse(&content("mixed-empty")),
        Err(Error::EmptyName { field: 1 })
    ));
    assert!(
        matches!(parse(&content("mixed-duplicate")),Err(Error::DuplicateName{field}) if field=="I")
    );
}

#[test]
fn field_and_temporal_diagnostics_precede_invalid_text() {
    for name in ["boolean", "utf8-bool"] {
        assert!(
            matches!(parse(&content(name)), Err(Error::Unsupported { stored_type, .. }) if stored_type == "bool"),
            "{name}"
        );
    }
    for name in ["fractional", "utf8-time"] {
        assert!(
            matches!(
                parse(&content(name)),
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
        parse(&content("bad-date")),
        Err(Error::Value {
            row: 1,
            value: -719_163,
            ..
        })
    ));
    assert!(matches!(
        parse(&content("empty-name")),
        Err(Error::EmptyName { field: 1 })
    ));
    assert!(
        matches!(parse(&content("duplicate")), Err(Error::DuplicateName { field }) if field == "I")
    );
    assert!(matches!(parse(&content("utf8")), Err(Error::Malformed)));
    let mut broken = content("boolean");
    broken.truncate(broken.len() - 1);
    assert!(matches!(parse(&broken), Err(Error::Malformed)));
}
