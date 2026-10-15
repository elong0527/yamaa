use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use yamaa_core::diagnostic::{ConditionCode, InheritanceFinding as F};

#[test]
fn contradictory_cycle_carriers_refuse_without_relabeling_the_cause() {
    for path in [
        vec![],
        vec!["entry".into()],
        vec!["entry".into(), "other".into()],
    ] {
        assert_eq!(
            F::Cycle {
                path: &path,
                returns_to_entry: true
            }
            .diagnostic(),
            Err(yamaa_core::diagnostic::ProjectionError::InvalidContext)
        );
    }
    let path = vec!["entry".into(), "parent".into(), "entry".into()];
    let diagnostic = F::Cycle {
        path: &path,
        returns_to_entry: true,
    }
    .diagnostic()
    .unwrap();
    assert_eq!(diagnostic.code, ConditionCode::InheritanceCycle);
    assert_eq!(diagnostic.spec_paths, ["parents"]);
    assert_eq!(
        diagnostic.context,
        [
            (
                "reason".into(),
                JsonValue::String("parent_chain_returns_to_entry".into())
            ),
            ("source".into(), JsonValue::String("parent".into())),
            (
                "cycle".into(),
                JsonValue::Array(
                    ["entry", "parent", "entry"]
                        .map(|name| JsonValue::String(name.into()))
                        .into()
                )
            ),
        ]
        .into()
    );
    assert_eq!(diagnostic.source_span, None);
    assert_eq!(diagnostic.operand_route, None);
}

pub fn reached() -> BTreeSet<ConditionCode> {
    let cycle = vec!["entry".into(), "base".into(), "entry".into()];
    let mut reached = BTreeSet::new();
    for (cause, condition, requirement, path, context) in [
        (
            F::InvalidParent {
                declaring: "child",
                path: "https://remote/base",
            },
            "invalid_parent_path",
            "REQ-0653",
            "parents",
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
            "REQ-0654",
            "parents",
            vec![("path", "../missing.yaml"), ("source", "child")],
        ),
        (
            F::Cycle {
                path: &cycle,
                returns_to_entry: true,
            },
            "inheritance_cycle",
            "REQ-0655",
            "parents",
            vec![
                ("reason", "parent_chain_returns_to_entry"),
                ("source", "base"),
            ],
        ),
        (
            F::Version {
                source: "base",
                entry: "entry",
                expected: "1.0",
                actual: "0",
                is_entry: false,
            },
            "schema_version_mismatch",
            "REQ-0656",
            "parents",
            vec![
                ("source", "base"),
                ("entry", "entry"),
                ("entry_version", "1.0"),
                ("parent_version", "0"),
            ],
        ),
        (
            F::Version {
                source: "entry",
                entry: "entry",
                expected: "1.0",
                actual: "0",
                is_entry: true,
            },
            "schema_version_mismatch",
            "REQ-0245",
            "schema_version",
            vec![
                ("source", "entry"),
                ("entry", "entry"),
                ("expected", "1.0"),
                ("actual", "0"),
            ],
        ),
    ] {
        let finding = cause.diagnostic().unwrap();
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().requirement
            ),
            ("validation", condition, Some(requirement))
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
        reached.insert(finding.code);
    }
    let dependencies = yamaa_core::diagnostic::inheritance_dependency_diagnostics(&[
        yamaa_core::schema::InheritanceDependencyIssue::Unknown {
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
            dependency.definition().requirement
        ),
        ("validation", "unknown_reference", Some("REQ-0070"))
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
    reached.insert(dependency.code);
    reached
}
