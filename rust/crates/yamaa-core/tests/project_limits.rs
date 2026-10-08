use yamaa_core::{
    project_environment::{Draft, Environment},
    project_function::{Case, Definition, Function, Language, Parameter},
    project_limits::{AdmissionError, Limit, Limits, Resource},
    project_terminology::{Catalogue, Codelist, DataType, Item, Source},
    value::{ColumnType, Value, ValueType},
};
fn definition() -> Definition {
    Definition {
        name: "constant".into(),
        function: "project.constant".into(),
        description: "μ\0".into(),
        params: vec![],
        returns: ColumnType::Int,
        may_return_missing: false,
        comparison_decimals: 4,
        tests: vec![Case {
            id: "ordinary".into(),
            covers: vec!["normal".into(), "boundary".into()],
            args: vec![],
            result: Value::Int(1),
        }],
    }
}
fn unlimited() -> Limits {
    Limits {
        nodes: usize::MAX,
        text_bytes: usize::MAX,
        work: usize::MAX,
        findings: usize::MAX,
    }
}
#[test]
fn byte_limits_count_utf8_and_nul_exactly_and_admit_the_exact_boundary() {
    // 8+16+3+8+6+8 authored UTF-8 bytes, including the embedded NUL.
    let mut limits = unlimited();
    limits.text_bytes = 49;
    assert!(Function::admit_with_limits(Language::Python, definition(), limits).is_ok());
    limits.text_bytes = 48;
    assert_eq!(
        Function::admit_with_limits(Language::Python, definition(), limits),
        Err(AdmissionError::Limit(Limit {
            resource: Resource::TextBytes
        }))
    );
}
#[test]
fn each_resource_rejects_the_whole_function_without_partial_semantic_findings() {
    for resource in [Resource::Nodes, Resource::Work, Resource::Findings] {
        let mut limits = unlimited();
        match resource {
            Resource::Nodes => limits.nodes = 0,
            Resource::Work => limits.work = 0,
            Resource::Findings => limits.findings = 0,
            _ => unreachable!(),
        }
        let mut def = definition();
        def.function = "invalid".into();
        assert_eq!(
            Function::admit_with_limits(Language::Python, def, limits),
            Err(AdmissionError::Limit(Limit { resource }))
        );
    }
}
#[test]
fn environment_budgets_are_cumulative_across_function_definitions() {
    let mut limits = unlimited();
    limits.text_bytes = 49;
    assert!(Function::admit_with_limits(Language::Python, definition(), limits).is_ok());
    let draft = Draft {
        language: None,
        lock: None,
        functions: Some(vec![definition(), definition()]),
        codelists: vec![],
        has_study: false,
        submissions: vec![],
    };
    assert_eq!(
        Environment::admit_with_limits(Language::Python, draft, limits),
        Err(AdmissionError::Limit(Limit {
            resource: Resource::TextBytes
        }))
    );
}
#[test]
fn parameter_case_products_are_bounded_before_static_coverage_and_lookup() {
    let mut def = definition();
    def.params = (0..100)
        .map(|i| Parameter {
            name: format!("parameter{i}"),
            kind: ValueType::Int,
            required: true,
            default: None,
            accepts_missing: false,
        })
        .collect();
    def.tests[0].args = def
        .params
        .iter()
        .map(|p| (p.name.clone(), Value::Int(1)))
        .collect();
    let mut limits = unlimited();
    limits.work = 100_000;
    assert_eq!(
        Function::admit_with_limits(Language::Python, def, limits),
        Err(AdmissionError::Limit(Limit {
            resource: Resource::Work
        }))
    );
}
#[test]
fn large_ordered_codelists_admit_without_quadratic_comparison_work() {
    let source = Source {
        standard: None,
        codelists: vec![Codelist {
            id: "LARGE".into(),
            name: "Large".into(),
            data_type: DataType::Integer,
            extensible: false,
            alias: None,
            format_name: None,
            external: None,
            items: Some(
                (0..10_000)
                    .map(|i| Item {
                        value: Value::Int(i),
                        decode: None,
                        rank: None,
                        alias: None,
                        extended: false,
                    })
                    .collect(),
            ),
        }],
    };
    let mut limits = unlimited();
    limits.work = 500_000;
    let catalogue = Catalogue::admit_with_limits(vec![source.clone()], limits).unwrap();
    let values: Vec<_> = (0..10_000).rev().map(Value::Int).collect();
    assert!(catalogue
        .bind("LARGE", ColumnType::Int, Some(&values))
        .is_empty());
    limits.nodes = 10;
    assert_eq!(
        Catalogue::admit_with_limits(vec![source], limits),
        Err(AdmissionError::Limit(Limit {
            resource: Resource::Nodes
        }))
    );
}

#[test]
fn environment_duplicate_function_lookup_is_bounded_before_static_findings() {
    let mut limits = unlimited();
    limits.work = 300_000;
    let functions = (0..32)
        .map(|i| {
            let mut def = definition();
            def.name = format!("{}{}", "a".repeat(1000), i);
            def
        })
        .collect();
    let draft = Draft {
        language: None,
        lock: None,
        functions: Some(functions),
        codelists: vec![],
        has_study: false,
        submissions: vec![],
    };
    assert_eq!(
        Environment::admit_with_limits(Language::Python, draft, limits),
        Err(AdmissionError::Limit(Limit {
            resource: Resource::Work
        }))
    );
}
