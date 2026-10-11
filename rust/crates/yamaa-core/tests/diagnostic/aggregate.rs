use std::collections::BTreeSet;
use yamaa_core::{
    dataset::AggregatePhase,
    diagnostic::{
        AggregateContextCause as Cause, AggregateEmptyKey, AggregateFinding as Finding,
        ConditionCode, ContextValue as V, ProjectionError,
    },
    key_relation::ComparableType,
    value::{ColumnType, Value, ValueType},
};

fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}

pub fn reached() -> BTreeSet<ConditionCode> {
    let group = vec![("D.ID".into(), Value::Missing)];
    let relations = vec!["D".into(), "<output>".into()];
    let types = [ValueType::Str, ValueType::Int, ValueType::Str];
    let keys = vec!["ID".into(), "PARAM".into()];
    let mut reached = BTreeSet::new();
    for (key, empty) in [
        (AggregateEmptyKey::Mapping, V::Object(Default::default())),
        (AggregateEmptyKey::List, V::Sequence(vec![])),
    ] {
        let diagnostic = Finding::MissingKeys {
            path: "aggregate",
            dataset: "D",
            key,
        }
        .diagnostic()
        .unwrap();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", "missing_aggregate_keys", Some("REQ-0140"))
        );
        assert_eq!(
            diagnostic.context,
            [("dataset".into(), text("D")), ("key".into(), empty)].into()
        );
        assert_eq!(diagnostic.spec_paths, ["aggregate"]);
        reached.insert(diagnostic.code);
    }
    for (phase, row, expected) in [
        (AggregatePhase::Derivation, None, "derivation"),
        (
            AggregatePhase::RowConstruction,
            Some("total"),
            "row_construction",
        ),
    ] {
        let diagnostic = Finding::MultipleRecords {
            path: "rows[0].derivations.V.aggregate",
            expression: "ONLY(D.V)",
            phase,
            count: 3,
            row,
            group: if row.is_some() { &group } else { &[] },
        }
        .diagnostic()
        .unwrap();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            (expected, "aggregate_multiple_records", Some("REQ-0501"))
        );
        let mut context = [
            ("expr".into(), text("ONLY(D.V)")),
            ("reducer".into(), text("ONLY")),
            ("record_count".into(), V::Integer("3".into())),
        ]
        .into_iter()
        .collect::<yamaa_core::diagnostic::Context>();
        if let Some(row) = row {
            context.insert("row".into(), text(row));
            context.insert(
                "group".into(),
                V::Object([("D.ID".into(), V::Scalar(Value::Missing))].into()),
            );
        }
        assert_eq!(diagnostic.context, context);
        assert_eq!(diagnostic.spec_paths, ["rows[0].derivations.V.aggregate"]);
        reached.insert(diagnostic.code);
    }
    let cases = [
        (
            Finding::IdentifierNotGrouped {
                path: "aggregate.expr",
                expression: "V + SUM(D.N)",
                dataset: Some("D"),
                identifier: "V",
            },
            "aggregate_identifier_not_grouped",
            "REQ-0503",
            vec![
                ("expr", text("V + SUM(D.N)")),
                ("dataset", text("D")),
                ("identifier", text("V")),
            ],
        ),
        (
            Finding::IncomparableSources {
                path: "aggregate.expr",
                expression: "MAX(D.V)",
                source: "D.V",
                types: &types,
            },
            "incomparable_sources",
            "REQ-0511",
            vec![
                ("expr", text("MAX(D.V)")),
                ("sources", V::Sequence(vec![text("D.V")])),
                ("types", V::Sequence(vec![text("int"), text("str")])),
            ],
        ),
        (
            Finding::MixedRelations {
                path: "aggregate.expr",
                expression: "SUM(D.N) + V",
                relations: &relations,
            },
            "mixed_relations",
            "REQ-0504",
            vec![
                ("expr", text("SUM(D.N) + V")),
                ("relations", V::Sequence(vec![text("D"), text("<output>")])),
            ],
        ),
        (
            Finding::InvalidContext {
                path: "aggregate",
                expression: "SUM(V)",
                cause: Cause::OutputNeedsGroups,
            },
            "invalid_aggregate_context",
            "REQ-0507",
            vec![
                ("expr", text("SUM(V)")),
                ("reason", text("an output-row reduction declares group_by")),
            ],
        ),
        (
            Finding::InvalidContext {
                path: "aggregate.key",
                expression: "SUM(D.N)",
                cause: Cause::RowNoKeys,
            },
            "invalid_aggregate_context",
            "REQ-0142",
            vec![
                ("expr", text("SUM(D.N)")),
                (
                    "reason",
                    text("a grouped row aggregate reads its own group and declares no key pairs"),
                ),
            ],
        ),
        (
            Finding::InvalidContext {
                path: "aggregate",
                expression: "SUM(D.N)",
                cause: Cause::UngroupedRow,
            },
            "invalid_aggregate_context",
            "REQ-0329",
            vec![
                ("expr", text("SUM(D.N)")),
                (
                    "reason",
                    text("an ungrouped row template has no aggregate context"),
                ),
            ],
        ),
        (
            Finding::KeyReference {
                path: "aggregate.expr",
                identifier: "D.ABSENT",
            },
            "unknown_field",
            "REQ-0141",
            vec![("identifier", text("D.ABSENT"))],
        ),
        (
            Finding::KeyType {
                path: "aggregate.expr",
                source: "ID",
                expected: ComparableType::Column(ColumnType::Str),
                actual: ColumnType::Int,
                inferred: false,
            },
            "incompatible_input_type",
            "REQ-0004",
            vec![
                ("source", text("ID")),
                ("expected", text("str")),
                ("actual", text("int")),
            ],
        ),
        (
            Finding::KeyType {
                path: "aggregate.expr",
                source: "ID",
                expected: ComparableType::Column(ColumnType::Str),
                actual: ColumnType::Int,
                inferred: true,
            },
            "incompatible_input_type",
            "REQ-0151",
            vec![
                ("source", text("ID")),
                ("expected", text("str")),
                ("actual", text("int")),
            ],
        ),
        (
            Finding::NoApplicableKeys {
                path: "aggregate",
                dataset: "D",
                keys: &keys,
            },
            "no_applicable_keys",
            "REQ-0152",
            vec![
                ("dataset", text("D")),
                ("keys", V::Sequence(vec![text("ID"), text("PARAM")])),
                (
                    "hint",
                    text("declare an explicit `intermediate:` with `source`/`key` pairs"),
                ),
            ],
        ),
    ];
    for (cause, condition, requirement, context) in cases {
        let diagnostic = cause.diagnostic().unwrap();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        assert!(diagnostic.source_span.is_none());
        assert!(diagnostic.operand_route.is_none());
        reached.insert(diagnostic.code);
    }
    reached
}

