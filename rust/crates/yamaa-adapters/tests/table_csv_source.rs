//! Table contracts: the CSV source profile's parser, and the findings its
//! refusals report.
//!
//! The seam is `csv_source::parse`'s typed result, the parsed records and
//! each `Error::Profile` cause with its record and field, which neither the
//! benchmark reader nor the host fixtures reach: they see only diagnostics.
mod table {
    mod csv_source {
        use yamaa_adapters::csv_source::{parse, CsvSource, Error, Field, ProfileCause};

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
            use yamaa_adapters::csv_source::parse_declared;
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
    mod csv_source_diagnostics {
        //! Independent complete outcomes over actual profile failures.
        use yamaa_adapters::csv_source;
        use yamaa_core::diagnostic::{source_diagnostics, ConditionCode as C, Diagnostic};
        use yamaa_core::json::Value as V;

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
    }
}
