//! Borrowed codec projections compare independently captured complete diagnostics.
use serde_json::{json, Value};
use yamaa_adapters::{
    csv_source::{self, TextTableError},
    parquet_source as parquet, source_diagnostics, specification_diagnostics as projection,
    specification_run::Error,
    typed_csv,
};
use yamaa_core::{
    diagnostic::{ParquetFinding, SourceFinding},
    specification::{SourceDeclaration, SourceProfile},
    value::ColumnType,
};

fn source(name: &str, path: &str) -> SourceDeclaration {
    SourceDeclaration {
        name: name.into(),
        path: path.into(),
        types: vec![],
        profile: SourceProfile::from_path(path).unwrap(),
        encoding: None,
        empty_string_present: false,
        ordinal: None,
    }
}

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn assert_complete(error: &Error, source: &SourceDeclaration, expected: &Value) {
    let wanted = expected.as_array().unwrap();
    assert_eq!(
        projection::findings(error, Some(source)).unwrap().unwrap(),
        *wanted
    );
    let failure: Value = serde_json::from_str(&projection::failure(error, Some(source))).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":wanted}})
    );
}

#[test]
fn all_independent_csv_profile_reports_preserve_original_projection() {
    let declaration = source("SRC", "source.csv");
    let mut count = 0;
    for row in
        serde_json::from_str::<Vec<Value>>(include_str!("../fixtures/csv_profile_diagnostics.json"))
            .unwrap()
            .iter()
    {
        let expected: Value = row["expected"].clone();
        let source_bytes: Vec<u8> = if let Some(text) = row["source_bytes"].as_str() {
            text.as_bytes().to_vec()
        } else {
            bytes(row["source_bytes"]["hex"].as_str().unwrap())
        };
        let error = Error::Source(TextTableError::Csv(
            csv_source::parse(&source_bytes).unwrap_err(),
        ));
        assert_complete(&error, &declaration, &expected["diagnostics"]);
        count += 1;
    }
    assert_eq!(count, 13);
}

#[test]
fn all_independent_typing_reports_keep_original_first_bad_field_and_value() {
    let declaration = source("SRC", "source.csv");
    let mut count = 0;
    for row in serde_json::from_str::<Vec<Value>>(include_str!("../fixtures/source_typing.json"))
        .unwrap()
        .iter()
    {
        let expected: Value = row["expected"].clone();
        let types = match row["case"].as_str().unwrap() {
            "unknown-field" => vec![
                ("ID".into(), ColumnType::Int),
                ("ABSENT".into(), ColumnType::Int),
            ],
            "unknown-before-conversion" => vec![
                ("V".into(), ColumnType::Int),
                ("ABSENT".into(), ColumnType::Date),
            ],
            "stored-order" => vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)],
            "invalid-temporal" => vec![("V".into(), ColumnType::Date)],
            _ => panic!("unknown independent fixture"),
        };
        let source_bytes: Vec<u8> = if let Some(text) = row["source_bytes"].as_str() {
            text.as_bytes().to_vec()
        } else {
            bytes(row["source_bytes"]["hex"].as_str().unwrap())
        };
        let error = Error::TypedSource(typed_csv::parse(&source_bytes, &types).unwrap_err());
        assert_complete(&error, &declaration, &expected["diagnostics"]);
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn independently_authored_parquet_schemas_keep_exact_spelling_and_raw_value() {
    let declaration = source("SRC", "held.parquet");
    for (fixture, condition, requirement, extra) in [
        ("", "source_parquet_invalid", "REQ-1038", json!({})),
        (
            "mixed-empty",
            "source_field_name_empty",
            "REQ-1039",
            json!({"field":1}),
        ),
        (
            "mixed-duplicate",
            "source_field_name_duplicate",
            "REQ-1039",
            json!({"field":"I"}),
        ),
        (
            "mixed-struct",
            "source_field_type_unsupported",
            "REQ-1040",
            json!({"field":"S","stored_type":"struct<left: int64, right: string>"}),
        ),
        (
            "utf8-time",
            "source_field_value_invalid",
            "REQ-1041",
            json!({"field":"OTHER","row":1,"value":1}),
        ),
        (
            "bad-date",
            "source_field_value_invalid",
            "REQ-1041",
            json!({"field":"D","row":1,"value":-719163}),
        ),
    ] {
        let bytes = if fixture.is_empty() {
            Vec::new()
        } else {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/pq")
                    .join(format!("{fixture}.parquet")),
            )
            .unwrap()
        };
        let error = Error::ParquetSource(parquet::parse(&bytes).unwrap_err());
        let mut context = json!({"dataset":"SRC","path":"held.parquet"});
        context
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_complete(
            &error,
            &declaration,
            &json!([{"phase":"ingest","condition":condition,
            "requirement":requirement,"spec_paths":["input.SRC.path"],"context":context}]),
        );
    }
}

