use yamaa_core::schema::{
    BundleLimits, Document, DocumentLimits, DocumentNode as N, NormalizationBudget,
    NormalizationError, NormalizationLimits, NormalizationResource, SchemaModule, SchemaStructure,
};

#[derive(Clone)]
enum V {
    Text(&'static str),
    Null,
    Bool(bool),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Bool, List, Map, Null, Text};
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(s) => N::Text(s.into()),
            Null => N::Null,
            Bool(value) => N::Boolean(value),
            List(items) => N::Sequence(items.into_iter().map(|x| push(x, nodes)).collect()),
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
fn field(name: &'static str, kind: &'static str) -> V {
    Map(vec![(name, Map(vec![("type", Text(kind))]))])
}
fn required(name: &'static str, kind: &'static str) -> V {
    Map(vec![(
        name,
        Map(vec![("type", Text(kind)), ("required", Bool(true))]),
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
                        field("value", "patch"),
                        required("schema_version", "str"),
                        field("parents", "list[str]"),
                        field("input", "dict[str, dataset_class]"),
                        field("columns", "list[column_class]"),
                        field("rows", "list[row_class]"),
                        field("intermediates", "list[intermediate_class]"),
                        field("windows", "dict[str, window_class]"),
                        field("metadata", "dict[str, str]"),
                        field("keys", "list[str]"),
                        required("output", "dict"),
                    ]),
                ),
                (
                    "dataset_class",
                    List(vec![field("path", "str"), field("types", "dict[str, str]")]),
                ),
                (
                    "column_class",
                    List(vec![
                        required("name", "str"),
                        required("type", "str"),
                        field("label", "str"),
                        field("mandatory", "required_options"),
                        field("options", "options_class"),
                        field("metadata", "dict[str, str]"),
                        field("items", "list[str]"),
                    ]),
                ),
                (
                    "row_class",
                    List(vec![
                        required("id", "str"),
                        field("options", "options_class"),
                    ]),
                ),
                (
                    "intermediate_class",
                    List(vec![
                        required("id", "str"),
                        field("options", "options_class"),
                    ]),
                ),
                (
                    "patch",
                    List(vec![
                        field("label", "str"),
                        field("metadata", "dict[str, str]"),
                        field("options", "options_class"),
                        field("items", "list[str]"),
                        field("key", "match_key"),
                        field("expression", "expression"),
                        field("window", "window_selection"),
                    ]),
                ),
                (
                    "options_class",
                    List(vec![
                        field("left", "str"),
                        field("right", "str"),
                        Map(vec![(
                            "defaulted",
                            Map(vec![("type", Text("str")), ("default", Text("DEFAULT"))]),
                        )]),
                    ]),
                ),
                ("required_options", List(vec![required("one", "str")])),
                ("match_key", Map(vec![("type", Text("dict[str, str]"))])),
                ("expression", Map(vec![("registry", Text("operations"))])),
                (
                    "operations",
                    Map(vec![
                        (
                            "select",
                            List(vec![field("left", "str"), field("right", "str")]),
                        ),
                        (
                            "literal",
                            Map(vec![("type", List(vec![Text("str"), Text("null")]))]),
                        ),
                    ]),
                ),
                (
                    "window_selection",
                    Map(vec![(
                        "type",
                        List(vec![Text("str"), Text("window_class")]),
                    )]),
                ),
                (
                    "window_class",
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
fn check(actual: &Document, expected: V) {
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
    let expected = document(expected);
    same(actual, actual.root(), &expected, expected.root());
}
fn compose(old: V, new: V) -> yamaa_core::schema::ComposedValue {
    let s = schema();
    s.compose_descriptor(
        s.root_class().fields[0].descriptor,
        &document(old),
        &document(new),
        &mut budget(),
    )
    .unwrap()
}

#[test]
fn recursive_patches_keep_positions_replace_lists_and_match_keys_and_defer_defaults() {
    let value = compose(
        Map(vec![
            ("label", Text("parent")),
            ("metadata", Map(vec![("a", Text("A")), ("b", Text("B"))])),
            ("options", Map(vec![("left", Text("L"))])),
            ("items", List(vec![Text("one"), Text("two")])),
            ("key", Map(vec![("a", Text("A")), ("b", Text("B"))])),
        ]),
        Map(vec![
            ("options", Map(vec![("right", Text("R"))])),
            ("metadata", Map(vec![("a", Text("new")), ("c", Text("C"))])),
            ("items", List(vec![Text("three")])),
            ("key", Map(vec![("b", Text("new B"))])),
        ]),
    );
    check(
        &value.document,
        Map(vec![
            ("label", Text("parent")),
            (
                "metadata",
                Map(vec![("a", Text("new")), ("b", Text("B")), ("c", Text("C"))]),
            ),
            (
                "options",
                Map(vec![("left", Text("L")), ("right", Text("R"))]),
            ),
            ("items", List(vec![Text("three")])),
            ("key", Map(vec![("b", Text("new B"))])),
        ]),
    );
}
#[test]
fn matching_registry_payloads_compose_but_different_keywords_replace() {
    let inherited = Map(vec![(
        "expression",
        Map(vec![("select", Map(vec![("left", Text("L"))]))]),
    )]);
    let value = compose(
        inherited.clone(),
        Map(vec![(
            "expression",
            Map(vec![("select", Map(vec![("right", Text("R"))]))]),
        )]),
    );
    check(
        &value.document,
        Map(vec![(
            "expression",
            Map(vec![(
                "select",
                Map(vec![("left", Text("L")), ("right", Text("R"))]),
            )]),
        )]),
    );
    let value = compose(
        inherited,
        Map(vec![("expression", Map(vec![("literal", Null)]))]),
    );
    check(
        &value.document,
        Map(vec![("expression", Map(vec![("literal", Null)]))]),
    );
}
#[test]
fn named_inline_window_transitions_replace_whole() {
    let inline = Map(vec![
        ("group_by", List(vec![Text("ID")])),
        ("order_by", List(vec![Text("SEQ")])),
    ]);
    let value = compose(
        Map(vec![("window", inline.clone())]),
        Map(vec![("window", Text("W"))]),
    );
    check(&value.document, Map(vec![("window", Text("W"))]));
    let value = compose(
        Map(vec![("window", Text("W"))]),
        Map(vec![("window", inline.clone())]),
    );
    check(&value.document, Map(vec![("window", inline)]));
}
#[test]
fn retained_and_replaced_leaves_point_to_their_original_occurrences() {
    let old = document(Map(vec![(
        "options",
        Map(vec![("left", Text("L")), ("right", Text("old"))]),
    )]));
    let new = document(Map(vec![("options", Map(vec![("right", Text("new"))]))]));
    let originals = [old.clone(), new.clone()];
    let s = schema();
    let value = s
        .compose_descriptor(
            s.root_class().fields[0].descriptor,
            &old,
            &new,
            &mut budget(),
        )
        .unwrap();
    assert_eq!(old, originals[0]);
    assert_eq!(new, originals[1]);
    assert_eq!(value.origins.len(), value.document.nodes().len());
    let options = value
        .document
        .field(value.document.root(), "options")
        .unwrap();
    for (name, input) in [("left", 0), ("right", 1)] {
        let result = value.document.field(options, name).unwrap();
        let origin = value.origins[result];
        assert_eq!(origin.input, input);
        let source = &originals[input];
        let parent = source.field(source.root(), "options").unwrap();
        assert_eq!(origin.node, source.field(parent, name).unwrap());
        assert_eq!(value.document.nodes()[result], source.nodes()[origin.node]);
    }
}
#[test]
fn cumulative_storage_and_depth_limits_refuse_without_refunding_prior_work() {
    let old = document(Map(vec![("label", Text("old"))]));
    let new = document(Map(vec![("label", Text("new"))]));
    let s = schema();
    let id = s.root_class().fields[0].descriptor;
    let mut limits = NormalizationLimits::default();
    limits.storage.nodes = 3;
    let mut shared = NormalizationBudget::new(limits);
    s.compose_descriptor(id, &old, &new, &mut shared).unwrap();
    let charged = shared.storage_used();
    assert!(matches!(
        s.compose_descriptor(id, &old, &new, &mut shared),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Nodes,
            ..
        })
    ));
    assert!(shared.storage_used().0 >= charged.0);
    s.compose_descriptor(id, &old, &new, &mut NormalizationBudget::new(limits))
        .unwrap();
    limits.depth = 0;
    assert!(matches!(
        s.compose_descriptor(id, &old, &new, &mut NormalizationBudget::new(limits)),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Depth,
            ..
        })
    ));
}

