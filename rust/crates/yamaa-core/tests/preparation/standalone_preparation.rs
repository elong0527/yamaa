//! Independent logical documents exercise the complete standalone application.
use yamaa_core::schema::{
    Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
    SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure, WindowReference,
};
use yamaa_core::standalone_preparation::{prepare, Error};

#[derive(Clone, Debug, PartialEq)]
enum V {
    Null,
    Int(&'static str),
    Text(&'static str),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Int, List, Map, Null, Text};
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Null => N::Null,
            Int(value) => N::Integer(value.into()),
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
    Document::new(nodes, root).unwrap()
}
#[derive(Debug, PartialEq)]
enum Logical {
    Null,
    Int(String),
    Text(String),
    List(Vec<Logical>),
    Map(Vec<(String, Logical)>),
}
fn logical(document: &Document, node: usize) -> Logical {
    match &document.nodes()[node] {
        N::Null => Logical::Null,
        N::Integer(text) => Logical::Int(text.clone()),
        N::Text(text) => Logical::Text(text.clone()),
        N::Sequence(items) => {
            Logical::List(items.iter().map(|&id| logical(document, id)).collect())
        }
        N::Mapping(items) => Logical::Map(
            items
                .iter()
                .map(|&(k, v)| {
                    let N::Text(key) = &document.nodes()[k] else {
                        panic!("text fixture keys")
                    };
                    (key.clone(), logical(document, v))
                })
                .collect(),
        ),
        _ => panic!("fixture scalar"),
    }
}
fn field(name: &'static str, kind: &'static str) -> V {
    Map(vec![(name, Map(vec![("type", Text(kind))]))])
}
fn schema(default_domain: bool, loose_domain: bool) -> SchemaStructure {
    let mut domain = vec![(
        "type",
        if loose_domain {
            List(vec![Text("str"), Text("int")])
        } else {
            Text("str")
        },
    )];
    if default_domain {
        domain.push(("default", Text("DEFAULT")));
    }
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", Text("1.0")),
                (
                    "root",
                    List(vec![
                        field("schema_version", "str"),
                        Map(vec![("domain", Map(domain))]),
                        field("input", "dict"),
                        field("keys", "list[str]"),
                        field("output", "dict"),
                        field("columns", "list[column_class]"),
                        field("windows", "dict[str, window_spec]"),
                    ]),
                ),
                (
                    "column_class",
                    List(vec![
                        field("name", "str"),
                        field("type", "str"),
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
                        field("order_by", "list[order_term]"),
                    ]),
                ),
                (
                    "order_term",
                    List(vec![
                        field("variable", "str"),
                        field("direction", "str"),
                        field("nulls", "str"),
                    ]),
                ),
            ])),
        }],
        0,
        "root",
    )
    .unwrap()
}
fn basic_fields() -> Vec<(&'static str, V)> {
    vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("X")),
        ("input", Map(vec![])),
        ("keys", List(vec![])),
        (
            "output",
            Map(vec![
                ("path", Text("output.csv")),
                ("columns", List(vec![])),
            ]),
        ),
        ("columns", List(vec![])),
    ]
}

fn column(name: &'static str, window: V) -> V {
    Map(vec![
        ("name", Text(name)),
        ("type", Text("int")),
        (
            "derivation",
            Map(vec![(
                "value",
                Map(vec![("row_number", Map(vec![("window", window)]))]),
            )]),
        ),
    ])
}
fn window() -> V {
    Map(vec![(
        "order_by",
        List(vec![Map(vec![("variable", Text("SEQ"))])]),
    )])
}
fn field_id(document: &Document, path: &[&str]) -> usize {
    path.iter().fold(document.root(), |id, name| {
        document.field(id, name).unwrap()
    })
}
fn item(document: &Document, sequence: usize, index: usize) -> usize {
    let N::Sequence(items) = &document.nodes()[sequence] else {
        panic!()
    };
    items[index]
}
fn window_id(document: &Document, index: usize) -> usize {
    ["derivation", "value", "row_number", "window"]
        .into_iter()
        .fold(
            item(document, field_id(document, &["columns"]), index),
            |id, name| document.field(id, name).unwrap(),
        )
}
fn unknown(name: &str, window: &str) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: format!("columns.{name}.derivation.row_number.window"),
        code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownWindow,
        context: vec![("window", C::Text(window.into()))],
    }
}

