//! Source contracts: CSV precision, CSV source, mapping cut, mapping dictionary
//! resources, ODM source, resource scheme, source diagnostics, table and typed
//! CSV.
mod source {
    mod csv_precision {
        use yamaa_core::{csv_precision::fixed_point, value::FiniteFloat};

        #[test]
        fn ties_and_near_ties_round_away_from_zero_with_independent_text() {
            // Scaled distances below the tie are from the exact binary64 value;
            // FORMULA-16 counts anything within 2^-26 (about 1.49e-8) as a tie.
            for (value, digits, expected) in [
                (0.125, 2, "0.13"),
                (-0.125, 2, "-0.13"),
                // 1.8e-14, 1.1e-14 and 2.4e-15 below the tie.
                (2.675, 2, "2.68"),
                (-2.675, 2, "-2.68"),
                (1.005, 2, "1.01"),
                (0.285, 2, "0.29"),
                // 1.0e-8 below the tie is inside the window; 1.0e-7 is not.
                (1.0049999999, 2, "1.01"),
                (1.004999999, 2, "1.00"),
                (25.0, 4, "25.0000"),
                (0.03125, 4, "0.0313"),
                (0.5, 0, "1"),
                (1.5, 0, "2"),
                (-0.5, 0, "-1"),
                // A value that rounds to zero is written without a sign.
                (-0.004, 2, "0.00"),
                (-0.4, 0, "0"),
                (0.0, 3, "0.000"),
                (-0.0, 3, "0.000"),
                (1.234, 2, "1.23"),
                (2.345, 2, "2.35"),
                // The binary64 values adjacent to 0.125: 1.4e-15 below, and above.
                (f64::from_bits(0x3fbf_ffff_ffff_ffff), 2, "0.13"),
                (f64::from_bits(0x3fc0_0000_0000_0001), 2, "0.13"),
                (-f64::from_bits(0x3fbf_ffff_ffff_ffff), 2, "-0.13"),
                (9.999, 2, "10.00"),
            ] {
                let finite = FiniteFloat::new(value).unwrap();
                assert_eq!(
                    fixed_point(finite, digits, expected.len()).unwrap(),
                    expected
                );
                assert!(fixed_point(finite, digits, expected.len() - 1).is_err());
                assert_eq!(finite.get().to_bits(), value.to_bits());
            }
        }

        #[test]
        fn large_precision_uses_the_exact_fraction_and_retains_all_declared_zeros() {
            // Exact powers of two give independently known finite decimal expansions.
            for (value, prefix, zero_count) in [
                (0.125, "0.125", 4997),
                (-0.0009765625, "-0.0009765625", 4990),
                (25.0, "25.", 5000),
                (-0.0, "0.", 5000),
            ] {
                let expected = prefix.to_owned() + &"0".repeat(zero_count);
                assert_eq!(
                    fixed_point(FiniteFloat::new(value).unwrap(), 5000, expected.len()).unwrap(),
                    expected
                );
            }
        }

        #[test]
        fn impossible_width_refuses_without_constructing_large_powers_or_text() {
            for digits in [usize::MAX, usize::MAX - 2, 1_000_000_000] {
                assert!(fixed_point(FiniteFloat::new(0.125).unwrap(), digits, 4096).is_err());
            }
            assert!(fixed_point(FiniteFloat::new(1e308).unwrap(), 0, 308).is_err());
        }

        #[test]
        fn subnormal_values_and_large_integral_values_preserve_fixed_width() {
            let smallest = FiniteFloat::new(f64::from_bits(1)).unwrap();
            assert_eq!(
                fixed_point(smallest, 324, 326).unwrap(),
                "0.".to_owned() + &"0".repeat(323) + "5"
            );
            assert_eq!(
                fixed_point(smallest, 325, 327).unwrap(),
                "0.".to_owned() + &"0".repeat(323) + "49"
            );
            let expected = "100000000000000000000.0000";
            assert_eq!(
                fixed_point(FiniteFloat::new(1e20).unwrap(), 4, expected.len()).unwrap(),
                expected
            );
        }
    }
    mod csv_source {
        use yamaa_core::csv_source::{parse, CsvSource, Error, Field, ProfileCause};

        #[test]
        fn collected_text_and_missing_remain_lossless() {
            let actual = parse(
                b"ID,N,T,Q\r\nS,9007199254740993,NA,\"\"\r\nT,,x\0y,\"line\n\"\"quoted\"\"\"",
            )
            .unwrap();
            assert_eq!(
                actual,
                CsvSource {
                    names: ["ID", "N", "T", "Q"].map(String::from).to_vec(),
                    records: vec![
                        vec![
                            Some("S".into()),
                            Some("9007199254740993".into()),
                            Some("NA".into()),
                            None
                        ],
                        vec![
                            Some("T".into()),
                            None,
                            Some("x\0y".into()),
                            Some("line\n\"quoted\"".into())
                        ],
                    ],
                }
            );
            assert_eq!(
                parse("λ\n € ".as_bytes()).unwrap().records,
                vec![vec![Some(" € ".into())]]
            );
            assert!(parse(b"A,B\n").unwrap().records.is_empty());
            assert_eq!(
                parse(b"A,B\nx,").unwrap().records,
                vec![vec![Some("x".into()), None]]
            );
        }

        #[test]
        fn defects_have_exact_priority_and_coordinates() {
            for (input, condition, record, field) in [
                (&b""[..], "source_header_absent", 1, 1),
                (&b"\xef\xbb\xbfA\n"[..], "source_byte_order_mark", 1, 1),
                (&b"A,B\nx\r"[..], "source_carriage_return", 2, 1),
                (&b"A\n\"x\r\n\""[..], "source_carriage_return", 2, 1),
                (&b"A\nx\"y"[..], "source_quote_in_bare_field", 2, 1),
                (&b"A\n\"x\" "[..], "source_text_after_quote", 2, 1),
                (&b"A\n\"x"[..], "source_quote_unterminated", 2, 1),
                (&b"A,B\nx"[..], "source_record_width", 2, 2),
                (&b"A\nx,y"[..], "source_record_width", 2, 2),
                (&b"A,\nx,y"[..], "source_field_name_empty", 1, 2),
                // Decode precedes BOM/syntax; coordinate scanning honors quoted LF.
                (&b"\xef\xbb\xbfA\n\"x\ny\",\xff"[..], "invalid_text", 2, 2),
                // Complete syntax precedes duplicate header/earlier width findings.
                (&b"A,A\nx\n\"z"[..], "source_quote_unterminated", 3, 1),
            ] {
                assert_eq!(
                    parse(input),
                    Err(Error::Profile {
                        cause: cause(condition),
                        record,
                        field: Field::Index(field)
                    })
                );
            }
            assert_eq!(
                parse(b"A,A\nx,y"),
                Err(Error::Profile {
                    cause: ProfileCause::FieldNameDuplicate,
                    record: 1,
                    field: Field::Name("A".into())
                })
            );
        }

