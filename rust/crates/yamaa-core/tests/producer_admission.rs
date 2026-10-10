#[path = "support/producer.rs"]
mod support;
use support::{candidate, consumer, document, producer, Tree};
use yamaa_core::{
    diagnostic::{ContextValue as V, Diagnostic},
    producer_admission::{prepare, Error, Input, Limits},
    producer_contract,
    specification::{PrepareError, PreparedSpecification},
    value::{ColumnType, Value},
};
fn invalid(result: Result<yamaa_core::producer_admission::Prepared, Error>) -> Vec<Diagnostic> {
    match result {
        Err(Error::Invalid(findings)) => findings,
        _ => panic!("producer admission findings"),
    }
}
fn text(s: &str) -> V {
    V::Scalar(Value::Str(s.into()))
}
#[test]
fn sealed_compiler_preserves_distinct_producer_identity_fields_and_authored_origins() {
    let source = producer();
    let consumer = consumer(None);
    let metadata = prepare(
        "/consumer.yaml",
        &consumer,
        &[candidate(&source)],
        None,
        Limits::default(),
    )
    .unwrap();
    let inputs = metadata.inputs().collect::<Vec<_>>();
    let [Input::Producer(declaration)] = inputs.as_slice() else {
        panic!("producer kind")
    };
    assert_eq!(declaration.dataset(), "SRC");
    assert_eq!(declaration.schema_path(), "nested/producer.yaml");
    assert_eq!(
        declaration.schema_origin().declaring_source(),
        "/parent.yaml"
    );
    assert_eq!(
        declaration.schema_origin().written(),
        "../nested/producer.yaml"
    );
    assert_eq!(
        declaration.input_origin().declaring_source(),
        "/consumer.yaml"
    );
    assert_eq!(declaration.input_origin().written(), "./produced.csv");
    assert_eq!(declaration.producer_identity(), "/producer.yaml");
    assert_eq!(declaration.artifact_identity(), "/produced.csv");
    assert_eq!(
        declaration
            .contract()
            .fields()
            .iter()
            .map(|f| (f.name.as_str(), f.kind, f.label.as_str()))
            .collect::<Vec<_>>(),
        [
            ("ID", ColumnType::Int, "Identifier"),
            ("VALUE", ColumnType::Float, "Reported value")
        ]
    );
    assert_eq!(metadata.projection(), ["ID"]);
    assert!(metadata.called_functions().is_empty());
    let PrepareError::Unsupported(features) = metadata.execution_refusal() else {
        panic!("execution refusal")
    };
    assert_eq!(features[0].operation, "producer_workflow");
    assert_eq!(features[0].path, "input.SRC.schema");
    assert!(matches!(
        PreparedSpecification::prepare(&consumer),
        Err(PrepareError::Unsupported(_))
    ));
}
#[test]
fn input_declaration_order_is_independent_of_supplied_metadata_order() {
    use Tree::*;
    let input = Map(vec![
        (
            "B",
            Map(vec![("path", Text("b.csv")), ("schema", Text("b.yaml"))]),
        ),
        ("EXT", Map(vec![("path", Text("external.csv"))])),
        (
            "A",
            Map(vec![("path", Text("a.csv")), ("schema", Text("a.yaml"))]),
        ),
    ]);
    let consumer = document(input, &[("ID", "int", Some("ID"))], &["ID"], false);
    let source = producer();
    let mut a = candidate(&source);
    let mut b = candidate(&source);
    a.dataset = "A";
    a.schema_path = "a.yaml";
    b.dataset = "B";
    b.schema_path = "b.yaml";
    let metadata = prepare(
        "/consumer.yaml",
        &consumer,
        &[a, b],
        None,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        metadata
            .inputs()
            .map(|i| match i {
                Input::Producer(p) => (p.dataset(), true),
                Input::External(s) => (s.name.as_str(), false),
            })
            .collect::<Vec<_>>(),
        [("B", true), ("EXT", false), ("A", true)]
    );
    assert_eq!(
        metadata
            .producers()
            .iter()
            .map(|p| p.dataset())
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
}
#[test]
fn inline_types_are_redundant_even_when_equal_empty_or_null() {
    use Tree::*;
    let source = producer();
    for types in [Map(vec![("ID", Text("int"))]), Map(vec![]), Null] {
        let findings = invalid(prepare(
            "/consumer.yaml",
            &consumer(Some(types)),
            &[candidate(&source)],
            None,
            Limits::default(),
        ));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            (
                findings[0].definition().condition,
                findings[0].definition().requirement
            ),
            ("redundant_field_type", Some("REQ-0523"))
        );
        assert_eq!(findings[0].context["dataset"], text("SRC"));
        assert!(findings[0].spec_paths[0].starts_with("input.SRC.types"));
    }
}
#[test]
fn missing_duplicate_and_undeclared_metadata_are_never_external_file_reads() {
    let source = producer();
    let consumer = consumer(None);
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer,
        &[],
        None,
        Limits::default(),
    ));
    assert_eq!(findings[0].context["reason"], text("missing_metadata"));
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer,
        &[candidate(&source), candidate(&source)],
        None,
        Limits::default(),
    ));
    assert_eq!(findings[0].context["reason"], text("duplicate_metadata"));
    let mut extra = candidate(&source);
    extra.dataset = "ABSENT";
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer,
        &[candidate(&source), extra],
        None,
        Limits::default(),
    ));
    assert_eq!(findings[0].context["reason"], text("undeclared_metadata"));
}
#[test]
fn contradictory_identities_and_self_dependency_fail_at_metadata_admission() {
    let source = producer();
    let consumer = consumer(None);
    for variation in 0..4 {
        let mut c = candidate(&source);
        match variation {
            0 => c.schema_path = "other.yaml",
            1 => c.schema_identity = "/other.yaml",
            2 => c.input_origin.written = "",
            _ => {
                c.schema_identity = "/consumer.yaml";
                c.producer_identity = "/consumer.yaml";
            }
        }
        let findings = invalid(prepare(
            "/consumer.yaml",
            &consumer,
            &[c],
            None,
            Limits::default(),
        ));
        assert_eq!(
            findings[0].context["reason"],
            text(if variation == 3 {
                "producer_workflow_cycle"
            } else {
                "contradictory_metadata"
            })
        );
        assert_eq!(findings[0].spec_paths, ["input.SRC.schema"]);
    }
}
#[test]
fn canonical_artifact_mismatch_keeps_written_geometry_and_normative_mapping() {
    let source = producer();
    let consumer = consumer(None);
    let mut c = candidate(&source);
    c.output_identity = "/other.csv";
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer,
        &[c],
        None,
        Limits::default(),
    ));
    let f = &findings[0];
    assert_eq!(
        (
            f.definition().phase,
            f.definition().condition,
            f.definition().requirement
        ),
        (
            "validation",
            "producer_output_path_mismatch",
            Some("REQ-0534")
        )
    );
    assert_eq!(f.spec_paths, ["input.SRC.path", "input.SRC.schema"]);
    assert_eq!(f.context["source_path"], text("./produced.csv"));
    assert_eq!(f.context["output_path"], text("produced.csv"));
}
#[test]
fn producer_contract_findings_retain_selected_field_order() {
    use Tree::*;
    let source = document(
        Map(vec![("RAW", Map(vec![("path", Text("raw.csv"))]))]),
        &[("ID", "int", None)],
        &["ID", "ID", "OTHER"],
        false,
    );
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer(None),
        &[candidate(&source)],
        None,
        Limits::default(),
    ));
    assert_eq!(
        findings
            .iter()
            .map(|f| f.spec_paths[0].as_str())
            .collect::<Vec<_>>(),
        [
            "input.SRC.schema.columns.ID.label",
            "input.SRC.schema.output.columns[1]",
            "input.SRC.schema.output.columns[2]"
        ]
    );
    assert!(findings
        .iter()
        .all(|f| f.definition().requirement == Some("REQ-0534")));
    assert_eq!(findings[2].context["declarations"], V::Integer("0".into()));
}
#[test]
fn aggregate_metadata_quotas_and_source_field_limits_precede_compiled_result() {
    let source = producer();
    let consumer = consumer(None);
    for (limits, name) in [
        (
            Limits {
                candidates: 0,
                ..Default::default()
            },
            "producer_candidates",
        ),
        (
            Limits {
                document_nodes: 0,
                ..Default::default()
            },
            "producer_document_nodes",
        ),
        (
            Limits {
                text_bytes: 0,
                ..Default::default()
            },
            "producer_metadata_text_bytes",
        ),
        (
            Limits {
                compilation: yamaa_core::specification::CompilationLimits {
                    source_fields: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            "source_fields",
        ),
    ] {
        assert!(
            matches!(prepare("/consumer.yaml",&consumer,&[candidate(&source)],None,limits),Err(Error::Limit(actual)) if actual==name)
        );
    }
}
#[test]
fn unrelated_unsupported_consumer_vocabulary_is_not_stripped() {
    use Tree::*;
    let source = producer();
    let consumer = document(
        Map(vec![(
            "SRC",
            Map(vec![
                ("path", Text("produced.csv")),
                ("schema", Text("nested/producer.yaml")),
            ]),
        )]),
        &[("ID", "int", Some("ID"))],
        &["ID"],
        true,
    );
    assert!(
        matches!(prepare("/consumer.yaml",&consumer,&[candidate(&source)],None,Limits::default()),Err(Error::Compilation(PrepareError::Unsupported(features))) if features.iter().any(|f| f.path=="filter"))
    );
}
#[test]
fn actual_metadata_comparison_is_ordered_typed_bounded_and_portable() {
    let contract = producer_contract::prepare(&producer(), Default::default()).unwrap();
    assert!(contract
        .validate_header("SRC", &["ID", "VALUE"], Default::default())
        .unwrap()
        .is_none());
    for actual in [
        vec!["VALUE", "ID"],
        vec!["ID"],
        vec!["ID", "VALUE", "OTHER"],
        vec!["ID", "ID"],
    ] {
        let finding = contract
            .validate_header("SRC", &actual, Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                finding.definition().condition,
                finding.definition().requirement
            ),
            ("producer_contract_mismatch", Some("REQ-0535"))
        );
        assert_eq!(finding.spec_paths, ["input.SRC.schema", "input.SRC.path"]);
    }
    let finding = contract
        .validate_typed_fields(
            "SRC",
            &[("ID", ColumnType::Int), ("VALUE", ColumnType::Str)],
            Default::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        finding.context["expected_types"],
        V::Sequence(vec![text("int"), text("float")])
    );
    assert_eq!(
        finding.context["actual_types"],
        V::Sequence(vec![text("int"), text("str")])
    );
    assert_eq!(finding.context["reordered"], V::Scalar(Value::Bool(false)));
    assert!(matches!(
        contract.validate_header(
            "SRC",
            &["ID", "VALUE"],
            producer_contract::Limits {
                text_bytes: 0,
                ..Default::default()
            }
        ),
        Err(producer_contract::Error::Limit(
            "producer_actual_text_bytes"
        ))
    ));
}
