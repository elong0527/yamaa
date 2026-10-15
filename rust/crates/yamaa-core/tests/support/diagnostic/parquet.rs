#![allow(unused_imports)]
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    diagnostic::{ConditionCode, Context},
    parquet_source::{self as p, Converted, Field, Logical, Physical, Representation},
    value::ColumnType,
};
pub(crate) fn field(name: &str) -> Field {
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
pub(crate) fn text(value: &str) -> V {
    V::String(value.into())
}
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
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
            vec![(
                "field",
                V::Number(("1").parse().expect("canonical integer")),
            )],
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
                ("row", V::Number(("2").parse().expect("canonical integer"))),
                ("value", V::from(i64::MAX)),
            ],
        ),
    ];
    let mut reached = BTreeSet::new();
    for (error, condition, requirement, extra) in errors {
        let finding = error.finding("SRC", "data.parquet").diagnostic();
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
