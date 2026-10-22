#![allow(unused_imports)]
use super::rendered;
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    dataset::AggregatePhase,
    diagnostic::{
        AggregateContextCause as Cause, AggregateFinding as Finding, ConditionCode, ProjectionError,
    },
    result_type::ResultType,
    value::{ColumnType, Value, ValueType},
};

pub(crate) use crate::support::project_call_compiler as specification_support;

pub(crate) use super::text;

pub fn reached() -> BTreeSet<ConditionCode> {
    let group = vec![("D.ID".into(), Value::Missing)];
    let relations = vec!["D".into(), "<output>".into()];
    let types = [ValueType::Str, ValueType::Int, ValueType::Str];
    let keys = vec!["ID".into(), "PARAM".into()];
    let mut reached = BTreeSet::new();
    // A written key naming no pair is the one empty-key cause of every
    // lookup (LOOKUP-5).
    let diagnostic = Finding::MissingKeys {
        path: "aggregate",
        dataset: "D",
    }
    .diagnostic()
    .unwrap();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "invalid_field_type", Some("LOOKUP-5"))
    );
    assert_eq!(
        diagnostic.context,
        [
            ("dataset".into(), text("D")),
            ("expected".into(), text("at least one match pair")),
        ]
        .into()
    );
    assert_eq!(diagnostic.spec_paths, ["aggregate"]);
    reached.insert(rendered(&diagnostic));
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
                diagnostic.definition().rule
            ),
            (expected, "aggregate_multiple_records", Some("FORMULA-20"))
        );
        let mut context = [
            ("expr".into(), text("ONLY(D.V)")),
            ("reducer".into(), text("ONLY")),
            (
                "record_count".into(),
                V::Number(("3").parse().expect("canonical integer")),
            ),
        ]
        .into_iter()
        .collect::<yamaa_core::diagnostic::Context>();
        if let Some(row) = row {
            context.insert("row".into(), text(row));
            context.insert(
                "group".into(),
                V::Object([("D.ID".into(), V::Null)].into_iter().collect()),
            );
        }
        assert_eq!(diagnostic.context, context);
        assert_eq!(diagnostic.spec_paths, ["rows[0].derivations.V.aggregate"]);
        reached.insert(rendered(&diagnostic));
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
            "FORMULA-22",
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
            "FORMULA-23",
            vec![
                ("expr", text("MAX(D.V)")),
                ("sources", V::Array(vec![text("D.V")])),
                ("types", V::Array(vec![text("int"), text("str")])),
            ],
        ),
        (
            Finding::MixedRelations {
                path: "aggregate.expr",
                expression: "SUM(D.N) + V",
                relations: &relations,
            },
            "mixed_relations",
            "LOOKUP-18",
            vec![
                ("expr", text("SUM(D.N) + V")),
                ("relations", V::Array(vec![text("D"), text("<output>")])),
            ],
        ),
        (
            Finding::InvalidContext {
                path: "aggregate",
                expression: "SUM(V)",
                cause: Cause::OutputNeedsGroups,
            },
            "invalid_aggregate_context",
            "LOOKUP-19",
            vec![
                ("expr", text("SUM(V)")),
                ("reason", text("an output-row reduction declares group_by")),
            ],
        ),
        (
            Finding::InvalidContext {
                path: "aggregate.group_by",
                expression: "SUM(D.N)",
                cause: Cause::QualifiedGroupNotOutputKey,
            },
            "invalid_aggregate_context",
            "LOOKUP-19",
            vec![
                ("expr", text("SUM(D.N)")),
                (
                    "reason",
                    text("a qualified aggregate group_by column must also be an output key"),
                ),
            ],
        ),
        (
            Finding::InvalidContext {
                path: "aggregate.key",
                expression: "SUM(D.N)",
                cause: Cause::RowNoKeys,
            },
            "invalid_aggregate_context",
            "LOOKUP-5",
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
            "LOOKUP-18",
            vec![
                ("expr", text("SUM(D.N)")),
                (
                    "reason",
                    text("an ungrouped row template has no aggregate context"),
                ),
            ],
        ),
        (
            Finding::DonorKeyReference {
                path: "aggregate.expr",
                identifier: "D.ABSENT",
            },
            "unknown_field",
            "LOOKUP-5",
            vec![("identifier", text("D.ABSENT"))],
        ),
        (
            Finding::MatchValueReference {
                path: "aggregate.expr",
                identifier: "ABSENT",
            },
            "unknown_field",
            "LOOKUP-5",
            vec![("identifier", text("ABSENT"))],
        ),
        (
            Finding::KeyType {
                path: "aggregate.expr",
                source: "ID",
                expected: ResultType::STR,
                actual: ColumnType::Int,
            },
            "incompatible_input_type",
            "LOOKUP-9",
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
            "LOOKUP-5",
            vec![
                ("dataset", text("D")),
                ("keys", V::Array(vec![text("ID"), text("PARAM")])),
                ("hint", text("declare the `key` explicitly")),
            ],
        ),
    ];
    for (cause, condition, rule, context) in cases {
        let diagnostic = cause.diagnostic().unwrap();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().rule
            ),
            ("validation", condition, Some(rule))
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
        reached.insert(rendered(&diagnostic));
    }
    reached.extend(derive_reached());
    reached
}

