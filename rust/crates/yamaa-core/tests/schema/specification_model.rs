use yamaa_core::schema::{Document, DocumentNode as N, SpecificationDocument};

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
    Document::new(nodes, root).unwrap()
}

#[test]
fn admitted_model_owns_unchanged_occurrences_without_claiming_execution() {
    let document = document();
    let expected = document.clone();
    let model = SpecificationDocument::admit(document).unwrap().unwrap();
    assert_eq!(model.document(), &expected);
    assert_eq!(model.default_driver(), None);
    assert_eq!(
        model.document().field(model.document().root(), "base"),
        None
    );
}

#[test]
fn invalid_models_keep_all_authored_findings_without_a_partial_model() {
    let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
    let findings = SpecificationDocument::admit(empty).unwrap().unwrap_err();
    assert!(!findings.is_empty());
    assert!(findings.iter().all(|finding| !finding.path.is_empty()));
    assert!(SpecificationDocument::admit(document()).unwrap().is_ok());
}

#[test]
fn independent_admissions_preserve_the_same_occurrences() {
    let input = document();
    let first = SpecificationDocument::admit(input.clone())
        .unwrap()
        .unwrap();
    let second = SpecificationDocument::admit(input.clone())
        .unwrap()
        .unwrap();
    assert_eq!(first.document(), &input);
    assert_eq!(second.document(), &input);
}
