use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue, InheritanceFinding as F},
    value::Value,
};

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
            assert_eq!(
                finding.context[key],
                ContextValue::Scalar(Value::Str(value.into()))
            );
        }
        assert!(finding.source_span.is_none());
        assert!(finding.operand_route.is_none());
        if condition == "inheritance_cycle" {
            assert_eq!(
                finding.context["cycle"],
                ContextValue::Sequence(
                    cycle
                        .iter()
                        .map(|s| ContextValue::Scalar(Value::Str(s.clone())))
                        .collect()
                )
            );
        }
        reached.insert(finding.code);
    }
    let dependencies = yamaa_core::diagnostic::inheritance_dependency_diagnostics(
        &[yamaa_core::schema::InheritanceDependencyIssue::Unknown {
            column: "VALUE".into(),
            dependency: "MISSING".into(),
        }],
        4096,
    )
    .unwrap();
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
            (
                "column".into(),
                ContextValue::Scalar(Value::Str("VALUE".into()))
            ),
            (
                "dependency".into(),
                ContextValue::Scalar(Value::Str("MISSING".into()))
            ),
        ]
        .into_iter()
        .collect()
    );
    reached.insert(dependency.code);
    reached
}