pub(crate) fn derive_reached() -> BTreeSet<ConditionCode> {
    use crate::support::specification::prepare;
    pub(crate) use specification_support::Tree as T;
    pub(crate) use yamaa_core::{
        schema::{Document, DocumentNode as N},
        specification::{PreflightFinding, PrepareError},
    };
    // An aggregate's bindings are a map keyed by name (P-BIND-1).
    let binding = |value| {
        (
            "V",
            T::map(vec![
                ("type", T::text("float")),
                ("derivation", T::map(vec![("value", value)])),
            ]),
        )
    };
    let specification = |bindings: Vec<(&str, T)>| {
        specification_support::specification(vec![(
            "RESULT",
            "float",
            T::map(vec![(
                "aggregate",
                T::map(vec![
                    ("derivations", T::map(bindings)),
                    ("expr", T::text("SUM(V)")),
                ]),
            )]),
        )])
    };
    let mut reached = BTreeSet::new();
    for (bindings, code, condition, rule) in [
        (
            vec![],
            ConditionCode::AggregateDeriveDeclaration,
            "invalid_derive_step",
            "LOOKUP-22",
        ),
        (
            vec![binding(T::Scalar(N::Integer("1".into())))],
            ConditionCode::AggregateDeriveRelation,
            "invalid_derive_step",
            "LOOKUP-22",
        ),
        (
            vec![binding(T::text("W"))],
            ConditionCode::AggregateDeriveVariable,
            "unknown_derive_variable",
            "LOOKUP-22",
        ),
    ] {
        let document = specification(bindings);
        let Err(PrepareError::Invalid(findings)) = prepare(&document) else {
            panic!("invalid derive declaration was admitted");
        };
        let diagnostic = findings
            .iter()
            .find_map(|finding| match finding {
                PreflightFinding::Scalar(diagnostic) if diagnostic.code == code => Some(diagnostic),
                _ => None,
            })
            .expect("original compiler cause");
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().rule
            ),
            ("validation", condition, Some(rule))
        );
        assert!(diagnostic.spec_paths[0].starts_with("columns.RESULT.derivation.aggregate"));
        reached.insert(rendered(&diagnostic));
    }
    let document = specification(vec![
        binding(T::text("SRC.ID")),
        (
            "C",
            T::map(vec![
                ("type", T::text("int")),
                ("derivation", T::map(vec![("value", T::text("CAP.ID"))])),
            ]),
        ),
    ]);
    let root = document.root();
    let mut nodes = document.nodes().to_vec();
    assert_eq!(root, nodes.len() - 1);
    let Some(N::Mapping(mut fields)) = nodes.pop() else {
        unreachable!()
    };
    pub(crate) fn append(tree: T, nodes: &mut Vec<N>) -> usize {
        tree.append(nodes)
    }
    let key = append(T::text("intermediates"), &mut nodes);
    let value = append(
        T::List(vec![T::map(vec![
            ("id", T::text("CAP")),
            ("dataset", T::text("SRC")),
            ("key", T::map(vec![("ID", T::text("SRC.ID"))])),
        ])]),
        &mut nodes,
    );
    fields.push((key, value));
    let root = nodes.len();
    nodes.push(N::Mapping(fields));
    let document = Document::new(nodes, root).unwrap();
    let Err(PrepareError::Invalid(findings)) = prepare(&document) else {
        panic!("derive read without keep was admitted");
    };
    let diagnostic = findings
        .iter()
        .find_map(|finding| match finding {
            PreflightFinding::Scalar(diagnostic)
                if diagnostic.code == ConditionCode::AggregateDeriveIntermediate =>
            {
                Some(diagnostic)
            }
            _ => None,
        })
        .expect("original named-intermediate cause");
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "invalid_derive_step", Some("LOOKUP-22"))
    );
    assert_eq!(diagnostic.context["intermediate"], text("CAP"));
    reached.insert(rendered(&diagnostic));
    reached
}