#[test]
fn layers_keep_keyed_order_replace_ordinary_fields_and_track_written_provenance() {
    let layers = vec![
        document(Map(vec![
            ("schema_version", Text("1.0")),
            ("parents", List(vec![Text("parent.yaml")])),
            ("keys", List(vec![Text("A")])),
            ("metadata", Map(vec![("old", Text("old"))])),
            (
                "input",
                Map(vec![(
                    "DS",
                    Map(vec![
                        ("path", Text("parent.csv")),
                        ("types", Map(vec![("A", Text("int"))])),
                    ]),
                )]),
            ),
            (
                "columns",
                List(vec![
                    Map(vec![
                        ("name", Text("A")),
                        ("options", Map(vec![("left", Text("L"))])),
                    ]),
                    Map(vec![("name", Text("B")), ("label", Text("before"))]),
                ]),
            ),
            (
                "windows",
                Map(vec![
                    (
                        "W",
                        Map(vec![
                            ("group_by", List(vec![Text("ID")])),
                            ("order_by", List(vec![Text("SEQ")])),
                        ]),
                    ),
                    ("KEEP", Map(vec![("order_by", List(vec![Text("K")]))])),
                ]),
            ),
            (
                "rows",
                List(vec![Map(vec![
                    ("id", Text("R")),
                    ("options", Map(vec![("left", Text("old"))])),
                ])]),
            ),
        ])),
        document(Map(vec![
            ("metadata", Map(vec![("new", Text("new"))])),
            (
                "input",
                Map(vec![(
                    "DS",
                    Map(vec![("types", Map(vec![("B", Text("str"))]))]),
                )]),
            ),
            (
                "columns",
                List(vec![
                    Map(vec![("name", Text("B")), ("label", Text("after"))]),
                    Map(vec![
                        ("name", Text("A")),
                        ("options", Map(vec![("right", Text("R"))])),
                    ]),
                ]),
            ),
            (
                "windows",
                Map(vec![(
                    "W",
                    Map(vec![("order_by", List(vec![Text("NEW")]))]),
                )]),
            ),
            (
                "rows",
                List(vec![Map(vec![
                    ("id", Text("R")),
                    ("options", Map(vec![("right", Text("only"))])),
                ])]),
            ),
        ])),
        document(Map(vec![
            ("keys", Null),
            (
                "columns",
                List(vec![Map(vec![("name", Text("C")), ("label", Text("new"))])]),
            ),
        ])),
    ];
    let original = layers.clone();
    let result = schema()
        .compose_layer_fragments(&layers, &mut budget())
        .unwrap();
    assert!(result.diagnostics.is_empty());
    assert_eq!(layers, original);
    check(
        &result.document,
        Map(vec![
            ("schema_version", Text("1.0")),
            ("metadata", Map(vec![("new", Text("new"))])),
            (
                "input",
                Map(vec![(
                    "DS",
                    Map(vec![
                        ("path", Text("parent.csv")),
                        ("types", Map(vec![("B", Text("str"))])),
                    ]),
                )]),
            ),
            (
                "columns",
                List(vec![
                    Map(vec![
                        ("name", Text("A")),
                        (
                            "options",
                            Map(vec![("left", Text("L")), ("right", Text("R"))]),
                        ),
                    ]),
                    Map(vec![("name", Text("B")), ("label", Text("after"))]),
                    Map(vec![("name", Text("C")), ("label", Text("new"))]),
                ]),
            ),
            (
                "windows",
                Map(vec![
                    ("W", Map(vec![("order_by", List(vec![Text("NEW")]))])),
                    ("KEEP", Map(vec![("order_by", List(vec![Text("K")]))])),
                ]),
            ),
            (
                "rows",
                List(vec![Map(vec![
                    ("id", Text("R")),
                    ("options", Map(vec![("right", Text("only"))])),
                ])]),
            ),
        ]),
    );
    for (path, layer) in [
        ("columns.A", 0),
        ("columns.A.name", 1),
        ("columns.A.options.left", 0),
        ("columns.A.options.right", 1),
        ("columns.B.label", 1),
        ("columns.C", 2),
        ("windows.W", 1),
        ("windows.KEEP", 0),
        ("input.DS.path", 0),
        ("input.DS.types", 1),
        ("metadata", 1),
    ] {
        assert_eq!(
            result
                .provenance
                .iter()
                .find(|p| p.path == path)
                .unwrap()
                .layer,
            layer,
            "{path}"
        );
    }
    assert!(!result.provenance.iter().any(|p| p.path == "keys"
        || p.path == "metadata.new"
        || p.path.starts_with("windows.W.group_by")));
}

