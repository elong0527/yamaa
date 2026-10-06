use yamaa_core::schema::{
    BundleLimits, Document, DocumentLimits, DocumentNode as N, NormalizationBudget,
    NormalizationError, NormalizationLimits, NormalizationResource, SchemaContext as C,
    SchemaDiagnostic, SchemaModule, SchemaSource, SchemaStructure, ValidationError,
};

#[derive(Clone)]
enum V {
    Text(&'static str),
    Int(&'static str),
    Bool(bool),
    Null,
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Bool, Int, List, Map, Null, Text};

fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(s) => N::Text(s.into()),
            Int(s) => N::Integer(s.into()),
            Bool(b) => N::Boolean(b),
            Null => N::Null,
            List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(Text(k), nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn field(name: &'static str, kind: &'static str, required: bool) -> V {
    Map(vec![(
        name,
        Map(vec![("type", Text(kind)), ("required", Bool(required))]),
    )])
}
fn schema() -> SchemaStructure {
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", Text("1.0")),
                (
                    "root_class",
                    List(vec![
                        field("schema_version", "str", true),
                        field("parents", "list[str]", false),
                        field("input", "dict[str, dataset_class]", false),
                        field("columns", "list[column_class]", false),
                        field("rows", "list[row_class]", false),
                        field("intermediates", "list[intermediate_class]", false),
                        field("settings", "options_class", false),
                        field("output", "str", true),
                    ]),
                ),
                ("identifier", Map(vec![("type", Text("str"))])),
                ("project_path", Map(vec![("type", Text("str"))])),
                ("column_type", Map(vec![("type", Text("str"))])),
                (
                    "column_class",
                    List(vec![
                        field("name", "str", true),
                        field("type", "column_type", true),
                        field("label", "str", false),
                        field("options", "options_class", false),
                    ]),
                ),
                (
                    "dataset_class",
                    List(vec![
                        field("path", "str", true),
                        field("options", "options_class", false),
                    ]),
                ),
                (
                    "row_class",
                    List(vec![
                        field("id", "str", true),
                        field("options", "options_class", false),
                    ]),
                ),
                (
                    "intermediate_class",
                    List(vec![
                        field("id", "str", true),
                        field("options", "options_class", false),
                    ]),
                ),
                (
                    "options_class",
                    List(vec![
                        field("needed", "str", true),
                        Map(vec![(
                            "defaulted",
                            Map(vec![("type", Text("str")), ("default", Text("DEFAULT"))]),
                        )]),
                    ]),
                ),
            ])),
        }],
        0,
        "root_class",
        BundleLimits::default(),
    )
    .unwrap()
}
fn budget() -> NormalizationBudget {
    NormalizationBudget::new(NormalizationLimits::default())
}
fn layer(mut fields: Vec<(&'static str, V)>) -> Document {
    fields.insert(0, ("schema_version", Text("1.0")));
    document(Map(fields))
}
fn diagnostics(input: Document) -> Vec<SchemaDiagnostic> {
    match schema().normalize_layer(&input, &mut budget()) {
        Err(NormalizationError::Invalid(findings)) => findings,
        other => panic!("expected ordered findings, got {other:?}"),
    }
}
fn finding(
    path: &str,
    condition: &'static str,
    requirement: &'static str,
    context: Vec<(&'static str, C)>,
) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: path.into(),
        condition,
        requirement: Some(requirement),
        context,
    }
}
fn text(value: &str) -> C {
    C::Text(value.into())
}
fn paths(findings: &[SchemaDiagnostic]) -> Vec<(&str, &str)> {
    findings
        .iter()
        .map(|d| (d.path.as_str(), d.condition))
        .collect()
}
fn get(doc: &Document, node: usize, name: &str) -> usize {
    let N::Mapping(entries) = &doc.nodes()[node] else {
        panic!("expected mapping")
    };
    entries
        .iter()
        .find_map(|&(k, v)| (doc.nodes()[k] == N::Text(name.into())).then_some(v))
        .unwrap()
}

// Arena allocation order is private; ordered keys, values and list members are not.
fn assert_document(actual: &Document, expected: &Document) {
    fn same(a: &Document, x: usize, b: &Document, y: usize) {
        match (&a.nodes()[x], &b.nodes()[y]) {
            (N::Mapping(xs), N::Mapping(ys)) => {
                assert_eq!(xs.len(), ys.len());
                for (&(ak, av), &(bk, bv)) in xs.iter().zip(ys) {
                    same(a, ak, b, bk);
                    same(a, av, b, bv);
                }
            }
            (N::Sequence(xs), N::Sequence(ys)) => {
                assert_eq!(xs.len(), ys.len());
                for (&x, &y) in xs.iter().zip(ys) {
                    same(a, x, b, y);
                }
            }
            (x, y) => assert_eq!(x, y),
        }
    }
    same(actual, actual.root(), expected, expected.root());
}