#[test]
fn invalid_causes_and_complete_context_budgets_refuse_before_projection() {
    for types in [
        &[ValueType::Int, ValueType::Float][..],
        &[ValueType::Str, ValueType::Str][..],
        &[][..],
    ] {
        assert_eq!(
            Finding::IncomparableSources {
                path: "aggregate",
                expression: "MAX(V)",
                source: "V",
                types
            }
            .diagnostic(),
            Err(ProjectionError::InvalidContext)
        );
    }
    for count in [0, 1] {
        assert_eq!(
            Finding::MultipleRecords {
                path: "aggregate",
                expression: "ONLY(V)",
                phase: AggregatePhase::Derivation,
                count,
                row: None,
                group: &[]
            }
            .diagnostic(),
            Err(ProjectionError::InvalidContext)
        );
    }
    let huge = "x".repeat(1_000_000);
    let group = vec![("D.ID".into(), Value::Str(huge.clone()))];
    let address = huge.as_ptr();
    assert_eq!(
        Finding::MultipleRecords {
            path: "aggregate",
            expression: "ONLY(D.V)",
            phase: AggregatePhase::RowConstruction,
            count: 2,
            row: Some("total"),
            group: &group
        }
        .diagnostic_with_limit(4096),
        Err(ProjectionError::Limit)
    );
    assert_eq!(huge.as_ptr(), address);
    assert_eq!(group[0].1, Value::Str(huge));
}

#[test]
#[cfg(target_pointer_width = "64")]
fn record_counts_keep_the_complete_unsigned_range() {
    let diagnostic = Finding::MultipleRecords {
        path: "aggregate",
        expression: "ONLY(V)",
        phase: AggregatePhase::Derivation,
        count: usize::MAX,
        row: None,
        group: &[],
    }
    .diagnostic()
    .unwrap();
    assert_eq!(
        diagnostic.context["record_count"],
        V::Integer(usize::MAX.to_string())
    );
}