        /// CSV-10: a declared mark begins the source and is skipped once; the
        /// first record starts after it. Every other leading mark fails, and
        /// a mark after the first character is text.
        #[test]
        fn a_declared_mark_is_skipped_once_and_any_other_mark_fails() {
            use yamaa_core::csv_source::parse_declared;
            let mark = |declared, record, field| {
                Err(Error::Profile {
                    cause: ProfileCause::ByteOrderMark { declared },
                    record,
                    field: Field::Index(field),
                })
            };
            // A spreadsheet's UTF-8 export: the mark, then CR LF records.
            assert_eq!(
                parse_declared(b"\xef\xbb\xbfID,NAME\r\n1,H\xc3\xb4pital\r\n", true),
                Ok(CsvSource {
                    names: vec!["ID".into(), "NAME".into()],
                    records: vec![vec![Some("1".into()), Some("H\u{f4}pital".into())]],
                })
            );
            // The declared mark followed by a source reads as the source
            // alone, findings and coordinates included, unless the source
            // starts with a mark of its own.
            for source in [
                &b"ID\n1\n"[..],
                b"",
                b"A,B\r\nx,y",
                b"\xff",
                b"\xef\xbb",
                b"A\n\"x",
                b"A,A\nx,y",
                b"A\nx\ry\n",
                b"A\n\"x\ny\",\xff",
            ] {
                let marked = [&b"\xef\xbb\xbf"[..], source].concat();
                assert_eq!(parse_declared(&marked, true), parse(source), "{source:?}");
            }
            // The first field starts after the mark, so it may be quoted.
            assert_eq!(
                parse_declared(b"\xef\xbb\xbf\"ID\"\n1", true)
                    .unwrap()
                    .names,
                ["ID"]
            );
            // A mark later in the source is a character of its field.
            assert_eq!(
                parse_declared(b"\xef\xbb\xbfID\nx\xef\xbb\xbfy\n", true)
                    .unwrap()
                    .records,
                [[Some("x\u{feff}y".into())]]
            );
            assert_eq!(
                parse(b"ID\nx\xef\xbb\xbfy\n").unwrap().records,
                [[Some("x\u{feff}y".into())]]
            );
            // The declaration decides: an absent declared mark, a second mark
            // and an undeclared mark fail at the first field of the header.
            assert_eq!(parse_declared(b"ID\n1\n", true), mark(true, 1, 1));
            assert_eq!(parse_declared(b"", true), mark(true, 1, 1));
            assert_eq!(
                parse_declared(b"\xef\xbb\xbf\xef\xbb\xbfID\n", true),
                mark(true, 1, 1)
            );
            assert_eq!(
                parse_declared(b"\xef\xbb\xbfID\r\n1\r\n", false),
                mark(false, 1, 1)
            );
            assert_eq!(parse(b"\xef\xbb\xbfID\n"), mark(false, 1, 1));
            // What follows a declared mark is read as any source: a mark
            // alone holds no header, and decoding precedes the mark.
            assert_eq!(
                parse_declared(b"\xef\xbb\xbf", true),
                Err(Error::Profile {
                    cause: ProfileCause::HeaderAbsent,
                    record: 1,
                    field: Field::Index(1),
                })
            );
            assert_eq!(
                parse_declared(b"ID\n\xff", true),
                Err(Error::Profile {
                    cause: ProfileCause::InvalidText,
                    record: 2,
                    field: Field::Index(1),
                })
            );
        }

        // Independent operands and expected results from python/tests/io/test_csv.py
        // at c7b88a4f. Keep the original edge cases after retiring the Python parser.
        #[test]
        fn retired_python_quoted_and_bare_empty_records_keep_order() {
            let source = parse(
        b"ID,EMPTY,COMMENT,SPACE\r\n007,,\"line one\nline two, still one field\", kept \r\n008,\"\",\"said \"\"hello\"\"\",next",
    )
    .unwrap();
            assert_eq!(source.names, ["ID", "EMPTY", "COMMENT", "SPACE"]);
            assert_eq!(
                source.records,
                vec![
                    vec![
                        Some("007".into()),
                        None,
                        Some("line one\nline two, still one field".into()),
                        Some(" kept ".into()),
                    ],
                    vec![
                        Some("008".into()),
                        None,
                        Some("said \"hello\"".into()),
                        Some("next".into()),
                    ],
                ]
            );
        }

        #[test]
        fn retired_python_nonconforming_csv_has_exact_coordinates() {
            for (input, condition, record, field) in [
                (&b""[..], "source_header_absent", 1, Field::Index(1)),
                (
                    &b"\xef\xbb\xbfID\n1\n"[..],
                    "source_byte_order_mark",
                    1,
                    Field::Index(1),
                ),
                (
                    &b"ID,\n1,2\n"[..],
                    "source_field_name_empty",
                    1,
                    Field::Index(2),
                ),
                (
                    &b"ID,ID\n1,2\n"[..],
                    "source_field_name_duplicate",
                    1,
                    Field::Name("ID".into()),
                ),
                (
                    &b"A,B\n1,2,3\n"[..],
                    "source_record_width",
                    2,
                    Field::Index(3),
                ),
                (
                    &b"A,B,C\n1,2\n"[..],
                    "source_record_width",
                    2,
                    Field::Index(3),
                ),
                (
                    &b"A,B\n1,\"x\n"[..],
                    "source_quote_unterminated",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B\n1,x\"y\n"[..],
                    "source_quote_in_bare_field",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B\n1,\"x\"y\n"[..],
                    "source_text_after_quote",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B\n1,x\ry\n"[..],
                    "source_carriage_return",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B\n1,\"x\ry\"\n"[..],
                    "source_carriage_return",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B\n1,x\r"[..],
                    "source_carriage_return",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B\n1,ok\n2,\xf4bad\n"[..],
                    "invalid_text",
                    3,
                    Field::Index(2),
                ),
                (&b"\xff"[..], "invalid_text", 1, Field::Index(1)),
                (&b"A,\xff"[..], "invalid_text", 1, Field::Index(2)),
                (
                    &b"\"abc"[..],
                    "source_quote_unterminated",
                    1,
                    Field::Index(1),
                ),
                (
                    &b"a\"b"[..],
                    "source_quote_in_bare_field",
                    1,
                    Field::Index(1),
                ),
                (
                    &b"A,B,C\n1\n"[..],
                    "source_record_width",
                    2,
                    Field::Index(2),
                ),
                (
                    &b"A,B,C\n1,2,3,4,5\n"[..],
                    "source_record_width",
                    2,
                    Field::Index(4),
                ),
                (
                    &b"A\n1,2,3\n"[..],
                    "source_record_width",
                    2,
                    Field::Index(2),
                ),
            ] {
                assert_eq!(
                    parse(input),
                    Err(Error::Profile {
                        cause: cause(condition),
                        record,
                        field
                    }),
                    "{input:?}",
                );
            }
        }

        #[test]
        fn retired_python_admitted_terminators_and_header_only_schema_are_exact() {
            for input in [&b"A,B\n1,2\n"[..], &b"A,B\r\n1,2\r\n"[..], &b"A,B\n1,2"[..]] {
                let source = parse(input).unwrap();
                assert_eq!(source.names, ["A", "B"]);
                assert_eq!(source.records, [vec![Some("1".into()), Some("2".into())]]);
            }
            let header = parse(b"A,B\n").unwrap();
            assert_eq!(header.names, ["A", "B"]);
            assert!(header.records.is_empty());
        }

        #[test]
        fn retired_python_prefix_coordinates_do_not_count_quoted_delimiters() {
            for (prefix, record, field) in [
                ("", 1, 1),
                ("A,", 1, 2),
                ("A,B\n1,", 2, 2),
                ("A\nB\n", 3, 1),
                ("A,\"x,y\",", 1, 3),
                ("A,\"x\"\"y\",", 1, 3),
                ("A,\"x\ny\",", 1, 3),
                ("A,\"x", 1, 2),
                ("\"\",x\n", 2, 1),
                ("A\tB", 1, 1),
            ] {
                // Exercise the actual decoder diagnostic instead of a private prefix
                // helper: invalid UTF-8 at the standing position owns its coordinate.
                let input = [prefix.as_bytes(), &[0xff]].concat();
                assert_eq!(
                    parse(&input),
                    Err(Error::Profile {
                        cause: ProfileCause::InvalidText,
                        record,
                        field: Field::Index(field),
                    }),
                    "{prefix:?}",
                );
            }
        }

        #[test]
        fn retired_python_bare_control_character_is_field_content() {
            assert_eq!(
                parse(b"A\n\x0b\n").unwrap().records,
                [vec![Some("\u{b}".into())]],
            );
        }

        #[test]
        fn retired_python_long_fields_keep_final_records_and_unterminated_quote_failure() {
            let field = "x".repeat(300);
            for input in [format!("A\n{field}\n"), format!("A\n{field}")] {
                assert_eq!(
                    parse(input.as_bytes()).unwrap().records,
                    [vec![Some(field.clone())]],
                );
            }
            assert_eq!(
                parse(format!("A\n\"{field}").as_bytes()),
                Err(Error::Profile {
                    cause: ProfileCause::QuoteUnterminated,
                    record: 2,
                    field: Field::Index(1),
                }),
            );
        }

        #[test]
        fn fixed_column_boundary_preserves_every_admitted_name() {
            let maximum = yamaa_core::limits::Limits::COLUMNS;
            for columns in [maximum - 1, maximum, maximum + 1] {
                let names = (0..columns)
                    .map(|index| format!("C{index}"))
                    .collect::<Vec<_>>();
                let input = names.join(",") + "\n";
                let result = parse(input.as_bytes());
                if columns <= maximum {
                    let source = result.unwrap();
                    assert_eq!(source.names, names);
                    assert!(source.records.is_empty());
                } else {
                    assert_eq!(
                        result,
                        Err(Error::Limit {
                            resource: "columns",
                            limit: maximum,
                            actual: Some(columns),
                        })
                    );
                }
            }
        }

