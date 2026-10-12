use yamaa_core::schema::{
    admit_schema_diagnostics, BundleLimits, ConstraintError, Document, DocumentLimits,
    DocumentNode as N, NormalizationBudget, NormalizationError, NormalizationLimits,
    SchemaDiagnosticBudget, SchemaModule, SchemaStructure, SpecificationDocument, ValidationBudget,
    ValidationError, ValidationLimits,
};

#[derive(Clone)]
enum V {
    Text(&'static str),
    Bool(bool),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Bool, List, Map, Text};
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(value) => N::Text(value.into()),
            Bool(value) => N::Boolean(value),
            List(values) => {
                N::Sequence(values.into_iter().map(|value| push(value, nodes)).collect())
            }
            Map(values) => N::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| (push(Text(key), nodes), push(value, nodes)))
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
fn union(name: &'static str, kinds: &[&'static str]) -> V {
    Map(vec![(
        name,
        Map(vec![(
            "type",
            List(kinds.iter().map(|kind| Text(kind)).collect()),
        )]),
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
                        field("schema_version", "str"),
                        field("domain", "str"),
                        field("input", "dict"),
                        field("keys", "list[str]"),
                        field("output", "dict"),
                        field("columns", "list[column_class]"),
                        field("rows", "list[row_class]"),
                        field("metadata", "dict"),
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
                    "row_class",
                    List(vec![
                        field("id", "str"),
                        field("derivations", "dict[str, handled_expression_class]"),
                    ]),
                ),
                (
                    "handled_expression_class",
                    List(vec![
                        field("value", "expression"),
                        field("unconvertible", "literal_value"),
                    ]),
                ),
                ("expression", Map(vec![("registry", Text("expressions"))])),
                ("project_path", Map(vec![("type", Text("str"))])),
                (
                    "literal_value",
                    Map(vec![(
                        "type",
                        List(vec![
                            Text("str"),
                            Text("int"),
                            Text("float"),
                            Text("bool"),
                            Text("null"),
                        ]),
                    )]),
                ),
                (
                    "branch_class",
                    List(vec![field("when", "str"), field("value", "expression")]),
                ),
                (
                    "expressions",
                    Map(vec![
                        (
                            "mapping",
                            List(vec![
                                field("source", "str"),
                                union("dict", &["dict[str, literal_value]", "project_path"]),
                            ]),
                        ),
                        ("case", List(vec![field("branches", "list[branch_class]")])),
                        ("nested", List(vec![field("values", "list[expression]")])),
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
fn mapping(dictionary: V) -> V {
    Map(vec![(
        "mapping",
        Map(vec![("source", Text("SRC.X")), ("dict", dictionary)]),
    )])
}
fn handled(value: V) -> V {
    Map(vec![("value", value)])
}
fn column(name: &'static str, value: V) -> V {
    Map(vec![
        ("name", Text(name)),
        ("type", Text("str")),
        ("derivation", handled(value)),
    ])
}
fn input(columns: Vec<V>, rows: Vec<V>) -> V {
    Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("DM")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("src.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                ("columns", List(vec![Text("X")])),
            ]),
        ),
        ("columns", List(columns)),
        ("rows", List(rows)),
    ])
}

#[test]
fn schema_requests_cover_nested_rows_in_schema_order_and_ignore_inline_and_metadata_values() {
    let case = Map(vec![(
        "case",
        Map(vec![(
            "branches",
            List(vec![Map(vec![
                ("when", Text("FALSE")),
                ("value", mapping(Text("unused.yaml"))),
            ])]),
        )]),
    )]);
    let nested = Map(vec![(
        "nested",
        Map(vec![("values", List(vec![mapping(Text("row.yaml"))]))]),
    )]);
    let row = Map(vec![
        ("id", Text("ROW")),
        ("derivations", Map(vec![("X", handled(nested))])),
    ]);
    let mut authored = input(
        vec![
            column("INLINE", mapping(Map(vec![("A", Text("dict.yaml"))]))),
            column("FIRST", mapping(Text("../common/dict.yaml"))),
            column("NESTED", case),
        ],
        vec![row],
    );
    let Map(ref mut fields) = authored else {
        panic!()
    };
    fields.push((
        "metadata",
        Map(vec![(
            "mapping",
            Map(vec![("dict", Text("never-read.yaml"))]),
        )]),
    ));
    let raw = document(authored);
    let original = raw.clone();
    let refs = schema()
        .mapping_dictionary_references(&raw, &mut budget())
        .unwrap();
    assert_eq!(
        refs.iter()
            .map(|r| (r.path(), r.written()))
            .collect::<Vec<_>>(),
        vec![
            (
                "columns.FIRST.derivation.mapping.dict",
                "../common/dict.yaml"
            ),
            (
                "columns.NESTED.derivation.case.branches[0].value.mapping.dict",
                "unused.yaml"
            ),
            (
                "rows.ROW.derivations.X.nested.values[0].mapping.dict",
                "row.yaml"
            ),
        ]
    );
    for r in refs {
        assert_eq!(raw.nodes()[r.node()], N::Text(r.written().into()));
    }
    assert_eq!(raw, original);
}

#[test]
fn dictionary_admission_retains_use_site_and_original_input_arena_for_schema_findings() {
    let schema = schema();
    let spec = document(input(vec![column("X", mapping(Text("dict.yaml")))], vec![]));
    let reference = schema
        .mapping_dictionary_references(&spec, &mut budget())
        .unwrap()
        .remove(0);
    let invalid = Document::new(
        vec![
            N::Integer("7".into()),
            N::Text("answer".into()),
            N::Mapping(vec![(0, 1)]),
        ],
        2,
        DocumentLimits::default(),
    )
    .unwrap();
    let NormalizationError::Invalid(findings) = schema
        .admit_mapping_dictionary(reference.clone(), &invalid, &mut budget())
        .unwrap_err()
    else {
        panic!()
    };
    let report = admit_schema_diagnostics(
        &schema,
        Some(&invalid),
        &findings,
        &[],
        &mut SchemaDiagnosticBudget::new(),
    )
    .unwrap();
    assert!(core::ptr::eq(report.document().unwrap(), &invalid));
    assert_eq!(
        report
            .iter()
            .map(|r| (r.phase, r.path, r.condition))
            .collect::<Vec<_>>(),
        vec![(
            "validation",
            "columns.X.derivation.mapping.dict.key(7)",
            "invalid_field_type"
        )]
    );
    for raw in [
        document(List(vec![Text("not a dictionary")])),
        document(Map(vec![("A", List(vec![Text("not literal")]))])),
    ] {
        let NormalizationError::Invalid(findings) = schema
            .admit_mapping_dictionary(reference.clone(), &raw, &mut budget())
            .unwrap_err()
        else {
            panic!()
        };
        let report = admit_schema_diagnostics(
            &schema,
            Some(&raw),
            &findings,
            &[],
            &mut SchemaDiagnosticBudget::new(),
        )
        .unwrap();
        assert!(report
            .iter()
            .all(|r| r.path.starts_with("columns.X.derivation.mapping.dict")));
    }
}

#[test]
fn compiler_materialization_copies_typed_held_content_and_keeps_original_model_immutable() {
    let schema = schema();
    let original = document(input(
        vec![
            column("X", mapping(Text("dict.yaml"))),
            column("Y", mapping(Text("dict.yaml"))),
        ],
        vec![],
    ));
    let model = SpecificationDocument::admit(
        original.clone(),
        &mut ValidationBudget::new(ValidationLimits::default()),
    )
    .unwrap()
    .unwrap();
    let references = schema
        .mapping_dictionary_references(model.document(), &mut budget())
        .unwrap();
    let dictionary = document(Map(vec![("A", Text("mapped")), ("B", Bool(false))]));
    let admitted = references
        .iter()
        .cloned()
        .map(|reference| {
            schema
                .admit_mapping_dictionary(reference, &dictionary, &mut budget())
                .unwrap()
        })
        .collect::<Vec<_>>();
    let compiled = schema
        .materialize_mapping_dictionaries(&model, &admitted, &mut budget())
        .unwrap();
    assert_eq!(
        compiled.document(),
        &document(input(
            vec![
                column(
                    "X",
                    mapping(Map(vec![("A", Text("mapped")), ("B", Bool(false))]))
                ),
                column(
                    "Y",
                    mapping(Map(vec![("A", Text("mapped")), ("B", Bool(false))]))
                )
            ],
            vec![]
        ))
    );
    assert_eq!(model.document(), &original);
    assert!(schema
        .mapping_dictionary_references(compiled.document(), &mut budget())
        .unwrap()
        .is_empty());
    assert!(schema
        .materialize_mapping_dictionaries(&model, &admitted[..1], &mut budget())
        .is_err());
    assert!(schema
        .materialize_mapping_dictionaries(
            &model,
            &[admitted[0].clone(), admitted[0].clone()],
            &mut budget()
        )
        .is_err());
}

#[test]
fn resource_request_and_materialization_limits_refuse_before_owned_copies() {
    let schema = schema();
    let model = SpecificationDocument::admit(
        document(input(vec![column("X", mapping(Text("dict.yaml")))], vec![])),
        &mut ValidationBudget::new(ValidationLimits::default()),
    )
    .unwrap()
    .unwrap();
    let limits = NormalizationLimits {
        validation: ValidationLimits {
            work: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        schema
            .mapping_dictionary_references(model.document(), &mut NormalizationBudget::new(limits)),
        Err(NormalizationError::Validation(ValidationError::Constraint(
            ConstraintError::Work { limit: 1 }
        )))
    ));
    let reference = schema
        .mapping_dictionary_references(model.document(), &mut budget())
        .unwrap()
        .remove(0);
    let admitted = schema
        .admit_mapping_dictionary(
            reference,
            &document(Map(vec![("A", Text("mapped"))])),
            &mut budget(),
        )
        .unwrap();
    let limits = NormalizationLimits {
        storage: DocumentLimits {
            text_bytes: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(schema
        .materialize_mapping_dictionaries(
            &model,
            &[admitted],
            &mut NormalizationBudget::new(limits)
        )
        .is_err());
}
