use yamaa_core::{
    diagnostic::{inheritance_dependency_diagnostics, ContextValue, ProjectionError},
    schema::InheritanceDependencyIssue as Issue,
    value::Value,
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
        },
    ];
    let findings = inheritance_dependency_diagnostics(&issues, 4096).unwrap();
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].spec_paths, ["columns.VALUE\0é.derivation"]);
    assert_eq!(
        findings[0].context["dependency"],
        ContextValue::Scalar(Value::Str("MISSING🙂".into()))
    );
    assert_eq!(findings[1].spec_paths, ["columns"]);
    assert_eq!(
        findings[1].context["cycle"],
        ContextValue::Sequence(
            ["DOWNSTREAM", "A", "B"]
                .into_iter()
                .map(|name| ContextValue::Scalar(Value::Str(name.into())))
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
fn whole_report_admission_has_an_inclusive_boundary_and_no_fitting_prefix() {
    let first = Issue::Unknown {
        column: "VALUE".into(),
        dependency: "FIRST".into(),
    };
    let later = Issue::Unknown {
        column: "LATER".into(),
        dependency: "\"\\\0🙂".repeat(1000),
    };
    let issues = [first.clone(), later];
    let expected = inheritance_dependency_diagnostics(&issues, 1_000_000).unwrap();
    let (mut low, mut high) = (0, 1_000_000);
    while low < high {
        let middle = low + (high - low) / 2;
        if inheritance_dependency_diagnostics(&issues, middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 1);
    assert_eq!(
        inheritance_dependency_diagnostics(&issues, low),
        Ok(expected)
    );
    assert_eq!(
        inheritance_dependency_diagnostics(&issues, low - 1),
        Err(ProjectionError::Limit)
    );
    assert!(inheritance_dependency_diagnostics(&[first], 4096).is_ok());
    assert_eq!(
        inheritance_dependency_diagnostics(&issues, 4096),
        Err(ProjectionError::Limit)
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
fn sequence_members_consume_capacity_even_with_empty_text() {
    let issues = [Issue::Cycle {
        columns: vec![String::new(); 1000],
    }];
    assert_eq!(
        inheritance_dependency_diagnostics(&issues, 4096),
        Err(ProjectionError::Limit)
    );
    assert_eq!(
        inheritance_dependency_diagnostics(&issues, 32_768).unwrap()[0].context["cycle"],
        ContextValue::Sequence(vec![ContextValue::Scalar(Value::Str(String::new())); 1000])
    );
}