        #[test]
        fn field_boundaries_cover_leading_empty_fields_and_records_after_crlf() {
            let maximum = yamaa_core::limits::Limits::COLUMNS;
            let names = (0..maximum)
                .map(|index| format!("C{index}"))
                .collect::<Vec<_>>();
            let header = names.join(",");
            for input in [
                ",".repeat(maximum) + "last",
                "\"\",".repeat(maximum) + "last",
                format!("{header}\r\n{}", ",".repeat(maximum)),
                format!("{header}\r\n{}\r\n", ",".repeat(maximum)),
            ] {
                assert_eq!(
                    parse(input.as_bytes()),
                    Err(Error::Limit {
                        resource: "columns",
                        limit: maximum,
                        actual: Some(maximum + 1),
                    })
                );
            }
            let input = format!("{header}\r\n{}\r\n", ",".repeat(maximum - 1));
            let admitted = parse(input.as_bytes()).unwrap();
            assert_eq!(admitted.names, names);
            assert_eq!(admitted.records, [vec![None; maximum]]);
        }

        #[test]
        fn discarded_ragged_rows_preserve_complete_syntax_and_header_priority() {
            for (input, cause, record, field) in [
                (
                    "A,B\nx\n1,2\n\"unfinished",
                    ProfileCause::QuoteUnterminated,
                    4,
                    1,
                ),
                ("A,\nx\n1,2", ProfileCause::FieldNameEmpty, 1, 2),
                ("A,B\nx\n1,2", ProfileCause::RecordWidth, 2, 2),
                ("A,B\nx\n1,2,3", ProfileCause::RecordWidth, 2, 2),
            ] {
                assert_eq!(
                    parse(input.as_bytes()),
                    Err(Error::Profile {
                        cause,
                        record,
                        field: Field::Index(field),
                    })
                );
            }
        }

        fn cause(condition: &str) -> ProfileCause {
            match condition {
                "invalid_text" => ProfileCause::InvalidText,
                "source_byte_order_mark" => ProfileCause::ByteOrderMark { declared: false },
                "source_header_absent" => ProfileCause::HeaderAbsent,
                "source_field_name_empty" => ProfileCause::FieldNameEmpty,
                "source_field_name_duplicate" => ProfileCause::FieldNameDuplicate,
                "source_record_width" => ProfileCause::RecordWidth,
                "source_quote_unterminated" => ProfileCause::QuoteUnterminated,
                "source_carriage_return" => ProfileCause::CarriageReturn,
                "source_text_after_quote" => ProfileCause::TextAfterQuote,
                "source_quote_in_bare_field" => ProfileCause::QuoteInBareField,
                _ => panic!("unknown independent profile fixture"),
            }
        }
    }
    mod mapping_cut {
        use yamaa_core::{
            mapping_cut::{AdmissionError, Cut, Mapping, SelectedValue},
            table::ValueRef,
            value::Value,
        };

        #[test]
        fn dictionary_results_retain_literal_types_and_missing_is_not_unmapped() {
            let mapping = Mapping::new(
                "SRC.V".into(),
                "columns.V.derivation.mapping".into(),
                vec![
                    ("N".into(), Value::Missing),
                    ("i".into(), Value::Int(7)),
                    ("f".into(), Value::float(2.5)),
                    ("b".into(), Value::Bool(false)),
                    ("s".into(), Value::Str("".into())),
                ],
                true,
                Some(Value::Int(3)),
                Some(Value::Bool(true)),
            )
            .unwrap();
            for (input, expected) in [
                ("N", Value::Missing),
                ("i", Value::Int(7)),
                ("f", Value::float(2.5)),
                ("b", Value::Bool(false)),
                ("s", Value::Str("".into())),
            ] {
                let result = mapping.select(ValueRef::Str(input)).unwrap();
                assert_eq!(result.value, SelectedValue::Literal(&expected));
                assert_eq!(result.handler, None);
            }
            let missing = mapping.select(ValueRef::Missing).unwrap();
            assert_eq!(missing.value, SelectedValue::Literal(&Value::Int(3)));
            assert_eq!(missing.handler, Some("missing"));
            let unmapped = mapping.select(ValueRef::Str("missing")).unwrap();
            assert_eq!(unmapped.value, SelectedValue::Literal(&Value::Bool(true)));
            assert_eq!(unmapped.handler, Some("unmapped"));
        }
        #[test]
        fn folding_is_ascii_only_and_includes_empty_and_nul_keys() {
            let map = Mapping::new(
                "V".into(),
                "mapping".into(),
                vec![
                    ("\u{00e9}a\0Z".into(), Value::Int(1)),
                    ("\u{00c9}a\0Z".into(), Value::Int(2)),
                    ("".into(), Value::Int(3)),
                    ("ss".into(), Value::Int(4)),
                    ("\u{00df}".into(), Value::Int(5)),
                ],
                false,
                None,
                None,
            )
            .unwrap();
            for (input, expected) in [
                ("\u{00e9}A\0z", 1),
                ("\u{00c9}A\0z", 2),
                ("", 3),
                ("SS", 4),
                ("\u{00df}", 5),
            ] {
                assert_eq!(
                    map.select(ValueRef::Str(input)).unwrap().value,
                    SelectedValue::Literal(&Value::Int(expected))
                );
            }
            assert!(map.select(ValueRef::Str("\u{00e9}aZ")).is_err());
            let exact = Mapping::new(
                "V".into(),
                "mapping".into(),
                vec![("Y".into(), Value::Int(1)), ("y".into(), Value::Int(2))],
                true,
                None,
                None,
            )
            .unwrap();
            assert_eq!(
                exact.select(ValueRef::Str("y")).unwrap().value,
                SelectedValue::Literal(&Value::Int(2))
            );
        }
        #[test]
        fn interval_closure_and_unbounded_tails_follow_numeric_order() {
            for right in [false, true] {
                let cut = Cut::new(
                    "AGE".into(),
                    "cut".into(),
                    vec![18.0, 65.0],
                    vec!["child".into(), "adult".into(), "older".into()],
                    right,
                    None,
                )
                .unwrap();
                for (input, expected) in [
                    (17, "child"),
                    (18, if right { "child" } else { "adult" }),
                    (19, "adult"),
                    (64, "adult"),
                    (65, if right { "adult" } else { "older" }),
                    (66, "older"),
                ] {
                    assert_eq!(
                        cut.select(ValueRef::Int(input)).unwrap().value,
                        SelectedValue::Label(expected)
                    );
                }
            }
            let empty = Cut::new(
                "AGE".into(),
                "cut".into(),
                vec![],
                vec!["all".into()],
                false,
                Some(Value::Missing),
            )
            .unwrap();
            assert_eq!(
                empty.select(ValueRef::Int(i64::MIN)).unwrap().value,
                SelectedValue::Label("all")
            );
            assert_eq!(
                empty.select(ValueRef::Missing).unwrap().handler,
                Some("missing")
            );
            for boundary in [f64::NEG_INFINITY, f64::INFINITY] {
                assert!(matches!(
                    Cut::new(
                        "AGE".into(),
                        "cut".into(),
                        vec![boundary],
                        vec!["low".into(), "high".into()],
                        true,
                        None
                    ),
                    Err(AdmissionError::Diagnostic(_))
                ));
            }
        }
        #[test]
        fn interval_comparison_does_not_round_an_integer_into_a_binary64_boundary() {
            let cut = Cut::new(
                "N".into(),
                "cut".into(),
                vec![9_007_199_254_740_992.0],
                vec!["before".into(), "after".into()],
                true,
                None,
            )
            .unwrap();
            assert_eq!(
                cut.select(ValueRef::Int(9_007_199_254_740_992))
                    .unwrap()
                    .value,
                SelectedValue::Label("before")
            );
            assert_eq!(
                cut.select(ValueRef::Int(9_007_199_254_740_993))
                    .unwrap()
                    .value,
                SelectedValue::Label("after")
            );
        }
        #[test]
        fn invalid_boundaries_and_label_counts_are_planning_findings() {
            for (breaks, labels) in [
                (vec![1.0, 1.0], vec!["a", "b", "c"]),
                (vec![2.0, 1.0], vec!["a", "b", "c"]),
                (vec![f64::NAN], vec!["a", "b"]),
                (vec![1.0], vec!["a"]),
            ] {
                assert!(matches!(
                    Cut::new(
                        "N".into(),
                        "cut".into(),
                        breaks,
                        labels.into_iter().map(Into::into).collect(),
                        false,
                        None
                    ),
                    Err(AdmissionError::Diagnostic(_))
                ));
            }
        }