#[test]
fn collects_all_field_errors_in_schema_order_before_normalization() {
    let findings = diagnostics(layer(vec![(
        "columns",
        List(vec![Map(vec![
            ("label", Int("2")),
            ("name", Text("X")),
            ("type", Int("1")),
        ])]),
    )]));
    assert_eq!(findings.len(), 2);
    assert_eq!(
        paths(&findings),
        [
            ("columns.X.type", "invalid_field_type"),
            ("columns.X.label", "invalid_field_type")
        ]
    );
    assert_eq!(
        findings[0].context,
        vec![("expected", text("column_type")), ("actual", text("int"))]
    );
    assert_eq!(
        findings[1].context,
        vec![("expected", text("str")), ("actual", text("int"))]
    );
}

#[test]
fn version_unknown_identity_and_duplicate_errors_keep_phase_order() {
    let findings = diagnostics(document(Map(vec![
        ("unknown", Null),
        (
            "columns",
            List(vec![
                Map(vec![("extra", Null)]),
                Map(vec![("name", Text("X"))]),
                Map(vec![("name", Text("X")), ("label", Int("2"))]),
            ]),
        ),
    ])));
    assert_eq!(
        paths(&findings),
        [
            ("schema_version", "schema_version_mismatch"),
            ("unknown", "unknown_field"),
            ("columns.0.name", "missing_required_field"),
            ("columns.0.extra", "unknown_field"),
            ("columns.X.label", "invalid_field_type"),
            ("columns.X.name", "duplicate_identifier"),
        ]
    );
    assert_eq!(
        findings[0],
        finding(
            "schema_version",
            "schema_version_mismatch",
            "REQ-0656",
            vec![("expected", text("1.0")), ("actual", C::Null)]
        )
    );
    assert_eq!(
        findings[5],
        finding(
            "columns.X.name",
            "duplicate_identifier",
            "REQ-0659",
            vec![("identifier", text("X"))]
        )
    );
}

#[test]
fn layer_shape_errors_use_python_type_labels() {
    for (input, path, expected, actual) in [
        (document(Map(vec![])), "$", "root_class", "dict"),
        (document(Null), "$", "root_class", "NoneType"),
        (
            layer(vec![("input", List(vec![]))]),
            "input",
            "dict",
            "list",
        ),
        (
            layer(vec![("columns", Map(vec![]))]),
            "columns",
            "list",
            "dict",
        ),
        (
            layer(vec![("columns", List(vec![Int("7")]))]),
            "columns.0",
            "column_class",
            "int",
        ),
    ] {
        assert_eq!(
            diagnostics(input),
            [finding(
                path,
                "invalid_field_type",
                "REQ-0658",
                vec![("expected", text(expected)), ("actual", text(actual))]
            )]
        );
    }
}

#[test]
fn required_and_identity_nulls_are_invalid_clears() {
    let findings = diagnostics(layer(vec![
        (
            "columns",
            List(vec![Map(vec![
                ("name", Null),
                ("type", Null),
                ("label", Null),
            ])]),
        ),
        ("output", Null),
    ]));
    assert_eq!(
        findings,
        [
            finding(
                "columns.0.name",
                "invalid_clear",
                "REQ-0660",
                vec![("field", text("name"))]
            ),
            finding(
                "columns.0.type",
                "invalid_clear",
                "REQ-0660",
                vec![("field", text("type"))]
            ),
            finding(
                "output",
                "invalid_clear",
                "REQ-0660",
                vec![("field", text("output"))]
            ),
        ]
    );
}

#[test]
fn column_fragments_defer_nested_requiredness_but_other_supplied_values_do_not() {
    let findings = diagnostics(layer(vec![
        (
            "input",
            Map(vec![("SRC", Map(vec![("options", Map(vec![]))]))]),
        ),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("X")),
                ("options", Map(vec![])),
            ])]),
        ),
        (
            "rows",
            List(vec![Map(vec![("id", Text("R")), ("options", Map(vec![]))])]),
        ),
        (
            "intermediates",
            List(vec![Map(vec![("id", Text("I")), ("options", Map(vec![]))])]),
        ),
        ("settings", Map(vec![])),
    ]));
    assert_eq!(
        paths(&findings),
        [
            ("input.SRC.options.needed", "missing_required_field"),
            ("rows.R.options.needed", "missing_required_field"),
            ("intermediates.I.options.needed", "missing_required_field"),
            ("settings.needed", "missing_required_field"),
        ]
    );
}

