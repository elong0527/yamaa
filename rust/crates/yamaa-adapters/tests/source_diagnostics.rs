//! Borrowed codec projections compare independently captured complete diagnostics.
use serde_json::{json, Value};
use yamaa_adapters::{
    arrow_table::TableLimits,
    csv_source::{self, TextTableError},
    parquet_source::{self as parquet, CompressionLimits, FramingLimits, MetadataLimits},
    source_diagnostics, specification_diagnostics as projection,
    specification_run::Error,
    typed_csv,
};
use yamaa_core::{
    diagnostic::{ParquetFinding, ProjectionError, SourceFinding, MAX_PROJECTION_BYTES},
    specification::{SourceDeclaration, SourceProfile},
    value::ColumnType,
};

fn source(name: &str, path: &str) -> SourceDeclaration {
    SourceDeclaration {
        name: name.into(),
        path: path.into(),
        types: vec![],
        profile: SourceProfile::from_path(path).unwrap(),
        empty_string_present: false,
        ordinal: None,
    }
}
fn table_limits() -> TableLimits {
    TableLimits {
        max_rows: 1000,
        max_columns: 64,
        max_batches: 1,
        max_cells: 10000,
    }
}
fn limits() -> parquet::Limits {
    parquet::Limits {
        framing: FramingLimits {
            source_bytes: 1_048_576,
            metadata_bytes: 131_072,
            metadata_nodes: 8192,
            header_bytes: 8192,
            header_nodes: 8192,
            row_groups: 32,
            columns: 64,
            rows: 1000,
            cells: 10000,
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
        array_elements: 10000,
        batches: 1,
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
        projection::findings_with_limit(error, Some(source), MAX_PROJECTION_BYTES)
            .unwrap()
            .unwrap(),
        *wanted
    );
    assert_eq!(projection::findings(error, Some(source)).unwrap(), *wanted);
    let failure: Value = serde_json::from_str(&projection::failure(error, Some(source))).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":wanted}})
    );
}

#[test]
fn all_independent_csv_profile_reports_survive_bounded_original_projection() {
    let declaration = source("SRC", "source.csv");
    let mut count = 0;
    for line in include_str!("fixtures/csv_profile_diagnostics.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        let expected: Value = serde_json::from_str(fields[2]).unwrap();
        let error = Error::Source(TextTableError::Csv(
            csv_source::parse(&bytes(fields[3]), Default::default()).unwrap_err(),
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
    for line in include_str!("fixtures/source_typing.tsv").lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        let expected: Value = serde_json::from_str(fields[2]).unwrap();
        let types = match fields[0] {
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
        let error = Error::TypedSource(
            typed_csv::parse(
                &bytes(fields[3]),
                &types,
                Default::default(),
                table_limits(),
            )
            .unwrap_err(),
        );
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
        let error =
            Error::ParquetSource(parquet::parse(&bytes, limits(), table_limits()).unwrap_err());
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

fn minimum(error: &Error) -> usize {
    let mut lower = 0;
    let mut upper = MAX_PROJECTION_BYTES;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if source_diagnostics::findings_with_limit(error, None, middle).is_ok() {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    lower
}
#[test]
fn nested_ordered_report_refuses_late_quota_before_copying_retained_codec_facts() {
    let error = Error::Sources(vec![
        (
            source("FIRST", "first.csv"),
            Error::Source(TextTableError::Csv(
                csv_source::parse(b"ID,ID\nx,y", Default::default()).unwrap_err(),
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
    let limit = minimum(&error);
    assert!(
        source_diagnostics::findings_with_limit(&errors[0].1, Some(&errors[0].0), limit - 1)
            .unwrap()
            .is_some()
    );
    assert!(
        source_diagnostics::findings_with_limit(&nested[0].1, Some(&nested[0].0), limit - 1)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, limit - 1),
        Err(source_diagnostics::SourceProjectionError::Context(
            ProjectionError::Limit
        ))
    );
    let actual = source_diagnostics::findings_with_limit(&error, None, limit)
        .unwrap()
        .unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|finding| finding.spec_paths[0].as_str())
            .collect::<Vec<_>>(),
        ["input.FIRST.path", "input.LAST.path"]
    );
    let portable = projection::findings_with_limit(&error, None, limit)
        .unwrap()
        .unwrap();
    assert_eq!(portable[1]["context"]["stored_type"], *stored_type);
    assert_eq!(portable[1]["context"]["field"], *field);
    assert!(serde_json::to_string(&portable).unwrap().len() <= limit);
    assert_eq!(
        (
            field.as_ptr(),
            stored_type.as_ptr(),
            nested[0].0.path.as_ptr()
        ),
        pointers
    );
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, limit)
            .unwrap()
            .unwrap(),
        actual
    );
}

#[test]
fn missing_identity_is_explicit_and_resource_barriers_precede_shape_and_quota() {
    let semantic = Error::ParquetSource(parquet::Error::Malformed);
    assert_eq!(
        source_diagnostics::findings_with_limit(&semantic, None, 0),
        Err(source_diagnostics::SourceProjectionError::Context(
            ProjectionError::InvalidContext
        ))
    );
    let failure: Value = serde_json::from_str(&projection::failure(&semantic, None)).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"ingest","code":"diagnostic_context"}})
    );
    let declaration = source("SRC", "source.csv");
    for error in [
        Error::Source(TextTableError::Csv(csv_source::Error::Limit {
            resource: "bytes",
            limit: 0,
        })),
        Error::TypedSource(typed_csv::Error::Typing(
            yamaa_core::typed_csv::Error::InvalidDeclarations,
        )),
        Error::ParquetSource(parquet::Error::Limit),
        Error::ParquetSource(parquet::Error::Unavailable { codec: i64::MAX }),
    ] {
        assert_eq!(
            source_diagnostics::findings_with_limit(&error, None, 0),
            Ok(None)
        );
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
        assert_eq!(
            source_diagnostics::findings_with_limit(&collected, None, 0),
            Ok(None)
        );
        let failure: Value = serde_json::from_str(&projection::failure(&collected, None)).unwrap();
        assert_eq!(
            failure["outcome"],
            json!({"status":"rejected","stage":"ingest","code":"source_collection_boundary"})
        );
    }
    assert_eq!(
        source_diagnostics::findings_with_limit(&Error::Sources(vec![]), None, 0),
        Ok(Some(vec![]))
    );
}

#[test]
fn default_projection_refuses_full_oversized_value_without_truncation_or_partial_report() {
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
                    value: "9".repeat(MAX_PROJECTION_BYTES / 6),
                },
            )),
        ),
    ]);
    assert_eq!(
        source_diagnostics::findings(&error, None),
        Err(source_diagnostics::SourceProjectionError::Context(
            ProjectionError::Limit
        ))
    );
    let failure: Value = serde_json::from_str(&projection::failure(&error, None)).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"ingest","code":"diagnostic_context_limit"}})
    );
    // The former owned API retains its contract while callers migrate explicitly.
    assert_eq!(projection::findings(&error, None).unwrap().len(), 2);
    let Error::Sources(errors) = &error else {
        panic!()
    };
    let Error::TypedSource(typed_csv::Error::Typing(yamaa_core::typed_csv::Error::FieldParse {
        value,
        ..
    })) = &errors[1].1
    else {
        panic!()
    };
    assert_eq!(value.len(), MAX_PROJECTION_BYTES / 6);
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
fn wide_nested_semantic_report_under_tiny_context_quota_uses_fixed_borrowed_frames() {
    // This input already owns its breadth. Traversal uses a fixed stack array of
    // 128 borrowed iterators and never allocates a pending sibling list. The zero
    // context quota also refuses borrowed finding staging before Vec allocation.
    let error = nested(Error::Sources(wide(50_000)), 2);
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, 0),
        Err(source_diagnostics::SourceProjectionError::Context(
            ProjectionError::Limit
        ))
    );
    let Error::Sources(outer) = &error else {
        panic!()
    };
    assert_eq!(outer[0].0.name, "DEEP");
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, 0),
        Err(source_diagnostics::SourceProjectionError::Context(
            ProjectionError::Limit
        ))
    );
}