#[test]
fn success_moves_raw_arena_and_retains_complete_default_and_window_provenance() {
    let mut fields = basic_fields();
    fields.remove(1);
    fields.last_mut().unwrap().1 = List(vec![
        column("Z", Text("VISITS")),
        column("A", Text("VISITS")),
    ]);
    fields.push(("windows", Map(vec![("VISITS", window())])));
    let raw = document(Map(fields));
    let raw_pointer = raw.nodes().as_ptr();
    let raw_expected = raw.clone();
    let schema = schema(true, false);
    let prepared = prepare(&schema, raw).unwrap();
    assert_eq!(prepared.raw().nodes().as_ptr(), raw_pointer);
    assert_eq!(prepared.raw(), &raw_expected);
    let mut expected = basic_fields();
    expected[1].1 = Text("DEFAULT");
    expected.last_mut().unwrap().1 = List(vec![column("Z", window()), column("A", window())]);
    let model = prepared.model().document();
    let expected = document(Map(expected));
    assert_eq!(
        logical(model, model.root()),
        logical(&expected, expected.root())
    );
    assert_eq!(
        prepared.windows(),
        &[
            WindowReference {
                path: "columns.Z.derivation.value.row_number.window".into(),
                definition: "VISITS".into()
            },
            WindowReference {
                path: "columns.A.derivation.value.row_number.window".into(),
                definition: "VISITS".into()
            },
        ]
    );
    assert_eq!(prepared.origins().len(), model.nodes().len());
    let descriptor = schema
        .class_named("root")
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == "domain")
        .unwrap()
        .descriptor;
    let located = &schema.descriptors()[descriptor];
    let default = located.descriptor.default_node().unwrap();
    let default_origin = SchemaOrigin {
        source: SchemaSource::Default {
            module: 0,
            descriptor,
        },
        node: default,
        generated: false,
    };
    let domain = model.field(model.root(), "domain").unwrap();
    assert_eq!(prepared.origins()[domain], default_origin);
    let N::Mapping(entries) = &model.nodes()[model.root()] else {
        panic!()
    };
    let domain_key = entries.iter().find(|(_, v)| *v == domain).unwrap().0;
    assert_eq!(
        prepared.origins()[domain_key],
        SchemaOrigin {
            generated: true,
            ..default_origin
        }
    );
    assert_eq!(
        prepared.origins()[model.root()],
        SchemaOrigin {
            source: SchemaSource::Input,
            node: raw_expected.root(),
            generated: false
        }
    );
    for index in 0..2 {
        let selected = window_id(model, index);
        let original = window_id(&raw_expected, index);
        assert_eq!(
            prepared.origins()[selected],
            SchemaOrigin {
                source: SchemaSource::Input,
                node: original,
                generated: false
            }
        );
        let copied = model.field(selected, "order_by").unwrap();
        let definition = field_id(&raw_expected, &["windows", "VISITS", "order_by"]);
        assert_eq!(
            prepared.origins()[copied],
            SchemaOrigin {
                source: SchemaSource::Input,
                node: definition,
                generated: false
            }
        );
    }
    // Independent complete provenance oracle: copy authored occurrences, replace
    // only the named selection's children from its definition, and materialize
    // the sole default key/value from its descriptor's captured schema arena.
    fn copied(
        model: &Document,
        id: usize,
        raw: &Document,
        input: usize,
        expected: &mut [Option<SchemaOrigin>],
    ) {
        expected[id] = Some(SchemaOrigin {
            source: SchemaSource::Input,
            node: input,
            generated: false,
        });
        match (&model.nodes()[id], &raw.nodes()[input]) {
            (N::Sequence(actual), N::Sequence(original)) => {
                assert_eq!(actual.len(), original.len());
                for (&actual, &original) in actual.iter().zip(original) {
                    copied(model, actual, raw, original, expected);
                }
            }
            (N::Mapping(actual), N::Mapping(original)) => {
                for &(key, value) in actual {
                    let &(original_key, original_value) = original
                        .iter()
                        .find(|&&(k, _)| raw.nodes()[k] == model.nodes()[key])
                        .unwrap();
                    copied(model, key, raw, original_key, expected);
                    copied(model, value, raw, original_value, expected);
                }
            }
            (N::Mapping(_), N::Text(name)) if name == "VISITS" => {
                let definition = field_id(raw, &["windows", "VISITS"]);
                copied(model, id, raw, definition, expected);
                expected[id] = Some(SchemaOrigin {
                    source: SchemaSource::Input,
                    node: input,
                    generated: false,
                });
            }
            (actual, original) => assert_eq!(actual, original),
        }
    }
    let mut expected_origins = vec![None; model.nodes().len()];
    expected_origins[model.root()] = Some(SchemaOrigin {
        source: SchemaSource::Input,
        node: raw_expected.root(),
        generated: false,
    });
    let N::Mapping(raw_fields) = &raw_expected.nodes()[raw_expected.root()] else {
        panic!()
    };
    for &(key, value) in entries {
        if value == domain {
            expected_origins[value] = Some(default_origin);
            expected_origins[key] = Some(SchemaOrigin {
                generated: true,
                ..default_origin
            });
        } else {
            let &(raw_key, raw_value) = raw_fields
                .iter()
                .find(|&&(k, _)| raw_expected.nodes()[k] == model.nodes()[key])
                .unwrap();
            copied(model, key, &raw_expected, raw_key, &mut expected_origins);
            copied(
                model,
                value,
                &raw_expected,
                raw_value,
                &mut expected_origins,
            );
        }
    }
    assert_eq!(
        prepared.origins(),
        expected_origins
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>()
    );
    let (raw, model, origins, windows) = prepared.into_parts();
    assert_eq!(raw.nodes().as_ptr(), raw_pointer);
    assert_eq!(origins.len(), model.document().nodes().len());
    assert_eq!(windows.len(), 2);
}

