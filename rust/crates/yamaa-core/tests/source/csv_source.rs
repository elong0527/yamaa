use yamaa_core::csv_source::{parse, CsvSource, Error, Field, ProfileCause};

#[test]
fn collected_text_and_missing_remain_lossless() {
    let actual =
        parse(b"ID,N,T,Q\r\nS,9007199254740993,NA,\"\"\r\nT,,x\0y,\"line\n\"\"quoted\"\"\"")
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

fn cause(condition: &str) -> ProfileCause {
    match condition {
        "invalid_text" => ProfileCause::InvalidText,
        "source_byte_order_mark" => ProfileCause::ByteOrderMark,
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