#[test]
fn missing_and_required_clears_fail_in_contribution_order_even_for_new_members() {
    let layers = vec![document(Map(vec![
        ("metadata", Null),
        (
            "columns",
            List(vec![Map(vec![("name", Text("NEW")), ("label", Null)])]),
        ),
        ("keys", Null),
        ("output", Null),
    ]))];
    let result = schema()
        .compose_layer_fragments(&layers, &mut budget())
        .unwrap();
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| (d.path.as_str(), d.condition, d.requirement))
            .collect::<Vec<_>>(),
        vec![
            ("metadata", "invalid_clear", Some("REQ-0660")),
            ("columns.NEW.label", "invalid_clear", Some("REQ-0660")),
            ("keys", "invalid_clear", Some("REQ-0660")),
            ("output", "invalid_clear", Some("REQ-0660")),
        ]
    );
    check(
        &result.document,
        Map(vec![(
            "columns",
            List(vec![Map(vec![("name", Text("NEW"))])]),
        )]),
    );
}

#[test]
fn malformed_collection_shapes_and_duplicate_identifiers_are_refused() {
    let schema = schema();
    for value in [
        Map(vec![("columns", Text("not a sequence"))]),
        Map(vec![("input", Map(vec![("DS", Text("not normalized"))]))]),
        Map(vec![(
            "columns",
            List(vec![
                Map(vec![("name", Text("A"))]),
                Map(vec![("name", Text("A"))]),
            ]),
        )]),
        Map(vec![(
            "columns",
            List(vec![Map(vec![("label", Text("no identity"))])]),
        )]),
    ] {
        assert!(schema
            .compose_layer_fragments(&[document(value)], &mut budget())
            .is_err());
    }
}

