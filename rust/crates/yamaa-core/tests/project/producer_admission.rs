use crate::support::producer as support;
use serde_json::Value as V;
use support::{candidate, consumer, document, producer, Tree};
use yamaa_core::{
    diagnostic::Diagnostic,
    producer_admission::{prepare, Error, Input},
    producer_contract,
    specification::{PrepareError, PreparedSpecification},
    value::ColumnType,
};
fn invalid(result: Result<yamaa_core::producer_admission::Prepared, Error>) -> Vec<Diagnostic> {
    match result {
        Err(Error::Invalid(findings)) => findings,
        _ => panic!("producer admission findings"),
    }
}
fn text(s: &str) -> V {
    V::String(s.into())
}
#[test]
fn sealed_compiler_preserves_distinct_producer_identity_fields_and_authored_origins() {
    let source = producer();
    let consumer = consumer(None);
    let metadata = prepare("/consumer.yaml", &consumer, &[candidate(&source)], None).unwrap();
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
    let metadata = prepare("/consumer.yaml", &consumer, &[a, b], None).unwrap();
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
    let findings = invalid(prepare("/consumer.yaml", &consumer, &[], None));
    assert_eq!(findings[0].context["reason"], text("missing_metadata"));
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer,
        &[candidate(&source), candidate(&source)],
        None,
    ));
    assert_eq!(findings[0].context["reason"], text("duplicate_metadata"));
    let mut extra = candidate(&source);
    extra.dataset = "ABSENT";
    let findings = invalid(prepare(
        "/consumer.yaml",
        &consumer,
        &[candidate(&source), extra],
        None,
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
        let findings = invalid(prepare("/consumer.yaml", &consumer, &[c], None));
        assert_eq!(
            findings[0].context["reason"],
            text(if variation == 3 {
                "producer_workflow_cycle"
            } else {
                "contradictory_metadata"
            })
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].spec_paths, ["input.SRC.schema"]);
        let definition = findings[0].definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("validation", "invalid_producer_contract", Some("REQ-0534")),
        );
        assert_eq!(findings[0].context.len(), 2);
        assert_eq!(findings[0].context["dataset"], text("SRC"));
    }
}
#[test]
fn canonical_artifact_mismatch_keeps_written_geometry_and_normative_mapping() {
    let source = producer();
    let consumer = consumer(None);
    let mut c = candidate(&source);
    c.output_identity = "/other.csv";
    let findings = invalid(prepare("/consumer.yaml", &consumer, &[c], None));
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
    assert_eq!(
        findings[2].context["declarations"],
        V::Number(("0").parse().expect("canonical integer"))
    );
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
        matches!(prepare("/consumer.yaml",&consumer,&[candidate(&source)],None),Err(Error::Compilation(PrepareError::Unsupported(features))) if features.iter().any(|f| f.path=="parents" && f.operation == "parents"))
    );
}
#[test]
fn actual_metadata_comparison_is_ordered_typed_bounded_and_portable() {
    let contract = producer_contract::prepare(&producer()).unwrap();
    assert!(contract
        .validate_header("SRC", &["ID", "VALUE"])
        .unwrap()
        .is_none());
    for actual in [
        vec!["VALUE", "ID"],
        vec!["ID"],
        vec!["ID", "VALUE", "OTHER"],
        vec!["ID", "ID"],
    ] {
        let finding = contract.validate_header("SRC", &actual).unwrap().unwrap();
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
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        finding.context["expected_types"],
        V::Array(vec![text("int"), text("float")])
    );
    assert_eq!(
        finding.context["actual_types"],
        V::Array(vec![text("int"), text("str")])
    );
    assert_eq!(finding.context["reordered"], V::Bool(false));
    assert!(contract
        .validate_header("SRC", &["ID", "VALUE"])
        .unwrap()
        .is_none());
}
#[test]
fn one_producer_identity_cannot_describe_two_different_output_contracts() {
    use Tree::*;
    let input = Map(vec![
        (
            "A",
            Map(vec![
                ("path", Text("produced.csv")),
                ("schema", Text("a.yaml")),
            ]),
        ),
        (
            "B",
            Map(vec![
                ("path", Text("produced.csv")),
                ("schema", Text("b.yaml")),
            ]),
        ),
    ]);
    let consumer = document(input, &[("ID", "int", Some("ID"))], &["ID"], false);
    let source = producer();
    let other = document(
        Map(vec![("RAW", Map(vec![("path", Text("raw.csv"))]))]),
        &[
            ("ID", "int", Some("Identifier")),
            ("VALUE", "str", Some("Reported value")),
        ],
        &["ID", "VALUE"],
        false,
    );
    let mut a = candidate(&source);
    a.dataset = "A";
    a.schema_path = "a.yaml";
    let mut b = candidate(&other);
    b.dataset = "B";
    b.schema_path = "b.yaml";
    let findings = invalid(prepare("/consumer.yaml", &consumer, &[a, b], None));
    assert_eq!(findings[0].spec_paths, ["input.B.schema"]);
    assert_eq!(
        findings[0].context["reason"],
        text("contradictory_metadata")
    );
}