#[test]
fn normalization_preserves_clears_defers_column_defaults_and_owns_shorthand() {
    let normalized = {
        let input = layer(vec![
            (
                "rows",
                List(vec![Map(vec![
                    ("id", Text("R")),
                    ("options", Map(vec![("needed", Text("yes"))])),
                ])]),
            ),
            (
                "columns",
                List(vec![Map(vec![
                    ("name", Text("X")),
                    ("label", Null),
                    ("options", Map(vec![])),
                ])]),
            ),
            (
                "input",
                Map(vec![("SRC", Text("data.csv")), ("EMPTY", Map(vec![]))]),
            ),
        ]);
        schema().normalize_layer(&input, &mut budget()).unwrap()
    };
    let expected = layer(vec![
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("data.csv"))])),
                ("EMPTY", Map(vec![])),
            ]),
        ),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("X")),
                ("label", Null),
                ("options", Map(vec![])),
            ])]),
        ),
        (
            "rows",
            List(vec![Map(vec![
                ("id", Text("R")),
                (
                    "options",
                    Map(vec![
                        ("needed", Text("yes")),
                        ("defaulted", Text("DEFAULT")),
                    ]),
                ),
            ])]),
        ),
    ]);
    assert_document(&normalized.document, &expected);
    assert_eq!(normalized.origins.len(), expected.nodes().len());
    assert!(normalized
        .origins
        .iter()
        .any(|o| matches!(o.source, SchemaSource::Default { .. })));
}

#[test]
fn generated_input_path_and_container_point_to_original_shorthand() {
    let input = layer(vec![("input", Map(vec![("SRC", Text("data.csv"))]))]);
    let source = get(&input, get(&input, input.root(), "input"), "SRC");
    let output = schema().normalize_layer(&input, &mut budget()).unwrap();
    let doc = &output.document;
    let member = get(doc, get(doc, doc.root(), "input"), "SRC");
    let N::Mapping(entries) = &doc.nodes()[member] else {
        panic!("expected mapping")
    };
    for node in [member, entries[0].0, entries[0].1] {
        let origin = output.origins[node];
        assert_eq!(origin.source, SchemaSource::Input);
        assert_eq!(origin.node, source);
        assert_eq!(origin.generated, node != entries[0].1);
    }
}

#[test]
fn invalid_attempts_retain_diagnostic_budget_but_fresh_attempts_succeed() {
    let schema = schema();
    let input = layer(vec![("output", Int("2"))]);
    let mut limits = NormalizationLimits::default();
    limits.validation.diagnostics = 1;
    let mut shared = NormalizationBudget::new(limits);
    assert!(matches!(
        schema.normalize_layer(&input, &mut shared),
        Err(NormalizationError::Invalid(_))
    ));
    assert_eq!(shared.storage_used(), (0, 0, 0));
    assert!(shared.work_used() > 0);
    assert_eq!(
        schema.normalize_layer(&input, &mut shared),
        Err(NormalizationError::Validation(
            ValidationError::Diagnostics { limit: 1 }
        ))
    );
    assert!(matches!(
        schema.normalize_layer(&input, &mut NormalizationBudget::new(limits)),
        Err(NormalizationError::Invalid(_))
    ));
}

#[test]
fn storage_and_depth_limits_prevent_publication_and_do_not_poison_fresh_requests() {
    let schema = schema();
    let input = layer(vec![(
        "columns",
        List(vec![Map(vec![("name", Text("X"))])]),
    )]);
    let mut limits = NormalizationLimits::default();
    limits.storage.nodes = 0;
    assert_eq!(
        schema.normalize_layer(&input, &mut NormalizationBudget::new(limits)),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Nodes,
            limit: 0
        })
    );
    limits = NormalizationLimits::default();
    limits.depth = 1;
    assert_eq!(
        schema.normalize_layer(&input, &mut NormalizationBudget::new(limits)),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Depth,
            limit: 1
        })
    );
    assert!(schema.normalize_layer(&input, &mut budget()).is_ok());
}

#[test]
fn empty_collections_and_input_shorthand_do_not_require_unused_member_classes() {
    let schema = SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", Text("1.0")),
                (
                    "root_class",
                    List(vec![
                        field("schema_version", "str", true),
                        field("columns", "list", false),
                        field("input", "dict", false),
                    ]),
                ),
                ("identifier", Map(vec![("type", Text("str"))])),
                ("project_path", Map(vec![("type", Text("str"))])),
            ])),
        }],
        0,
        "root_class",
        BundleLimits::default(),
    )
    .unwrap();
    let input = layer(vec![
        ("columns", List(vec![])),
        ("input", Map(vec![("SRC", Text("data.csv"))])),
    ]);
    let output = schema.normalize_layer(&input, &mut budget()).unwrap();
    assert_document(
        &output.document,
        &layer(vec![
            ("columns", List(vec![])),
            (
                "input",
                Map(vec![("SRC", Map(vec![("path", Text("data.csv"))]))]),
            ),
        ]),
    );
}