#[test]
fn nested_ordered_report_keeps_complete_borrowed_codec_facts() {
    let error = Error::Sources(vec![
        (
            source("FIRST", "first.csv"),
            Error::Source(TextTableError::Csv(
                csv_source::parse(b"ID,ID\nx,y").unwrap_err(),
            )),
        ),
        (
            source("IGNORED", "outer.csv"),
            Error::Sources(vec![(
                source("LAST", "last.parquet"),
                Error::ParquetSource(parquet::Error::Unsupported {
                    field: "F\0β".into(),
                    stored_type: "\0\"\\β".repeat(4000),
                }),
            )]),
        ),
    ]);
    let Error::Sources(errors) = &error else {
        panic!()
    };
    let Error::Sources(nested) = &errors[1].1 else {
        panic!()
    };
    let Error::ParquetSource(parquet::Error::Unsupported { field, stored_type }) = &nested[0].1
    else {
        panic!()
    };
    let pointers = (
        field.as_ptr(),
        stored_type.as_ptr(),
        nested[0].0.path.as_ptr(),
    );
    let finding = match &nested[0].1 {
        Error::ParquetSource(error) => error.finding(&nested[0].0.name, &nested[0].0.path).unwrap(),
        _ => panic!(),
    };
    let SourceFinding::Parquet {
        finding:
            ParquetFinding::Unsupported {
                field: borrowed_field,
                stored_type: borrowed_type,
            },
        path,
        ..
    } = finding
    else {
        panic!()
    };
    assert_eq!(
        (
            borrowed_field.as_ptr(),
            borrowed_type.as_ptr(),
            path.as_ptr()
        ),
        pointers
    );
    assert!(
        source_diagnostics::findings(&errors[0].1, Some(&errors[0].0))
            .unwrap()
            .is_some()
    );
    assert!(
        source_diagnostics::findings(&nested[0].1, Some(&nested[0].0))
            .unwrap()
            .is_some()
    );
    let actual = source_diagnostics::findings(&error, None).unwrap().unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|finding| finding.spec_paths[0].as_str())
            .collect::<Vec<_>>(),
        ["input.FIRST.path", "input.LAST.path"]
    );
    let portable = projection::findings(&error, None).unwrap().unwrap();
    assert_eq!(portable[1]["context"]["stored_type"], *stored_type);
    assert_eq!(portable[1]["context"]["field"], *field);
    assert_eq!(
        (
            field.as_ptr(),
            stored_type.as_ptr(),
            nested[0].0.path.as_ptr()
        ),
        pointers
    );
    assert_eq!(
        source_diagnostics::findings(&error, None).unwrap().unwrap(),
        actual
    );
}

