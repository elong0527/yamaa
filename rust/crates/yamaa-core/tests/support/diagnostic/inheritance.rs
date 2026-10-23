#![allow(unused_imports)]
use super::rendered;
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::diagnostic::{ConditionCode, InheritanceFinding as F};
pub(crate) use yamaa_core::inheritance::Named;
pub(crate) use yamaa_core::json::Value as JsonValue;

pub fn reached() -> BTreeSet<ConditionCode> {
    let cycle = vec!["entry".into(), "base".into(), "entry".into()];
    let mut reached = BTreeSet::new();
    for (cause, condition, rule, path, context) in [
        (
            F::InvalidParent {
                declaring: "child",
                path: "https://remote/base",
            },
            "invalid_parent_path",
            "LAYER-2",
            "parent",
            vec![
                ("reason", "remote_reference"),
                ("source", "child"),
                ("parent", "https://remote/base"),
            ],
        ),
        (
            F::ParentUnavailable {
                declaring: "child",
                path: "../missing.yaml",
            },
            "parent_not_found",
            "LAYER-2",
            "parent",
            vec![("path", "../missing.yaml"), ("source", "child")],
        ),
        (
            F::ParentOnAnotherShare {
                source: "//server/share/std/base.yaml",
                path: "input/dm.csv",
                parent_root: "//server/share",
                entry_root: "C:",
            },
            "parent_on_another_share",
            "LAYER-10",
            "parent",
            vec![
                ("source", "//server/share/std/base.yaml"),
                ("path", "input/dm.csv"),
                ("parent_root", "//server/share"),
                ("entry_root", "C:"),
            ],
        ),
        (
            F::Cycle {
                path: &cycle,
                returns_to_entry: true,
                named: Named::Parent,
            },
            "inheritance_cycle",
            "LAYER-3",
            "parent",
            vec![
                ("reason", "parent_chain_returns_to_entry"),
                ("source", "base"),
            ],
        ),
        // An include entry that closes the cycle is the field reported.
        (
            F::Cycle {
                path: &cycle,
                returns_to_entry: true,
                named: Named::Include(2),
            },
            "inheritance_cycle",
            "LAYER-3",
            "include[2]",
            vec![
                ("reason", "parent_chain_returns_to_entry"),
                ("source", "base"),
            ],
        ),
        (
            F::Repeated {
                declaring: "child",
                index: 1,
                path: "shared/duration.yaml",
                layer: "/study/shared/duration.yaml",
            },
            "duplicate_layer",
            "LAYER-3",
            "include[1]",
            vec![
                ("source", "child"),
                ("path", "shared/duration.yaml"),
                ("layer", "/study/shared/duration.yaml"),
            ],
        ),
        (
            F::Fragment {
                source: "fragment",
                entry: "entry",
                field: "parent",
            },
            "prohibited_construct",
            "LAYER-4",
            "parent",
            vec![
                ("construct", "parent"),
                ("source", "fragment"),
                ("entry", "entry"),
            ],
        ),
    ] {
        let finding = cause.diagnostic().unwrap();
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().rule
            ),
            ("validation", condition, Some(rule))
        );
        assert_eq!(finding.spec_paths, [path]);
        for (key, value) in context {
            assert_eq!(finding.context[key], JsonValue::String(value.into()));
        }
        assert!(finding.source_span.is_none());
        assert!(finding.operand_route.is_none());
        if condition == "inheritance_cycle" {
            assert_eq!(
                finding.context["cycle"],
                JsonValue::Array(cycle.iter().map(|s| JsonValue::String(s.clone())).collect())
            );
        }
        reached.insert(rendered(&finding));
    }
    let dependencies = yamaa_core::diagnostic::inheritance_dependency_diagnostics(&[
        yamaa_core::resolution::Issue::Unknown {
            column: "VALUE".into(),
            dependency: "MISSING".into(),
        },
    ]);
    assert_eq!(dependencies.len(), 1);
    let dependency = &dependencies[0];
    assert_eq!(
        (
            dependency.definition().phase,
            dependency.definition().condition,
            dependency.definition().rule
        ),
        ("validation", "unknown_reference", Some("COLUMN-5"))
    );
    assert_eq!(dependency.spec_paths, ["columns.VALUE.derivation"]);
    assert_eq!(
        dependency.context,
        [
            ("column".into(), JsonValue::String("VALUE".into())),
            ("dependency".into(), JsonValue::String("MISSING".into())),
        ]
        .into_iter()
        .collect()
    );
    reached.insert(rendered(&dependency));
    reached
}
