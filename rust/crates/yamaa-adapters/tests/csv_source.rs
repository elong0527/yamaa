use yamaa_adapters::csv_source::{parse, CsvSource, Error, Field, Limits};

#[test]
fn collected_text_and_missing_remain_lossless() {
    let actual = parse(
        b"ID,N,T,Q\r\nS,9007199254740993,NA,\"\"\r\nT,,x\0y,\"line\n\"\"quoted\"\"\"",
        Limits::default(),
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
        parse("λ\n € ".as_bytes(), Limits::default())
            .unwrap()
            .records,
        vec![vec![Some(" € ".into())]]
    );
    assert!(parse(b"A,B\n", Limits::default())
        .unwrap()
        .records
        .is_empty());
    assert_eq!(
        parse(b"A,B\nx,", Limits::default()).unwrap().records,
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
            parse(input, Limits::default()),
            Err(Error::Profile {
                condition,
                record,
                field: Field::Index(field)
            })
        );
    }
    assert_eq!(
        parse(b"A,A\nx,y", Limits::default()),
        Err(Error::Profile {
            condition: "source_field_name_duplicate",
            record: 1,
            field: Field::Name("A".into())
        })
    );
}

#[test]
fn policy_limits_do_not_publish_partial_rows() {
    for (limits, resource) in [
        (
            Limits {
                bytes: 2,
                ..Limits::default()
            },
            "bytes",
        ),
        (
            Limits {
                fields: 2,
                ..Limits::default()
            },
            "fields",
        ),
        (
            Limits {
                records: 1,
                ..Limits::default()
            },
            "records",
        ),
    ] {
        assert!(
            matches!(parse(b"A,B\nx,y",limits),Err(Error::Limit {resource: actual,..}) if actual==resource)
        );
    }
}

#[test]
fn untyped_arrow_snapshot_owns_exact_source_cells_and_limits() {
    use yamaa_adapters::{
        arrow_table::{TableError, TableLimits},
        csv_source::{parse_text_table, TextTableError},
    };
    use yamaa_core::table::{TableAccess, ValueRef};
    let limits = TableLimits {
        max_rows: 10,
        max_columns: 4,
        max_batches: 1,
        max_cells: 40,
    };
    let mut input = b"A,B\n9007199254740993,\"\"\nx\0y,NA".to_vec();
    let table = parse_text_table(&input, Limits::default(), limits).unwrap();
    input.fill(0);
    drop(input);
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("9007199254740993"));
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Missing);
    assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("x\0y"));
    assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("NA"));
    assert!(table.cell(2, 0).is_err());
    assert_eq!(
        parse_text_table(b"A,B", Limits::default(), limits)
            .unwrap()
            .row_count(),
        0
    );
    assert!(matches!(
        parse_text_table(
            b"A\nx",
            Limits::default(),
            TableLimits {
                max_cells: 0,
                ..limits
            }
        ),
        Err(TextTableError::Table(TableError::Limit {
            resource: "cells",
            ..
        }))
    ));
}
