use yamaa_core::schema::{
    Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    ValidationError, ValidationLimits,
};

fn document() -> Document {
    let mut nodes = Vec::new();
    let mut fields = Vec::new();
    for (name, value) in [
        ("schema_version", N::Text("1.0".into())),
        ("domain", N::Text("X".into())),
        ("input", N::Mapping(vec![])),
        ("keys", N::Sequence(vec![])),
        ("columns", N::Sequence(vec![])),
    ] {
        let key = nodes.len();
        nodes.push(N::Text(name.into()));
        let value_id = nodes.len();
        nodes.push(value);
        fields.push((key, value_id));
    }
    let path = nodes.len();
    nodes.push(N::Text("path".into()));
    let value = nodes.len();
    nodes.push(N::Text("output.csv".into()));
    let columns = nodes.len();
    nodes.push(N::Text("columns".into()));
    let empty = nodes.len();
    nodes.push(N::Sequence(vec![]));
    let output = nodes.len();
    nodes.push(N::Mapping(vec![(path, value), (columns, empty)]));
    let key = nodes.len();
    nodes.push(N::Text("output".into()));
    fields.push((key, output));
    let root = nodes.len();
    nodes.push(N::Mapping(fields));
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}

#[test]
fn admitted_model_owns_unchanged_occurrences_without_claiming_execution() {
    let document = document();
    let expected = document.clone();
    let mut budget = ValidationBudget::new(ValidationLimits::default());
    let model = SpecificationDocument::admit(document, &mut budget)
        .unwrap()
        .unwrap();
    assert_eq!(model.document(), &expected);
    assert_eq!(model.default_driver(), None);
    assert!(budget.work_used() > 0);
    assert_eq!(
        model.document().field(model.document().root(), "base"),
        None
    );
}

#[test]
fn resource_failures_never_become_authored_diagnostics_or_partial_models() {
    for limits in [
        ValidationLimits {
            work: 0,
            ..ValidationLimits::default()
        },
        ValidationLimits {
            depth: 0,
            ..ValidationLimits::default()
        },
        ValidationLimits {
            diagnostic_text_bytes: 0,
            ..ValidationLimits::default()
        },
    ] {
        assert!(
            SpecificationDocument::admit(document(), &mut ValidationBudget::new(limits)).is_err()
        );
    }
    let empty = Document::new(vec![N::Mapping(vec![])], 0, DocumentLimits::default()).unwrap();
    let mut budget = ValidationBudget::new(ValidationLimits {
        diagnostics: 0,
        ..ValidationLimits::default()
    });
    assert_eq!(
        SpecificationDocument::admit(empty, &mut budget),
        Err(ValidationError::Diagnostics { limit: 0 })
    );
}

#[test]
fn related_admissions_share_the_request_work_budget() {
    let mut measured = ValidationBudget::new(ValidationLimits::default());
    SpecificationDocument::admit(document(), &mut measured)
        .unwrap()
        .unwrap();
    let mut budget = ValidationBudget::new(ValidationLimits {
        work: measured.work_used(),
        ..ValidationLimits::default()
    });
    SpecificationDocument::admit(document(), &mut budget)
        .unwrap()
        .unwrap();
    assert!(SpecificationDocument::admit(document(), &mut budget).is_err());
}
