//! Table contracts.
#[allow(dead_code)]
mod support;
mod table {
    #[cfg(feature = "readstat")]
    mod sas_source {
        //! Binary source truth is authored independently of the candidate parser.
        use std::path::Path;
        use yamaa_adapters::sas_source::{self, Error};
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            table::{TableAccess, ValueRef},
            value::ColumnType,
        };

        fn bytes(name: &str) -> Vec<u8> {
            std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/sas")
                    .join(name),
            )
            .unwrap()
        }
        fn declaration(name: &str) -> SourceDeclaration {
            SourceDeclaration {
                name: "SRC".into(),
                path: name.into(),
                types: vec![],
                profile: SourceProfile::from_path(name).unwrap(),
                encoding: Some("windows-1252".into()),
                empty_string_present: false,
                ordinal: None,
            }
        }
        #[test]
        fn rle_and_rdc_compression_equal_independent_upstream_csv_truth() {
            let expected =
                yamaa_adapters::csv_source::parse(&bytes("compressed-truth.csv")).unwrap();
            for file in ["rle.sas7bdat", "rdc.sas7bdat"] {
                let table = sas_source::parse(&bytes(file), &declaration(file)).unwrap();
                assert_eq!(table.row_count(), expected.records.len());
                assert_eq!(table.schema().columns().len(), 100);
                for (column, field) in table.schema().columns().iter().enumerate() {
                    assert_eq!(field.name, expected.names[column]);
                    assert_eq!(
                        field.kind,
                        if column % 4 == 1 {
                            ColumnType::Str
                        } else {
                            ColumnType::Float
                        }
                    );
                    for (row, record) in expected.records.iter().enumerate() {
                        let actual = table.cell(row, column).unwrap();
                        match record[column].as_deref() {
                            None => assert_eq!(actual, ValueRef::Missing),
                            Some(text) if column % 4 == 1 => {
                                assert_eq!(actual, ValueRef::Str(text))
                            }
                            Some(number) => assert_eq!(
                                actual,
                                ValueRef::Float(
                                    yamaa_core::value::FiniteFloat::new(number.parse().unwrap())
                                        .unwrap()
                                )
                            ),
                        }
                    }
                }
            }
        }
        #[test]
        fn malformed_compressed_controls_fail_closed() {
            // Independently identified first compressed-row command offsets from the
            // upstream pandas SAS corruption controls, retained in the source inventory.
            for (file, offset, byte) in [
                ("rle.sas7bdat", 0x10000 + 55229, 0x10),
                ("rdc.sas7bdat", 118170, 184),
            ] {
                let mut data = bytes(file);
                data[offset] = byte;
                assert!(
                    sas_source::parse(&data, &declaration(file)).is_err(),
                    "{file}"
                );
            }
        }
        #[test]
        fn rdc_overlapping_back_reference_propagates_newly_emitted_bytes() {
            let file = "rdc-big-endian.sas7bdat";
            let mut data = bytes(file);
            assert_eq!(&data[120922..120924], &[0x8d, 0x00]);
            data[120922..120924].copy_from_slice(&[0x81, 0x00]);
            let table = sas_source::parse(&data, &declaration(file)).unwrap();
            let independent_original = 0.621f64.to_be_bytes();
            let mut repeated = [0; 8];
            repeated[..4].copy_from_slice(&independent_original[4..]);
            repeated[4..].copy_from_slice(&independent_original[4..]);
            assert_eq!(
                table.cell(0, 10).unwrap(),
                ValueRef::Float(
                    yamaa_core::value::FiniteFloat::new(f64::from_be_bytes(repeated)).unwrap()
                )
            );
        }
        #[test]
        fn both_xport_versions_preserve_order_padding_unicode_and_default_float() {
            for (file, text) in [("v5.xpt", "TEXT"), ("v8.xpt", "LONG_TEXT_NAME")] {
                let mut source = declaration(file);
                let table = sas_source::parse(&bytes(file), &source).unwrap();
                assert_eq!(table.row_count(), 3);
                assert_eq!(table.schema().columns()[0].kind, ColumnType::Float);
                assert_eq!(table.schema().columns()[1].name, text);
                assert_eq!(
                    table.cell(0, 0).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(2.0).unwrap())
                );
                assert_eq!(
                    table.cell(1, 0).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(1.0).unwrap())
                );
                assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str(" café"));
                assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Missing);
                assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Str("."));
                assert_eq!(table.cell(1, 2).unwrap(), ValueRef::Missing);
                // DATE9-like physical formats above do not infer the column type.
                assert_eq!(table.schema().columns()[3].kind, ColumnType::Float);
                source.empty_string_present = true;
                source.types = vec![
                    ("ID".into(), ColumnType::Int),
                    ("DAY".into(), ColumnType::Date),
                    ("STAMP".into(), ColumnType::DateTime),
                ];
                let table = sas_source::parse(&bytes(file), &source).unwrap();
                assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(2));
                assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str(""));
                assert_eq!(
                    table.cell(0, 3).unwrap(),
                    ValueRef::Date("1960-01-01".parse().unwrap())
                );
                assert_eq!(
                    table.cell(1, 3).unwrap(),
                    ValueRef::Date("1970-01-01".parse().unwrap())
                );
                assert_eq!(
                    table.cell(1, 4).unwrap(),
                    ValueRef::DateTime("1959-12-31T23:59:59".parse().unwrap())
                );
            }
        }
        #[test]
        fn sas_dataset_numeric_date_formats_never_infer_and_epoch_narrowing_is_explicit() {
            let mut source = declaration("datetime.sas7bdat");
            let table = sas_source::parse(&bytes("datetime.sas7bdat"), &source).unwrap();
            assert_eq!(table.schema().columns().len(), 5);
            assert!(table
                .schema()
                .columns()
                .iter()
                .all(|column| column.kind == ColumnType::Float));
            source.types = vec![
                ("VAR1".into(), ColumnType::DateTime),
                ("VAR2".into(), ColumnType::Date),
                ("VAR3".into(), ColumnType::Date),
                ("VAR4".into(), ColumnType::Date),
                ("VAR5".into(), ColumnType::Int),
            ];
            let table = sas_source::parse(&bytes("datetime.sas7bdat"), &source).unwrap();
            assert_eq!(
                table.cell(0, 0).unwrap(),
                ValueRef::DateTime("2015-02-02T14:42:12".parse().unwrap())
            );
            for column in [1, 2, 3] {
                assert_eq!(
                    table.cell(0, column).unwrap(),
                    ValueRef::Date("2015-02-02".parse().unwrap())
                );
            }
            assert_eq!(table.cell(0, 4).unwrap(), ValueRef::Int(52932));
        }
        #[test]
        fn all_numeric_missing_tags_map_to_missing() {
            let table =
                sas_source::parse(&bytes("missing.xpt"), &declaration("missing.xpt")).unwrap();
            assert_eq!(table.row_count(), 28);
            for row in 0..28 {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
            }
        }
        #[test]
        fn nonfinite_numeric_source_is_missing_before_every_authored_narrowing() {
            let mut payload = bytes("fractional.xpt");
            // Independent v5 layout: 640 header bytes, 160 NAMESTR/padding bytes,
            // 80 OBS bytes, then the first 8-byte numeric cell. ReadStat's documented
            // IBM non-finite sentinel maps to infinity, then core missing normalization.
            payload[880..888].copy_from_slice(&[0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
            for kind in [
                ColumnType::Float,
                ColumnType::Int,
                ColumnType::Date,
                ColumnType::DateTime,
            ] {
                let mut source = declaration("fractional.xpt");
                source.types = vec![("NUM".into(), kind)];
                let table = sas_source::parse(&payload, &source).unwrap();
                assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
            }
        }
        #[test]
        fn utf8_and_integer_boundaries_have_independent_exact_truth() {
            let mut source = declaration("utf8.xpt");
            source.encoding = Some("utf-8".into());
            let table = sas_source::parse(&bytes("utf8.xpt"), &source).unwrap();
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str(" café"));
            let mut source = declaration("integer-bounds.xpt");
            source.types = vec![("NUM".into(), ColumnType::Int)];
            let table = sas_source::parse(&bytes("integer-bounds.xpt"), &source).unwrap();
            for (row, number) in [i64::MIN, 0, 9_007_199_254_740_992, i64::MAX - 1023]
                .into_iter()
                .enumerate()
            {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(number));
            }
            source.path = "integer-overflow.xpt".into();
            assert!(matches!(
                sas_source::parse(&bytes("integer-overflow.xpt"), &source,),
                Err(Error::Typing(
                    yamaa_core::typed_csv::Error::FieldParse { .. }
                ))
            ));
        }
        #[test]
        fn unsupported_compression_has_a_specific_retained_cause() {
            let source = declaration("unsupported-compression.sas7bdat");
            assert!(matches!(
                sas_source::parse(&bytes(&source.path), &source),
                Err(Error::Invalid("compression"))
            ));
        }
        #[test]
        fn reject_multiple_members_in_both_transport_versions() {
            for name in ["multi-v5.xpt", "multi-v8.xpt"] {
                assert!(matches!(
                    sas_source::parse(&bytes(name), &declaration(name)),
                    Err(Error::Invalid("members"))
                ));
            }
        }
        #[test]
        fn member_header_shaped_character_data_is_an_ordinary_value() {
            let source = declaration("member-text.xpt");
            let table = sas_source::parse(&bytes(&source.path), &source).unwrap();
            assert_eq!(table.row_count(), 1);
            assert_eq!(
        table.cell(0, 0).unwrap(),
        ValueRef::Str(
            "HEADER RECORD*******MEMBER  HEADER RECORD!!!!!!!000000000000000000000000000000"
        )
    );
        }
        #[test]
        fn ordinary_float_keeps_readstat_conversion_while_exact_narrowing_is_guarded() {
            for (file, expected) in [
                ("decimal.xpt", 0.1),
                ("precision.xpt", 9_007_199_254_740_992.0),
            ] {
                let source = declaration(file);
                let table = sas_source::parse(&bytes(file), &source).unwrap();
                assert_eq!(
                    table.cell(0, 0).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(expected).unwrap())
                );
            }
        }
        #[test]
        fn narrowing_never_truncates_or_accepts_an_ibm_precision_loss() {
            for name in ["fractional.xpt", "precision.xpt"] {
                for kind in [ColumnType::Int, ColumnType::Date, ColumnType::DateTime] {
                    let mut source = declaration(name);
                    source.types = vec![("NUM".into(), kind)];
                    assert!(matches!(
                        sas_source::parse(&bytes(name), &source),
                        Err(Error::Typing(
                            yamaa_core::typed_csv::Error::FieldParse { .. }
                        ))
                    ));
                }
            }
        }
        #[test]
        fn malformed_truncated_encoding_and_unknown_fields_fail_before_execution() {
            let source = declaration("v5.xpt");
            let payload = bytes("v5.xpt");
            for length in [0, 1, 79, 640, payload.len() - 1] {
                assert!(sas_source::parse(&payload[..length], &source).is_err());
            }
            let mut source = source;
            source.encoding = Some("utf-8".into());
            assert!(matches!(
                sas_source::parse(&payload, &source),
                Err(Error::Invalid("encoding"))
            ));
            source.encoding = Some("windows-1252".into());
            source.types = vec![("ABSENT".into(), ColumnType::Int)];
            assert!(matches!(
                sas_source::parse(&payload, &source),
                Err(Error::Typing(
                    yamaa_core::typed_csv::Error::UnknownField { .. }
                ))
            ));
            let sas = bytes("datetime.sas7bdat");
            for length in [0, 31, 1024, sas.len() - 1] {
                assert!(matches!(
                    sas_source::parse(&sas[..length], &declaration("datetime.sas7bdat"),),
                    Err(Error::Invalid(_))
                ));
            }
        }
    }
    mod source_diagnostics {
        //! Borrowed codec projections compare independently captured complete diagnostics.
        use yamaa_adapters::json;
        use yamaa_adapters::{
            csv_source::{self, TextTableError},
            parquet_source as parquet, source_diagnostics, specification_diagnostics as projection,
            specification_run::Error,
            typed_csv,
        };
        use yamaa_core::json::Value;
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
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect()
        }
        fn assert_complete(error: &Error, source: &SourceDeclaration, expected: &Value) {
            let wanted = expected.as_array().unwrap();
            assert_eq!(
                projection::findings(error, Some(source)).unwrap().unwrap(),
                *wanted
            );
            let failure: Value =
                yamaa_adapters::json_wire::decode(&projection::failure(error, Some(source)))
                    .unwrap();
            assert_eq!(
                failure,
                json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":wanted}})
            );
        }

        #[test]
        fn all_independent_csv_profile_reports_preserve_original_projection() {
            let declaration = source("SRC", "source.csv");
            let mut count = 0;
            for row in yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/csv_profile_diagnostics.json"
            ))
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
            for row in
                yamaa_adapters::json_wire::decode_array(include_str!("fixtures/source_typing.json"))
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
                    "stored-order" => {
                        vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)]
                    }
                    "invalid-temporal" => vec![("V".into(), ColumnType::Date)],
                    _ => panic!("unknown independent fixture"),
                };
                let source_bytes: Vec<u8> = if let Some(text) = row["source_bytes"].as_str() {
                    text.as_bytes().to_vec()
                } else {
                    bytes(row["source_bytes"]["hex"].as_str().unwrap())
                };
                let error =
                    Error::TypedSource(typed_csv::parse(&source_bytes, &types).unwrap_err());
                assert_complete(&error, &declaration, &expected["diagnostics"]);
                count += 1;
            }
            assert_eq!(count, 4);
        }

        #[test]
        #[cfg(feature = "parquet")]
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
            let Error::ParquetSource(parquet::Error::Unsupported { field, stored_type }) =
                &nested[0].1
            else {
                panic!()
            };
            let pointers = (
                field.as_ptr(),
                stored_type.as_ptr(),
                nested[0].0.path.as_ptr(),
            );
            let finding = match &nested[0].1 {
                Error::ParquetSource(error) => {
                    error.finding(&nested[0].0.name, &nested[0].0.path).unwrap()
                }
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
            let failure: Value =
                yamaa_adapters::json_wire::decode(&projection::failure(&semantic, None)).unwrap();
            assert_eq!(
                failure,
                json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"ingest","code":"diagnostic_context"}})
            );
            let declaration = source("SRC", "source.csv");
            for error in [
                Error::Source(TextTableError::Csv(csv_source::Error::Limit {
                    resource: "source_bytes",
                    limit: yamaa_core::limits::Limits::SOURCE_BYTES,
                    actual: Some(yamaa_core::limits::Limits::SOURCE_BYTES + 1),
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
                let failure: Value =
                    yamaa_adapters::json_wire::decode(&projection::failure(&collected, None))
                        .unwrap();
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
            let failure: Value =
                yamaa_adapters::json_wire::decode(&projection::failure(&error, None)).unwrap();
            assert_eq!(
                failure,
                json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":portable}})
            );
            let Error::Sources(errors) = &error else {
                panic!("retained collection")
            };
            let Error::TypedSource(typed_csv::Error::Typing(
                yamaa_core::typed_csv::Error::FieldParse { value, .. },
            )) = &errors[1].1
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
                yamaa_adapters::arrow_table::TableError::Arrow(
                    arrow_schema::ArrowError::ComputeError(
                        "original opaque codec allocation".into(),
                    ),
                ),
            ))
        }
        fn opaque_text(error: &Error) -> &str {
            let Error::ParquetSource(parquet::Error::Table(
                yamaa_adapters::arrow_table::TableError::Arrow(
                    arrow_schema::ArrowError::ComputeError(text),
                ),
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
            let failure: Value =
                yamaa_adapters::json_wire::decode(&projection::failure(&error, None)).unwrap();
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
            let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = &leaf
            else {
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
            let failure: Value =
                yamaa_adapters::json_wire::decode(&projection::failure(&held, None)).unwrap();
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
    }
    mod typed_csv {
        use yamaa_adapters::{
            csv_source,
            typed_csv::{self, Error},
        };
        use yamaa_core::{
            table::{TableAccess, ValueRef},
            typed_csv::Error as TypingError,
            value::ColumnType as T,
        };
        #[test]
        fn declared_types_keep_exact_i64_float_bits_temporal_precision_and_undeclared_text() {
            let data=b"I,F,D,T,RAW\n9007199254740993,-0.0,2024-03-02,2024-03-02T00:00:00,0012\n,1.5,2024-01-01,2024-03-02T01:02:03,NA\n";
            let table = typed_csv::parse(
                data,
                &[
                    ("I".into(), T::Int),
                    ("F".into(), T::Float),
                    ("D".into(), T::Date),
                    ("T".into(), T::DateTime),
                ],
            )
            .unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(9007199254740993));
            let ValueRef::Float(value) = table.cell(0, 1).unwrap() else {
                panic!()
            };
            assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
            let ValueRef::Date(value) = table.cell(0, 2).unwrap() else {
                panic!()
            };
            assert_eq!(value.to_string(), "2024-03-02");
            assert_eq!(
                value.collected_precision(),
                yamaa_core::temporal::DatePrecision::Day
            );
            let ValueRef::DateTime(value) = table.cell(0, 3).unwrap() else {
                panic!()
            };
            assert_eq!(
                value.collected_precision(),
                yamaa_core::temporal::DateTimePrecision::Second
            );
            assert_eq!(table.cell(0, 4).unwrap(), ValueRef::Str("0012"));
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(1, 4).unwrap(), ValueRef::Str("NA"));
        }
        #[test]
        fn unknown_fields_follow_written_declaration_order_before_conversion() {
            let error = typed_csv::parse(
                b"X\nnot-an-int",
                &[
                    ("X".into(), T::Int),
                    ("Z".into(), T::Float),
                    ("A".into(), T::Int),
                ],
            )
            .unwrap_err();
            assert!(matches!(error,Error::Typing(TypingError::UnknownField{field}) if field=="Z"));
        }
        #[test]
        fn conversion_failure_follows_rows_then_stored_fields_not_declaration_order() {
            let error = typed_csv::parse(
                b"A,B\n1,bad\nworse,2",
                &[("B".into(), T::Int), ("A".into(), T::Int)],
            )
            .unwrap_err();
            assert!(
                matches!(error,Error::Typing(TypingError::FieldParse{field,target:T::Int,value}) if field=="B"&&value=="bad")
            );
            let error = typed_csv::parse(
                b"A,B\nfirst,second",
                &[("B".into(), T::Int), ("A".into(), T::Int)],
            )
            .unwrap_err();
            assert!(
                matches!(error,Error::Typing(TypingError::FieldParse{field,value,..}) if field=="A"&&value=="first")
            );
        }
        #[test]
        fn profile_failure_precedes_unknown_fields_and_nonfinite_spellings_normalize() {
            let error = typed_csv::parse(b"X\n\"bad", &[("Z".into(), T::Int)]).unwrap_err();
            assert!(matches!(
                error,
                Error::Csv(csv_source::Error::Profile {
                    cause: csv_source::ProfileCause::QuoteUnterminated,
                    ..
                })
            ));
            let table =
                typed_csv::parse(b"F\n.nan\n+.inf\n-.inf", &[("F".into(), T::Float)]).unwrap();
            for row in 0..3 {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
            }
        }

        #[test]
        fn incomplete_temporal_text_is_not_implicitly_imputed() {
            for (kind, text) in [(T::Date, "2024-03"), (T::DateTime, "2024-03-02")] {
                let input = format!("X\n{text}");
                assert!(
                    matches!(typed_csv::parse(input.as_bytes(),&[("X".into(),kind)]),Err(Error::Typing(TypingError::FieldParse {field,value,..})) if field=="X"&&value==text)
                );
            }
        }

        #[test]
        fn retired_python_integer_ingestion_keeps_every_i64_through_long_zero_prefixes() {
            // Original test_source.py operands: CSV admission must not inherit a host
            // integer-string digit ceiling or round the values through binary64.
            let expected = [0_i64, 1, 9_007_199_254_740_993, i64::MAX, i64::MIN];
            let mut input = String::from("VALUE\n");
            for value in expected {
                input.push(if value < 0 { '-' } else { '+' });
                input.push_str(&"0".repeat(5000));
                input.push_str(&value.unsigned_abs().to_string());
                input.push('\n');
            }
            let table = typed_csv::parse(input.as_bytes(), &[("VALUE".into(), T::Int)]).unwrap();
            assert_eq!(table.row_count(), expected.len());
            for (row, value) in expected.into_iter().enumerate() {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(value));
            }
        }

        #[test]
        fn retired_python_typed_csv_keeps_original_field_order_missingness_and_civil_time() {
            let declared = [
                ("AGE".into(), T::Int),
                ("SCORE".into(), T::Float),
                ("DATE".into(), T::Date),
                ("MOMENT".into(), T::DateTime),
            ];
            let table = typed_csv::parse(
                b"ID,AGE,SCORE,DATE,MOMENT,EMPTY,TEXT\n\
007,42,1.5,2025-01-02,2025-01-02T03:04,,\"\"\n\
008,,.Inf,2025-02-03,2025-02-03T04:05:06,NA,unknown\n",
                &declared,
            )
            .unwrap();
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| (column.name.as_str(), column.kind))
                    .collect::<Vec<_>>(),
                [
                    ("ID", T::Str),
                    ("AGE", T::Int),
                    ("SCORE", T::Float),
                    ("DATE", T::Date),
                    ("MOMENT", T::DateTime),
                    ("EMPTY", T::Str),
                    ("TEXT", T::Str),
                ]
            );
            let expected = [
                vec![
                    ValueRef::Str("007"),
                    ValueRef::Int(42),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(1.5).unwrap()),
                    ValueRef::Date("2025-01-02".parse().unwrap()),
                    ValueRef::DateTime("2025-01-02T03:04".parse().unwrap()),
                    ValueRef::Missing,
                    ValueRef::Missing,
                ],
                vec![
                    ValueRef::Str("008"),
                    ValueRef::Missing,
                    ValueRef::Missing,
                    ValueRef::Date("2025-02-03".parse().unwrap()),
                    ValueRef::DateTime("2025-02-03T04:05:06".parse().unwrap()),
                    ValueRef::Str("NA"),
                    ValueRef::Str("unknown"),
                ],
            ];
            assert_eq!(table.row_count(), expected.len());
            for (row, values) in expected.into_iter().enumerate() {
                for (column, value) in values.into_iter().enumerate() {
                    assert_eq!(table.cell(row, column).unwrap(), value, "{row}/{column}");
                }
            }

            let empty = typed_csv::parse(b"ID,AGE\n", &declared[..1]).unwrap();
            assert_eq!(empty.row_count(), 0);
            assert_eq!(empty.schema().columns()[0].kind, T::Str);
            assert_eq!(empty.schema().columns()[1].kind, T::Int);
            let quoted = typed_csv::parse(b"AGE\n\"\"\n", &declared[..1]).unwrap();
            assert_eq!(quoted.cell(0, 0).unwrap(), ValueRef::Missing);
        }

        #[test]
        fn retired_python_missing_looking_text_is_never_inferred_as_missing() {
            let table = typed_csv::parse(b"VALUE\nNA\nNULL\n.\nunknown\n", &[]).unwrap();
            for (row, expected) in ["NA", "NULL", ".", "unknown"].into_iter().enumerate() {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Str(expected));
            }
        }
    }
}