#[test]
fn late_opaque_after_wide_nested_report_precedes_tiny_context_quota_and_keeps_allocation() {
    let mut children = wide(50_000);
    let original = opaque();
    let pointer = opaque_text(&original).as_ptr();
    children.push((source("OPAQUE", "opaque.parquet"), original));
    let error = nested(Error::Sources(children), 2);
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, 0),
        Ok(None)
    );
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
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, 0),
        Ok(None)
    );
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
            value: "9".repeat(MAX_PROJECTION_BYTES / 6),
        },
    ));
    let error = Error::Sources(vec![
        (source("FIRST", "first.parquet"), original),
        (
            source("LATER", "later.csv"),
            nested(late, source_diagnostics::MAX_SOURCE_REPORT_FRAMES + 1),
        ),
    ]);
    assert_eq!(
        source_diagnostics::findings_with_limit(&error, None, 0),
        Ok(None)
    );
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
fn structural_frame_refusal_precedes_only_unreached_late_barriers_and_preserves_facts() {
    let leaf = Error::ParquetSource(parquet::Error::Unsupported {
        field: "F".into(),
        stored_type: "held physical type".into(),
    });
    let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = &leaf else {
        panic!()
    };
    let pointer = stored_type.as_ptr();
    let admitted = nested(leaf, source_diagnostics::MAX_SOURCE_REPORT_FRAMES);
    let diagnostics = source_diagnostics::findings(&admitted, None)
        .unwrap()
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].spec_paths, ["input.DEEP.path"]);
    // The outer collection requires one more active frame, and must refuse before
    // storing it or inspecting either the deep leaf or the later opaque sibling.
    let refused = Error::Sources(vec![
        (source("DEEP", "deep.parquet"), admitted),
        (source("LATE", "late.parquet"), opaque()),
    ]);
    assert_eq!(
        source_diagnostics::findings_with_limit(&refused, None, 0),
        Err(source_diagnostics::SourceProjectionError::TraversalLimit)
    );
    assert_eq!(
        source_diagnostics::findings(&refused, None),
        Err(source_diagnostics::SourceProjectionError::TraversalLimit)
    );
    let failure: Value = serde_json::from_str(&projection::failure(&refused, None)).unwrap();
    assert_eq!(
        failure,
        json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"ingest","code":"diagnostic_traversal_limit"}})
    );
    let Error::Sources(children) = &refused else {
        panic!()
    };
    let mut held = &children[0].1;
    for _ in 0..source_diagnostics::MAX_SOURCE_REPORT_FRAMES {
        let Error::Sources(child) = held else {
            panic!()
        };
        held = &child[0].1;
    }
    let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = held else {
        panic!()
    };
    assert_eq!(stored_type.as_ptr(), pointer);
    assert_eq!(stored_type, "held physical type");
}
