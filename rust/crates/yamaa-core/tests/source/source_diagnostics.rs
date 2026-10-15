//! Independent complete outcomes over actual profile/type/physical-schema failures.
use serde_json::Value as V;
use yamaa_core::{
    csv_source::{self, Field as CsvField},
    diagnostic::{
        source_diagnostics, ConditionCode as C, Diagnostic, ParquetFinding, SourceFinding,
    },
    parquet_source::{self as parquet, Converted, Field, Logical, Physical, Representation},
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
            expected(
                code,
                "input.SRC.path",
                vec![
                    ("dataset", text("SRC")),
                    ("path", text("source.csv")),
                    ("record", V::from(record)),
                    ("field", field),
                ],
            )
        })
        .collect();
    assert_eq!(actual, wanted);
    for (index, finding) in actual.iter().enumerate() {
        assert_eq!(finding.definition().phase, "ingest");
        assert_eq!(
            finding.definition().requirement,
            Some(if index == 9 { "REQ-0853" } else { "REQ-0851" })
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
        (
            actual[0].definition().phase,
            actual[0].definition().requirement
        ),
        ("validation", Some("REQ-0532"))
    );
    assert_eq!(
        (
            actual[1].definition().phase,
            actual[1].definition().requirement
        ),
        ("ingest", Some("REQ-0536"))
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
        (C::ParquetInvalid, "REQ-1038", vec![]),
        (
            C::ParquetFieldNameEmpty,
            "REQ-1039",
            vec![(
                "field",
                V::Number(("1").parse().expect("canonical integer")),
            )],
        ),
        (
            C::ParquetFieldNameDuplicate,
            "REQ-1039",
            vec![("field", text("I"))],
        ),
        (
            C::ParquetFieldTypeUnsupported,
            "REQ-1040",
            vec![
                ("field", text("S")),
                ("stored_type", text("struct<left: int64, right: string>")),
            ],
        ),
        (
            C::ParquetFieldValueInvalid,
            "REQ-1041",
            vec![
                ("field", text("DT")),
                ("row", V::Number(("2").parse().expect("canonical integer"))),
                ("value", V::from(i64::MAX)),
            ],
        ),
    ];
    for (actual, (code, requirement, extra)) in source_diagnostics(&borrowed).zip(wanted) {
        let fields = vec![("dataset", text("SRC")), ("path", text("data.parquet"))]
            .into_iter()
            .chain(extra)
            .collect();
        assert_eq!(actual, expected(code, "input.SRC.path", fields));
        assert_eq!(
            (actual.definition().phase, actual.definition().requirement),
            ("ingest", Some(requirement))
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
    assert_eq!(actual.context["row"], V::from(usize::MAX));
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
