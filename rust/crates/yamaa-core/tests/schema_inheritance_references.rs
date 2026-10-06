use yamaa_core::schema::{
    BundleLimits, Document, DocumentLimits, DocumentNode as N, InheritanceReference,
    InheritanceReferenceError, InheritanceReferenceKind as K, NormalizationBudget,
    NormalizationLimits, SchemaModule, SchemaStructure,
};

#[derive(Clone)]
enum V {
    Text(String),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
    Integer,
}
use V::{List, Map};
fn text(s: &str) -> V {
    V::Text(s.into())
}
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let n = match value {
            V::Text(t) => N::Text(t),
            V::Integer => N::Integer("7".into()),
            List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(text(k), nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(n);
        id
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn descriptor(kind: V) -> V {
    Map(vec![("type", kind)])
}
fn field(name: &'static str, kind: &str) -> V {
    Map(vec![(name, descriptor(text(kind)))])
}
fn schema() -> SchemaStructure {
    let mut declarations = vec![
        ("version", text("1.0")),
        (
            "root_class",
            List(vec![
                field("base", "identifier"),
                field("input", "dict[identifier, dataset_class]"),
                field("intermediates", "list[intermediate_class]"),
                field("filter", "predicate"),
                field("verifications", "list[variable]"),
                field("keys", "list[identifier]"),
                field("description", "identifier"),
                field("columns", "list[column_class]"),
                field("output", "output_class"),
                field("operations", "list[expression]"),
                field("rows", "list[row_class]"),
            ]),
        ),
        (
            "column_class",
            List(vec![
                field("name", "identifier"),
                field("label", "identifier"),
                field("derivation", "derivation"),
                field("verifications", "list[variable]"),
            ]),
        ),
        (
            "output_class",
            List(vec![field("columns", "list[identifier]")]),
        ),
        (
            "row_class",
            List(vec![
                field("id", "identifier"),
                field("dataset", "identifier"),
                field("derivations", "dict[identifier, derivation]"),
                field("group_by", "list[variable]"),
                field("filter", "predicate"),
            ]),
        ),
        ("dataset_class", List(vec![field("path", "str")])),
        (
            "intermediate_class",
            List(vec![
                field("id", "identifier"),
                field("dataset", "identifier"),
                field("key", "variable"),
                field("filter", "predicate"),
                field("order_by", "list[variable]"),
            ]),
        ),
        (
            "derivation",
            descriptor(List(vec![text("variable"), text("expression")])),
        ),
        ("variable", descriptor(text("str"))),
        ("identifier", descriptor(text("str"))),
        ("numeric_expression", descriptor(text("str"))),
        ("aggregate_expression", descriptor(text("str"))),
        ("predicate", descriptor(text("str"))),
        ("string_template", descriptor(text("str"))),
        (
            "literal_first",
            descriptor(List(vec![text("str"), text("variable")])),
        ),
        (
            "reference_first",
            descriptor(List(vec![text("variable"), text("str")])),
        ),
        ("expression", Map(vec![("registry", text("expressions"))])),
        (
            "expressions",
            Map(vec![
                ("source", descriptor(text("variable"))),
                ("compute", descriptor(text("numeric_expression"))),
                ("aggregate", descriptor(text("aggregate_expression"))),
                ("template", descriptor(text("string_template"))),
                ("literal", descriptor(text("str"))),
                (
                    "join",
                    List(vec![field("source", "variable"), field("label", "str")]),
                ),
            ]),
        ),
    ];
    // Recursive alias fallback must terminate without interpreting a literal as a variable.
    declarations.push((
        "recursive",
        descriptor(List(vec![text("recursive"), text("str")])),
    ));
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(declarations)),
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
fn refs(
    schema: &SchemaStructure,
    value: V,
    types: &[&str],
    scope: Option<(&str, &str)>,
    budget: &mut NormalizationBudget,
) -> Result<Vec<InheritanceReference>, InheritanceReferenceError> {
    let input = document(value);
    let types = schema
        .parse_query_types(
            &types.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            budget.validation_scope(),
        )
        .unwrap();
    schema.inheritance_references(&types, &input, input.root(), scope, budget)
}
fn variable(name: &str) -> InheritanceReference {
    InheritanceReference {
        kind: K::Variable,
        name: name.into(),
    }
}
fn dataset(name: &str) -> InheritanceReference {
    InheritanceReference {
        kind: K::Dataset,
        name: name.into(),
    }
}

#[test]
fn identifiers_are_references_only_in_semantic_roles() {
    let schema = schema();
    let input = Map(vec![
        ("base", text("DM")),
        ("description", text("NOT_A_DEPENDENCY")),
        ("keys", List(vec![text("ID")])),
        (
            "columns",
            List(vec![Map(vec![
                ("name", text("RESULT")),
                ("label", text("LITERAL")),
                ("derivation", text("DM.VALUE")),
            ])]),
        ),
        ("output", Map(vec![("columns", List(vec![text("RESULT")]))])),
        (
            "rows",
            List(vec![Map(vec![
                ("id", text("ROW_LITERAL")),
                ("dataset", text("EX")),
            ])]),
        ),
    ]);
    assert_eq!(
        refs(&schema, input, &["root_class"], None, &mut budget()).unwrap(),
        vec![
            dataset("DM"),
            variable("ID"),
            variable("RESULT"),
            variable("DM.VALUE"),
            dataset("EX")
        ]
    );
    assert_eq!(
        refs(&schema, text("DM"), &["identifier"], None, &mut budget()).unwrap(),
        vec![]
    );
}

#[test]
fn first_matching_union_and_registry_payloads_preserve_literal_boundaries() {
    let schema = schema();
    assert_eq!(
        refs(&schema, text("A"), &["literal_first"], None, &mut budget()).unwrap(),
        vec![]
    );
    assert_eq!(
        refs(
            &schema,
            text("A"),
            &["reference_first"],
            None,
            &mut budget()
        )
        .unwrap(),
        vec![variable("A")]
    );
    assert_eq!(
        refs(
            &schema,
            text("A"),
            &["int", "variable"],
            None,
            &mut budget()
        )
        .unwrap(),
        vec![variable("A")]
    );
    assert_eq!(
        refs(&schema, text("A"), &["recursive"], None, &mut budget()).unwrap(),
        vec![]
    );
    let input = List(vec![
        Map(vec![("literal", text("WRONG"))]),
        Map(vec![("source", text("DM.X"))]),
        Map(vec![(
            "join",
            Map(vec![("label", text("WRONG")), ("source", text("OTHER"))]),
        )]),
    ]);
    assert_eq!(
        refs(&schema, input, &["list[expression]"], None, &mut budget()).unwrap(),
        vec![variable("DM.X"), variable("OTHER")]
    );
}

#[test]
fn dictionaries_visit_values_and_scope_survives_containers() {
    let schema = schema();
    assert_eq!(
        refs(
            &schema,
            Map(vec![("KEY_LITERAL", text("A")), ("OTHER_KEY", text("A"))]),
            &["dict[str, variable]"],
            None,
            &mut budget()
        )
        .unwrap(),
        vec![variable("A")]
    );
    assert_eq!(
        refs(
            &schema,
            List(vec![text("A"), text("B")]),
            &["list[identifier]"],
            Some(("output_class", "columns")),
            &mut budget()
        )
        .unwrap(),
        vec![variable("A"), variable("B")]
    );
    // Matching is complete, not fragment validation or a permissive structural walk.
    assert_eq!(
        refs(
            &schema,
            List(vec![text("A"), V::Integer]),
            &["list[variable]"],
            None,
            &mut budget()
        )
        .unwrap(),
        vec![]
    );
}

#[test]
fn closed_languages_retain_field_and_record_count_dependencies() {
    let schema = schema();
    for (kind, input, expected) in [
        (
            "numeric_expression",
            "A + DM.X + A",
            vec![variable("A"), variable("DM.X")],
        ),
        (
            "aggregate_expression",
            "COUNT(EX.*) + SUM(EX.DOSE)",
            vec![variable("EX.DOSE"), variable("EX.*")],
        ),
        (
            "aggregate_expression",
            "COUNT(EX.*)",
            vec![variable("EX.*")],
        ),
        (
            "predicate",
            "A = 'LITERAL' AND DM.X > 0",
            vec![variable("A"), variable("DM.X")],
        ),
        (
            "string_template",
            "{{{A}}} {DM.X} {A}",
            vec![variable("A"), variable("DM.X")],
        ),
    ] {
        assert_eq!(
            refs(&schema, text(input), &[kind], None, &mut budget()).unwrap(),
            expected,
            "{kind}: {input}"
        );
    }
}

#[test]
fn malformed_language_has_no_partial_dependencies() {
    let schema = schema();
    for (kind, input) in [
        ("numeric_expression", "A +"),
        ("aggregate_expression", "SUM(A) +"),
        ("predicate", "A ="),
        ("string_template", "{A} {bad-name}"),
        ("string_template", "{A} }"),
        ("string_template", "{A} {"),
        ("string_template", "{A..B}"),
    ] {
        assert_eq!(
            refs(&schema, text(input), &[kind], None, &mut budget()).unwrap(),
            vec![],
            "{kind}: {input}"
        );
    }
    for input in ["{{literal}}", "plain unicode café", ""] {
        assert!(refs(
            &schema,
            text(input),
            &["string_template"],
            None,
            &mut budget()
        )
        .unwrap()
        .is_empty());
    }
}

#[test]
fn parser_refusal_is_not_an_empty_reference_set() {
    let schema = schema();
    let input = format!("{}A{}", "(".repeat(70), ")".repeat(70));
    assert!(matches!(
        refs(
            &schema,
            text(&input),
            &["numeric_expression"],
            None,
            &mut budget()
        ),
        Err(InheritanceReferenceError::Numeric(
            yamaa_core::numeric_parser::ParseError::Limit { .. }
        ))
    ));
}

#[test]
fn failed_attempts_keep_request_budget_and_fresh_request_recovers() {
    let schema = schema();
    let input = document(text("A + B"));
    let types = schema
        .parse_query_types(&["numeric_expression".into()], budget().validation_scope())
        .unwrap();
    let mut limits = NormalizationLimits::default();
    limits.validation.work = 100;
    let mut shared = NormalizationBudget::new(limits);
    assert!(schema
        .inheritance_references(&types, &input, input.root(), None, &mut shared)
        .is_err());
    let used = shared.work_used();
    assert!(used > 0);
    assert!(schema
        .inheritance_references(&types, &input, input.root(), None, &mut shared)
        .is_err());
    assert_eq!(
        schema
            .inheritance_references(&types, &input, input.root(), None, &mut budget())
            .unwrap(),
        vec![variable("A"), variable("B")]
    );
}

fn column(name: &str, derivation: V) -> V {
    Map(vec![("name", text(name)), ("derivation", derivation)])
}
fn operation(name: &'static str, value: &str) -> V {
    Map(vec![(name, text(value))])
}
fn source(path: &str) -> V {
    Map(vec![("path", text(path))])
}
fn names(d: &Document, collection: &str, identity: &str) -> Vec<String> {
    let node = d.field(d.root(), collection).unwrap();
    let N::Sequence(items) = &d.nodes()[node] else {
        panic!()
    };
    items
        .iter()
        .map(|&item| match &d.nodes()[d.field(item, identity).unwrap()] {
            N::Text(s) => s.clone(),
            _ => panic!(),
        })
        .collect()
}
fn keys(d: &Document, node: usize) -> Vec<String> {
    let N::Mapping(items) = &d.nodes()[node] else {
        panic!()
    };
    items
        .iter()
        .map(|&(key, _)| match &d.nodes()[key] {
            N::Text(s) => s.clone(),
            _ => panic!(),
        })
        .collect()
}
fn same(a: &Document, ai: usize, b: &Document, bi: usize) {
    match (&a.nodes()[ai], &b.nodes()[bi]) {
        (N::Sequence(x), N::Sequence(y)) => {
            assert_eq!(x.len(), y.len());
            for (&x, &y) in x.iter().zip(y) {
                same(a, x, b, y);
            }
        }
        (N::Mapping(x), N::Mapping(y)) => {
            assert_eq!(x.len(), y.len());
            for (&(ak, av), &(bk, bv)) in x.iter().zip(y) {
                same(a, ak, b, bk);
                same(a, av, b, bv);
            }
        }
        (x, y) => assert_eq!(x, y),
    }
}

#[test]
fn prunes_dead_sources_follows_count_and_stably_orders_without_changing_output() {
    let schema = schema();
    let input = document(Map(vec![
        ("base", text("DM")),
        (
            "input",
            Map(vec![
                ("DM", source("dm.csv")),
                ("EX", source("ex.csv")),
                ("DEAD", source("never.csv")),
            ]),
        ),
        (
            "output",
            Map(vec![("columns", List(vec![text("OUT"), text("N")]))]),
        ),
        (
            "columns",
            List(vec![
                column("OUT", operation("compute", "INNER + 1")),
                column("N", operation("aggregate", "COUNT(EX.*)")),
                column("INNER", text("DM.X")),
                column("DEAD", text("GHOST")),
            ]),
        ),
        ("intermediates", List(vec![])),
    ]));
    let result = schema
        .resolve_inheritance_dependencies(&input, &mut budget())
        .unwrap();
    let expected = document(Map(vec![
        ("base", text("DM")),
        (
            "input",
            Map(vec![("DM", source("dm.csv")), ("EX", source("ex.csv"))]),
        ),
        (
            "output",
            Map(vec![("columns", List(vec![text("OUT"), text("N")]))]),
        ),
        (
            "columns",
            List(vec![
                column("N", operation("aggregate", "COUNT(EX.*)")),
                column("INNER", text("DM.X")),
                column("OUT", operation("compute", "INNER + 1")),
            ]),
        ),
    ]));
    same(
        &result.document,
        result.document.root(),
        &expected,
        expected.root(),
    );
    assert_eq!(result.origins.len(), result.document.nodes().len());
    for (id, origin) in result.origins.iter().enumerate() {
        assert_eq!(origin.source, yamaa_core::schema::SchemaSource::Input);
        assert!(!origin.generated);
        if !matches!(result.document.nodes()[id], N::Sequence(_) | N::Mapping(_)) {
            assert_eq!(result.document.nodes()[id], input.nodes()[origin.node]);
        }
    }
}

#[test]
fn rows_and_assertions_are_roots_and_lookup_matching_inputs_precede_users() {
    let schema = schema();
    let input = document(Map(vec![
        (
            "input",
            Map(vec![
                ("DM", source("dm")),
                ("EX", source("ex")),
                ("ROW", source("row")),
                ("DEAD", source("dead")),
            ]),
        ),
        ("base", text("DM")),
        ("output", Map(vec![("columns", List(vec![text("LIVE")]))])),
        ("verifications", List(vec![text("CHECK")])),
        (
            "columns",
            List(vec![
                column("LIVE", operation("literal", "fixed")),
                column("KEY", text("DM.ID")),
                column("CHECK", text("DM.X")),
                Map(vec![
                    ("name", text("ASSERT")),
                    ("verifications", List(vec![text("CHECK")])),
                ]),
                column("UNUSED", text("MISSING")),
            ]),
        ),
        (
            "intermediates",
            List(vec![
                Map(vec![
                    ("id", text("LOOKUP")),
                    ("dataset", text("EX")),
                    ("key", text("KEY")),
                ]),
                Map(vec![
                    ("id", text("UNUSED_LOOKUP")),
                    ("dataset", text("DEAD")),
                ]),
            ]),
        ),
        (
            "rows",
            List(vec![
                Map(vec![
                    ("id", text("R1")),
                    ("dataset", text("ROW")),
                    (
                        "derivations",
                        Map(vec![
                            ("LIVE", text("LOOKUP.VALUE")),
                            ("UNUSED", text("DEAD.X")),
                        ]),
                    ),
                ]),
                Map(vec![("id", text("R2")), ("derivations", Map(vec![]))]),
            ]),
        ),
    ]));
    let result = schema
        .resolve_inheritance_dependencies(&input, &mut budget())
        .unwrap();
    let d = &result.document;
    assert_eq!(
        keys(d, d.field(d.root(), "input").unwrap()),
        vec!["DM", "EX", "ROW"]
    );
    assert_eq!(
        names(d, "columns", "name"),
        vec!["KEY", "LIVE", "CHECK", "ASSERT"]
    );
    assert_eq!(names(d, "intermediates", "id"), vec!["LOOKUP"]);
    assert_eq!(names(d, "rows", "id"), vec!["R1", "R2"]);
    let N::Sequence(rows) = &d.nodes()[d.field(d.root(), "rows").unwrap()] else {
        panic!()
    };
    assert_eq!(
        keys(d, d.field(rows[0], "derivations").unwrap()),
        vec!["LIVE"]
    );
    assert!(keys(d, d.field(rows[1], "derivations").unwrap()).is_empty());
}

#[test]
fn unknown_dependencies_are_lexical_and_precede_cycle_reporting() {
    use yamaa_core::schema::{InheritanceDependencyError as E, InheritanceDependencyIssue as I};
    let input = document(Map(vec![
        ("output", Map(vec![("columns", List(vec![text("X")]))])),
        (
            "columns",
            List(vec![
                column("X", operation("compute", "Z + B + A")),
                column("B", text("B")),
            ]),
        ),
    ]));
    assert_eq!(
        schema().resolve_inheritance_dependencies(&input, &mut budget()),
        Err(E::Invalid(vec![
            I::Unknown {
                column: "X".into(),
                dependency: "A".into()
            },
            I::Unknown {
                column: "X".into(),
                dependency: "Z".into()
            }
        ]))
    );
}

#[test]
fn cycle_reports_all_unscheduled_columns_in_original_order() {
    use yamaa_core::schema::{InheritanceDependencyError as E, InheritanceDependencyIssue as I};
    let input = document(Map(vec![
        (
            "output",
            Map(vec![(
                "columns",
                List(vec![text("DOWNSTREAM"), text("FREE")]),
            )]),
        ),
        (
            "columns",
            List(vec![
                column("DOWNSTREAM", text("A")),
                column("A", text("B")),
                column("B", text("A")),
                column("FREE", operation("literal", "no reference")),
            ]),
        ),
    ]));
    assert_eq!(
        schema().resolve_inheritance_dependencies(&input, &mut budget()),
        Err(E::Invalid(vec![I::Cycle {
            columns: vec!["DOWNSTREAM".into(), "A".into(), "B".into()]
        }]))
    );
}

#[test]
fn dependency_pass_refuses_shared_storage_exhaustion_without_partial_document() {
    let schema = schema();
    let input = document(Map(vec![
        ("output", Map(vec![("columns", List(vec![text("A")]))])),
        (
            "columns",
            List(vec![column("A", operation("literal", "constant"))]),
        ),
    ]));
    let mut limits = NormalizationLimits::default();
    limits.storage.nodes = 3;
    let mut shared = NormalizationBudget::new(limits);
    assert!(schema
        .resolve_inheritance_dependencies(&input, &mut shared)
        .is_err());
    assert!(schema
        .resolve_inheritance_dependencies(&input, &mut shared)
        .is_err());
    assert!(schema
        .resolve_inheritance_dependencies(&input, &mut budget())
        .is_ok());
}

#[test]
fn dependency_findings_share_the_request_diagnostic_quota() {
    use yamaa_core::schema::{
        InheritanceDependencyError as E, NormalizationError as N, ValidationError,
    };
    let schema = schema();
    let input = document(Map(vec![
        ("output", Map(vec![("columns", List(vec![text("X")]))])),
        ("columns", List(vec![column("X", text("MISSING"))])),
    ]));
    let mut limits = NormalizationLimits::default();
    limits.validation.diagnostics = 0;
    let mut shared = NormalizationBudget::new(limits);
    assert_eq!(
        schema.resolve_inheritance_dependencies(&input, &mut shared),
        Err(E::Normalization(N::Validation(
            ValidationError::Diagnostics { limit: 0 }
        )))
    );
}
