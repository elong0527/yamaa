use serde_json::Value as C;
use std::collections::BTreeSet;
use yamaa_core::{csv_source::parse, diagnostic::ConditionCode};
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (input, condition, requirement, record, field) in [
        (
            &b""[..],
            "source_header_absent",
            "REQ-0851",
            1,
            C::Number(("1").parse().expect("canonical integer")),
        ),
        (
            &b"\xef\xbb\xbfID\n"[..],
            "source_byte_order_mark",
            "REQ-0851",
            1,
            C::Number(("1").parse().expect("canonical integer")),
        ),
        (
            &b"ID\nx\r"[..],
            "source_carriage_return",
            "REQ-0851",
            2,
            C::Number(("1").parse().expect("canonical integer")),
        ),
        (
            &b"ID\nx\"y"[..],
            "source_quote_in_bare_field",
            "REQ-0851",
            2,
            C::Number(("1").parse().expect("canonical integer")),
        ),
        (
            &b"ID\n\"x\" "[..],
            "source_text_after_quote",
            "REQ-0851",
            2,
            C::Number(("1").parse().expect("canonical integer")),
        ),
        (
            &b"ID\n\"x"[..],
            "source_quote_unterminated",
            "REQ-0851",
            2,
            C::Number(("1").parse().expect("canonical integer")),
        ),
        (
            &b"ID,V\nx"[..],
            "source_record_width",
            "REQ-0851",
            2,
            C::Number(("2").parse().expect("canonical integer")),
        ),
        (
            &b"ID,\nx,y"[..],
            "source_field_name_empty",
            "REQ-0851",
            1,
            C::Number(("2").parse().expect("canonical integer")),
        ),
        (
            &b"ID,ID\nx,y"[..],
            "source_field_name_duplicate",
            "REQ-0851",
            1,
            C::String("ID".into()),
        ),
        (
            &b"\xef\xbb\xbfID\n\"x\ny\",\xff"[..],
            "invalid_text",
            "REQ-0853",
            2,
            C::Number(("2").parse().expect("canonical integer")),
        ),
    ] {
        let error = parse(input).unwrap_err();
        let finding = error.finding("SRC", "source.csv").unwrap().diagnostic();
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
        assert_eq!(finding.context["dataset"], C::String("SRC".into()));
        assert_eq!(finding.context["path"], C::String("source.csv".into()));
        assert_eq!(finding.context["record"], C::from(record));
        assert_eq!(finding.context["field"], field);
        assert_eq!(finding.context.len(), 4);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(finding.code);
    }
    let header = (0..=yamaa_core::limits::Limits::COLUMNS)
        .map(|index| format!("C{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let error = parse(header.as_bytes()).unwrap_err();
    assert!(error.finding("SRC", "source.csv").is_none());
    assert!(error.requirement().is_none());
    reached
}

pub(super) fn typing_reached() -> std::collections::BTreeSet<yamaa_core::diagnostic::ConditionCode>
{
    use serde_json::Value as JsonValue;
    use yamaa_core::{
        csv_source, diagnostic::Context, typed_csv::PreparedTypes, value::ColumnType,
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
        let types = PreparedTypes::new(&declarations).unwrap();
        let error = types
            .convert(csv_source::parse(content).unwrap())
            .unwrap_err();
        let diagnostic = error.finding("SRC").unwrap().diagnostic();
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
            .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
            .collect();
        assert_eq!(diagnostic.context, expected);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
