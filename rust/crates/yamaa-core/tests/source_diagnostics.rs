//! Independent complete outcomes over actual profile/type/physical-schema failures.
use yamaa_core::{
    csv_source::{self, Field as CsvField},
    diagnostic::{
        admit_source_diagnostics, ConditionCode as C, ContextValue as V, Diagnostic,
        ParquetFinding, ProjectionError, SourceFinding, MAX_PROJECTION_BYTES,
    },
    parquet_source::{self as parquet, Converted, Field, Logical, Physical, Representation},
    typed_csv::PreparedTypes,
    value::{ColumnType, Value},
};

fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
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
            V::Integer("1".into()),
        ),
        (
            b"\xef\xbb\xbfID\n".as_slice(),
            C::CsvByteOrderMark,
            1,
            V::Integer("1".into()),
        ),
        (
            b"ID\nx\r".as_slice(),
            C::CsvCarriageReturn,
            2,
            V::Integer("1".into()),
        ),
        (
            b"ID\nx\"y".as_slice(),
            C::CsvQuoteInBareField,
            2,
            V::Integer("1".into()),
        ),
        (
            b"ID\n\"x\" ".as_slice(),
            C::CsvTextAfterQuote,
            2,
            V::Integer("1".into()),
        ),
        (
            b"ID\n\"x".as_slice(),
            C::CsvQuoteUnterminated,
            2,
            V::Integer("1".into()),
        ),
        (
            b"ID,V\nx".as_slice(),
            C::CsvRecordWidth,
            2,
            V::Integer("2".into()),
        ),
        (
            b"ID,\nx,y".as_slice(),
            C::CsvFieldNameEmpty,
            1,
            V::Integer("2".into()),
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
            V::Integer("2".into()),
        ),
    ];
    let errors: Vec<_> = cases
        .iter()
        .map(|(bytes, ..)| csv_source::parse(bytes, Default::default()).unwrap_err())
        .collect();
    let borrowed: Vec<_> = errors
        .iter()
        .map(|error| error.finding("SRC", "source.csv").unwrap())
        .collect();
    let actual: Vec<_> = admit_source_diagnostics(&borrowed, MAX_PROJECTION_BYTES)
        .unwrap()
        .iter()
        .collect();
    let wanted: Vec<_> = cases
        .into_iter()
        .map(|(_, code, record, field)| {
            expected(
                code,
                "input.SRC.path",
                vec![
                    ("dataset", text("SRC")),
                    ("path", text("source.csv")),
                    ("record", V::Integer(record.to_string())),
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
    let absent = PreparedTypes::new(&declarations, 16)
        .unwrap()
        .convert(csv_source::parse(b"X\nnot-an-int", Default::default()).unwrap())
        .unwrap_err();
    let declarations = vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)];
    let first = PreparedTypes::new(&declarations, 16)
        .unwrap()
        .convert(csv_source::parse(b"A,B\n1,bad\nworse,2", Default::default()).unwrap())
        .unwrap_err();
    let findings = [
        absent.finding("SRC").unwrap(),
        first.finding("SRC").unwrap(),
    ];
    let actual: Vec<_> = admit_source_diagnostics(&findings, MAX_PROJECTION_BYTES)
        .unwrap()
        .iter()
        .collect();
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
    let actual = error.finding("SRC").unwrap().diagnostic().unwrap();
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
            vec![("field", V::Integer("1".into()))],
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
                ("row", V::Integer("2".into())),
                ("value", V::Scalar(Value::Int(i64::MAX))),
            ],
        ),
    ];
    for (actual, (code, requirement, extra)) in
        admit_source_diagnostics(&borrowed, MAX_PROJECTION_BYTES)
            .unwrap()
            .iter()
            .zip(wanted)
    {
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

fn minimum(findings: &[SourceFinding<'_>]) -> usize {
    let mut lower = 0;
    let mut upper = MAX_PROJECTION_BYTES;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if admit_source_diagnostics(findings, middle).is_ok() {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    lower
}
#[test]
fn whole_ordered_report_is_admitted_before_any_owned_materialization() {
    let error = csv_source::parse(b"ID,ID\nx,y", Default::default()).unwrap_err();
    let long = "\0\"\\β".repeat(2000);
    let late = SourceFinding::FieldParse {
        dataset: "LAST",
        field: "F",
        target: ColumnType::Int,
        value: &long,
    };
    let findings = [error.finding("FIRST", "first.csv").unwrap(), late];
    let limit = minimum(&findings);
    assert!(admit_source_diagnostics(&findings[..1], limit - 1).is_ok());
    assert!(admit_source_diagnostics(&findings[1..], limit - 1).is_ok());
    assert!(matches!(
        admit_source_diagnostics(&findings, limit - 1),
        Err(ProjectionError::Limit)
    ));
    let report = admit_source_diagnostics(&findings, limit).unwrap();
    assert!(std::ptr::eq(report.findings().as_ptr(), findings.as_ptr()));
    let [SourceFinding::Csv {
        field: borrowed_field,
        ..
    }, SourceFinding::FieldParse { value, .. }] = report.findings()
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
    let actual: Vec<_> = report.iter().collect();
    assert_eq!(actual[0].context["field"], text("ID"));
    assert_eq!(actual[1].context["value"], text(&long));
    assert_eq!(report.iter().collect::<Vec<_>>(), actual);
    assert!(matches!(
        admit_source_diagnostics(&findings, 0),
        Err(ProjectionError::Limit)
    ));
    assert_eq!(admit_source_diagnostics(&[], 0).unwrap().iter().len(), 0);
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
    let actual = finding.diagnostic().unwrap();
    assert_eq!(actual.context["record"], V::Integer(usize::MAX.to_string()));
    assert_eq!(actual.context["field"], V::Integer(usize::MAX.to_string()));
    let actual = SourceFinding::Parquet {
        finding: ParquetFinding::Value {
            field: "DT",
            row: usize::MAX,
            value: i64::MIN,
        },
        dataset: "SRC",
        path: "data.parquet",
    }
    .diagnostic()
    .unwrap();
    assert_eq!(actual.context["row"], V::Integer(usize::MAX.to_string()));
    assert_eq!(actual.context["value"], V::Scalar(Value::Int(i64::MIN)));
    assert!(csv_source::Error::Limit {
        resource: "bytes",
        limit: 0
    }
    .finding("SRC", "s.csv")
    .is_none());
    for error in [
        yamaa_core::typed_csv::Error::InvalidDeclarations,
        yamaa_core::typed_csv::Error::InvalidSource,
    ] {
        assert!(error.finding("SRC").is_none());
        assert!(error.diagnostic("SRC").is_none());
    }
}
