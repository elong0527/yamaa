use yamaa_core::{
    dataset::Check,
    diagnostic::ContextValue,
    project_codelist_binding::{bind, AllowedValues, Binding, Cause, Error},
    project_limits::{Limits, Resource},
    project_terminology::{Catalogue, Codelist, DataType, External, Item, Source},
    value::{ColumnType, Value},
};
fn catalogue(kind: DataType, values: Vec<Value>, extensible: bool, external: bool) -> Catalogue {
    Catalogue::admit(vec![Source {
        standard: None,
        codelists: vec![Codelist {
            id: "CODE".into(),
            name: "Codes".into(),
            data_type: kind,
            extensible,
            alias: None,
            format_name: None,
            items: (!external).then(|| {
                values
                    .into_iter()
                    .map(|value| Item {
                        value,
                        decode: None,
                        rank: None,
                        alias: None,
                        extended: false,
                    })
                    .collect()
            }),
            external: external.then(|| External {
                dictionary: "dictionary".into(),
                version: "1".into(),
                href: Some("relative/path".into()),
            }),
        }],
    }])
    .unwrap()
}
fn binding<'a>(
    id: &'a str,
    kind: ColumnType,
    allowed_values: &'a [AllowedValues<'a>],
) -> Binding<'a> {
    Binding {
        column: 1,
        name: "VALUE",
        kind,
        path: yamaa_core::project_codelist_binding::Path::Authored(
            "columns.VALUE.submission.codelist",
        ),
        codelist: id,
        allowed_values,
    }
}
#[test]
fn complete_static_findings_preserve_all_bindings_both_sets_and_original_paths() {
    let cat = catalogue(
        DataType::Text,
        vec![Value::Str("F".into()), Value::Str("M\0🙂".into())],
        false,
        false,
    );
    let values = [Value::Str("F".into())];
    let allowed = [AllowedValues {
        path: "columns.VALUE.verifications[0].allowed_values",
        values: &values,
    }];
    let Err(Error::Findings(findings)) = bind(
        &cat,
        &[
            binding("UNKNOWN", ColumnType::Str, &[]),
            binding("CODE", ColumnType::Int, &allowed),
        ],
        Limits::default(),
    ) else {
        panic!("complete rejection")
    };
    assert_eq!(findings.len(), 3);
    for (finding, condition, requirement) in [
        (&findings[0], "unknown_codelist", "REQ-0953"),
        (&findings[1], "codelist_type_mismatch", "REQ-0954"),
        (&findings[2], "codelist_values_conflict", "REQ-0955"),
    ] {
        let d = finding.diagnostic();
        assert_eq!(
            (
                d.definition().phase,
                d.definition().condition,
                d.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(
            d.context["column"],
            ContextValue::Scalar(Value::Str("VALUE".into()))
        );
    }
    let d = findings[2].diagnostic();
    assert_eq!(
        d.spec_paths,
        [
            "columns.VALUE.submission.codelist",
            "columns.VALUE.verifications[0].allowed_values"
        ]
    );
    assert_eq!(
        d.context["codelist_values"],
        ContextValue::Sequence(vec![
            ContextValue::Scalar(Value::Str("F".into())),
            ContextValue::Scalar(Value::Str("M\0🙂".into()))
        ])
    );
    assert!(matches!(
        findings[1].cause(),
        Cause::TypeMismatch {
            expected: ColumnType::Str,
            actual: ColumnType::Int
        }
    ));
}
#[test]
fn fixed_binding_keeps_numeric_authorship_and_compares_sets_exactly() {
    let cat = catalogue(
        DataType::Float,
        vec![Value::Int(9_007_199_254_740_993), Value::float(-0.0)],
        false,
        false,
    );
    let matching = [
        Value::Int(0),
        Value::Int(9_007_199_254_740_993),
        Value::float(0.0),
    ];
    let allowed = [AllowedValues {
        path: "allowed",
        values: &matching,
    }];
    let checks = bind(
        &cat,
        &[binding("CODE", ColumnType::Float, &allowed)],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].column, 1);
    let Check::Codelist { id, values } = &checks[0].verification.check else {
        panic!("fixed check")
    };
    assert_eq!(id, "CODE");
    assert_eq!(values[0], Value::Int(9_007_199_254_740_993));
    let Value::Float(zero) = values[1] else {
        panic!("authored float")
    };
    assert_eq!(zero.get().to_bits(), (-0.0f64).to_bits());
    let rounded = [Value::float(9_007_199_254_740_992.0), Value::Int(0)];
    let different = [AllowedValues {
        path: "allowed",
        values: &rounded,
    }];
    assert!(
        matches!(bind(&cat, &[binding("CODE", ColumnType::Float, &different)], Limits::default()), Err(Error::Findings(f)) if f.len()==1)
    );
}
#[test]
fn extensible_and_external_bindings_leave_explicit_verifications_independent() {
    for external in [false, true] {
        let cat = catalogue(
            DataType::Text,
            vec![Value::Str("F".into())],
            !external,
            external,
        );
        let values = [Value::Str("OTHER".into())];
        let allowed = [AllowedValues {
            path: "allowed",
            values: &values,
        }];
        assert!(bind(
            &cat,
            &[binding("CODE", ColumnType::Str, &allowed)],
            Limits::default()
        )
        .unwrap()
        .is_empty());
        assert!(
            matches!(bind(&cat, &[binding("CODE", ColumnType::Int, &allowed)], Limits::default()), Err(Error::Findings(f)) if f.len()==1)
        );
    }
}
#[test]
fn binding_quotas_are_aggregate_and_do_not_return_a_partial_prefix() {
    let cat = catalogue(
        DataType::Text,
        vec![Value::Str("x".repeat(20))],
        false,
        false,
    );
    let request = binding("CODE", ColumnType::Str, &[]);
    let minimum_text = (0..1000)
        .find(|&text_bytes| {
            bind(
                &cat,
                &[request],
                Limits {
                    text_bytes,
                    ..Default::default()
                },
            )
            .is_ok()
        })
        .unwrap();
    assert!(
        matches!(bind(&cat, &[request,request], Limits { text_bytes: minimum_text, ..Default::default() }), Err(Error::Limit(l)) if l.resource==Resource::TextBytes)
    );
    assert!(
        matches!(bind(&cat, &[request], Limits { findings: 1, ..Default::default() }), Err(Error::Limit(l)) if l.resource==Resource::Findings)
    );
    assert!(
        matches!(bind(&cat, &[request], Limits { work: 1, ..Default::default() }), Err(Error::Limit(l)) if l.resource==Resource::Work)
    );
}
