#![allow(unused_imports)]
pub(crate) use serde_json::Value as V;
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::{
    dataset::AggregatePhase,
    diagnostic::{ConditionCode, OdmDeclared, OdmFinding as F},
    value::{ColumnType, Value},
};
pub(crate) fn text(value: &str) -> V {
    V::String(value.into())
}
pub(crate) fn check(
    finding: F<'_>,
    phase: &str,
    condition: &str,
    requirement: &str,
    path: &str,
    context: Vec<(&str, V)>,
    reached: &mut BTreeSet<ConditionCode>,
) {
    let diagnostic = finding.diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        (phase, condition, Some(requirement))
    );
    assert_eq!(diagnostic.spec_paths, [path]);
    assert_eq!(
        diagnostic.context,
        context
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect()
    );
    reached.insert(diagnostic.code);
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (declared, name, requirement) in [
        (OdmDeclared::Types, "types", "REQ-1275"),
        (OdmDeclared::Schema, "schema", "REQ-1275"),
        (OdmDeclared::Ordinal, "ordinal", "REQ-1295"),
    ] {
        check(
            F::Declared {
                dataset: "D",
                declared,
            },
            "validation",
            "odm_schema_field_type",
            requirement,
            &format!("input.D.{name}"),
            vec![("dataset", text("D")), ("declared", text(name))],
            &mut reached,
        );
    }
    check(
        F::MissingFields {
            dataset: "D",
            fields: &["FormOID", "Value"],
        },
        "validation",
        "odm_schema_field_missing",
        "REQ-1275",
        "input.D.path",
        vec![
            ("dataset", text("D")),
            ("fields", V::Array(vec![text("FormOID"), text("Value")])),
        ],
        &mut reached,
    );
    let stored = ["ITEMoid".into(), "ItemOID".into()];
    check(
        F::AmbiguousField {
            dataset: "D",
            field: "ItemOID",
            stored: &stored,
        },
        "validation",
        "odm_schema_field_ambiguous",
        "REQ-1275",
        "input.D.path",
        vec![
            ("dataset", text("D")),
            ("field", text("ItemOID")),
            ("stored", V::Array(vec![text("ITEMoid"), text("ItemOID")])),
        ],
        &mut reached,
    );
    for (kind, expected) in [(Some(ColumnType::Int), "int"), (None, "unsupported")] {
        check(
            F::FieldType {
                dataset: "D",
                field: "Value",
                stored_field: "VALUE",
                kind,
            },
            "validation",
            "odm_schema_field_type",
            "REQ-1275",
            "input.D.path",
            vec![
                ("dataset", text("D")),
                ("field", text("Value")),
                ("stored_field", text("VALUE")),
                ("stored_type", text(expected)),
            ],
            &mut reached,
        );
    }
    check(
        F::MissingValues {
            dataset: "D",
            field: "SubjectKey",
            record: 2,
            records: 3,
        },
        "ingest",
        "odm_schema_value_missing",
        "REQ-1276",
        "input.D.path",
        vec![
            ("dataset", text("D")),
            ("field", text("SubjectKey")),
            (
                "record",
                V::Number(("2").parse().expect("canonical integer")),
            ),
            (
                "records",
                V::Number(("3").parse().expect("canonical integer")),
            ),
        ],
        &mut reached,
    );
    for location in [
        yamaa_core::diagnostic::OdmLocation::Column,
        yamaa_core::diagnostic::OdmLocation::Row,
        yamaa_core::diagnostic::OdmLocation::Intermediate,
        yamaa_core::diagnostic::OdmLocation::Derive,
    ] {
        check(
            F::InvalidContext {
                path: "columns.V.derivation.odm",
                dataset: "D",
                location,
            },
            "validation",
            "invalid_odm_context",
            "REQ-1277",
            "columns.V.derivation.odm",
            vec![("dataset", text("D")), ("location", text(location.name()))],
            &mut reached,
        );
    }
    let scope = [
        ("StudyEventRepeatKey".into(), Value::Missing),
        ("SubjectKey".into(), Value::Str("S1".into())),
    ];
    let differ = [(
        "FormOID".into(),
        vec![Value::Str("F2".into()), Value::Str("F1".into())],
    )];
    for (phase, expected_phase) in [
        (AggregatePhase::Derivation, "derivation"),
        (AggregatePhase::RowConstruction, "row_construction"),
    ] {
        for (differences, expected_differ, repeated) in [
            (
                differ.as_slice(),
                V::Object(
                    [("FormOID".into(), V::Array(vec![text("F2"), text("F1")]))]
                        .into_iter()
                        .collect(),
                ),
                false,
            ),
            (&[][..], V::Object(Default::default()), true),
        ] {
            check(
                F::NotUnique {
                    path: "rows[0].derivations.V.odm",
                    item: "D.Item.with.periods",
                    row: Some("sample"),
                    phase,
                    scope: &scope,
                    differ: differences,
                    records: 2,
                },
                expected_phase,
                "odm_not_unique",
                "REQ-1278",
                "rows[0].derivations.V.odm",
                vec![
                    ("item", text("D.Item.with.periods")),
                    ("row", text("sample")),
                    (
                        "scope",
                        V::Object(
                            [
                                ("StudyEventRepeatKey".into(), V::Null),
                                ("SubjectKey".into(), text("S1")),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    ),
                    (
                        "records",
                        V::Number(("2").parse().expect("canonical integer")),
                    ),
                    ("differ", expected_differ),
                    ("repeated", V::Bool(repeated)),
                ],
                &mut reached,
            );
        }
    }
    reached
}
