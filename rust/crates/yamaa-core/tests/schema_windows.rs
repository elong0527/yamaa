use yamaa_core::schema::{
    BundleLimits, Document, DocumentLimits, DocumentNode as N, ExpandedWindows,
    NormalizationBudget, NormalizationError, NormalizationLimits, NormalizationResource,
    SchemaContext, SchemaModule, SchemaStructure, WindowReference,
};

#[derive(Clone)]
enum V {
    Text(&'static str),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{List, Map, Text};

fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(text) => N::Text(text.into()),
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
fn descriptor(kind: &'static str) -> V {
    Map(vec![("type", Text(kind))])
}
fn field(name: &'static str, kind: &'static str) -> V {
    Map(vec![(name, descriptor(kind))])
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
                        field("schema_version", "str"),
                        field("windows", "dict[str, window_spec]"),
                        field("columns", "list[column_class]"),
                        field("metadata", "dict"),
                    ]),
                ),
                (
                    "column_class",
                    List(vec![
                        field("name", "str"),
                        field("derivation", "handled_expression_class"),
                    ]),
                ),
                (
                    "handled_expression_class",
                    List(vec![field("value", "expression")]),
                ),
                ("expression", Map(vec![("registry", Text("expressions"))])),
                (
                    "expressions",
                    Map(vec![(
                        "row_number",
                        List(vec![field("window", "window_selection")]),
                    )]),
                ),
                (
                    "window_selection",
                    Map(vec![("type", List(vec![Text("str"), Text("window_spec")]))]),
                ),
                (
                    "window_spec",
                    List(vec![
                        field("group_by", "list[str]"),
                        field("order_by", "list[str]"),
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
fn window() -> V {
    Map(vec![("order_by", List(vec![Text("SEQ")]))])
}
fn column(name: &'static str, selection: V) -> V {
    Map(vec![
        ("name", Text(name)),
        (
            "derivation",
            Map(vec![(
                "value",
                Map(vec![("row_number", Map(vec![("window", selection)]))]),
            )]),
        ),
    ])
}
fn input(columns: Vec<V>) -> V {
    Map(vec![
        ("windows", Map(vec![("VISITS", window())])),
        ("columns", List(columns)),
        ("metadata", Map(vec![("window", Text("UNKNOWN"))])),
    ])
}
fn selection(result: &ExpandedWindows, index: usize) -> usize {
    let d = &result.document;
    let N::Sequence(columns) = &d.nodes()[d.field(d.root(), "columns").unwrap()] else {
        panic!()
    };
    ["derivation", "value", "row_number", "window"]
        .into_iter()
        .fold(columns[index], |node, name| d.field(node, name).unwrap())
}

#[test]
fn expands_independent_copies_keeps_order_scope_and_definition_origins() {
    let input = document(input(vec![
        column("FIRST", Text("VISITS")),
        column("SECOND", Text("VISITS")),
    ]));
    let original = input.clone();
    let result = schema()
        .expand_named_windows(&input, true, &mut budget())
        .unwrap();
    let expected = document(Map(vec![
        (
            "columns",
            List(vec![column("FIRST", window()), column("SECOND", window())]),
        ),
        ("metadata", Map(vec![("window", Text("UNKNOWN"))])),
    ]));
    assert_eq!(result.document, expected);
    assert_eq!(input, original);
    assert_ne!(selection(&result, 0), selection(&result, 1));
    assert_eq!(result.origins.len(), result.document.nodes().len());
    for (index, name) in ["FIRST", "SECOND"].into_iter().enumerate() {
        let selected = selection(&result, index);
        assert_eq!(
            input.nodes()[result.origins[selected]],
            N::Text("VISITS".into())
        );
        let copied = result.document.field(selected, "order_by").unwrap();
        let definition = input
            .field(input.field(input.root(), "windows").unwrap(), "VISITS")
            .unwrap();
        assert_eq!(
            result.origins[copied],
            input.field(definition, "order_by").unwrap()
        );
        assert_eq!(
            result.references[index],
            WindowReference {
                path: format!("columns.{name}.derivation.value.row_number.window"),
                definition: "VISITS".into()
            }
        );
    }
}

#[test]
fn non_strict_expansion_precedes_pruning_and_strict_failure_names_surviving_use() {
    let input = document(input(vec![
        column("DEAD", Text("UNKNOWN")),
        column("LIVE", Text("VISITS")),
    ]));
    let schema = schema();
    let expanded = schema
        .expand_named_windows(&input, false, &mut budget())
        .unwrap();
    assert!(expanded
        .document
        .field(expanded.document.root(), "windows")
        .is_some());
    assert_eq!(
        expanded.document.nodes()[selection(&expanded, 0)],
        N::Text("UNKNOWN".into())
    );
    assert_eq!(expanded.references.len(), 1);
    let NormalizationError::Invalid(findings) = schema
        .expand_named_windows(&input, true, &mut budget())
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].condition, "unknown_window");
    assert_eq!(findings[0].requirement, Some("REQ-1253"));
    assert_eq!(
        findings[0].path,
        "columns.DEAD.derivation.row_number.window"
    );
    assert_eq!(
        findings[0].context,
        vec![("window", SchemaContext::Text("UNKNOWN".into()))]
    );
    // Independently authored post-pruning input: unknown references are absent.
    let alive = document(input_value_live());
    assert!(schema
        .expand_named_windows(&alive, true, &mut budget())
        .is_ok());
}
fn input_value_live() -> V {
    input(vec![column("LIVE", Text("VISITS"))])
}

#[test]
fn unused_malformed_definition_precedes_reference_errors() {
    let input = document(Map(vec![
        ("windows", Map(vec![("UNUSED", Text("VISITS"))])),
        ("columns", List(vec![column("A", Text("UNKNOWN"))])),
    ]));
    for strict in [false, true] {
        let NormalizationError::Invalid(findings) = schema()
            .expand_named_windows(&input, strict, &mut budget())
            .unwrap_err()
        else {
            panic!()
        };
        assert_eq!(findings[0].path, "windows.UNUSED");
        assert_eq!(findings[0].condition, "invalid_field_type");
    }
}

#[test]
fn strict_unknown_diagnostics_keep_column_declaration_order() {
    let input = document(Map(vec![(
        "columns",
        List(vec![
            column("Z", Text("SECOND")),
            column("A", Text("FIRST")),
        ]),
    )]));
    let NormalizationError::Invalid(findings) = schema()
        .expand_named_windows(&input, true, &mut budget())
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(
        findings.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        vec![
            "columns.Z.derivation.row_number.window",
            "columns.A.derivation.row_number.window"
        ]
    );
}

#[test]
fn repeated_expansion_charges_cumulative_storage_and_failed_calls_do_not_refund() {
    let schema = schema();
    let input = document(input_value_live());
    let mut limited = NormalizationBudget::new(NormalizationLimits {
        storage: DocumentLimits {
            nodes: 3,
            ..DocumentLimits::default()
        },
        ..NormalizationLimits::default()
    });
    assert_eq!(
        schema.expand_named_windows(&input, true, &mut limited),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Nodes,
            limit: 3
        })
    );
    assert_eq!(limited.storage_used().0, 3);
    assert!(schema
        .expand_named_windows(&input, true, &mut limited)
        .is_err());
    assert_eq!(limited.storage_used().0, 3);
    let mut shared = budget();
    let first = schema
        .expand_named_windows(&input, true, &mut shared)
        .unwrap();
    let used = shared.storage_used();
    let second = schema
        .expand_named_windows(&input, true, &mut shared)
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(shared.storage_used(), (used.0 * 2, used.1 * 2, used.2 * 2));
}