#[test]
fn missing_identity_is_explicit_and_resource_barriers_preserve_original_error_precedence() {
    let semantic = Error::ParquetSource(parquet::Error::Malformed);
    assert_eq!(
        source_diagnostics::findings(&semantic, None),
        Err(source_diagnostics::SourceProjectionError::InvalidContext)
    );
    let failure: Value = serde_json::from_str(&projection::failure(&semantic, None)).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"ingest","code":"diagnostic_context"}})
    );
    let declaration = source("SRC", "source.csv");
    for error in [
        Error::Source(TextTableError::Csv(csv_source::Error::Limit {
            resource: "source_bytes",
            limit: yamaa_core::limits::Limits::SOURCE_BYTES,
        })),
        Error::TypedSource(typed_csv::Error::Typing(
            yamaa_core::typed_csv::Error::InvalidDeclarations,
        )),
        Error::ParquetSource(parquet::Error::ResourceLimit {
            resource: "source_bytes",
            limit: yamaa_core::limits::Limits::SOURCE_BYTES,
            actual: Some(yamaa_core::limits::Limits::SOURCE_BYTES + 1),
        }),
        Error::ParquetSource(parquet::Error::Representation {
            representation: "Utf8 offset width",
        }),
        Error::ParquetSource(parquet::Error::Unavailable { codec: i64::MAX }),
    ] {
        assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
        let collected = Error::Sources(vec![
            (
                declaration.clone(),
                Error::ParquetSource(parquet::Error::Unsupported {
                    field: "F".into(),
                    stored_type: "late".repeat(1000),
                }),
            ),
            (declaration.clone(), error),
        ]);
        assert_eq!(source_diagnostics::findings(&collected, None), Ok(None));
        let failure: Value = serde_json::from_str(&projection::failure(&collected, None)).unwrap();
        assert_eq!(
            failure["outcome"],
            json!({"status":"rejected","stage":"ingest","code":"source_collection_boundary"})
        );
    }
    assert_eq!(
        source_diagnostics::findings(&Error::Sources(vec![]), None),
        Ok(Some(vec![]))
    );
}

#[test]
fn complete_projection_preserves_full_oversized_value_and_both_findings() {
    let original = "9".repeat(1_400_000);
    let error = Error::Sources(vec![
        (
            source("FIRST", "first.csv"),
            Error::ParquetSource(parquet::Error::Malformed),
        ),
        (
            source("LAST", "last.csv"),
            Error::TypedSource(typed_csv::Error::Typing(
                yamaa_core::typed_csv::Error::FieldParse {
                    field: "F".into(),
                    target: ColumnType::Int,
                    value: original.clone(),
                },
            )),
        ),
    ]);
    let diagnostics = source_diagnostics::findings(&error, None).unwrap().unwrap();
    assert_eq!(diagnostics.len(), 2);
    let portable = projection::findings(&error, None).unwrap().unwrap();
    assert_eq!(portable.len(), 2);
    assert_eq!(portable[1]["context"]["value"], original);
    let failure: Value = serde_json::from_str(&projection::failure(&error, None)).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":portable}})
    );
    let Error::Sources(errors) = &error else {
        panic!("retained collection")
    };
    let Error::TypedSource(typed_csv::Error::Typing(yamaa_core::typed_csv::Error::FieldParse {
        value,
        ..
    })) = &errors[1].1
    else {
        panic!("retained value")
    };
    assert_eq!(value, &original);
}

fn nested(mut error: Error, depth: usize) -> Error {
    for _ in 0..depth {
        error = Error::Sources(vec![(source("DEEP", "deep.parquet"), error)]);
    }
    error
}
fn opaque() -> Error {
    Error::ParquetSource(parquet::Error::Table(
        yamaa_adapters::arrow_table::TableError::Arrow(arrow_schema::ArrowError::ComputeError(
            "original opaque codec allocation".into(),
        )),
    ))
}
fn opaque_text(error: &Error) -> &str {
    let Error::ParquetSource(parquet::Error::Table(
        yamaa_adapters::arrow_table::TableError::Arrow(arrow_schema::ArrowError::ComputeError(
            text,
        )),
    )) = error
    else {
        panic!("original opaque allocation")
    };
    text
}
fn wide(width: usize) -> Vec<(SourceDeclaration, Error)> {
    (0..width)
        .map(|_| {
            (
                source("WIDE", "wide.parquet"),
                Error::ParquetSource(parquet::Error::Unsupported {
                    field: "F".into(),
                    stored_type: "captured physical metadata".into(),
                }),
            )
        })
        .collect()
}

