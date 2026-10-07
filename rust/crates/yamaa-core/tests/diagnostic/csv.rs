use std::collections::BTreeSet;
use yamaa_core::{
    csv_source::{parse, Limits},
    diagnostic::{ConditionCode, ContextValue as C},
    value::Value,
};
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (input, condition, requirement, record, field) in [
        (
            &b""[..],
            "source_header_absent",
            "REQ-0851",
            1,
            C::Integer("1".into()),
        ),
        (
            &b"\xef\xbb\xbfID\n"[..],
            "source_byte_order_mark",
            "REQ-0851",
            1,
            C::Integer("1".into()),
        ),
        (
            &b"ID\nx\r"[..],
            "source_carriage_return",
            "REQ-0851",
            2,
            C::Integer("1".into()),
        ),
        (
            &b"ID\nx\"y"[..],
            "source_quote_in_bare_field",
            "REQ-0851",
            2,
            C::Integer("1".into()),
        ),
        (
            &b"ID\n\"x\" "[..],
            "source_text_after_quote",
            "REQ-0851",
            2,
            C::Integer("1".into()),
        ),
        (
            &b"ID\n\"x"[..],
            "source_quote_unterminated",
            "REQ-0851",
            2,
            C::Integer("1".into()),
        ),
        (
            &b"ID,V\nx"[..],
            "source_record_width",
            "REQ-0851",
            2,
            C::Integer("2".into()),
        ),
        (
            &b"ID,\nx,y"[..],
            "source_field_name_empty",
            "REQ-0851",
            1,
            C::Integer("2".into()),
        ),
        (
            &b"ID,ID\nx,y"[..],
            "source_field_name_duplicate",
            "REQ-0851",
            1,
            C::Scalar(Value::Str("ID".into())),
        ),
        (
            &b"\xef\xbb\xbfID\n\"x\ny\",\xff"[..],
            "invalid_text",
            "REQ-0853",
            2,
            C::Integer("2".into()),
        ),
    ] {
        let error = parse(input, Limits::default()).unwrap_err();
        let finding = error.diagnostic("SRC", "source.csv").unwrap();
        let definition = finding.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("ingest", condition, Some(requirement))
        );
        assert_eq!(error.requirement(), Some(requirement));
        assert_eq!(finding.spec_paths, ["input.SRC.path"]);
        assert_eq!(
            finding.context["dataset"],
            C::Scalar(Value::Str("SRC".into()))
        );
        assert_eq!(
            finding.context["path"],
            C::Scalar(Value::Str("source.csv".into()))
        );
        assert_eq!(finding.context["record"], C::Integer(record.to_string()));
        assert_eq!(finding.context["field"], field);
        assert_eq!(finding.context.len(), 4);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(finding.code);
    }
    let error = parse(
        b"ID\n1",
        Limits {
            bytes: 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(error.diagnostic("SRC", "source.csv").is_none());
    assert!(error.requirement().is_none());
    reached
}

pub(super) fn typing_reached() -> std::collections::BTreeSet<yamaa_core::diagnostic::ConditionCode>
{
    use yamaa_core::{
        csv_source,
        diagnostic::{Context, ContextValue},
        typed_csv::PreparedTypes,
        value::{ColumnType, Value},
    };
    let mut reached = std::collections::BTreeSet::new();
    for (declarations, content, phase, condition, requirement, field, extra) in [
        (
            vec![
                ("X".into(), ColumnType::Int),
                ("ABSENT".into(), ColumnType::Int),
            ],
            b"X\ninvalid".as_slice(),
            "validation",
            "unknown_field",
            "REQ-0532",
            "ABSENT",
            vec![],
        ),
        (
            vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)],
            b"A,B\n1,bad\nworse,2".as_slice(),
            "ingest",
            "field_parse_failed",
            "REQ-0536",
            "B",
            vec![("type", "int"), ("value", "bad")],
        ),
    ] {
        let types = PreparedTypes::new(&declarations, 16).unwrap();
        let error = types
            .convert(csv_source::parse(content, Default::default()).unwrap())
            .unwrap_err();
        let diagnostic = error.diagnostic("SRC").unwrap();
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            (phase, condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, [format!("input.SRC.types.{field}")]);
        let expected: Context = [("dataset", "SRC"), ("field", field)]
            .into_iter()
            .chain(extra)
            .map(|(key, value)| (key.into(), ContextValue::Scalar(Value::Str(value.into()))))
            .collect();
        assert_eq!(diagnostic.context, expected);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