        #[test]
        fn empty_source_or_path_is_a_shape_error_for_both_declarations() {
            for (source, path) in [("", "mapping"), ("SRC.V", ""), ("", "")] {
                assert_eq!(
                    Mapping::new(source.into(), path.into(), vec![], true, None, None),
                    Err(AdmissionError::Shape)
                );
                assert_eq!(
                    Cut::new(
                        source.into(),
                        path.into(),
                        vec![18.0],
                        vec!["child".into(), "adult".into()],
                        false,
                        None
                    ),
                    Err(AdmissionError::Shape)
                );
            }
        }

        #[test]
        fn large_mapping_results_and_cut_labels_preserve_the_declared_text() {
            let size = 8 * 1024 * 1024;
            let mapping = Mapping::new(
                "SRC.V".into(),
                "mapping".into(),
                vec![("Y".into(), Value::Str("z".repeat(size)))],
                true,
                None,
                None,
            )
            .unwrap();
            let SelectedValue::Literal(Value::Str(selected)) =
                mapping.select(ValueRef::Str("Y")).unwrap().value
            else {
                panic!("declared string literal");
            };
            assert_eq!(selected.len(), size);
            assert!(selected.bytes().all(|byte| byte == b'z'));
            let cut = Cut::new(
                "AGE".into(),
                "cut".into(),
                vec![18.0],
                vec!["z".repeat(size), "adult".into()],
                false,
                None,
            )
            .unwrap();
            let SelectedValue::Label(selected) = cut.select(ValueRef::Int(17)).unwrap().value
            else {
                panic!("declared string label");
            };
            assert_eq!(selected.len(), size);
            assert!(selected.bytes().all(|byte| byte == b'z'));
        }
    }
    mod mapping_dictionary_resources {
        use yamaa_core::schema::{
            admit_schema_diagnostics, Document, DocumentNode as N, NormalizationError,
            SchemaModule, SchemaStructure,
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, V::map(vec![("type", V::text(kind))]))])
        }
        fn union(name: &'static str, kinds: &[&'static str]) -> V {
            V::map(vec![(
                name,
                V::map(vec![("type", V::List(kinds.iter().map(V::text).collect()))]),
            )])
        }
        fn schema() -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("yamaa", "str"),
                                field("type", "str"),
                                field("domain", "str"),
                                field("input", "dict"),
                                field("keys", "list[str]"),
                                field("output", "dict"),
                                field("columns", "list[column_class]"),
                                field("rows", "list[row_class]"),
                                field("metadata", "dict"),
                            ]),
                        ),
                        (
                            "column_class",
                            V::List(vec![
                                field("name", "str"),
                                field("type", "str"),
                                field("derivation", "handled_expression_class"),
                            ]),
                        ),
                        (
                            "row_class",
                            V::List(vec![
                                field("id", "str"),
                                field("derivations", "dict[str, handled_expression_class]"),
                            ]),
                        ),
                        (
                            "handled_expression_class",
                            V::List(vec![
                                field("value", "expression"),
                                field("unconvertible", "literal_value"),
                            ]),
                        ),
                        (
                            "expression",
                            V::map(vec![("registry", V::text("expressions"))]),
                        ),
                        ("project_path", V::map(vec![("type", V::text("str"))])),
                        (
                            "literal_value",
                            V::map(vec![(
                                "type",
                                V::List(vec![
                                    V::text("str"),
                                    V::text("int"),
                                    V::text("float"),
                                    V::text("bool"),
                                    V::text("null"),
                                ]),
                            )]),
                        ),
                        (
                            "branch_class",
                            V::List(vec![field("when", "str"), field("value", "expression")]),
                        ),
                        (
                            "expressions",
                            V::map(vec![
                                (
                                    "mapping",
                                    V::List(vec![
                                        field("source", "str"),
                                        union(
                                            "dict",
                                            &["dict[str, literal_value]", "project_path"],
                                        ),
                                    ]),
                                ),
                                (
                                    "case",
                                    V::List(vec![field("branches", "list[branch_class]")]),
                                ),
                                ("nested", V::List(vec![field("values", "list[expression]")])),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        fn mapping(dictionary: V) -> V {
            V::map(vec![(
                "mapping",
                V::map(vec![("source", V::text("SRC.X")), ("dict", dictionary)]),
            )])
        }
        fn handled(value: V) -> V {
            V::map(vec![("value", value)])
        }
        fn column(name: &'static str, value: V) -> V {
            V::map(vec![
                ("name", V::text(name)),
                ("type", V::text("str")),
                ("derivation", handled(value)),
            ])
        }
        fn input(columns: Vec<V>, rows: Vec<V>) -> V {
            V::map(vec![
                ("yamaa", V::text("1.0")),
                ("type", V::text("spec")),
                ("domain", V::text("DM")),
                (
                    "input",
                    V::map(vec![("SRC", V::map(vec![("path", V::text("src.csv"))]))]),
                ),
                ("keys", V::List(vec![V::text("ID")])),
                (
                    "output",
                    V::map(vec![
                        ("path", V::text("out.csv")),
                        ("columns", V::List(vec![V::text("X")])),
                    ]),
                ),
                ("columns", V::List(columns)),
                ("rows", V::List(rows)),
            ])
        }

        #[test]
        fn schema_requests_cover_nested_rows_in_schema_order_and_ignore_inline_and_metadata_values()
        {
            let case = V::map(vec![(
                "case",
                V::map(vec![(
                    "branches",
                    V::List(vec![V::map(vec![
                        ("when", V::text("FALSE")),
                        ("value", mapping(V::text("unused.yaml"))),
                    ])]),
                )]),
            )]);
            let nested = V::map(vec![(
                "nested",
                V::map(vec![(
                    "values",
                    V::List(vec![mapping(V::text("row.yaml"))]),
                )]),
            )]);
            let row = V::map(vec![
                ("id", V::text("ROW")),
                ("derivations", V::map(vec![("X", handled(nested))])),
            ]);
            let mut authored = input(
                vec![
                    column("INLINE", mapping(V::map(vec![("A", V::text("dict.yaml"))]))),
                    column("FIRST", mapping(V::text("../common/dict.yaml"))),
                    column("NESTED", case),
                ],
                vec![row],
            );
            let V::Map(ref mut fields) = authored else {
                panic!()
            };
            fields.push((
                "metadata".into(),
                V::map(vec![(
                    "mapping",
                    V::map(vec![("dict", V::text("never-read.yaml"))]),
                )]),
            ));
            let raw = document(authored);
            let original = raw.clone();
            let refs = schema().mapping_dictionary_references(&raw).unwrap();
            assert_eq!(
                refs.iter()
                    .map(|r| (r.path(), r.written()))
                    .collect::<Vec<_>>(),
                vec![
                    (
                        "columns.FIRST.derivation.mapping.dict",
                        "../common/dict.yaml"
                    ),
                    (
                        "columns.NESTED.derivation.case.branches[0].value.mapping.dict",
                        "unused.yaml"
                    ),
                    (
                        "rows.ROW.derivations.X.nested.values[0].mapping.dict",
                        "row.yaml"
                    ),
                ]
            );
            for r in refs {
                assert_eq!(raw.nodes()[r.node()], N::Text(r.written().into()));
            }
            assert_eq!(raw, original);
        }

        #[test]
        fn dictionary_admission_retains_use_site_and_original_input_arena_for_schema_findings() {
            let schema = schema();
            let spec = document(input(
                vec![column("X", mapping(V::text("dict.yaml")))],
                vec![],
            ));
            let reference = schema
                .mapping_dictionary_references(&spec)
                .unwrap()
                .remove(0);
            let invalid = Document::new(
                vec![
                    N::Integer("7".into()),
                    N::Text("answer".into()),
                    N::Mapping(vec![(0, 1)]),
                ],
                2,
            )
            .unwrap();
            let NormalizationError::Invalid(findings) = schema
                .admit_mapping_dictionary(reference.clone(), &invalid)
                .unwrap_err()
            else {
                panic!()
            };
            let report = admit_schema_diagnostics(&schema, Some(&invalid), &findings, &[]).unwrap();
            assert!(std::ptr::eq(report.document().unwrap(), &invalid));
            assert_eq!(
                report
                    .iter()
                    .map(|r| (r.phase, r.path, r.condition))
                    .collect::<Vec<_>>(),
                vec![(
                    "validation",
                    "columns.X.derivation.mapping.dict.key(7)",
                    "invalid_field_type"
                )]
            );
            for raw in [
                document(V::List(vec![V::text("not a dictionary")])),
                document(V::map(vec![("A", V::List(vec![V::text("not literal")]))])),
            ] {
                let NormalizationError::Invalid(findings) = schema
                    .admit_mapping_dictionary(reference.clone(), &raw)
                    .unwrap_err()
                else {
                    panic!()
                };
                let report = admit_schema_diagnostics(&schema, Some(&raw), &findings, &[]).unwrap();
                assert!(report
                    .iter()
                    .all(|r| r.path.starts_with("columns.X.derivation.mapping.dict")));
            }
        }

        #[test]
        fn compiler_materialization_copies_typed_held_content_and_keeps_original_model_immutable() {
            let schema = schema();
            let original = document(input(
                vec![
                    column("X", mapping(V::text("dict.yaml"))),
                    column("Y", mapping(V::text("dict.yaml"))),
                ],
                vec![],
            ));
            let unchanged = original.clone();
            let references = schema.mapping_dictionary_references(&original).unwrap();
            let dictionary = document(V::map(vec![
                ("A", V::text("mapped")),
                ("B", V::boolean(false)),
            ]));
            let admitted = references
                .iter()
                .cloned()
                .map(|reference| {
                    schema
                        .admit_mapping_dictionary(reference, &dictionary)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let (compiled, _) = schema
                .materialize_mapping_dictionary_document(&original, &admitted)
                .unwrap();
            assert_eq!(
                compiled,
                document(input(
                    vec![
                        column(
                            "X",
                            mapping(V::map(vec![
                                ("A", V::text("mapped")),
                                ("B", V::boolean(false))
                            ]))
                        ),
                        column(
                            "Y",
                            mapping(V::map(vec![
                                ("A", V::text("mapped")),
                                ("B", V::boolean(false))
                            ]))
                        )
                    ],
                    vec![]
                ))
            );
            assert_eq!(original, unchanged);
            assert!(schema
                .mapping_dictionary_references(&compiled)
                .unwrap()
                .is_empty());
            assert!(schema
                .materialize_mapping_dictionary_document(&original, &admitted[..1])
                .is_err());
            assert!(schema
                .materialize_mapping_dictionary_document(
                    &original,
                    &[admitted[0].clone(), admitted[0].clone()]
                )
                .is_err());
        }
    }
    mod odm_source {
        use yamaa_core::{
            odm_source::{self, SchemaError, StoredField},
            value::ColumnType,
        };

        #[test]
        fn fixed_coordinates_ignore_untyped_vendor_fields_and_fold_only_ascii() {
            let names = [
                "Vendor",
                "value",
                "ITEMoid",
                "itemgrouprepeatkey",
                "itemgroupoid",
                "formrepeatkey",
                "formoid",
                "studyeventrepeatkey",
                "studyeventoid",
                "subjectkey",
                "metadataversionoid",
                "studyoid",
                "İtemOID",
            ];
            let fields = names
                .iter()
                .enumerate()
                .map(|(index, &name)| StoredField {
                    name,
                    kind: if index == 0 || index == 12 {
                        None
                    } else {
                        Some(ColumnType::Str)
                    },
                })
                .collect::<Vec<_>>();
            let binding = odm_source::bind_fields(&fields).unwrap();
            assert_eq!(binding.stored(), &[11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
            assert_eq!(odm_source::canonical("İtemOID"), None);
            assert_eq!(odm_source::canonical("ItemOID.extra"), None);
        }

        #[test]
        fn metadata_findings_keep_missing_schema_order_and_stored_ambiguity_type_order() {
            let fields = [
                StoredField {
                    name: "VALUE",
                    kind: Some(ColumnType::Int),
                },
                StoredField {
                    name: "FormOID",
                    kind: None,
                },
                StoredField {
                    name: "subjectkey",
                    kind: Some(ColumnType::Str),
                },
                StoredField {
                    name: "SubjectKey",
                    kind: Some(ColumnType::Float),
                },
                StoredField {
                    name: "vendor",
                    kind: Some(ColumnType::Date),
                },
                StoredField {
                    name: "STUDYOID",
                    kind: Some(ColumnType::Str),
                },
                StoredField {
                    name: "StudyOID",
                    kind: Some(ColumnType::Str),
                },
            ];
            assert_eq!(
                odm_source::bind_fields(&fields),
                Err(vec![
                    SchemaError::Missing {
                        fields: vec![
                            "MetaDataVersionOID",
                            "StudyEventOID",
                            "StudyEventRepeatKey",
                            "FormRepeatKey",
                            "ItemGroupOID",
                            "ItemGroupRepeatKey",
                            "ItemOID",
                        ]
                    },
                    SchemaError::Ambiguous {
                        field: "SubjectKey",
                        stored: vec!["subjectkey".into(), "SubjectKey".into()]
                    },
                    SchemaError::Ambiguous {
                        field: "StudyOID",
                        stored: vec!["STUDYOID".into(), "StudyOID".into()]
                    },
                    SchemaError::Type {
                        field: "Value",
                        stored_field: "VALUE".into(),
                        kind: Some(ColumnType::Int)
                    },
                    SchemaError::Type {
                        field: "FormOID",
                        stored_field: "FormOID".into(),
                        kind: None
                    },
                ])
            );
        }

        #[test]
        fn vendor_metadata_does_not_change_the_eleven_exposed_coordinates() {
            let large = "v".repeat(1_048_577);
            let mut fields = odm_source::FIELDS
                .iter()
                .map(|name| StoredField {
                    name,
                    kind: Some(ColumnType::Str),
                })
                .collect::<Vec<_>>();
            fields.push(StoredField {
                name: &large,
                kind: None,
            });
            fields.extend(std::iter::repeat_n(
                StoredField {
                    name: "vendor",
                    kind: None,
                },
                4097,
            ));
            let binding = odm_source::bind_fields(&fields).unwrap();
            assert_eq!(binding.stored(), &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        }

        struct Records {
            schema: yamaa_core::table::TableSchema,
            values: Vec<Vec<yamaa_core::value::Value>>,
            reads: std::cell::RefCell<Vec<(usize, usize)>>,
            failure: Option<(usize, usize)>,
            marker: std::rc::Rc<String>,
        }
        impl Records {
            fn new() -> Self {
                use yamaa_core::{
                    table::{Column, TableSchema},
                    value::Value,
                };
                Self {
                    schema: TableSchema::new(
                        odm_source::FIELDS
                            .iter()
                            .map(|&name| Column {
                                name: name.into(),
                                kind: ColumnType::Str,
                            })
                            .collect(),
                    )
                    .unwrap(),
                    values: vec![vec![Value::Str("id".into()); 11]; 2],
                    reads: Default::default(),
                    failure: None,
                    marker: std::rc::Rc::new("opaque original".into()),
                }
            }
        }
        impl yamaa_core::table::TableAccess for Records {
            type Error = std::rc::Rc<String>;
            fn schema(&self) -> &yamaa_core::table::TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.values.len()
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<yamaa_core::table::ValueRef<'_>, yamaa_core::table::CellError<Self::Error>>
            {
                self.reads.borrow_mut().push((row, column));
                if self.failure == Some((row, column)) {
                    return Err(yamaa_core::table::CellError::Access(self.marker.clone()));
                }
                let value = self
                    .values
                    .get(row)
                    .and_then(|values| values.get(column))
                    .ok_or(yamaa_core::table::CellError::OutOfBounds { row, column })?;
                Ok(yamaa_core::table::ValueRef::from(value))
            }
        }

        #[test]
        fn required_identifiers_report_all_fields_in_fixed_order_and_storage_record_coordinates() {
            use yamaa_core::value::Value;
            let mut records = Records::new();
            records.values[1][0] = Value::Missing;
            for row in &mut records.values {
                row[2] = Value::Missing;
                row[9] = Value::Missing;
                row[4] = Value::Missing;
                row[10] = Value::Missing;
            }
            assert_eq!(
                odm_source::missing_values(&records, 2).unwrap(),
                [
                    odm_source::MissingValues {
                        field: "StudyOID",
                        record: 3,
                        records: 1
                    },
                    odm_source::MissingValues {
                        field: "SubjectKey",
                        record: 2,
                        records: 2
                    },
                    odm_source::MissingValues {
                        field: "ItemOID",
                        record: 2,
                        records: 2
                    },
                ]
            );
            assert_eq!(
                *records.reads.borrow(),
                [
                    (0, 0),
                    (1, 0),
                    (0, 1),
                    (1, 1),
                    (0, 2),
                    (1, 2),
                    (0, 3),
                    (1, 3),
                    (0, 5),
                    (1, 5),
                    (0, 7),
                    (1, 7),
                    (0, 9),
                    (1, 9)
                ]
            );
            assert_eq!(
                odm_source::missing_values(&records, 1).unwrap()[0].record,
                2
            );
        }

        #[test]
        fn a_later_original_ingestion_read_failure_suppresses_partial_findings_without_retry() {
            let mut records = Records::new();
            records.values[0][0] = yamaa_core::value::Value::Missing;
            records.failure = Some((1, 1));
            let odm_source::RecordError::Cell {
                row: 1,
                column: 1,
                error: yamaa_core::table::CellError::Access(marker),
            } = odm_source::missing_values(&records, 2).unwrap_err()
            else {
                panic!("original cell failure required")
            };
            assert!(std::rc::Rc::ptr_eq(&marker, &records.marker));
            assert_eq!(*records.reads.borrow(), [(0, 0), (1, 0), (0, 1), (1, 1)]);
        }
    }
    mod resource_scheme {
        use yamaa_core::resource::{has_uri_scheme, written_path_failure, ResourceFailure};

        #[test]
        fn uri_grammar_preserves_local_absolute_parent_and_windows_drive_paths() {
            for path in [
                "https://example.invalid/x.csv",
                "s3:bucket/x.csv",
                "file:///tmp/x.csv",
                "git+ssh:resource",
                "a.b-c:value",
            ] {
                assert!(has_uri_scheme(path), "{path}");
            }
            for path in [
                "input.csv",
                "../input.csv",
                "/tmp/input.csv",
                "C:/input.csv",
                "C:\\input.csv",
                "1bad:value",
                ":value",
                "folder/input:name.csv",
            ] {
                assert!(!has_uri_scheme(path), "{path}");
            }
        }

        #[test]
        fn written_form_checks_keep_normative_order_and_allow_root_dot_and_parent_paths() {
            for (path, expected) in [
                ("file:input\\x.csv", ResourceFailure::UriScheme),
                ("input\\x.csv/", ResourceFailure::Backslash),
                ("C:\\input.csv", ResourceFailure::Backslash),
                ("input//x.csv", ResourceFailure::EmptySegment),
                ("input/x.csv/", ResourceFailure::EmptySegment),
                ("", ResourceFailure::EmptySegment),
                ("/", ResourceFailure::EmptySegment),
                ("//input.csv", ResourceFailure::EmptySegment),
                ("C:/", ResourceFailure::EmptySegment),
                ("C://input.csv", ResourceFailure::EmptySegment),
            ] {
                assert_eq!(written_path_failure(path), Some(expected), "{path}");
            }
            for path in [
                "input.csv",
                "./input.csv",
                "input/../input.csv",
                "../input.csv",
                "/tmp/../input.csv",
                "C:/input.csv",
                "C:/../input.csv",
                "folder/input:name.csv",
            ] {
                assert_eq!(written_path_failure(path), None, "{path}");
            }
        }
    }
    mod source_diagnostics {
        //! Independent complete outcomes over actual profile/type/physical-schema failures.
        use yamaa_core::json::Value as V;
        use yamaa_core::{
            csv_source::{self, Field as CsvField},
            diagnostic::{
                source_diagnostics, ConditionCode as C, Diagnostic, ParquetFinding, SourceFinding,
            },
            parquet_source::{
                self as parquet, Converted, Field, Logical, Physical, Representation,
            },
            typed_csv::PreparedTypes,
            value::ColumnType,
        };

        fn text(value: &str) -> V {
            V::String(value.into())
        }
        fn expected(code: C, path: &str, fields: Vec<(&str, V)>) -> Diagnostic {
            Diagnostic {
                code,
                spec_paths: vec![path.into()],
                context: fields
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect(),
                source_span: None,
                operand_route: None,
            }
        }

        #[test]
        fn every_csv_cause_retains_the_complete_independent_context() {
            let cases = [
                (
                    b"".as_slice(),
                    C::CsvHeaderAbsent,
                    1,
                    V::Number(("1").parse().expect("canonical integer")),
                ),
                (
                    b"\xef\xbb\xbfID\n".as_slice(),
                    C::CsvByteOrderMark,
                    1,
                    V::Number(("1").parse().expect("canonical integer")),
                ),
                (
                    b"ID\nx\r".as_slice(),
                    C::CsvCarriageReturn,
                    2,
                    V::Number(("1").parse().expect("canonical integer")),
                ),
                (
                    b"ID\nx\"y".as_slice(),
                    C::CsvQuoteInBareField,
                    2,
                    V::Number(("1").parse().expect("canonical integer")),
                ),
                (
                    b"ID\n\"x\" ".as_slice(),
                    C::CsvTextAfterQuote,
                    2,
                    V::Number(("1").parse().expect("canonical integer")),
                ),
                (
                    b"ID\n\"x".as_slice(),
                    C::CsvQuoteUnterminated,
                    2,
                    V::Number(("1").parse().expect("canonical integer")),
                ),
                (
                    b"ID,V\nx".as_slice(),
                    C::CsvRecordWidth,
                    2,
                    V::Number(("2").parse().expect("canonical integer")),
                ),
                (
                    b"ID,\nx,y".as_slice(),
                    C::CsvFieldNameEmpty,
                    1,
                    V::Number(("2").parse().expect("canonical integer")),
                ),
                (
                    b"ID,ID\nx,y".as_slice(),
                    C::CsvFieldNameDuplicate,
                    1,
                    text("ID"),
                ),
                (
                    b"\xef\xbb\xbfID\n\"x\ny\",\xff".as_slice(),
                    C::CsvInvalidText,
                    2,
                    V::Number(("2").parse().expect("canonical integer")),
                ),
            ];
            let errors: Vec<_> = cases
                .iter()
                .map(|(bytes, ..)| csv_source::parse(bytes).unwrap_err())
                .collect();
            let borrowed: Vec<_> = errors
                .iter()
                .map(|error| error.finding("SRC", "source.csv").unwrap())
                .collect();
            let actual: Vec<_> = source_diagnostics(&borrowed).collect();
            let wanted: Vec<_> = cases
                .into_iter()
                .map(|(_, code, record, field)| {
                    let mut context = vec![
                        ("dataset", text("SRC")),
                        ("path", text("source.csv")),
                        ("record", V::from(record)),
                        ("field", field),
                    ];
                    // A mark records whether its input declares one (CSV-10).
                    if code == C::CsvByteOrderMark {
                        context.push(("declared", V::Bool(false)));
                    }
                    expected(code, "input.SRC.path", context)
                })
                .collect();
            assert_eq!(actual, wanted);
            for (index, finding) in actual.iter().enumerate() {
                assert_eq!(finding.definition().phase, "ingest");
                assert_eq!(
                    finding.definition().rule,
                    Some(if index == 9 { "CSV-10" } else { "CSV-16" })
                );
            }
        }

        /// A declared mark that is absent, and a second mark after the
        /// declared one, keep the undeclared mark's condition, rule and path;
        /// the context records the declaration (CSV-10).
        #[test]
        fn a_declared_mark_that_fails_records_its_declaration() {
            for bytes in [b"ID\n".as_slice(), b"\xef\xbb\xbf\xef\xbb\xbfID\n"] {
                let error = csv_source::parse_declared(bytes, true).unwrap_err();
                assert_eq!(error.rule(), Some("CSV-16"));
                let finding = error.finding("SRC", "source.csv").unwrap();
                assert_eq!(
                    finding.diagnostic(),
                    expected(
                        C::CsvByteOrderMark,
                        "input.SRC.path",
                        vec![
                            ("dataset", text("SRC")),
                            ("path", text("source.csv")),
                            ("record", V::from(1usize)),
                            ("field", V::from(1usize)),
                            ("declared", V::Bool(true)),
                        ],
                    )
                );
                assert_eq!(
                    finding.diagnostic().message(),
                    "source.csv must start with exactly one byte order mark when its input \
                     declares one, and none otherwise"
                );
            }
        }

        #[test]
        fn source_types_keep_declaration_then_stored_row_field_precedence_and_all_value_text() {
            let declarations = vec![
                ("X".into(), ColumnType::Int),
                ("Z".into(), ColumnType::Float),
                ("A".into(), ColumnType::Int),
            ];
            let absent = PreparedTypes::new(&declarations)
                .unwrap()
                .convert(csv_source::parse(b"X\nnot-an-int").unwrap())
                .unwrap_err();
            let declarations = vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)];
            let first = PreparedTypes::new(&declarations)
                .unwrap()
                .convert(csv_source::parse(b"A,B\n1,bad\nworse,2").unwrap())
                .unwrap_err();
            let findings = [
                absent.finding("SRC").unwrap(),
                first.finding("SRC").unwrap(),
            ];
            let actual: Vec<_> = source_diagnostics(&findings).collect();
            assert_eq!(
                actual,
                vec![
                    expected(
                        C::SourceTypeUnknownField,
                        "input.SRC.types.Z",
                        vec![("dataset", text("SRC")), ("field", text("Z"))]
                    ),
                    expected(
                        C::SourceTypeFieldParse,
                        "input.SRC.types.B",
                        vec![
                            ("dataset", text("SRC")),
                            ("field", text("B")),
                            ("type", text("int")),
                            ("value", text("bad"))
                        ]
                    ),
                ]
            );
            assert_eq!(
                (actual[0].definition().phase, actual[0].definition().rule),
                ("validation", Some("INPUT-6"))
            );
            assert_eq!(
                (actual[1].definition().phase, actual[1].definition().rule),
                ("ingest", Some("INPUT-7"))
            );
            let long = format!("{}β\0\"\\", "9".repeat(6000));
            let error = yamaa_core::typed_csv::Error::FieldParse {
                field: "I".into(),
                target: ColumnType::Int,
                value: long.clone(),
            };
            let actual = error.finding("SRC").unwrap().diagnostic();
            assert_eq!(actual.context["value"], text(&long));
        }

        fn field(name: &str) -> Field {
            Field {
                name: name.into(),
                leaves: 1,
                path: name.into(),
                repetition: 0,
                physical: Physical::Int64,
                logical: Logical::None,
                converted: Converted::None,
                representation: Representation::Int64,
                stored_type: "int64".into(),
            }
        }
        #[test]
        fn signed_integer_fields_that_fit_their_physical_type_read_as_int() {
            let signed = |bits| Logical::Integer { bits, signed: true };
            let legacy = |bits| Converted::Integer { bits, signed: true };
            // An unannotated INT32 or INT64 is a signed integer of its width.
            for (physical, representation) in [
                (Physical::Int32, Representation::Int32),
                (Physical::Int64, Representation::Int64),
            ] {
                assert_eq!(
                    parquet::column_type(physical, Logical::None, Converted::None, representation),
                    Some(ColumnType::Int),
                    "{physical:?}"
                );
            }
            for (physical, bits, representation) in [
                (Physical::Int64, 64, Representation::Int64),
                (Physical::Int32, 32, Representation::Int32),
                (Physical::Int32, 16, Representation::Int16),
                (Physical::Int32, 8, Representation::Int8),
            ] {
                // The logical type alone, both annotations, and the converted
                // type alone, as DuckDB writes it.
                for (logical, converted) in [
                    (signed(bits), Converted::None),
                    (signed(bits), legacy(bits)),
                    (Logical::None, legacy(bits)),
                ] {
                    assert_eq!(
                        parquet::column_type(physical, logical, converted, representation),
                        Some(ColumnType::Int),
                        "{bits}: {logical:?} {converted:?}"
                    );
                }
                let unsigned = Logical::Integer {
                    bits,
                    signed: false,
                };
                let legacy_unsigned = Converted::Integer {
                    bits,
                    signed: false,
                };
                for (logical, converted) in [
                    (unsigned, Converted::None),
                    (unsigned, legacy_unsigned),
                    (Logical::None, legacy_unsigned),
                ] {
                    assert_eq!(
                        parquet::column_type(physical, logical, converted, representation),
                        None,
                        "{bits}: {logical:?} {converted:?}"
                    );
                }
            }
            // A width the physical type does not hold, and a layout of another
            // width or type, stay refused.
            let microseconds = Representation::Timestamp {
                unit: parquet::Unit::Microsecond,
                timezone: false,
            };
            for (physical, logical, converted, representation) in [
                (
                    Physical::Int32,
                    signed(64),
                    legacy(64),
                    Representation::Int64,
                ),
                (
                    Physical::Int64,
                    signed(32),
                    legacy(32),
                    Representation::Int32,
                ),
                (Physical::Int32, signed(8), legacy(8), Representation::Int16),
                (Physical::Int64, signed(64), legacy(64), microseconds),
                (
                    Physical::Int32,
                    Logical::None,
                    legacy(16),
                    Representation::Int8,
                ),
                (
                    Physical::Int32,
                    Logical::None,
                    Converted::None,
                    Representation::Int16,
                ),
            ] {
                assert_eq!(
                    parquet::column_type(physical, logical, converted, representation),
                    None,
                    "{physical:?} {logical:?} {converted:?} {representation:?}"
                );
            }
        }
        #[test]
        fn physical_schema_and_raw_temporal_findings_keep_all_original_facts() {
            let mut nested = field("S");
            nested.leaves = 2;
            nested.stored_type = "struct<left: int64, right: string>".into();
            let errors = [
                parquet::columns(vec![], 0).unwrap_err(),
                parquet::columns(vec![field(""), field("I")], 2).unwrap_err(),
                parquet::columns(vec![field("I"), field("I")], 2).unwrap_err(),
                parquet::columns(vec![field("I"), nested], 2).unwrap_err(),
                parquet::temporal(ColumnType::DateTime, i64::MAX, "DT", 2).unwrap_err(),
            ];
            let borrowed: Vec<_> = errors
                .iter()
                .map(|error| error.finding("SRC", "data.parquet"))
                .collect();
            let wanted = [
                (C::ParquetInvalid, "PARQUET-5", vec![]),
                (
                    C::ParquetFieldNameEmpty,
                    "PARQUET-6",
                    vec![(
                        "field",
                        V::Number(("1").parse().expect("canonical integer")),
                    )],
                ),
                (
                    C::ParquetFieldNameDuplicate,
                    "PARQUET-6",
                    vec![("field", text("I"))],
                ),
                (
                    C::ParquetFieldTypeUnsupported,
                    "PARQUET-7",
                    vec![
                        ("field", text("S")),
                        ("stored_type", text("struct<left: int64, right: string>")),
                    ],
                ),
                (
                    C::ParquetFieldValueInvalid,
                    "PARQUET-9",
                    vec![
                        ("field", text("DT")),
                        (
                            "record",
                            V::Number(("2").parse().expect("canonical integer")),
                        ),
                        ("value", V::from(i64::MAX)),
                    ],
                ),
            ];
            for (actual, (code, rule, extra)) in source_diagnostics(&borrowed).zip(wanted) {
                let fields = vec![("dataset", text("SRC")), ("path", text("data.parquet"))]
                    .into_iter()
                    .chain(extra)
                    .collect();
                assert_eq!(actual, expected(code, "input.SRC.path", fields));
                assert_eq!(
                    (actual.definition().phase, actual.definition().rule),
                    ("ingest", Some(rule))
                );
            }
            let mut inspected = 0;
            let fields = ["", "LATER"].into_iter().map(|name| {
                inspected += 1;
                field(name)
            });
            assert_eq!(
                parquet::columns(fields, 2).unwrap_err(),
                parquet::Error::EmptyName { field: 1 }
            );
            assert_eq!(inspected, 1);
        }

        #[test]
        fn whole_ordered_report_preserves_borrowed_facts_and_full_late_values() {
            let error = csv_source::parse(b"ID,ID\nx,y").unwrap_err();
            let long = "\0\"\\β".repeat(2000);
            let findings = [
                error.finding("FIRST", "first.csv").unwrap(),
                SourceFinding::FieldParse {
                    dataset: "LAST",
                    field: "F",
                    target: ColumnType::Int,
                    value: &long,
                },
            ];
            let [SourceFinding::Csv {
                field: borrowed_field,
                ..
            }, SourceFinding::FieldParse { value, .. }] = &findings
            else {
                panic!()
            };
            let csv_source::Error::Profile {
                field: original_field,
                ..
            } = &error
            else {
                panic!()
            };
            assert!(std::ptr::eq(*borrowed_field, original_field));
            assert!(std::ptr::eq(value.as_ptr(), long.as_ptr()));
            let projected = source_diagnostics(&findings);
            assert_eq!(projected.len(), 2);
            let actual = projected.collect::<Vec<_>>();
            assert_eq!(actual[0].context["field"], text("ID"));
            assert_eq!(
                actual[1],
                expected(
                    C::SourceTypeFieldParse,
                    "input.LAST.types.F",
                    vec![
                        ("dataset", text("LAST")),
                        ("field", text("F")),
                        ("type", text("int")),
                        ("value", text(&long)),
                    ]
                )
            );
            assert_eq!(source_diagnostics(&findings).collect::<Vec<_>>(), actual);
            assert_eq!(source_diagnostics(&[]).len(), 0);
        }

        #[test]
        fn diagnostic_positions_stay_mathematical_and_boundary_errors_have_no_condition() {
            let field = CsvField::Index(usize::MAX);
            let finding = SourceFinding::Csv {
                cause: csv_source::ProfileCause::RecordWidth,
                record: usize::MAX,
                field: &field,
                dataset: "SRC",
                path: "source.csv",
            };
            let actual = finding.diagnostic();
            assert_eq!(actual.context["record"], V::from(usize::MAX));
            assert_eq!(actual.context["field"], V::from(usize::MAX));
            let actual = SourceFinding::Parquet {
                finding: ParquetFinding::Value {
                    field: "DT",
                    row: usize::MAX,
                    value: i64::MIN,
                },
                dataset: "SRC",
                path: "data.parquet",
            }
            .diagnostic();
            assert_eq!(actual.context["record"], V::from(usize::MAX));
            assert_eq!(actual.context["value"], V::from(i64::MIN));
            assert!(csv_source::Error::Limit {
                resource: "source_bytes",
                limit: 0,
                actual: Some(1)
            }
            .finding("SRC", "s.csv")
            .is_none());
            for error in [
                yamaa_core::typed_csv::Error::InvalidDeclarations,
                yamaa_core::typed_csv::Error::InvalidSource,
            ] {
                assert!(error.finding("SRC").is_none());
            }
        }
    }
    mod table {
        use yamaa_core::{
            table::{Column, SchemaError, TableSchema, ValueRef},
            temporal::{Date, DateTime, Precision},
            value::{ColumnType, Value},
        };

        #[test]
        /// Preserve empty-table schema identity and reject ambiguous column names.
        fn schema_retains_order_and_all_closed_column_types() {
            let kinds = [
                ColumnType::DateTime,
                ColumnType::Str,
                ColumnType::Int,
                ColumnType::Date,
                ColumnType::Float,
            ];
            let columns: Vec<_> = kinds
                .iter()
                .enumerate()
                .map(|(i, &kind)| Column {
                    name: format!("C{i}"),
                    kind,
                })
                .collect();
            let schema = TableSchema::new(columns.clone()).unwrap();
            assert_eq!(schema.columns(), columns);
            assert!(TableSchema::new(vec![]).unwrap().columns().is_empty());
            assert_eq!(
                TableSchema::new(vec![Column {
                    name: "".into(),
                    kind: ColumnType::Int
                }]),
                Err(SchemaError::EmptyName { column: 0 })
            );
            assert_eq!(
                TableSchema::new(vec![columns[0].clone(), columns[0].clone()]),
                Err(SchemaError::DuplicateName { column: 1 })
            );
        }

        #[test]
        /// Prove borrowed text ownership and exact scalar/temporal metadata.
        fn borrowing_retains_text_backing_exact_values_and_temporal_precision() {
            let date = Date::new(2025, 1, 1, Precision::Year).unwrap();
            let datetime = DateTime::new(date, 0, 0, 0, Precision::Day).unwrap();
            let values = [
                Value::Missing,
                Value::Str("text\0\u{1f980}".into()),
                Value::Int(i64::MIN),
                Value::Int(i64::MAX),
                Value::float(-0.0),
                Value::Bool(true),
                Value::Date(date),
                Value::DateTime(datetime),
            ];
            let views: Vec<_> = values.iter().map(ValueRef::from).collect();
            for (owned, view) in values.iter().zip(&views) {
                assert_eq!(owned.value_type(), view.value_type());
            }
            let (Value::Str(owned), ValueRef::Str(view)) = (&values[1], views[1]) else {
                panic!("text required")
            };
            assert_eq!(owned.as_ptr(), view.as_ptr());
            assert_eq!(owned.len(), view.len());
            assert_eq!(views[2], ValueRef::Int(i64::MIN));
            assert_eq!(views[3], ValueRef::Int(i64::MAX));
            let ValueRef::Float(zero) = views[4] else {
                panic!("float required")
            };
            assert_eq!(zero.get().to_bits(), 1 << 63);
            let ValueRef::Date(date) = views[6] else {
                panic!("date required")
            };
            assert_eq!(date.collected_precision(), Precision::Year);
            let ValueRef::DateTime(datetime) = views[7] else {
                panic!("datetime required")
            };
            assert_eq!(datetime.collected_precision(), Precision::Day);
        }
    }
    mod typed_csv {
        use yamaa_core::{
            csv_source::{self, CsvSource},
            table::{CellError, TableAccess, ValueRef},
            typed_csv::{Error, PreparedTypes},
            value::ColumnType,
        };

        #[test]
        fn malformed_external_carriers_have_no_language_projection_or_index_panic() {
            for source in [
                CsvSource {
                    names: vec![],
                    records: vec![],
                },
                CsvSource {
                    names: vec!["".into()],
                    records: vec![vec![None]],
                },
                CsvSource {
                    names: vec!["A".into(), "A".into()],
                    records: vec![vec![None, None]],
                },
                CsvSource {
                    names: vec!["A".into()],
                    records: vec![vec![]],
                },
                CsvSource {
                    names: vec!["A".into()],
                    records: vec![vec![None, None]],
                },
            ] {
                let error = PreparedTypes::new(&[])
                    .unwrap()
                    .convert(source)
                    .unwrap_err();
                assert!(matches!(error, Error::InvalidSource));
                assert!(error.finding("SRC").is_none());
            }
            for declarations in [
                vec![("".into(), ColumnType::Int)],
                vec![
                    ("A".into(), ColumnType::Int),
                    ("A".into(), ColumnType::Date),
                ],
            ] {
                let error = PreparedTypes::new(&declarations).err().unwrap();
                assert!(matches!(error, Error::InvalidDeclarations));
                assert!(error.finding("SRC").is_none());
            }
        }

        #[test]
        fn fixed_declaration_width_admits_the_edge_and_keeps_the_observed_refusal() {
            let maximum = yamaa_core::limits::Limits::COLUMNS;
            for columns in [maximum - 1, maximum, maximum + 1] {
                let declarations = (0..columns)
                    .map(|index| (format!("C{index}"), ColumnType::Int))
                    .collect::<Vec<_>>();
                match PreparedTypes::new(&declarations) {
                    Ok(_) => assert!(columns <= maximum),
                    Err(error) => {
                        assert_eq!(columns, maximum + 1);
                        assert!(
                            matches!(error, Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "columns", limit, actual: Some(actual),
                }) if limit == maximum && actual == columns)
                        );
                        assert!(error.finding("SRC").is_none());
                    }
                }
            }
        }

        #[test]
        fn converted_core_values_outlive_all_input_authority_and_reject_invalid_cell_indexes() {
            let rows = {
                let declarations = vec![("I".into(), ColumnType::Int)];
                let bytes = b"I,T\n9007199254740993,\"retained\"\n,NA\n".to_vec();
                let source = csv_source::parse(&bytes).unwrap();
                PreparedTypes::new(&declarations)
                    .unwrap()
                    .convert(source)
                    .unwrap()
            };
            assert_eq!(rows.row_count(), 2);
            assert_eq!(rows.cell(0, 0).unwrap(), ValueRef::Int(9007199254740993));
            assert_eq!(rows.cell(0, 1).unwrap(), ValueRef::Str("retained"));
            assert_eq!(rows.cell(1, 0).unwrap(), ValueRef::Missing);
            assert_eq!(rows.cell(1, 1).unwrap(), ValueRef::Str("NA"));
            assert!(matches!(
                rows.cell(2, 0),
                Err(CellError::OutOfBounds { row: 2, column: 0 })
            ));
            assert!(matches!(
                rows.cell(0, 2),
                Err(CellError::OutOfBounds { row: 0, column: 2 })
            ));
        }
    }
}
