use super::rendered;
use std::collections::BTreeSet;
use yamaa_core::json::Value as C;
use yamaa_core::{
    csv_source::{Error, Field, ProfileCause},
    diagnostic::ConditionCode,
};
/// Each CSV profile refusal the adapters' parser reports (its own suite pins
/// which bytes give which cause, record and field) reaches its condition.
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (cause, condition, rule, record, field) in [
        (
            ProfileCause::HeaderAbsent,
            "source_header_absent",
            "CSV-16",
            1,
            Field::Index(1),
        ),
        (
            ProfileCause::ByteOrderMark { declared: false },
            "source_byte_order_mark",
            "CSV-16",
            1,
            Field::Index(1),
        ),
        (
            ProfileCause::CarriageReturn,
            "source_carriage_return",
            "CSV-16",
            2,
            Field::Index(1),
        ),
        (
            ProfileCause::QuoteInBareField,
            "source_quote_in_bare_field",
            "CSV-16",
            2,
            Field::Index(1),
        ),
        (
            ProfileCause::TextAfterQuote,
            "source_text_after_quote",
            "CSV-16",
            2,
            Field::Index(1),
        ),
        (
            ProfileCause::QuoteUnterminated,
            "source_quote_unterminated",
            "CSV-16",
            2,
            Field::Index(1),
        ),
        (
            ProfileCause::RecordWidth,
            "source_record_width",
            "CSV-16",
            2,
            Field::Index(2),
        ),
        (
            ProfileCause::FieldNameEmpty,
            "source_field_name_empty",
            "CSV-16",
            1,
            Field::Index(2),
        ),
        (
            ProfileCause::FieldNameDuplicate,
            "source_field_name_duplicate",
            "CSV-16",
            1,
            Field::Name("ID".into()),
        ),
        (
            ProfileCause::InvalidText,
            "invalid_text",
            "CSV-10",
            2,
            Field::Index(2),
        ),
    ] {
        let expected_field = match &field {
            Field::Index(index) => C::from(*index),
            Field::Name(name) => C::String(name.clone()),
        };
        let error = Error::Profile {
            cause,
            record,
            field,
        };
        let finding = error.finding("SRC", "source.csv").unwrap().diagnostic();
        let definition = finding.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("ingest", condition, Some(rule))
        );
        assert_eq!(error.rule(), Some(rule));
        assert_eq!(finding.spec_paths, ["input.SRC.path"]);
        assert_eq!(finding.context["dataset"], C::String("SRC".into()));
        assert_eq!(finding.context["path"], C::String("source.csv".into()));
        assert_eq!(finding.context["record"], C::from(record));
        assert_eq!(finding.context["field"], expected_field);
        // A mark also records whether its input declares one (CSV-10).
        if condition == "source_byte_order_mark" {
            assert_eq!(finding.context["declared"], C::Bool(false));
            assert_eq!(finding.context.len(), 5);
        } else {
            assert_eq!(finding.context.len(), 4);
        }
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(rendered(&finding));
    }
    // A declared mark that is absent keeps the condition and records the
    // declaration.
    let error = Error::Profile {
        cause: ProfileCause::ByteOrderMark { declared: true },
        record: 1,
        field: Field::Index(1),
    };
    let finding = error.finding("SRC", "source.csv").unwrap().diagnostic();
    assert_eq!(finding.condition(), "source_byte_order_mark");
    assert_eq!(finding.rule(), Some("CSV-16"));
    assert_eq!(finding.spec_paths, ["input.SRC.path"]);
    assert_eq!(finding.context["declared"], C::Bool(true));
    reached.insert(rendered(&finding));
    // A boundary refusal is no profile finding.
    let error = Error::Limit {
        resource: "columns",
        limit: yamaa_core::limits::Limits::COLUMNS,
        actual: Some(yamaa_core::limits::Limits::COLUMNS + 1),
    };
    assert!(error.finding("SRC", "source.csv").is_none());
    assert!(error.rule().is_none());
    reached
}

pub(crate) fn typing_reached() -> std::collections::BTreeSet<yamaa_core::diagnostic::ConditionCode>
{
    use yamaa_core::json::Value as JsonValue;
    use yamaa_core::{diagnostic::Context, typed_csv::PreparedTypes, value::ColumnType};
    let mut reached = std::collections::BTreeSet::new();
    for (declarations, source, phase, condition, rule, field, extra) in [
        (
            vec![
                ("X".into(), ColumnType::Int),
                ("ABSENT".into(), ColumnType::Int),
            ],
            crate::support::text_table(&["X"], &[&[Some("invalid")]]),
            "validation",
            "unknown_field",
            "INPUT-6",
            "ABSENT",
            vec![],
        ),
        (
            vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)],
            crate::support::text_table(
                &["A", "B"],
                &[&[Some("1"), Some("bad")], &[Some("worse"), Some("2")]],
            ),
            "ingest",
            "field_parse_failed",
            "INPUT-7",
            "B",
            vec![("type", "int"), ("value", "bad")],
        ),
    ] {
        let types = PreparedTypes::new(&declarations).unwrap();
        let error = types.convert(source).unwrap_err();
        let diagnostic = error.finding("SRC").unwrap().diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            (phase, condition, Some(rule))
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
        reached.insert(rendered(&diagnostic));
    }
    reached
}
