//! Schema contracts.
#[allow(dead_code)]
mod support;
mod schema {
    mod schema_inheritance_references {
        use yamaa_core::schema::{
            Document, DocumentNode as N, InheritanceReference, InheritanceReferenceError,
            InheritanceReferenceKind as K, SchemaModule, SchemaStructure,
        };

        use crate::support::document::Tree as V;

        fn text(s: &str) -> V {
            V::text(s)
        }
        fn document(value: V) -> Document {
            value.document()
        }
        fn descriptor(kind: V) -> V {
            V::map(vec![("type", kind)])
        }
        fn field(name: &'static str, kind: &str) -> V {
            V::map(vec![(name, descriptor(text(kind)))])
        }
        fn schema() -> SchemaStructure {
            let mut declarations = vec![
                ("version", text("1.0")),
                (
                    "root_class",
                    V::List(vec![
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
                    V::List(vec![
                        field("name", "identifier"),
                        field("label", "identifier"),
                        field("derivation", "derivation"),
                        field("verifications", "list[variable]"),
                    ]),
                ),
                (
                    "output_class",
                    V::List(vec![field("columns", "list[identifier]")]),
                ),
                (
                    "row_class",
                    V::List(vec![
                        field("id", "identifier"),
                        field("dataset", "identifier"),
                        field("derivations", "dict[identifier, derivation]"),
                        field("group_by", "list[variable]"),
                        field("filter", "predicate"),
                    ]),
                ),
                ("dataset_class", V::List(vec![field("path", "str")])),
                (
                    "intermediate_class",
                    V::List(vec![
                        field("id", "identifier"),
                        field("dataset", "identifier"),
                        field("key", "variable"),
                        field("filter", "predicate"),
                        field("order_by", "list[variable]"),
                    ]),
                ),
                (
                    "derivation",
                    descriptor(V::List(vec![text("variable"), text("expression")])),
                ),
                (
                    "aggregate_class",
                    V::List(vec![
                        field("expr", "aggregate_expression"),
                        field("filter", "predicate"),
                        field("derive", "list[derive_binding_class]"),
                    ]),
                ),
                (
                    "derive_binding_class",
                    V::List(vec![
                        field("name", "identifier"),
                        field("derivation", "derivation"),
                    ]),
                ),
                ("variable", descriptor(text("str"))),
                ("identifier", descriptor(text("str"))),
                ("numeric_expression", descriptor(text("str"))),
                ("aggregate_expression", descriptor(text("str"))),
                ("predicate", descriptor(text("str"))),
                ("string_template", descriptor(text("str"))),
                (
                    "literal_first",
                    descriptor(V::List(vec![text("str"), text("variable")])),
                ),
                (
                    "reference_first",
                    descriptor(V::List(vec![text("variable"), text("str")])),
                ),
                (
                    "expression",
                    V::map(vec![("registry", text("expressions"))]),
                ),
                (
                    "expressions",
                    V::map(vec![
                        ("source", descriptor(text("variable"))),
                        ("compute", descriptor(text("numeric_expression"))),
                        (
                            "aggregate",
                            descriptor(V::List(vec![
                                text("aggregate_expression"),
                                text("aggregate_class"),
                            ])),
                        ),
                        ("template", descriptor(text("string_template"))),
                        ("literal", descriptor(text("str"))),
                        (
                            "join",
                            V::List(vec![field("source", "variable"), field("label", "str")]),
                        ),
                    ]),
                ),
            ];
            // Recursive alias fallback must terminate without interpreting a literal as a variable.
            declarations.push((
                "recursive",
                descriptor(V::List(vec![text("recursive"), text("str")])),
            ));
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(declarations)),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        fn refs(
            schema: &SchemaStructure,
            value: V,
            types: &[&str],
            scope: Option<(&str, &str)>,
        ) -> Result<Vec<InheritanceReference>, InheritanceReferenceError> {
            let input = document(value);
            let types = schema
                .parse_query_types(&types.iter().map(|s| (*s).into()).collect::<Vec<_>>())
                .unwrap();
            schema.inheritance_references(&types, &input, input.root(), scope)
        }
        fn variable(name: &str) -> InheritanceReference {
            InheritanceReference {
                kind: K::Variable,
                name: name.into(),
            }
        }
        fn predicate_variable(name: &str) -> InheritanceReference {
            InheritanceReference {
                kind: K::PredicateVariable,
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
            let input = V::map(vec![
                ("base", text("DM")),
                ("description", text("NOT_A_DEPENDENCY")),
                ("keys", V::List(vec![text("ID")])),
                (
                    "columns",
                    V::List(vec![V::map(vec![
                        ("name", text("RESULT")),
                        ("label", text("LITERAL")),
                        ("derivation", text("DM.VALUE")),
                    ])]),
                ),
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("RESULT")]))]),
                ),
                (
                    "rows",
                    V::List(vec![V::map(vec![
                        ("id", text("ROW_LITERAL")),
                        ("dataset", text("EX")),
                    ])]),
                ),
            ]);
            assert_eq!(
                refs(&schema, input, &["root_class"], None).unwrap(),
                vec![
                    dataset("DM"),
                    variable("ID"),
                    variable("RESULT"),
                    variable("DM.VALUE"),
                    dataset("EX")
                ]
            );
            assert_eq!(
                refs(&schema, text("DM"), &["identifier"], None).unwrap(),
                vec![]
            );
        }

        #[test]
        fn first_matching_union_and_registry_payloads_preserve_literal_boundaries() {
            let schema = schema();
            assert_eq!(
                refs(&schema, text("A"), &["literal_first"], None).unwrap(),
                vec![]
            );
            assert_eq!(
                refs(&schema, text("A"), &["reference_first"], None).unwrap(),
                vec![variable("A")]
            );
            assert_eq!(
                refs(&schema, text("A"), &["int", "variable"], None).unwrap(),
                vec![variable("A")]
            );
            assert_eq!(
                refs(&schema, text("A"), &["recursive"], None).unwrap(),
                vec![]
            );
            let input = V::List(vec![
                V::map(vec![("literal", text("WRONG"))]),
                V::map(vec![("source", text("DM.X"))]),
                V::map(vec![(
                    "join",
                    V::map(vec![("label", text("WRONG")), ("source", text("OTHER"))]),
                )]),
            ]);
            assert_eq!(
                refs(&schema, input, &["list[expression]"], None).unwrap(),
                vec![variable("DM.X"), variable("OTHER")]
            );
        }

        #[test]
        fn dictionaries_visit_values_and_scope_survives_containers() {
            let schema = schema();
            assert_eq!(
                refs(
                    &schema,
                    V::map(vec![("KEY_LITERAL", text("A")), ("OTHER_KEY", text("A"))]),
                    &["dict[str, variable]"],
                    None
                )
                .unwrap(),
                vec![variable("A")]
            );
            assert_eq!(
                refs(
                    &schema,
                    V::List(vec![text("A"), text("B")]),
                    &["list[identifier]"],
                    Some(("output_class", "columns"))
                )
                .unwrap(),
                vec![variable("A"), variable("B")]
            );
            // Matching is complete, not fragment validation or a permissive structural walk.
            assert_eq!(
                refs(
                    &schema,
                    V::List(vec![text("A"), V::integer("7")]),
                    &["list[variable]"],
                    None
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
                    vec![predicate_variable("A"), predicate_variable("DM.X")],
                ),
                (
                    "string_template",
                    "{{{A}}} {DM.X} {A}",
                    vec![variable("A"), variable("DM.X")],
                ),
            ] {
                assert_eq!(
                    refs(&schema, text(input), &[kind], None).unwrap(),
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
                    refs(&schema, text(input), &[kind], None).unwrap(),
                    vec![],
                    "{kind}: {input}"
                );
            }
            for input in ["{{literal}}", "plain unicode café", ""] {
                assert!(refs(&schema, text(input), &["string_template"], None)
                    .unwrap()
                    .is_empty());
            }
        }

        #[test]
        fn parser_refusal_is_not_an_empty_reference_set() {
            let schema = schema();
            let input = format!("{}A{}", "(".repeat(129), ")".repeat(129));
            assert!(matches!(
                refs(&schema, text(&input), &["numeric_expression"], None),
                Err(InheritanceReferenceError::Numeric(
                    yamaa_core::numeric_parser::ParseError::Limit { .. }
                ))
            ));
        }

        #[test]
        fn independent_reference_requests_keep_written_identifiers() {
            let schema = schema();
            let input = document(text("A + B"));
            let types = schema
                .parse_query_types(&["numeric_expression".into()])
                .unwrap();
            for _ in 0..3 {
                assert_eq!(
                    schema
                        .inheritance_references(&types, &input, input.root(), None)
                        .unwrap(),
                    vec![variable("A"), variable("B")]
                );
            }
        }

        fn column(name: &str, derivation: V) -> V {
            V::map(vec![("name", text(name)), ("derivation", derivation)])
        }
        fn operation(name: &'static str, value: &str) -> V {
            V::map(vec![(name, text(value))])
        }
        fn source(path: &str) -> V {
            V::map(vec![("path", text(path))])
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
            let input = document(V::map(vec![
                ("base", text("DM")),
                (
                    "input",
                    V::map(vec![
                        ("DM", source("dm.csv")),
                        ("EX", source("ex.csv")),
                        ("DEAD", source("never.csv")),
                    ]),
                ),
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("OUT"), text("N")]))]),
                ),
                (
                    "columns",
                    V::List(vec![
                        column("OUT", operation("compute", "INNER + 1")),
                        column("N", operation("aggregate", "COUNT(EX.*)")),
                        column("INNER", text("DM.X")),
                        column("DEAD", text("GHOST")),
                    ]),
                ),
                ("intermediates", V::List(vec![])),
            ]));
            let result = schema.resolve_inheritance_dependencies(&input).unwrap();
            let expected = document(V::map(vec![
                ("base", text("DM")),
                (
                    "input",
                    V::map(vec![("DM", source("dm.csv")), ("EX", source("ex.csv"))]),
                ),
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("OUT"), text("N")]))]),
                ),
                (
                    "columns",
                    V::List(vec![
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
            let input = document(V::map(vec![
                (
                    "input",
                    V::map(vec![
                        ("DM", source("dm")),
                        ("EX", source("ex")),
                        ("ROW", source("row")),
                        ("DEAD", source("dead")),
                    ]),
                ),
                ("base", text("DM")),
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("LIVE")]))]),
                ),
                ("verifications", V::List(vec![text("CHECK")])),
                (
                    "columns",
                    V::List(vec![
                        column("LIVE", operation("literal", "fixed")),
                        column("KEY", text("DM.ID")),
                        column("CHECK", text("DM.X")),
                        V::map(vec![
                            ("name", text("ASSERT")),
                            ("verifications", V::List(vec![text("CHECK")])),
                        ]),
                        column("UNUSED", text("MISSING")),
                    ]),
                ),
                (
                    "intermediates",
                    V::List(vec![
                        V::map(vec![
                            ("id", text("LOOKUP")),
                            ("dataset", text("EX")),
                            ("key", text("KEY")),
                        ]),
                        V::map(vec![
                            ("id", text("UNUSED_LOOKUP")),
                            ("dataset", text("DEAD")),
                        ]),
                    ]),
                ),
                (
                    "rows",
                    V::List(vec![
                        V::map(vec![
                            ("id", text("R1")),
                            ("dataset", text("ROW")),
                            (
                                "derivations",
                                V::map(vec![
                                    ("LIVE", text("LOOKUP.VALUE")),
                                    ("UNUSED", text("DEAD.X")),
                                ]),
                            ),
                        ]),
                        V::map(vec![("id", text("R2")), ("derivations", V::map(vec![]))]),
                    ]),
                ),
            ]));
            let result = schema.resolve_inheritance_dependencies(&input).unwrap();
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
            use yamaa_core::schema::{
                InheritanceDependencyError as E, InheritanceDependencyIssue as I,
            };
            let input = document(V::map(vec![
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("X")]))]),
                ),
                (
                    "columns",
                    V::List(vec![
                        column("X", operation("compute", "Z + B + A")),
                        column("B", text("B")),
                    ]),
                ),
            ]));
            assert_eq!(
                schema().resolve_inheritance_dependencies(&input),
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
            let E::Invalid(issues) = schema()
                .resolve_inheritance_dependencies(&input)
                .unwrap_err()
            else {
                panic!("expected completed dependency findings")
            };
            let diagnostics = yamaa_core::diagnostic::inheritance_dependency_diagnostics(&issues);
            assert_eq!(diagnostics.len(), 2);
            assert_eq!(diagnostics[0].spec_paths, ["columns.X.derivation"]);
            assert_eq!(diagnostics[1].spec_paths, ["columns.X.derivation"]);
            for (diagnostic, expected) in diagnostics.iter().zip(["A", "Z"]) {
                assert_eq!(diagnostic.definition().condition, "unknown_reference");
                assert_eq!(diagnostic.definition().requirement, Some("REQ-0070"));
                assert_eq!(
                    diagnostic.context["dependency"],
                    yamaa_core::json::Value::String(expected.into())
                );
            }
        }

        #[test]
        fn cycle_reports_all_unscheduled_columns_in_original_order() {
            use yamaa_core::schema::{
                InheritanceDependencyError as E, InheritanceDependencyIssue as I,
            };
            let input = document(V::map(vec![
                (
                    "output",
                    V::map(vec![(
                        "columns",
                        V::List(vec![text("DOWNSTREAM"), text("FREE")]),
                    )]),
                ),
                (
                    "columns",
                    V::List(vec![
                        column("DOWNSTREAM", text("A")),
                        column("A", text("B")),
                        column("B", text("A")),
                        column("FREE", operation("literal", "no reference")),
                    ]),
                ),
            ]));
            assert_eq!(
                schema().resolve_inheritance_dependencies(&input),
                Err(E::Invalid(vec![I::Cycle {
                    columns: vec!["DOWNSTREAM".into(), "A".into(), "B".into()],
                    cycle: vec!["A".into(), "B".into(), "A".into()],
                    paths: vec!["columns.A.derivation".into(), "columns.B.derivation".into()],
                }]))
            );
        }

        #[test]
        fn independent_dependency_requests_keep_the_complete_document() {
            let schema = schema();
            let input = document(V::map(vec![
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("A")]))]),
                ),
                (
                    "columns",
                    V::List(vec![column("A", operation("literal", "constant"))]),
                ),
            ]));
            let first = schema.resolve_inheritance_dependencies(&input).unwrap();
            let second = schema.resolve_inheritance_dependencies(&input).unwrap();
            assert_eq!(first, second);
            assert_eq!(names(&first.document, "columns", "name"), vec!["A"]);
        }

        #[test]
        fn unknown_dependency_retains_its_authored_column_and_name() {
            use yamaa_core::schema::{
                InheritanceDependencyError as E, InheritanceDependencyIssue as I,
            };
            let input = document(V::map(vec![
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("X")]))]),
                ),
                ("columns", V::List(vec![column("X", text("MISSING"))])),
            ]));
            assert_eq!(
                schema().resolve_inheritance_dependencies(&input),
                Err(E::Invalid(vec![I::Unknown {
                    column: "X".into(),
                    dependency: "MISSING".into()
                }]))
            );
        }

        #[test]
        fn inheritance_and_graph_analysis_match_independently_declared_schedules() {
            use yamaa_core::dependency_analysis::analyze;
            let cases = [
                (vec![vec![2], vec![2], vec![], vec![]], vec![2, 0, 1, 3]),
                (vec![vec![2, 1], vec![3], vec![3], vec![]], vec![3, 1, 2, 0]),
                (vec![vec![1, 1, 1], vec![2, 2], vec![]], vec![2, 1, 0]),
                (vec![vec![], vec![], vec![]], vec![0, 1, 2]),
            ];
            for (graph, expected) in cases {
                let column_names: Vec<_> =
                    (0..graph.len()).map(|index| format!("N{index}")).collect();
                let columns = graph
                    .iter()
                    .zip(&column_names)
                    .map(|(edges, name)| {
                        let derivation = if edges.is_empty() {
                            operation("literal", "constant")
                        } else {
                            let expression = edges
                                .iter()
                                .map(|&index| column_names[index].as_str())
                                .collect::<Vec<_>>()
                                .join(" + ");
                            operation("compute", &expression)
                        };
                        column(name, derivation)
                    })
                    .collect();
                let input = document(V::map(vec![
                    (
                        "output",
                        V::map(vec![(
                            "columns",
                            V::List(column_names.iter().map(|name| text(name)).collect()),
                        )]),
                    ),
                    ("columns", V::List(columns)),
                ]));
                let normalized = schema().resolve_inheritance_dependencies(&input).unwrap();
                let expected_names: Vec<_> = expected
                    .iter()
                    .map(|&index| column_names[index].as_str())
                    .collect();
                assert_eq!(
                    names(&normalized.document, "columns", "name"),
                    expected_names
                );
                let analyzed = analyze(&graph).unwrap();
                assert_eq!(analyzed.order, expected);
                assert_eq!(analyzed.cycle, None);
            }
        }

        #[test]
        fn shared_schedule_keeps_inheritance_residuals_distinct_from_first_cycle() {
            use yamaa_core::{
                dependency_analysis::analyze,
                schema::{InheritanceDependencyError as E, InheritanceDependencyIssue as I},
            };
            let input = document(V::map(vec![
                (
                    "output",
                    V::map(vec![(
                        "columns",
                        V::List(
                            ["DOWNSTREAM", "A", "B", "FREE", "SELF", "FREE_READER"]
                                .iter()
                                .map(|name| text(name))
                                .collect(),
                        ),
                    )]),
                ),
                (
                    "columns",
                    V::List(vec![
                        column("DOWNSTREAM", text("A")),
                        column("A", text("B")),
                        column("B", text("A")),
                        column("FREE", operation("literal", "constant")),
                        column("SELF", text("SELF")),
                        column("FREE_READER", text("FREE")),
                    ]),
                ),
            ]));
            assert_eq!(
                schema().resolve_inheritance_dependencies(&input),
                Err(E::Invalid(vec![I::Cycle {
                    columns: vec!["DOWNSTREAM".into(), "A".into(), "B".into(), "SELF".into()],
                    cycle: vec!["A".into(), "B".into(), "A".into()],
                    paths: vec!["columns.A.derivation".into(), "columns.B.derivation".into()],
                }]))
            );
            let analyzed = analyze(&[vec![1], vec![2], vec![1], vec![], vec![4], vec![3]]).unwrap();
            assert_eq!(analyzed.order, vec![3, 5]);
            assert_eq!(analyzed.cycle, Some(vec![1, 2, 1]));
        }

        #[test]
        fn independent_scheduler_requests_keep_authored_column_order_and_input() {
            let input = document(V::map(vec![
                (
                    "output",
                    V::map(vec![("columns", V::List(vec![text("A"), text("B")]))]),
                ),
                (
                    "columns",
                    V::List(vec![
                        V::map(vec![("name", text("A"))]),
                        V::map(vec![("name", text("B"))]),
                    ]),
                ),
            ]));
            let original = input.clone();
            let schema = schema();
            for _ in 0..2 {
                assert_eq!(
                    names(
                        &schema
                            .resolve_inheritance_dependencies(&input)
                            .unwrap()
                            .document,
                        "columns",
                        "name"
                    ),
                    vec!["A", "B"]
                );
            }
            assert_eq!(input, original);
        }

        #[test]
        fn aggregate_binding_scope_preserves_filters_external_reads_and_invalid_forward_reads() {
            let schema = schema();
            let aggregate = V::map(vec![(
                "aggregate",
                V::map(vec![
                    ("expr", text("SUM(B)")),
                    ("filter", text("A > 0")),
                    (
                        "derive",
                        V::List(vec![
                            V::map(vec![("name", text("A")), ("derivation", text("DM.X"))]),
                            V::map(vec![
                                ("name", text("B")),
                                ("derivation", V::map(vec![("compute", text("A + OFFSET"))])),
                            ]),
                        ]),
                    ),
                ]),
            )]);
            assert_eq!(
                refs(&schema, aggregate, &["expression"], None).unwrap(),
                vec![
                    predicate_variable("A"),
                    variable("DM.X"),
                    variable("OFFSET")
                ]
            );
            let forward = V::map(vec![(
                "aggregate",
                V::map(vec![
                    (
                        "derive",
                        V::List(vec![
                            V::map(vec![("name", text("A")), ("derivation", text("B"))]),
                            V::map(vec![("name", text("B")), ("derivation", text("DM.X"))]),
                        ]),
                    ),
                    ("expr", text("SUM(A)")),
                ]),
            )]);
            assert_eq!(
                refs(&schema, forward, &["expression"], None).unwrap(),
                vec![variable("B"), variable("DM.X")]
            );
        }
    }
    mod schema_layer_admission {
        use yamaa_core::schema::{
            Document, DocumentKind as K, DocumentNode as N, NormalizationError, SchemaContext as C,
            SchemaDiagnostic, SchemaModule, SchemaSource, SchemaStructure,
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        fn field(name: &'static str, kind: &'static str, required: bool) -> V {
            V::map(vec![(
                name,
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(required)),
                ]),
            )])
        }
        fn schema() -> SchemaStructure {
            schema_with_identifier(V::text("str"))
        }

        fn schema_with_identifier(identifier_type: V) -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("schema_version", "str", true),
                                field("parent", "str", false),
                                field("input", "dict[str, dataset_class]", false),
                                field("columns", "list[column_class]", false),
                                field("rows", "list[row_class]", false),
                                field("intermediates", "list[intermediate_class]", false),
                                field("settings", "options_class", false),
                                field("output", "str", true),
                            ]),
                        ),
                        ("identifier", V::map(vec![("type", identifier_type)])),
                        ("project_path", V::map(vec![("type", V::text("str"))])),
                        ("column_type", V::map(vec![("type", V::text("str"))])),
                        (
                            "column_class",
                            V::List(vec![
                                field("name", "str", true),
                                field("type", "column_type", true),
                                field("label", "str", false),
                                field("options", "options_class", false),
                            ]),
                        ),
                        (
                            "dataset_class",
                            V::List(vec![
                                field("path", "str", true),
                                field("options", "options_class", false),
                            ]),
                        ),
                        (
                            "row_class",
                            V::List(vec![
                                field("id", "str", true),
                                field("options", "options_class", false),
                            ]),
                        ),
                        (
                            "intermediate_class",
                            V::List(vec![
                                field("id", "str", true),
                                field("options", "options_class", false),
                            ]),
                        ),
                        (
                            "options_class",
                            V::List(vec![
                                field("needed", "str", true),
                                V::map(vec![(
                                    "defaulted",
                                    V::map(vec![
                                        ("type", V::text("str")),
                                        ("default", V::text("DEFAULT")),
                                    ]),
                                )]),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        fn layer(mut fields: Vec<(&'static str, V)>) -> Document {
            fields.insert(0, ("schema_version", V::text("1.0")));
            document(V::map(fields))
        }
        fn diagnostics(input: Document) -> Vec<SchemaDiagnostic> {
            match schema().normalize_layer(&input) {
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
                code: match (condition, requirement) {
                    ("schema_version_mismatch", "REQ-0656") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerVersionMismatch
                    }
                    ("duplicate_identifier", "REQ-0659") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerIdentifierRepeated
                    }
                    ("invalid_field_type", "REQ-0658") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerInvalidFieldType
                    }
                    ("invalid_clear", "REQ-0660") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerInvalidClear
                    }
                    _ => panic!("unexpected test cause"),
                },
                context,
            }
        }
        fn text(value: &str) -> C {
            C::Text(value.into())
        }
        fn paths(findings: &[SchemaDiagnostic]) -> Vec<(&str, &str)> {
            findings
                .iter()
                .map(|d| (d.path.as_str(), d.definition().condition))
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
                V::List(vec![V::map(vec![
                    ("label", V::integer("2")),
                    ("name", V::text("X")),
                    ("type", V::integer("1")),
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
                vec![
                    ("expected", text("column_type")),
                    ("actual", C::ValueKind(K::Integer))
                ]
            );
            assert_eq!(
                findings[1].context,
                vec![
                    ("expected", text("str")),
                    ("actual", C::ValueKind(K::Integer))
                ]
            );
        }

        #[test]
        fn version_unknown_identity_and_duplicate_errors_keep_phase_order() {
            let findings = diagnostics(document(V::map(vec![
                ("unknown", V::Scalar(N::Null)),
                (
                    "columns",
                    V::List(vec![
                        V::map(vec![("extra", V::Scalar(N::Null))]),
                        V::map(vec![("name", V::text("X"))]),
                        V::map(vec![("name", V::text("X")), ("label", V::integer("2"))]),
                    ]),
                ),
            ])));
            assert_eq!(
                paths(&findings),
                [
                    ("schema_version", "schema_version_mismatch"),
                    ("unknown", "unknown_field"),
                    ("columns[0].name", "missing_required_field"),
                    ("columns[0].extra", "unknown_field"),
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
        fn layer_shape_errors_retain_portable_kinds_and_named_classes() {
            for (input, path, expected, actual) in [
                (
                    document(V::map(vec![])),
                    "$",
                    text("root_class"),
                    K::Mapping,
                ),
                (
                    document(V::Scalar(N::Null)),
                    "$",
                    text("root_class"),
                    K::Null,
                ),
                (
                    layer(vec![("input", V::List(vec![]))]),
                    "input",
                    C::LayerKind(K::Mapping),
                    K::Sequence,
                ),
                (
                    layer(vec![("columns", V::map(vec![]))]),
                    "columns",
                    C::LayerKind(K::Sequence),
                    K::Mapping,
                ),
                (
                    layer(vec![("columns", V::List(vec![V::integer("7")]))]),
                    "columns[0]",
                    text("column_class"),
                    K::Integer,
                ),
            ] {
                assert_eq!(
                    diagnostics(input),
                    [finding(
                        path,
                        "invalid_field_type",
                        "REQ-0658",
                        vec![("expected", expected), ("actual", C::LayerKind(actual))]
                    )]
                );
            }
        }

        #[test]
        fn required_and_identity_nulls_are_invalid_clears() {
            let findings = diagnostics(layer(vec![
                (
                    "columns",
                    V::List(vec![V::map(vec![
                        ("name", V::Scalar(N::Null)),
                        ("type", V::Scalar(N::Null)),
                        ("label", V::Scalar(N::Null)),
                    ])]),
                ),
                ("output", V::Scalar(N::Null)),
            ]));
            assert_eq!(
                findings,
                [
                    finding(
                        "columns[0].name",
                        "invalid_clear",
                        "REQ-0660",
                        vec![("field", text("name"))]
                    ),
                    finding(
                        "columns[0].type",
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
        fn keyed_member_fragments_defer_nested_requiredness_but_root_values_do_not() {
            let findings = diagnostics(layer(vec![
                (
                    "input",
                    V::map(vec![("SRC", V::map(vec![("options", V::map(vec![]))]))]),
                ),
                (
                    "columns",
                    V::List(vec![V::map(vec![
                        ("name", V::text("X")),
                        ("options", V::map(vec![])),
                    ])]),
                ),
                (
                    "rows",
                    V::List(vec![V::map(vec![
                        ("id", V::text("R")),
                        ("options", V::map(vec![])),
                    ])]),
                ),
                (
                    "intermediates",
                    V::List(vec![V::map(vec![
                        ("id", V::text("I")),
                        ("options", V::map(vec![])),
                    ])]),
                ),
                ("settings", V::map(vec![])),
            ]));
            assert_eq!(
                paths(&findings),
                [("settings.needed", "missing_required_field")]
            );
        }

        #[test]
        fn nested_required_nulls_are_invalid_clears_in_members_and_root_values() {
            let findings = diagnostics(layer(vec![
                (
                    "rows",
                    V::List(vec![V::map(vec![
                        ("id", V::text("R")),
                        ("options", V::map(vec![("needed", V::Scalar(N::Null))])),
                    ])]),
                ),
                ("settings", V::map(vec![("needed", V::Scalar(N::Null))])),
            ]));
            assert_eq!(
                findings,
                [
                    finding(
                        "rows.R.options.needed",
                        "invalid_clear",
                        "REQ-0660",
                        vec![("field", text("needed"))]
                    ),
                    finding(
                        "settings.needed",
                        "invalid_clear",
                        "REQ-0660",
                        vec![("field", text("needed"))]
                    ),
                ]
            );
        }

        #[test]
        fn normalization_preserves_clears_at_every_depth_defers_defaults_and_owns_shorthand() {
            let normalized = {
                let input = layer(vec![
                    (
                        "rows",
                        V::List(vec![V::map(vec![
                            ("id", V::text("R")),
                            (
                                "options",
                                V::map(vec![
                                    ("needed", V::text("yes")),
                                    ("defaulted", V::Scalar(N::Null)),
                                ]),
                            ),
                        ])]),
                    ),
                    (
                        "columns",
                        V::List(vec![V::map(vec![
                            ("name", V::text("X")),
                            ("label", V::Scalar(N::Null)),
                            ("options", V::map(vec![])),
                        ])]),
                    ),
                    (
                        "input",
                        V::map(vec![
                            ("SRC", V::text("data.csv")),
                            ("EMPTY", V::map(vec![])),
                        ]),
                    ),
                ]);
                schema().normalize_layer(&input).unwrap()
            };
            let expected = layer(vec![
                (
                    "input",
                    V::map(vec![
                        ("SRC", V::map(vec![("path", V::text("data.csv"))])),
                        ("EMPTY", V::map(vec![])),
                    ]),
                ),
                (
                    "columns",
                    V::List(vec![V::map(vec![
                        ("name", V::text("X")),
                        ("label", V::Scalar(N::Null)),
                        ("options", V::map(vec![])),
                    ])]),
                ),
                (
                    "rows",
                    V::List(vec![V::map(vec![
                        ("id", V::text("R")),
                        (
                            "options",
                            V::map(vec![
                                ("needed", V::text("yes")),
                                ("defaulted", V::Scalar(N::Null)),
                            ]),
                        ),
                    ])]),
                ),
            ]);
            assert_document(&normalized.document, &expected);
            assert_eq!(normalized.origins.len(), expected.nodes().len());
            assert!(!normalized
                .origins
                .iter()
                .any(|o| matches!(o.source, SchemaSource::Default { .. })));
        }

        #[test]
        fn generated_input_path_and_container_point_to_original_shorthand() {
            let input = layer(vec![("input", V::map(vec![("SRC", V::text("data.csv"))]))]);
            let source = get(&input, get(&input, input.root(), "input"), "SRC");
            let output = schema().normalize_layer(&input).unwrap();
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
        fn repeated_invalid_layers_retain_the_same_complete_findings() {
            let schema = schema();
            let input = layer(vec![("output", V::integer("2"))]);
            let first = schema.normalize_layer(&input).unwrap_err();
            assert!(matches!(&first, NormalizationError::Invalid(_)));
            assert_eq!(schema.normalize_layer(&input).unwrap_err(), first);
        }

        #[test]
        fn independent_normalized_layers_keep_the_complete_authored_column() {
            let schema = schema();
            let input = layer(vec![(
                "columns",
                V::List(vec![V::map(vec![("name", V::text("X"))])]),
            )]);
            let first = schema.normalize_layer(&input).unwrap();
            let second = schema.normalize_layer(&input).unwrap();
            assert_eq!(first, second);
        }

        #[test]
        fn empty_collections_and_input_shorthand_do_not_require_unused_member_classes() {
            let schema = SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("schema_version", "str", true),
                                field("columns", "list", false),
                                field("input", "dict", false),
                            ]),
                        ),
                        ("identifier", V::map(vec![("type", V::text("str"))])),
                        ("project_path", V::map(vec![("type", V::text("str"))])),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap();
            let input = layer(vec![
                ("columns", V::List(vec![])),
                ("input", V::map(vec![("SRC", V::text("data.csv"))])),
            ]);
            let output = schema.normalize_layer(&input).unwrap();
            assert_document(
                &output.document,
                &layer(vec![
                    ("columns", V::List(vec![])),
                    (
                        "input",
                        V::map(vec![("SRC", V::map(vec![("path", V::text("data.csv"))]))]),
                    ),
                ]),
            );
        }

        #[test]
        fn paths_distinguish_positional_numeric_boolean_and_textual_members() {
            let input = document(V::Pairs(vec![
                (V::text("schema_version"), V::text("1.0")),
                (V::boolean(true), V::Scalar(N::Null)),
                (
                    V::text("input"),
                    V::Pairs(vec![
                        (V::boolean(true), V::List(vec![])),
                        (V::integer("2"), V::List(vec![])),
                        (V::text("2"), V::List(vec![])),
                        (V::Scalar(N::Null), V::List(vec![])),
                    ]),
                ),
                (
                    V::text("columns"),
                    V::List(vec![
                        V::map(vec![]),
                        V::Pairs(vec![
                            (V::text("name"), V::text("2")),
                            (V::boolean(false), V::Scalar(N::Null)),
                            (V::integer("3"), V::Scalar(N::Null)),
                        ]),
                    ]),
                ),
            ]));
            let findings = diagnostics(input);
            assert_eq!(
                paths(&findings),
                [
                    ("True", "unknown_field"),
                    ("input.key(True)", "invalid_field_type"),
                    ("input[True]", "invalid_field_type"),
                    ("input.key(2)", "invalid_field_type"),
                    ("input[2]", "invalid_field_type"),
                    ("input.2", "invalid_field_type"),
                    ("input.key(None)", "invalid_field_type"),
                    ("input.None", "invalid_field_type"),
                    ("columns[0].name", "missing_required_field"),
                    ("columns.2[False]", "unknown_field"),
                    ("columns.2[3]", "unknown_field"),
                ]
            );
        }

        #[test]
        fn stringified_input_collisions_keep_first_position_and_last_value_with_origins() {
            let schema = schema_with_identifier(V::List(vec![
                V::text("str"),
                V::text("int"),
                V::text("bool"),
                V::text("null"),
            ]));
            let input = layer(vec![(
                "input",
                V::Pairs(vec![
                    (
                        V::integer("2"),
                        V::map(vec![("path", V::text("first.csv"))]),
                    ),
                    (
                        V::text("KEEP"),
                        V::map(vec![("path", V::text("middle.csv"))]),
                    ),
                    (V::text("2"), V::map(vec![("path", V::text("last.csv"))])),
                    (V::boolean(true), V::text("first-bool.csv")),
                    (V::text("True"), V::text("last-bool.csv")),
                    (V::Scalar(N::Null), V::text("first-null.csv")),
                    (V::text("None"), V::text("last-null.csv")),
                ]),
            )]);
            let output = schema.normalize_layer(&input).unwrap();
            assert_document(
                &output.document,
                &layer(vec![(
                    "input",
                    V::map(vec![
                        ("2", V::map(vec![("path", V::text("last.csv"))])),
                        ("KEEP", V::map(vec![("path", V::text("middle.csv"))])),
                        ("True", V::map(vec![("path", V::text("last-bool.csv"))])),
                        ("None", V::map(vec![("path", V::text("last-null.csv"))])),
                    ]),
                )]),
            );
            let input_members = get(&input, input.root(), "input");
            let N::Mapping(entries) = &input.nodes()[input_members] else {
                panic!("input map")
            };
            let output_members = get(&output.document, output.document.root(), "input");
            let N::Mapping(result) = &output.document.nodes()[output_members] else {
                panic!("output map")
            };
            assert_eq!(output.origins[result[0].0].node, entries[0].0);
            assert!(output.origins[result[0].0].generated);
            assert_eq!(output.origins[result[0].1].node, entries[2].1);
            assert!(!output.origins[result[0].1].generated);
        }

        #[test]
        fn overwritten_input_members_are_still_validated_before_key_selection() {
            let schema = schema_with_identifier(V::List(vec![V::text("str"), V::text("int")]));
            let input = layer(vec![(
                "input",
                V::Pairs(vec![
                    (V::integer("2"), V::map(vec![("path", V::boolean(false))])),
                    (V::text("2"), V::map(vec![("path", V::text("last.csv"))])),
                ]),
            )]);
            let Err(NormalizationError::Invalid(findings)) = schema.normalize_layer(&input) else {
                panic!("shadowed invalid member must fail")
            };
            assert_eq!(paths(&findings), [("input[2].path", "invalid_field_type")]);
        }
    }
    mod schema_type_expression {
        use yamaa_core::schema::{TypeError, TypeExpression, TypeNode, TypeResource};

        #[test]
        fn authored_nested_type_arena_retains_occurrences_and_original_spans() {
            let text = " dict[ str, list[dict[str, int]] ] ";
            let parsed = TypeExpression::parse(text).unwrap();
            assert_eq!(parsed.expression(), text);
            assert_eq!(parsed.names().collect::<Vec<_>>(), ["str", "str", "int"]);
            assert_eq!(
                parsed.nodes(),
                [
                    TypeNode::Name(7..10),
                    TypeNode::Name(22..25),
                    TypeNode::Name(27..30),
                    TypeNode::Dictionary { key: 1, value: 2 },
                    TypeNode::List(3),
                    TypeNode::Dictionary { key: 0, value: 4 },
                ]
            );
            assert_eq!(parsed.root(), 5);
        }

        #[test]
        fn builtins_and_unresolved_names_are_syntax_not_bundle_resolution() {
            for name in [
                "str",
                "int",
                "float",
                "bool",
                "null",
                "list",
                "dict",
                "missing_type",
                "_X9",
            ] {
                let parsed = TypeExpression::parse(name).unwrap();
                assert_eq!(parsed.names().collect::<Vec<_>>(), [name]);
                assert_eq!(parsed.nodes(), [TypeNode::Name(0..name.len())]);
            }
            let parsed = TypeExpression::parse("\u{1c}\u{a0}list[\tstr\n]\u{2003}").unwrap();
            assert_eq!(parsed.names().collect::<Vec<_>>(), ["str"]);
        }

        #[test]
        fn malformed_types_and_string_encoded_unions_are_rejected() {
            for text in [
                "",
                " ",
                "9name",
                "é",
                "str|int",
                "str int",
                "list[]",
                "list[str,int]",
                "dict[str]",
                "dict[,int]",
                "dict[str,]",
                "dict[str,int,bool]",
                "dict[str,list[int]",
                "list [str]",
                "set[str]",
                "list[str]]",
                "dict[str,int]junk",
            ] {
                assert!(
                    matches!(TypeExpression::parse(text), Err(TypeError::Invalid { .. })),
                    "{text:?}"
                );
            }
            assert_eq!(
                TypeExpression::parse("\u{a0}é"),
                Err(TypeError::Invalid { byte: 2 })
            );
        }

        #[test]
        fn fixed_type_depth_refuses_the_next_layer_and_fresh_parse_succeeds() {
            let admitted = format!("{}str{}", "list[".repeat(127), "]".repeat(127));
            assert!(TypeExpression::parse(&admitted).is_ok());
            let too_deep = format!("list[{admitted}]");
            assert_eq!(
                TypeExpression::parse(&too_deep),
                Err(TypeError::Limit {
                    resource: TypeResource::Depth,
                    limit: 128
                })
            );
            assert!(TypeExpression::parse("dict[str,int]").is_ok());
        }
    }
    mod schema_validation {
        use yamaa_core::schema::{
            Document, DocumentKind, DocumentNode as N, NormalizationError, NormalizationResource,
            NormalizedDocument, SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaSource,
            SchemaStructure,
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        fn descriptor(kind: &'static str) -> V {
            V::map(vec![("type", V::text(kind))])
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, descriptor(kind))])
        }
        fn bundle(fields: Vec<V>, declarations: Vec<(&'static str, V)>) -> SchemaStructure {
            let mut fields_with_version = vec![field("schema_version", "str")];
            fields_with_version.extend(fields);
            let mut entries = vec![
                ("version", V::text("1.0")),
                ("root_class", V::List(fields_with_version)),
            ];
            entries.extend(declarations);
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(entries)),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        #[test]
        fn invalid_default_preparation_never_bypasses_normalization_admission() {
            let mut schema = bundle(
                vec![V::map(vec![(
                    "value",
                    V::map(vec![("type", V::text("str")), ("default", V::integer("2"))]),
                )])],
                vec![],
            );
            let input = document(V::map(vec![("schema_version", V::text("1.0"))]));
            assert!(matches!(
                schema.normalize_document(&input),
                Err(NormalizationError::Defaults(_))
            ));
            let findings = schema.prepare_defaults().unwrap();
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].diagnostics[0].definition().condition,
                "invalid_field_type"
            );
            assert!(matches!(
                schema.normalize_document(&input),
                Err(NormalizationError::Defaults(_))
            ));
        }
        fn validate(
            schema: &SchemaStructure,
            field: &str,
            value: V,
            fragment: bool,
        ) -> Vec<SchemaDiagnostic> {
            let input = document(value);
            let descriptor = schema
                .root_class()
                .fields
                .iter()
                .find(|f| f.name == field)
                .unwrap()
                .descriptor;
            schema
                .validate_descriptor(descriptor, &input, input.root(), field, fragment)
                .unwrap()
        }
        fn findings(diagnostics: &[SchemaDiagnostic]) -> Vec<(&str, &str)> {
            diagnostics
                .iter()
                .map(|d| (d.path.as_str(), d.definition().condition))
                .collect()
        }

        #[test]
        fn incompatible_decoded_values_keep_all_portable_kinds() {
            let schema = bundle(vec![field("value", "int"), field("text", "str")], vec![]);
            for (value, kind, name, field) in [
                (V::Scalar(N::Null), DocumentKind::Null, "null", "value"),
                (V::boolean(true), DocumentKind::Boolean, "boolean", "value"),
                (V::integer("7"), DocumentKind::Integer, "integer", "text"),
                (V::float(1.5), DocumentKind::Float, "float", "value"),
                (V::text("x"), DocumentKind::Text, "text", "value"),
                (V::List(vec![]), DocumentKind::Sequence, "sequence", "value"),
                (V::map(vec![]), DocumentKind::Mapping, "mapping", "value"),
            ] {
                let result = validate(&schema, field, value, false);
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].context[1], ("actual", C::ValueKind(kind)));
                assert_eq!(kind.name(), name);
            }
            let input = document(V::Scalar(N::Null));
            let descriptor = schema.root_class().fields[1].descriptor;
            let result = schema
                .validate_descriptor(descriptor, &input, input.root(), "value", false)
                .unwrap();
            assert_eq!(
                result[0].context[1],
                ("actual", C::ValueKind(DocumentKind::Null))
            );
        }

        #[test]
        fn version_precedes_field_findings_and_classes_keep_schema_then_input_order() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "needed",
                        V::map(vec![
                            ("type", V::text("str")),
                            ("required", V::boolean(true)),
                        ]),
                    )]),
                    field("count", "int"),
                ],
                vec![],
            );
            let input = document(V::map(vec![
                ("unknown", V::Scalar(N::Null)),
                ("count", V::boolean(true)),
                ("schema_version", V::text("old")),
            ]));
            let result = schema.validate_document(&input).unwrap();
            assert_eq!(
                findings(&result),
                [("schema_version", "schema_version_mismatch")]
            );
            assert_eq!(result[0].context[0], ("expected", C::Text("1.0".into())));
            let input = document(V::map(vec![
                ("unknown", V::Scalar(N::Null)),
                ("count", V::boolean(true)),
                ("schema_version", V::text("1.0")),
            ]));
            let result = schema.validate_document(&input).unwrap();
            assert_eq!(
                findings(&result),
                [
                    ("needed", "missing_required_field"),
                    ("count", "invalid_field_type"),
                    ("unknown", "unknown_field"),
                ]
            );
            assert_eq!(
                result[1].context,
                [
                    ("expected", C::Text("int".into())),
                    ("actual", C::ValueKind(DocumentKind::Boolean))
                ]
            );
            for value in [V::Scalar(N::Null), V::List(vec![]), V::map(vec![])] {
                let result = schema.validate_document(&document(value)).unwrap();
                assert_eq!(findings(&result), [("$", "invalid_field_type")]);
            }
        }

        #[test]
        fn union_selects_success_then_matching_outer_type_and_recursion_is_value_local() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "choice",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("int"), V::text("record")]),
                        )]),
                    )]),
                    field("tree", "tree_value"),
                ],
                vec![
                    (
                        "record",
                        V::List(vec![V::map(vec![(
                            "id",
                            V::map(vec![
                                ("type", V::text("str")),
                                ("required", V::boolean(true)),
                            ]),
                        )])]),
                    ),
                    (
                        "tree_value",
                        V::map(vec![(
                            "type",
                            V::List(vec![
                                V::text("tree_value"),
                                V::text("list[tree_value]"),
                                V::text("str"),
                            ]),
                        )]),
                    ),
                ],
            );
            assert!(validate(&schema, "choice", V::integer("1"), false).is_empty());
            assert_eq!(
                findings(&validate(&schema, "choice", V::map(vec![]), false)),
                [("choice.id", "missing_required_field")]
            );
            assert_eq!(
                validate(&schema, "choice", V::boolean(false), false)[0].context[0],
                ("expected", C::Text("int".into()))
            );
            assert!(validate(
                &schema,
                "tree",
                V::List(vec![V::text("leaf"), V::List(vec![V::text("nested")])]),
                false
            )
            .is_empty());
            let result = validate(
                &schema,
                "tree",
                V::List(vec![V::text("leaf"), V::List(vec![V::boolean(true)])]),
                false,
            );
            assert_eq!(findings(&result), [("tree[1][0]", "invalid_field_type")]);
            assert_eq!(
                result[0].context[0],
                ("expected", C::Text("tree_value".into()))
            );
        }

        fn required_field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(
                name,
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(true)),
                ]),
            )])
        }
        fn operations_registry() -> Vec<(&'static str, V)> {
            vec![
                (
                    "expression",
                    V::map(vec![("registry", V::text("operations"))]),
                ),
                (
                    "operations",
                    V::map(vec![
                        ("source", descriptor("str")),
                        ("literal", descriptor("int")),
                    ]),
                ),
            ]
        }
        fn handled_bundle() -> SchemaStructure {
            let mut declarations = vec![
                (
                    "derivation",
                    V::map(vec![(
                        "type",
                        V::List(vec![
                            V::text("str"),
                            V::text("expression"),
                            V::text("handled_expression_class"),
                        ]),
                    )]),
                ),
                (
                    "handled_expression_class",
                    V::List(vec![
                        required_field("value", "expression"),
                        field("unconvertible", "int"),
                    ]),
                ),
            ];
            declarations.extend(operations_registry());
            bundle(vec![field("derive", "derivation")], declarations)
        }

        #[test]
        fn union_reports_inside_the_member_declaring_the_written_keys() {
            // Handled derivation: the written keys belong to
            // handled_expression_class, not to the expression registry
            // member tried before it, so the finding lands inside `value`.
            let schema = handled_bundle();
            let result = validate(
                &schema,
                "derive",
                V::map(vec![
                    ("value", V::map(vec![("literal", V::text("x"))])),
                    ("unconvertible", V::integer("0")),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [("derive.value.literal", "invalid_field_type")]
            );

            // Typed intermediate derivation: the written keys belong to
            // intermediate_binding_class, not to the untyped member tried
            // before it, so the finding lands inside `derivation`.
            let mut declarations = vec![
                (
                    "intermediate_derivation",
                    V::map(vec![(
                        "type",
                        V::List(vec![
                            V::text("untyped_intermediate_derivation"),
                            V::text("intermediate_binding_class"),
                        ]),
                    )]),
                ),
                (
                    "untyped_intermediate_derivation",
                    V::map(vec![(
                        "type",
                        V::List(vec![
                            V::text("str"),
                            V::text("expression"),
                            V::text("intermediate_expression_class"),
                        ]),
                    )]),
                ),
                (
                    "intermediate_expression_class",
                    V::List(vec![required_field("value", "expression")]),
                ),
                (
                    "intermediate_binding_class",
                    V::List(vec![
                        required_field("type", "str"),
                        required_field("derivation", "untyped_intermediate_derivation"),
                        field("unconvertible", "int"),
                    ]),
                ),
            ];
            declarations.extend(operations_registry());
            let schema = bundle(
                vec![field("binding", "intermediate_derivation")],
                declarations,
            );
            let result = validate(
                &schema,
                "binding",
                V::map(vec![
                    ("type", V::text("str")),
                    ("derivation", V::map(vec![("literal", V::text("x"))])),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [("binding.derivation.literal", "invalid_field_type")]
            );
        }

        #[test]
        fn union_keeps_the_previous_fallback_when_no_member_declares_the_keys() {
            let schema = handled_bundle();
            // An empty mapping declares no written keys.
            assert_eq!(
                findings(&validate(&schema, "derive", V::map(vec![]), false)),
                [("derive", "invalid_operation_count")]
            );
            // A key that no member declares keeps the registry fallback.
            assert_eq!(
                findings(&validate(
                    &schema,
                    "derive",
                    V::map(vec![("absent", V::Scalar(N::Null))]),
                    false
                )),
                [("derive.absent", "unknown_operation")]
            );
        }

        #[test]
        fn case_otherwise_reports_inside_the_otherwise_member() {
            let schema = case_bundle("expressions");
            let result = validate(
                &schema,
                "derive",
                case(vec![
                    branch(V::integer(1)),
                    otherwise(V::map(vec![("literal", V::text("x"))])),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [("derive.case[1].otherwise.literal", "invalid_field_type")]
            );
        }

        #[test]
        fn fragments_suppress_nested_requiredness_but_keep_type_and_unknown_field_checks() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![(
                    "record",
                    V::List(vec![
                        field("name", "str"),
                        V::map(vec![(
                            "id",
                            V::map(vec![
                                ("type", V::text("int")),
                                ("required", V::boolean(true)),
                            ]),
                        )]),
                    ]),
                )],
            );
            assert_eq!(
                findings(&validate(
                    &schema,
                    "items",
                    V::List(vec![V::map(vec![("name", V::text("DM"))])]),
                    false
                )),
                [("items.DM.id", "missing_required_field")]
            );
            assert!(validate(
                &schema,
                "items",
                V::List(vec![V::map(vec![("name", V::text("DM"))])]),
                true
            )
            .is_empty());
            assert_eq!(
                findings(&validate(
                    &schema,
                    "items",
                    V::List(vec![V::map(vec![
                        ("name", V::text("DM")),
                        ("extra", V::Scalar(N::Null)),
                        ("id", V::boolean(false))
                    ])]),
                    true
                )),
                [
                    ("items.DM.id", "invalid_field_type"),
                    ("items.DM.extra", "unknown_field"),
                ]
            );
        }

        #[test]
        fn constraints_keep_order_unicode_lengths_and_alias_requirements_after_type_success() {
            let schema = bundle(
                vec![field("kind", "column_type"), field("number", "float")],
                vec![(
                    "column_type",
                    V::map(vec![
                        ("type", V::text("str")),
                        ("values", V::List(vec![V::text("ok")])),
                        ("pattern", V::text("[A-Z]+")),
                        ("min_length", V::integer("2")),
                    ]),
                )],
            );
            let result = validate(&schema, "kind", V::text("\u{e9}"), false);
            assert_eq!(
                findings(&result),
                [
                    ("kind", "value_not_permitted"),
                    ("kind", "pattern_mismatch"),
                    ("kind", "minimum_length")
                ]
            );
            assert!(result
                .iter()
                .all(|d| d.definition().requirement == Some("REQ-0012")));
            let result = validate(&schema, "kind", V::boolean(true), false);
            assert_eq!(findings(&result), [("kind", "invalid_field_type")]);
            assert_eq!(result[0].definition().requirement, Some("REQ-0287"));
            assert!(validate(
                &schema,
                "number",
                V::integer("99999999999999999999999999999999999999999999999999"),
                false
            )
            .is_empty());
            assert!(!validate(&schema, "number", V::boolean(false), false).is_empty());
        }

        #[test]
        fn defaults_resolve_forward_aliases_and_return_the_declaring_descriptor() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "good",
                        V::map(vec![
                            ("type", V::text("later")),
                            ("default", V::text("valid")),
                        ]),
                    )]),
                    V::map(vec![(
                        "bad",
                        V::map(vec![
                            ("type", V::text("later")),
                            ("default", V::integer("9")),
                        ]),
                    )]),
                ],
                vec![("later", descriptor("str"))],
            );
            let result = schema.validate_defaults().unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(
                schema.descriptors()[result[0].descriptor].path,
                "root_class.bad"
            );
            assert_eq!(
                findings(&result[0].diagnostics),
                [("root_class.bad.default", "invalid_field_type")]
            );
            assert_eq!(
                result[0].diagnostics[0].context[0],
                ("expected", C::Text("later".into()))
            );
        }

        #[test]
        fn registry_and_bare_derivation_diagnostics_preserve_paths_and_context() {
            let schema = bundle(
                vec![field("derive", "derivation")],
                vec![
                    (
                        "derivation",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("str"), V::text("expression")]),
                        )]),
                    ),
                    (
                        "expression",
                        V::map(vec![("registry", V::text("operations"))]),
                    ),
                    (
                        "operations",
                        V::map(vec![
                            ("source", descriptor("str")),
                            ("literal", descriptor("int")),
                        ]),
                    ),
                ],
            );
            assert!(validate(
                &schema,
                "derive",
                V::map(vec![("source", V::text("DM.AGE"))]),
                false
            )
            .is_empty());
            assert_eq!(
                findings(&validate(&schema, "derive", V::map(vec![]), false)),
                [("derive", "invalid_operation_count")]
            );
            assert_eq!(
                findings(&validate(
                    &schema,
                    "derive",
                    V::map(vec![("absent", V::Scalar(N::Null))]),
                    false
                )),
                [("derive.absent", "unknown_operation")]
            );
            let result = validate(&schema, "derive", V::integer("4"), false);
            assert_eq!(findings(&result), [("derive", "bare_derivation_scalar")]);
            assert_eq!(result[0].definition().requirement, Some("REQ-0320"));
        }

        fn case_bundle(registry: &'static str) -> SchemaStructure {
            let required = |kind: &'static str| {
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(true)),
                ])
            };
            bundle(
                vec![field("derive", "expression")],
                vec![
                    ("expression", V::map(vec![("registry", V::text(registry))])),
                    (
                        registry,
                        V::map(vec![
                            ("case", descriptor("list[case_item_class]")),
                            ("literal", descriptor("int")),
                        ]),
                    ),
                    (
                        "case_item_class",
                        V::map(vec![(
                            "type",
                            V::List(vec![
                                V::text("case_branch_class"),
                                V::text("case_otherwise_class"),
                            ]),
                        )]),
                    ),
                    (
                        "case_branch_class",
                        V::List(vec![
                            V::map(vec![("when", required("str"))]),
                            V::map(vec![("then", required("case_result"))]),
                        ]),
                    ),
                    (
                        "case_otherwise_class",
                        V::List(vec![V::map(vec![("otherwise", required("case_result"))])]),
                    ),
                    (
                        "case_result",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("int"), V::text("expression")]),
                        )]),
                    ),
                ],
            )
        }
        fn branch(then: V) -> V {
            V::map(vec![("when", V::text("AGE >= 65")), ("then", then)])
        }
        fn otherwise(value: V) -> V {
            V::map(vec![("otherwise", value)])
        }
        fn case(items: Vec<V>) -> V {
            V::map(vec![("case", V::List(items))])
        }

        #[test]
        fn case_item_order_is_a_req_0339_finding_after_every_item_matched_its_class() {
            let schema = case_bundle("expressions");
            for items in [
                vec![branch(V::integer(1)), otherwise(V::integer(0))],
                vec![branch(V::integer(1))],
                vec![
                    branch(V::integer(1)),
                    branch(V::integer(2)),
                    otherwise(V::integer(0)),
                ],
            ] {
                assert!(validate(&schema, "derive", case(items), false).is_empty());
            }
            let no_branch = "at least one when/then item";
            let not_last = "otherwise only as the last item";
            for (items, expected) in [
                (
                    vec![otherwise(V::integer(0))],
                    vec![("derive.case", no_branch)],
                ),
                (vec![], vec![("derive.case", no_branch)]),
                (
                    vec![otherwise(V::integer(0)), branch(V::integer(1))],
                    vec![("derive.case[0]", not_last)],
                ),
                (
                    vec![
                        branch(V::integer(1)),
                        otherwise(V::integer(0)),
                        otherwise(V::integer(2)),
                    ],
                    vec![("derive.case[1]", not_last)],
                ),
                (
                    vec![otherwise(V::integer(0)), otherwise(V::integer(2))],
                    vec![("derive.case", no_branch), ("derive.case[0]", not_last)],
                ),
                (
                    vec![
                        branch(case(vec![otherwise(V::integer(1))])),
                        otherwise(V::integer(0)),
                    ],
                    vec![("derive.case[0].then.case", no_branch)],
                ),
            ] {
                for fragment in [false, true] {
                    let result = validate(&schema, "derive", case(items.clone()), fragment);
                    let actual: Vec<_> = result
                        .iter()
                        .map(|d| {
                            let definition = d.definition();
                            assert_eq!(
                                (
                                    definition.phase,
                                    definition.condition,
                                    definition.requirement
                                ),
                                ("validation", "invalid_field_type", Some("REQ-0339"))
                            );
                            let [("expected", C::Text(text))] = d.context.as_slice() else {
                                panic!("expected context: {d:?}");
                            };
                            (d.path.as_str(), text.as_str())
                        })
                        .collect();
                    assert_eq!(actual, expected, "fragment={fragment}");
                }
            }
            // Item findings come first; the order rule waits for well-formed items.
            let result = validate(
                &schema,
                "derive",
                case(vec![otherwise(V::text("N"))]),
                false,
            );
            assert!(!result.is_empty());
            assert!(
                result
                    .iter()
                    .all(|d| d.definition().requirement != Some("REQ-0339")),
                "{result:?}"
            );
            // The rule belongs to the expression registry's case, not to a like-named entry.
            let other = case_bundle("operations");
            assert!(validate(
                &other,
                "derive",
                case(vec![otherwise(V::integer(0))]),
                false
            )
            .is_empty());
        }

        #[test]
        fn typed_dictionary_keys_have_independent_key_and_value_paths() {
            let schema = bundle(vec![field("mapping", "dict[str, int]")], vec![]);
            for (key, spelling, valuepath) in [
                (V::float(0.0), "0.0", "mapping.0.0"),
                (V::float(-0.0), "-0.0", "mapping.-0.0"),
                (V::float(1.0), "1.0", "mapping.1.0"),
                (V::float(1e-5), "1e-05", "mapping.1e-05"),
                (V::float(1e16), "1e+16", "mapping.1e+16"),
                (V::float(f64::from_bits(1)), "5e-324", "mapping.5e-324"),
                (V::integer("42"), "42", "mapping[42]"),
                (V::boolean(true), "True", "mapping[True]"),
                (V::Scalar(N::Null), "None", "mapping.None"),
            ] {
                let result = validate(
                    &schema,
                    "mapping",
                    V::Pairs(vec![(key, V::boolean(false))]),
                    false,
                );
                assert_eq!(
                    findings(&result),
                    [
                        (
                            format!("mapping.key({spelling})").as_str(),
                            "invalid_field_type"
                        ),
                        (valuepath, "invalid_field_type")
                    ]
                );
            }
        }

        #[test]
        fn independent_union_attempts_do_not_publish_failed_alternative_findings() {
            let schema = bundle(
                vec![V::map(vec![(
                    "choice",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("int"), V::text("str")]),
                    )]),
                )])],
                vec![],
            );
            let descriptor = schema.root_class().fields[1].descriptor;
            let input = document(V::text("valid"));
            for _ in 0..3 {
                assert!(schema
                    .validate_descriptor(descriptor, &input, input.root(), "choice", false)
                    .unwrap()
                    .is_empty());
            }
        }

        fn normalize(
            schema: &SchemaStructure,
            field: &str,
            value: V,
            fragment: bool,
        ) -> NormalizedDocument {
            let input = document(value);
            let descriptor = schema
                .root_class()
                .fields
                .iter()
                .find(|f| f.name == field)
                .unwrap()
                .descriptor;
            schema
                .normalize_descriptor(descriptor, &input, input.root(), fragment)
                .unwrap()
        }

        /// Compare independently authored trees, including mapping order, without arena layout coupling.
        fn assert_value(actual: &Document, expected: V) {
            fn compare(a: &Document, ai: usize, b: &Document, bi: usize) {
                match (&a.nodes()[ai], &b.nodes()[bi]) {
                    (N::Sequence(aids), N::Sequence(bids)) => {
                        assert_eq!(aids.len(), bids.len());
                        for (&ai, &bi) in aids.iter().zip(bids) {
                            compare(a, ai, b, bi);
                        }
                    }
                    (N::Mapping(aids), N::Mapping(bids)) => {
                        assert_eq!(aids.len(), bids.len());
                        for (&(ak, av), &(bk, bv)) in aids.iter().zip(bids) {
                            compare(a, ak, b, bk);
                            compare(a, av, b, bv);
                        }
                    }
                    (a, b) => assert_eq!(a, b),
                }
            }
            let expected = document(expected);
            compare(actual, actual.root(), &expected, expected.root());
        }

        #[test]
        fn collection_normalization_prioritizes_canonical_lists_even_when_t_is_a_list() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "simple",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("str"), V::text("list[str]")]),
                        )]),
                    )]),
                    V::map(vec![(
                        "nested",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("list[str]"), V::text("list[list[str]]")]),
                        )]),
                    )]),
                    V::map(vec![(
                        "larger",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("str"), V::text("list[str]"), V::text("null")]),
                        )]),
                    )]),
                ],
                vec![],
            );
            let result = normalize(&schema, "simple", V::text("DM"), false);
            assert_value(&result.document, V::List(vec![V::text("DM")]));
            assert!(result.origins[result.document.root()].generated);
            assert!(result
                .origins
                .iter()
                .all(|o| o.source == SchemaSource::Input));
            assert_value(
                &normalize(
                    &schema,
                    "simple",
                    V::List(vec![V::text("DM"), V::text("AE")]),
                    false,
                )
                .document,
                V::List(vec![V::text("DM"), V::text("AE")]),
            );
            assert_value(
                &normalize(&schema, "nested", V::List(vec![V::text("DM")]), false).document,
                V::List(vec![V::List(vec![V::text("DM")])]),
            );
            assert_value(
                &normalize(
                    &schema,
                    "nested",
                    V::List(vec![V::List(vec![V::text("DM")])]),
                    false,
                )
                .document,
                V::List(vec![V::List(vec![V::text("DM")])]),
            );
            assert_value(
                &normalize(&schema, "nested", V::List(vec![]), false).document,
                V::List(vec![]),
            );
            assert_value(
                &normalize(&schema, "larger", V::text("DM"), false).document,
                V::text("DM"),
            );
        }

        #[test]
        fn class_shorthand_materializes_defaults_but_fragments_keep_only_authored_values() {
            let schema = bundle(
                vec![V::map(vec![(
                    "item",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("str"), V::text("record")]),
                    )]),
                )])],
                vec![(
                    "record",
                    V::List(vec![
                        V::map(vec![(
                            "enabled",
                            V::map(vec![
                                ("type", V::text("bool")),
                                ("default", V::boolean(true)),
                            ]),
                        )]),
                        V::map(vec![(
                            "name",
                            V::map(vec![
                                ("type", V::text("str")),
                                ("required", V::boolean(true)),
                            ]),
                        )]),
                        field("optional", "int"),
                    ]),
                )],
            );
            let result = normalize(&schema, "item", V::text("DM"), false);
            assert_value(
                &result.document,
                V::map(vec![("name", V::text("DM")), ("enabled", V::boolean(true))]),
            );
            let enabled = result
                .document
                .field(result.document.root(), "enabled")
                .unwrap();
            assert!(matches!(
                result.origins[enabled].source,
                SchemaSource::Default { module: 0, .. }
            ));
            assert!(!result.origins[enabled].generated);
            assert_value(
                &normalize(&schema, "item", V::text("DM"), true).document,
                V::map(vec![("name", V::text("DM"))]),
            );
            assert_value(
                &normalize(
                    &schema,
                    "item",
                    V::map(vec![("name", V::text("DM"))]),
                    false,
                )
                .document,
                V::map(vec![("enabled", V::boolean(true)), ("name", V::text("DM"))]),
            );
            assert_value(
                &normalize(&schema, "item", V::map(vec![]), true).document,
                V::map(vec![]),
            );
        }

        #[test]
        fn derivation_calls_redispatch_through_registry_and_class_while_case_results_remain_sources(
        ) {
            let schema = bundle(
                vec![
                    field("derive", "derivation"),
                    field("result", "case_result"),
                ],
                vec![
                    (
                        "derivation",
                        V::map(vec![(
                            "type",
                            V::List(vec![
                                V::text("str"),
                                V::text("expression"),
                                V::text("handled"),
                            ]),
                        )]),
                    ),
                    (
                        "case_result",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("str"), V::text("expression")]),
                        )]),
                    ),
                    (
                        "expression",
                        V::map(vec![("registry", V::text("operations"))]),
                    ),
                    (
                        "handled",
                        V::List(vec![
                            V::map(vec![(
                                "value",
                                V::map(vec![
                                    ("type", V::text("expression")),
                                    ("required", V::boolean(true)),
                                ]),
                            )]),
                            V::map(vec![(
                                "fallback",
                                V::map(vec![
                                    ("type", V::text("int")),
                                    ("default", V::integer("0")),
                                ]),
                            )]),
                        ]),
                    ),
                    (
                        "operations",
                        V::map(vec![
                            ("source", descriptor("str")),
                            ("to_number", descriptor("expression")),
                        ]),
                    ),
                ],
            );
            assert_value(
                &normalize(&schema, "derive", V::text("DM.AGE"), false).document,
                V::map(vec![
                    ("value", V::map(vec![("source", V::text("DM.AGE"))])),
                    ("fallback", V::integer("0")),
                ]),
            );
            assert_value(
                &normalize(&schema, "derive", V::text("  to_number( DM.AGE )  "), false).document,
                V::map(vec![
                    (
                        "value",
                        V::map(vec![(
                            "to_number",
                            V::map(vec![("source", V::text("DM.AGE"))]),
                        )]),
                    ),
                    ("fallback", V::integer("0")),
                ]),
            );
            assert_value(
                &normalize(&schema, "derive", V::text("to_number(DM.AGE)"), true).document,
                V::map(vec![(
                    "value",
                    V::map(vec![(
                        "to_number",
                        V::map(vec![("source", V::text("DM.AGE"))]),
                    )]),
                )]),
            );
            assert_value(
                &normalize(&schema, "result", V::text("to_number(DM.AGE)"), false).document,
                V::map(vec![("source", V::text("to_number(DM.AGE)"))]),
            );
            for source in ["nested(call(DM.AGE))", "1invalid(DM.AGE)", "op (DM.AGE)"] {
                assert_value(
                    &normalize(&schema, "derive", V::text(source), false).document,
                    V::map(vec![
                        ("value", V::map(vec![("source", V::text(source))])),
                        ("fallback", V::integer("0")),
                    ]),
                );
            }
        }

        #[test]
        fn recursive_defaults_are_resource_refusals_instead_of_silently_truncated_objects() {
            let schema = bundle(
                vec![field("tree", "record")],
                vec![(
                    "record",
                    V::List(vec![V::map(vec![(
                        "child",
                        V::map(vec![
                            ("type", V::text("record")),
                            ("default", V::map(vec![])),
                        ]),
                    )])]),
                )],
            );
            let input = document(V::map(vec![]));
            let descriptor = schema.root_class().fields[1].descriptor;
            assert_eq!(
                schema.normalize_descriptor(descriptor, &input, input.root(), false),
                Err(NormalizationError::Limit {
                    resource: NormalizationResource::Depth,
                    limit: 128
                })
            );
            assert_value(
                &normalize(&schema, "tree", V::map(vec![]), true).document,
                V::map(vec![]),
            );
        }

        #[test]
        fn normalization_validates_written_constraints_and_defaults_before_allocating_output() {
            let schema = bundle(
                vec![V::map(vec![(
                    "name",
                    V::map(vec![
                        ("type", V::text("str")),
                        ("values", V::List(vec![V::text("ok")])),
                    ]),
                )])],
                vec![],
            );
            let input = document(V::text("bad"));
            assert!(matches!(
                schema.normalize_descriptor(
                    schema.root_class().fields[1].descriptor,
                    &input,
                    input.root(),
                    false
                ),
                Err(NormalizationError::Invalid(_))
            ));
            let invalid_defaults = bundle(
                vec![V::map(vec![(
                    "bad",
                    V::map(vec![("type", V::text("str")), ("default", V::integer("1"))]),
                )])],
                vec![],
            );
            assert!(matches!(
                invalid_defaults.normalize_descriptor(
                    invalid_defaults.root_class().fields[1].descriptor,
                    &document(V::text("valid")),
                    0,
                    true
                ),
                Err(NormalizationError::Defaults(_))
            ));
        }

        #[test]
        fn independent_normalization_keeps_complete_results_and_original_input() {
            let schema = bundle(
                vec![V::map(vec![(
                    "value",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("str"), V::text("list[str]")]),
                    )]),
                )])],
                vec![],
            );
            let input = document(V::text("DM"));
            let before = input.clone();
            let descriptor = schema.root_class().fields[1].descriptor;
            let first = schema
                .normalize_descriptor(descriptor, &input, input.root(), false)
                .unwrap();
            let second = schema
                .normalize_descriptor(descriptor, &input, input.root(), false)
                .unwrap();
            assert_eq!(first, second);
            assert_eq!(input, before);
            assert_value(&first.document, V::List(vec![V::text("DM")]));
        }

        #[test]
        fn whole_document_normalization_keeps_schema_order_and_nested_default_provenance() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "groups",
                        V::map(vec![
                            ("type", V::text("dict[str, list[record]]")),
                            (
                                "default",
                                V::map(vec![(
                                    "DM",
                                    V::List(vec![V::map(vec![("name", V::text("AGE"))])]),
                                )]),
                            ),
                        ]),
                    )]),
                    field("explicit", "str"),
                ],
                vec![(
                    "record",
                    V::List(vec![
                        field("name", "str"),
                        V::map(vec![(
                            "enabled",
                            V::map(vec![
                                ("type", V::text("bool")),
                                ("default", V::boolean(true)),
                            ]),
                        )]),
                    ]),
                )],
            );
            let input = document(V::map(vec![
                ("explicit", V::text("written")),
                ("schema_version", V::text("1.0")),
            ]));
            let result = schema.normalize_document(&input).unwrap();
            assert_value(
                &result.document,
                V::map(vec![
                    ("schema_version", V::text("1.0")),
                    (
                        "groups",
                        V::map(vec![(
                            "DM",
                            V::List(vec![V::map(vec![
                                ("name", V::text("AGE")),
                                ("enabled", V::boolean(true)),
                            ])]),
                        )]),
                    ),
                    ("explicit", V::text("written")),
                ]),
            );
            assert_eq!(result.origins.len(), result.document.nodes().len());
            for (node, origin) in result.document.nodes().iter().zip(&result.origins) {
                if matches!(node, N::Text(text) if text == "AGE")
                    || matches!(node, N::Boolean(true))
                {
                    let SchemaSource::Default { module, descriptor } = origin.source else {
                        panic!("lost default provenance")
                    };
                    assert_eq!(
                        *node,
                        schema.modules()[module].document.nodes()[origin.node]
                    );
                    assert!(schema.descriptors()[descriptor]
                        .descriptor
                        .default_node()
                        .is_some());
                }
            }
            let fragment = normalize(
                &schema,
                "groups",
                V::map(vec![(
                    "DM",
                    V::List(vec![V::map(vec![("name", V::text("AGE"))])]),
                )]),
                true,
            );
            assert_value(
                &fragment.document,
                V::map(vec![(
                    "DM",
                    V::List(vec![V::map(vec![("name", V::text("AGE"))])]),
                )]),
            );
            assert!(fragment
                .origins
                .iter()
                .all(|origin| origin.source == SchemaSource::Input));
        }

        #[test]
        fn shorthand_checks_the_written_member_without_reinterpreting_generated_fields() {
            let schema = bundle(
                vec![V::map(vec![(
                    "item",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("str"), V::text("record")]),
                    )]),
                )])],
                vec![(
                    "record",
                    V::List(vec![V::map(vec![(
                        "name",
                        V::map(vec![
                            ("type", V::text("str")),
                            ("required", V::boolean(true)),
                            ("values", V::List(vec![V::text("canonical-only")])),
                        ]),
                    )])]),
                )],
            );
            assert_value(
                &normalize(&schema, "item", V::text("written-string"), false).document,
                V::map(vec![("name", V::text("written-string"))]),
            );
            let input = document(V::map(vec![("name", V::text("written-string"))]));
            let result = schema.normalize_descriptor(
                schema.root_class().fields[1].descriptor,
                &input,
                input.root(),
                false,
            );
            assert!(
                matches!(result, Err(NormalizationError::Invalid(ref findings)) if findings[0].definition().condition == "value_not_permitted")
            );
        }

        #[test]
        fn malformed_compound_names_and_ids_remain_ordered_validation_findings() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![(
                    "record",
                    V::List(vec![field("name", "str"), field("id", "int")]),
                )],
            );
            let result = validate(
                &schema,
                "items",
                V::List(vec![
                    V::map(vec![(
                        "name",
                        V::List(vec![
                            V::text("DM"),
                            V::integer("2"),
                            V::boolean(true),
                            V::Scalar(N::Null),
                            V::float(-0.0),
                        ]),
                    )]),
                    V::map(vec![(
                        "id",
                        V::map(vec![("inner", V::List(vec![V::text("value")]))]),
                    )]),
                    V::map(vec![("name", V::List(vec![])), ("id", V::integer("10"))]),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [
                    (
                        "items.['DM', 2, True, None, -0.0].name",
                        "invalid_field_type"
                    ),
                    ("items.{'inner': ['value']}.id", "invalid_field_type"),
                    ("items.[].name", "invalid_field_type"),
                ]
            );
            assert_eq!(
                result[0].context,
                [
                    ("expected", C::Text("str".into())),
                    ("actual", C::ValueKind(DocumentKind::Sequence))
                ]
            );
        }

        #[test]
        fn compound_path_quotes_escape_only_the_required_characters_without_normalization() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![("record", V::List(vec![field("name", "str")]))],
            );
            let result = validate(
                &schema,
                "items",
                V::List(vec![V::map(vec![(
                    "name",
                    V::List(vec![
                        V::text("single'quote"),
                        V::text("both'\"quotes"),
                        V::text("\\\n\r\t\0"),
                        V::text("\u{e9} e\u{301} \u{1f600}"),
                        V::text("\u{a0}\u{2028}\u{202e}\u{e000}\u{0378}\u{10ffff}"),
                    ]),
                )])]),
                false,
            );
            assert_eq!(findings(&result), [(
        "items.[\"single'quote\", 'both\\'\"quotes', '\\\\\\n\\r\\t\\x00', '\u{e9} e\u{301} \u{1f600}', '\\xa0\\u2028\\u202e\\ue000\\u0378\\U0010ffff'].name",
        "invalid_field_type",
    )]);
        }

        #[test]
        fn invalid_compound_labels_keep_the_complete_authored_path() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![("record", V::List(vec![field("name", "str")]))],
            );
            let input = document(V::List(vec![V::map(vec![(
                "name",
                V::List(vec![V::text("oversized-label")]),
            )])]));
            let descriptor = schema.root_class().fields[1].descriptor;
            let result = schema
                .validate_descriptor(descriptor, &input, input.root(), "items", false)
                .unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].definition().condition, "invalid_field_type");
            assert_eq!(result[0].path, "items.['oversized-label'].name");
        }

        #[test]
        fn quoted_diagnostic_text_preserves_each_complete_unicode_spelling() {
            use yamaa_core::schema::quoted_diagnostic_text;
            assert_eq!(quoted_diagnostic_text("a'b"), "\"a'b\"");
            assert_eq!(quoted_diagnostic_text("\u{a0}\n\u{1f600}"), "'\\xa0\\n😀'");
            for value in ["a", "b", "c"] {
                assert_eq!(quoted_diagnostic_text(value), format!("'{value}'"));
            }
        }
    }
}
