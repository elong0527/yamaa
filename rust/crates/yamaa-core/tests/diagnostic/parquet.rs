use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, Context, ContextValue as V},
    parquet_source::{self as p, Converted, Field, Logical, Physical, Representation},
    value::{ColumnType, Value},
};
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
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut nested = field("S");
    nested.leaves = 2;
    nested.stored_type = "struct<left: int64, right: string>".into();
    let errors = [
        (
            p::columns(vec![], 0).unwrap_err(),
            "source_parquet_invalid",
            "REQ-1038",
            vec![],
        ),
        (
            p::columns(vec![field("")], 1).unwrap_err(),
            "source_field_name_empty",
            "REQ-1039",
            vec![("field", V::Integer("1".into()))],
        ),
        (
            p::columns(vec![field("I"), field("I")], 2).unwrap_err(),
            "source_field_name_duplicate",
            "REQ-1039",
            vec![("field", text("I"))],
        ),
        (
            p::columns(vec![field("I"), nested], 2).unwrap_err(),
            "source_field_type_unsupported",
            "REQ-1040",
            vec![
                ("field", text("S")),
                ("stored_type", text("struct<left: int64, right: string>")),
            ],
        ),
        (
            p::temporal(ColumnType::DateTime, i64::MAX, "DT", 2).unwrap_err(),
            "source_field_value_invalid",
            "REQ-1041",
            vec![
                ("field", text("DT")),
                ("row", V::Integer("2".into())),
                ("value", V::Scalar(Value::Int(i64::MAX))),
            ],
        ),
    ];
    let mut reached = BTreeSet::new();
    for (error, condition, requirement, extra) in errors {
        let finding = error.diagnostic("SRC", "data.parquet");
        let def = finding.definition();
        assert_eq!(
            (def.phase, def.condition, def.requirement),
            ("ingest", condition, Some(requirement))
        );
        assert_eq!(finding.spec_paths, ["input.SRC.path"]);
        let expected: Context = [("dataset", text("SRC")), ("path", text("data.parquet"))]
            .into_iter()
            .chain(extra)
            .map(|(key, value)| (key.into(), value))
            .collect();
        assert_eq!(finding.context, expected);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(finding.code);
    }
    reached
}

#[test]
fn alignment_and_early_profile_failure_do_not_inspect_later_codec_metadata() {
    let mut inspected = 0;
    let fields = ["I", ""].into_iter().map(|name| {
        inspected += 1;
        field(name)
    });
    assert_eq!(p::columns(fields, 1).unwrap_err(), p::Error::Invalid);
    assert_eq!(inspected, 0);
    let mut inspected = 0;
    let fields = ["", "I"].into_iter().map(|name| {
        inspected += 1;
        field(name)
    });
    assert_eq!(
        p::columns(fields, 2).unwrap_err(),
        p::Error::EmptyName { field: 1 }
    );
    assert_eq!(inspected, 1);
}
