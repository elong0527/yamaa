use serde_json::Value as JsonValue;
use yamaa_core::{
    diagnostic::inheritance_dependency_diagnostics, schema::InheritanceDependencyIssue as Issue,
};

#[test]
fn complete_ordered_facts_keep_dependency_cycles_distinct_from_traversal_loops() {
    let issues = [
        Issue::Unknown {
            column: "VALUE\0é".into(),
            dependency: "MISSING🙂".into(),
        },
        Issue::Cycle {
            columns: vec!["DOWNSTREAM".into(), "A".into(), "B".into()],
            cycle: vec!["A".into(), "B".into(), "A".into()],
            paths: vec![
                "columns.A.derivation.compute".into(),
                "columns.B.derivation.compute".into(),
            ],
        },
    ];
    let findings = inheritance_dependency_diagnostics(&issues);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].spec_paths, ["columns.VALUE\0é.derivation"]);
    assert_eq!(
        findings[0].context["dependency"],
        JsonValue::String("MISSING🙂".into())
    );
    assert_eq!(
        findings[1].spec_paths,
        [
            "columns.A.derivation.compute",
            "columns.B.derivation.compute"
        ]
    );
    assert_eq!(
        findings[1].context["cycle"],
        JsonValue::Array(
            ["DOWNSTREAM", "A", "B"]
                .into_iter()
                .map(|name| JsonValue::String(name.into()))
                .collect()
        )
    );
    assert_eq!(findings[1].definition().condition, "dependency_cycle");
    assert_eq!(findings[1].definition().requirement, Some("REQ-0072"));
    assert!(findings
        .iter()
        .all(|finding| finding.source_span.is_none() && finding.operand_route.is_none()));
}

#[test]
fn later_long_dependency_facts_remain_complete_in_order() {
    let later = "\"\\\0🙂".repeat(1000);
    let issues = [
        Issue::Unknown {
            column: "VALUE".into(),
            dependency: "FIRST".into(),
        },
        Issue::Unknown {
            column: "LATER".into(),
            dependency: later.clone(),
        },
    ];
    let projected = inheritance_dependency_diagnostics(&issues);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].spec_paths, ["columns.VALUE.derivation"]);
    assert_eq!(
        projected[0].context,
        [
            ("column".into(), JsonValue::String("VALUE".into())),
            ("dependency".into(), JsonValue::String("FIRST".into())),
        ]
        .into()
    );
    assert_eq!(projected[1].spec_paths, ["columns.LATER.derivation"]);
    assert_eq!(
        projected[1].context,
        [
            ("column".into(), JsonValue::String("LATER".into())),
            ("dependency".into(), JsonValue::String(later)),
        ]
        .into()
    );
    assert_eq!(
        issues[0],
        Issue::Unknown {
            column: "VALUE".into(),
            dependency: "FIRST".into()
        }
    );
}

#[test]
fn all_empty_sequence_members_remain_in_the_retained_order() {
    let issues = [Issue::Cycle {
        columns: vec![String::new(); 1000],
        cycle: vec![String::new(); 2],
        paths: vec![String::new()],
    }];
    let projected = inheritance_dependency_diagnostics(&issues);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].spec_paths, [""]);
    assert_eq!(
        projected[0].context["cycle"],
        JsonValue::Array(vec![JsonValue::String(String::new()); 1000])
    );
}