#[test]
fn parents_presence_selects_inheritance_before_standalone_semantic_checks() {
    let schema = schema(false, false);
    for parents in [Null, List(vec![]), List(vec![Text("parent.yaml")])] {
        let raw = document(Map(vec![
            ("schema_version", Text("99.0")),
            ("parents", parents),
            ("domain", Int("8")),
        ]));

        assert!(matches!(
            prepare(&schema, raw),
            Err(Error::InheritanceRequired)
        ));
    }
}

#[test]
fn normalization_findings_keep_original_raw_allocation_and_full_version_precedence() {
    let raw = document(Map(vec![
        ("schema_version", Int("92233720368547758081234567890")),
        ("domain", Int("8")),
        ("windows", Map(vec![("BAD", Text("broken"))])),
    ]));
    let pointer = raw.nodes().as_ptr();
    let expected = raw.clone();
    let actual_node = field_id(&raw, &["schema_version"]);
    let Err(Error::Normalize(failure)) = prepare(&schema(false, false), raw) else {
        panic!()
    };
    let held = failure.input.unwrap();
    assert_eq!(held.nodes().as_ptr(), pointer);
    assert_eq!(held, expected);
    assert_eq!(
        failure.error,
        NormalizationError::Invalid(vec![SchemaDiagnostic {
            path: "schema_version".into(),
            code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
            context: vec![
                ("expected", C::Text("1.0".into())),
                ("actual", C::InputValue(actual_node))
            ]
        }])
    );
}

#[test]
fn strict_windows_preserve_complete_order_and_precede_model_findings() {
    let mut fields = basic_fields();
    fields[4].1 = Map(vec![]);
    fields.last_mut().unwrap().1 = List(vec![
        column("Z", Text("SECOND")),
        column("A", Text("FIRST")),
    ]);
    let Err(Error::Normalize(failure)) = prepare(&schema(false, false), document(Map(fields)))
    else {
        panic!()
    };
    assert_eq!(
        failure.error,
        NormalizationError::Invalid(vec![unknown("Z", "SECOND"), unknown("A", "FIRST")])
    );
    assert!(failure.input.is_some());
}

#[test]
fn model_findings_remain_complete_and_have_no_document_value_references() {
    let mut fields = basic_fields();
    fields[1].1 = Int("7");
    fields[4].1 = Map(vec![]);
    let Err(Error::Model(findings)) = prepare(&schema(false, true), document(Map(fields))) else {
        panic!()
    };
    let finding = |path: &str, reason: &str| SchemaDiagnostic {
        path: path.into(),
        code: if reason == "Field required" {
            yamaa_core::diagnostic::ConditionCode::SchemaModelMissingField
        } else {
            yamaa_core::diagnostic::ConditionCode::SchemaModelInvalidValue
        },
        context: vec![("reason", C::Text(reason.into()))],
    };
    assert_eq!(
        findings,
        vec![
            finding("domain", "Input should be a valid string"),
            finding("output.path", "Field required"),
            finding("output.columns", "Field required")
        ]
    );
}
