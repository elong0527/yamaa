use std::collections::BTreeSet;
use yamaa_core::{
    dataset::AggregatePhase,
    diagnostic::{ConditionCode, ContextValue as V, OdmDeclared, OdmFinding as F, ProjectionError},
    value::{ColumnType, Value},
};
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn check(
    finding: F<'_>,
    phase: &str,
    condition: &str,
    requirement: &str,
    path: &str,
    context: Vec<(&str, V)>,
    reached: &mut BTreeSet<ConditionCode>,
) {
    let diagnostic = finding.diagnostic().unwrap();
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
            ("fields", V::Sequence(vec![text("FormOID"), text("Value")])),
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
            (
                "stored",
                V::Sequence(vec![text("ITEMoid"), text("ItemOID")]),
            ),
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
            ("record", V::Integer("2".into())),
            ("records", V::Integer("3".into())),
        ],
        &mut reached,
    );
    for (row, expected) in [
        (Some("sample"), text("sample")),
        (None, V::Scalar(Value::Missing)),
    ] {
        check(
            F::InvalidContext {
                path: "columns.V.derivation.odm",
                dataset: "D",
                row,
            },
            "validation",
            "invalid_odm_context",
            "REQ-1277",
            "columns.V.derivation.odm",
            vec![("dataset", text("D")), ("row", expected)],
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
                V::Object([("FormOID".into(), V::Sequence(vec![text("F2"), text("F1")]))].into()),
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
                                ("StudyEventRepeatKey".into(), V::Scalar(Value::Missing)),
                                ("SubjectKey".into(), text("S1")),
                            ]
                            .into(),
                        ),
                    ),
                    ("records", V::Integer("2".into())),
                    ("differ", expected_differ),
                    ("repeated", V::Scalar(Value::Bool(repeated))),
                ],
                &mut reached,
            );
        }
    }
    reached
}

#[test]
fn whole_odm_projection_accounts_for_nested_values_and_escaped_metadata() {
    let large = "\"".repeat(400);
    let names = [large];
    let finding = F::AmbiguousField {
        dataset: "D",
        field: "ItemOID",
        stored: &names,
    };
    assert_eq!(
        finding.diagnostic_with_limit(1024),
        Err(ProjectionError::Limit)
    );
    assert!(finding.diagnostic_with_limit(4096).is_ok());
    let differ = [("FormOID".into(), vec![Value::Str("\\".repeat(400))])];
    assert_eq!(
        F::NotUnique {
            path: "odm",
            item: "D.Item",
            row: None,
            phase: AggregatePhase::Derivation,
            scope: &[],
            differ: &differ,
            records: 2
        }
        .diagnostic_with_limit(1024),
        Err(ProjectionError::Limit)
    );
}