#[test]
fn layer_requests_share_storage_and_a_fresh_request_recovers() {
    let layers = [document(Map(vec![("schema_version", Text("1.0"))]))];
    let schema = schema();
    let mut limits = NormalizationLimits::default();
    limits.storage.nodes = 4;
    let mut shared = NormalizationBudget::new(limits);
    schema
        .compose_layer_fragments(&layers, &mut shared)
        .unwrap();
    assert!(matches!(
        schema.compose_layer_fragments(&layers, &mut shared),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Nodes,
            ..
        })
    ));
    schema
        .compose_layer_fragments(&layers, &mut NormalizationBudget::new(limits))
        .unwrap();
}

#[test]
fn complete_composition_materializes_defaults_once_and_keeps_written_provenance() {
    let layers = vec![
        document(Map(vec![(
            "columns",
            List(vec![Map(vec![
                ("name", Text("A")),
                ("options", Map(vec![("defaulted", Text("AUTHORED"))])),
            ])]),
        )])),
        document(Map(vec![(
            "columns",
            List(vec![
                Map(vec![
                    ("name", Text("A")),
                    ("options", Map(vec![("left", Text("L"))])),
                ]),
                Map(vec![("name", Text("B")), ("options", Map(vec![]))]),
            ]),
        )])),
        document(Map(vec![(
            "columns",
            List(vec![Map(vec![
                ("name", Text("A")),
                ("options", Map(vec![("right", Text("R"))])),
            ])]),
        )])),
    ];
    let result = schema().compose_layers(&layers, &mut budget()).unwrap();
    assert!(result.diagnostics.is_empty());
    check(
        &result.document,
        Map(vec![(
            "columns",
            List(vec![
                Map(vec![
                    ("name", Text("A")),
                    (
                        "options",
                        Map(vec![
                            ("left", Text("L")),
                            ("right", Text("R")),
                            ("defaulted", Text("AUTHORED")),
                        ]),
                    ),
                ]),
                Map(vec![
                    ("name", Text("B")),
                    ("options", Map(vec![("defaulted", Text("DEFAULT"))])),
                ]),
            ]),
        )]),
    );
    assert_eq!(
        result
            .provenance
            .iter()
            .find(|p| p.path == "columns.A.options.defaulted")
            .unwrap()
            .layer,
        0
    );
    assert!(!result
        .provenance
        .iter()
        .any(|p| p.path == "columns.B.options.defaulted"));
}

#[test]
fn materialization_failure_precedes_collected_clear_diagnostics_without_partial_output() {
    let layers = [document(Map(vec![
        ("metadata", Null),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("A")),
                ("mandatory", Map(vec![])),
            ])]),
        ),
    ]))];
    let Err(yamaa_core::schema::LayerCompositionError {
        error: NormalizationError::Invalid(diagnostics),
        ..
    }) = schema().compose_layers(&layers, &mut budget())
    else {
        panic!("expected normalization failure")
    };
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].condition, "missing_required_field");
    assert_eq!(diagnostics[0].path, "<normalization>.one");
    let layers = [document(Map(vec![("metadata", Null)]))];
    let Err(yamaa_core::schema::LayerCompositionError {
        error: NormalizationError::Invalid(diagnostics),
        ..
    }) = schema().compose_layers(&layers, &mut budget())
    else {
        panic!("expected clear failure")
    };
    assert_eq!(diagnostics[0].condition, "invalid_clear");
    assert_eq!(diagnostics[0].path, "metadata");
}