#[test]
fn wide_nested_semantic_report_preserves_every_supplied_finding() {
    let error = nested(Error::Sources(wide(50_000)), 2);
    let actual = source_diagnostics::findings(&error, None).unwrap().unwrap();
    assert_eq!(actual.len(), 50_000);
    for finding in &actual {
        assert_eq!(finding.spec_paths, ["input.WIDE.path"]);
    }
    let Error::Sources(outer) = &error else {
        panic!("retained outer")
    };
    assert_eq!(outer[0].0.name, "DEEP");
    assert_eq!(
        source_diagnostics::findings(&error, None).unwrap().unwrap(),
        actual
    );
}

#[test]
fn late_opaque_after_wide_nested_report_keeps_original_allocation_and_precedence() {
    let mut children = wide(50_000);
    let original = opaque();
    let pointer = opaque_text(&original).as_ptr();
    children.push((source("OPAQUE", "opaque.parquet"), original));
    let error = nested(Error::Sources(children), 2);
    assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
    let Error::Sources(outer) = &error else {
        panic!()
    };
    let Error::Sources(middle) = &outer[0].1 else {
        panic!()
    };
    let Error::Sources(children) = &middle[0].1 else {
        panic!()
    };
    assert_eq!(children.len(), 50_001);
    assert_eq!(opaque_text(&children[50_000].1).as_ptr(), pointer);
    assert_eq!(
        opaque_text(&children[50_000].1),
        "original opaque codec allocation"
    );
    assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
    assert_eq!(opaque_text(&children[50_000].1).as_ptr(), pointer);
}

#[test]
fn reached_opaque_barrier_stops_before_later_deep_and_oversized_semantic_facts() {
    let original = opaque();
    let pointer = opaque_text(&original).as_ptr();
    let late = Error::TypedSource(typed_csv::Error::Typing(
        yamaa_core::typed_csv::Error::FieldParse {
            field: "F".into(),
            target: ColumnType::Int,
            value: "9".repeat(1_400_000),
        },
    ));
    let error = Error::Sources(vec![
        (source("FIRST", "first.parquet"), original),
        (source("LATER", "later.csv"), nested(late, 129)),
    ]);
    assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
    assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
    let failure: Value = serde_json::from_str(&projection::failure(&error, None)).unwrap();
    assert_eq!(
        failure["outcome"],
        json!({"status":"rejected","stage":"ingest","code":"source_collection_boundary"})
    );
    let Error::Sources(children) = &error else {
        panic!()
    };
    assert_eq!(opaque_text(&children[0].1).as_ptr(), pointer);
}

#[test]
fn deep_borrowed_reports_keep_full_facts_and_later_opaque_errors_preserve_precedence() {
    let leaf = Error::ParquetSource(parquet::Error::Unsupported {
        field: "F".into(),
        stored_type: "held physical type".into(),
    });
    let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = &leaf else {
        panic!("retained metadata")
    };
    let pointer = stored_type.as_ptr();
    let admitted = nested(leaf, 129);
    let diagnostics = source_diagnostics::findings(&admitted, None)
        .unwrap()
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].spec_paths, ["input.DEEP.path"]);
    let held = Error::Sources(vec![
        (source("DEEP", "deep.parquet"), admitted),
        (source("LATE", "late.parquet"), opaque()),
    ]);
    assert_eq!(source_diagnostics::findings(&held, None), Ok(None));
    let failure: Value = serde_json::from_str(&projection::failure(&held, None)).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"ingest","code":"source_collection_boundary"}})
    );
    let Error::Sources(children) = &held else {
        panic!("retained collection")
    };
    let mut leaf = &children[0].1;
    for _ in 0..129 {
        let Error::Sources(child) = leaf else {
            panic!("retained nested metadata")
        };
        leaf = &child[0].1;
    }
    let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = leaf else {
        panic!("retained metadata")
    };
    assert_eq!(stored_type.as_ptr(), pointer);
    assert_eq!(stored_type, "held physical type");
}
