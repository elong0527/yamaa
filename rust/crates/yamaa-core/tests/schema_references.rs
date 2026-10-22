//! Schema contracts: resolution pruning, schema layer admission, schema type
//! expression and schema validation.
mod schema {
    mod resolution_pruning {
        //! LAYER-11 pruning and the LAYER-13 order on the AST (#3134), over
        //! documents in the normalized form both lifecycles prune.
        use yamaa_core::expression::{CompileError, Context};
        use yamaa_core::resolution::{prune, Error as E, Issue as I, Order};
        use yamaa_core::schema::{Document, DocumentNode as N, NormalizedDocument, SchemaSource};

        use crate::support::document::Tree as V;

        fn text(s: &str) -> V {
            V::text(s)
        }
        fn texts(names: &[&str]) -> V {
            V::List(names.iter().map(|name| text(name)).collect())
        }
        fn op(name: &str, payload: V) -> V {
            V::map(vec![(name, payload)])
        }
        fn variable(name: &str) -> V {
            V::map(vec![("variable", text(name))])
        }
        fn source(name: &str) -> V {
            op("source", variable(name))
        }
        fn compute(expr: &str) -> V {
            op("compute", V::map(vec![("expr", text(expr))]))
        }
        fn literal(value: &str) -> V {
            op("literal", text(value))
        }
        fn aggregate(fields: Vec<(&str, V)>) -> V {
            op("aggregate", V::map(fields))
        }
        fn derivation(expression: V) -> V {
            V::map(vec![("value", expression)])
        }
        fn column(name: &str, expression: V) -> V {
            V::map(vec![
                ("name", text(name)),
                ("type", text("str")),
                ("derivation", derivation(expression)),
            ])
        }
        fn bare(name: &str) -> V {
            V::map(vec![("name", text(name)), ("type", text("str"))])
        }
        fn binding(name: &str, expression: V) -> V {
            V::map(vec![
                ("name", text(name)),
                ("type", text("float")),
                ("derivation", derivation(expression)),
            ])
        }
        fn dataset(path: &str) -> V {
            V::map(vec![("path", text(path))])
        }
        /// A spec whose output names `output`, with `fields` after it.
        fn tree(output: &[&str], fields: Vec<(&str, V)>) -> V {
            let mut all = vec![
                ("schema_version", text("1.0")),
                ("domain", text("TEST")),
                (
                    "output",
                    V::map(vec![("path", text("out.csv")), ("columns", texts(output))]),
                ),
            ];
            all.extend(fields);
            V::map(all)
        }
        fn spec(output: &[&str], fields: Vec<(&str, V)>) -> Document {
            tree(output, fields).document()
        }
        fn names(d: &Document, collection: &str, identity: &str) -> Vec<String> {
            let Some(node) = d.field(d.root(), collection) else {
                return Vec::new();
            };
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
        fn inputs(d: &Document) -> Vec<String> {
            d.field(d.root(), "input")
                .map(|node| keys(d, node))
                .unwrap_or_default()
        }
        fn columns(result: &NormalizedDocument) -> Vec<String> {
            names(&result.document, "columns", "name")
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
        fn only_semantic_roles_read_names() {
            // A label, a metadata value and a row id name columns without
            // reading them; `base` and a row's dataset are live inputs.
            let mut result = column("RESULT", source("DM.VALUE"));
            result.set(&["label"], text("LABEL"));
            let input = spec(
                &["RESULT"],
                vec![
                    ("base", text("DM")),
                    (
                        "input",
                        V::map(vec![
                            ("DM", dataset("dm.csv")),
                            ("EX", dataset("ex.csv")),
                            ("UNUSED", dataset("unused.csv")),
                        ]),
                    ),
                    ("keys", texts(&["ID"])),
                    ("metadata", V::map(vec![("note", text("NOTE"))])),
                    (
                        "columns",
                        V::List(vec![
                            column("ID", source("DM.ID")),
                            result,
                            column("LABEL", literal("label")),
                            column("NOTE", literal("note")),
                            column("ROW", literal("row")),
                        ]),
                    ),
                    (
                        "rows",
                        V::List(vec![V::map(vec![
                            ("id", text("ROW")),
                            ("dataset", text("EX")),
                            ("derivations", V::map(vec![])),
                        ])]),
                    ),
                ],
            );
            for order in [Order::Declared, Order::Dependencies] {
                let result = prune(&input, order).unwrap();
                assert_eq!(inputs(&result.document), ["DM", "EX"]);
                assert_eq!(columns(&result), ["ID", "RESULT"]);
            }
        }

        #[test]
        fn literal_values_are_not_reads() {
            let case = op(
                "case",
                V::List(vec![
                    V::map(vec![
                        ("when", text("FLAG = 'Y'")),
                        ("then", literal("WRONG")),
                    ]),
                    V::map(vec![("otherwise", source("OTHER"))]),
                ]),
            );
            let flag = op(
                "flag",
                V::map(vec![
                    ("condition", text("FLAG = 'Y'")),
                    ("true_value", text("WRONG")),
                ]),
            );
            let input = spec(
                &["OUT", "MARK"],
                vec![(
                    "columns",
                    V::List(vec![
                        column("OUT", case),
                        column("MARK", flag),
                        column("FLAG", literal("Y")),
                        column("WRONG", literal("w")),
                        column("OTHER", literal("o")),
                    ]),
                )],
            );
            let result = prune(&input, Order::Declared).unwrap();
            assert_eq!(columns(&result), ["OUT", "MARK", "FLAG", "OTHER"]);
        }

        #[test]
        fn dictionary_entries_are_values_not_reads() {
            let mapping = op(
                "mapping",
                V::map(vec![
                    ("source", variable("CODE")),
                    ("dict", V::map(vec![("KEY", text("VALUE"))])),
                ]),
            );
            let input = spec(
                &["OUT", "SPARE"],
                vec![(
                    "columns",
                    V::List(vec![
                        column("OUT", mapping),
                        column("CODE", literal("c")),
                        column("KEY", literal("k")),
                        column("VALUE", literal("v")),
                        bare("SPARE"),
                    ]),
                )],
            );
            let result = prune(&input, Order::Declared).unwrap();
            assert_eq!(columns(&result), ["OUT", "CODE", "SPARE"]);
        }

        #[test]
        fn closed_languages_retain_field_and_record_count_dependencies() {
            // Every grammar is parsed for its names: a numeric expression, a
            // reducer expression, a predicate and a string template (COLUMN-3).
            // Each name keeps its column live and orders it first.
            let flag = |predicate: &str| op("flag", V::map(vec![("condition", text(predicate))]));
            let template =
                |template: &str| op("str_template", V::map(vec![("template", text(template))]));
            for (expression, live_columns, live_inputs) in [
                (compute("A + DM.X + A"), &["A", "OUT"][..], &["DM"][..]),
                (
                    aggregate(vec![("expr", text("COUNT(EX.*) + SUM(EX.DOSE)"))]),
                    &["OUT"][..],
                    &["EX"][..],
                ),
                (
                    aggregate(vec![("expr", text("COUNT(EX.*)"))]),
                    &["OUT"][..],
                    &["EX"][..],
                ),
                (
                    flag("A = 'LITERAL' AND DM.X > 0"),
                    &["A", "OUT"][..],
                    &["DM"][..],
                ),
                (
                    template("{{{A}}} {DM.X} {A}"),
                    &["A", "OUT"][..],
                    &["DM"][..],
                ),
            ] {
                let input = spec(
                    &["OUT"],
                    vec![
                        (
                            "input",
                            V::map(vec![
                                ("DM", dataset("dm.csv")),
                                ("EX", dataset("ex.csv")),
                                ("UNUSED", dataset("unused.csv")),
                            ]),
                        ),
                        (
                            "columns",
                            V::List(vec![
                                column("OUT", expression.clone()),
                                column("A", literal("a")),
                                column("LITERAL", literal("literal")),
                            ]),
                        ),
                    ],
                );
                let result = prune(&input, Order::Dependencies).unwrap();
                assert_eq!(columns(&result), live_columns, "{expression:?}");
                assert_eq!(inputs(&result.document), live_inputs, "{expression:?}");
            }
        }

        #[test]
        fn malformed_texts_read_nothing() {
            let template =
                |template: &str| op("str_template", V::map(vec![("template", text(template))]));
            let flag = |predicate: &str| op("flag", V::map(vec![("condition", text(predicate))]));
            for expression in [
                compute("A +"),
                aggregate(vec![("expr", text("SUM(A) +"))]),
                flag("A ="),
                template("{A} {bad-name}"),
                template("{A} }"),
                template("{A} {"),
                template("{A..B}"),
                template("{{A}}"),
            ] {
                let input = spec(
                    &["OUT"],
                    vec![(
                        "columns",
                        V::List(vec![
                            column("OUT", expression.clone()),
                            column("A", literal("a")),
                        ]),
                    )],
                );
                let result = prune(&input, Order::Dependencies).unwrap();
                assert_eq!(columns(&result), ["OUT"], "{expression:?}");
            }
        }

        #[test]
        fn parser_refusal_is_not_an_empty_reference_set() {
            // A live text over the parser's depth refuses the spec, so that a
            // policy refusal never makes the declarations it reads dead.
            let deep = format!("{}A{}", "(".repeat(129), ")".repeat(129));
            let flag = op(
                "flag",
                V::map(vec![("condition", text(&format!("{deep} > 0")))]),
            );
            for (expression, context) in [
                (compute(&deep), Context::Compute),
                (aggregate(vec![("expr", text(&deep))]), Context::Aggregate),
                (flag, Context::Predicate),
            ] {
                let live = spec(
                    &["OUT"],
                    vec![("columns", V::List(vec![column("OUT", expression.clone())]))],
                );
                let refused = prune(&live, Order::Declared);
                assert!(
                    matches!(
                        &refused,
                        Err(E::Limit { context: refused, error: CompileError::Limit { .. } })
                            if *refused == context
                    ),
                    "{refused:?}"
                );
                // A dead declaration's text is never followed.
                let dead = spec(
                    &["A"],
                    vec![(
                        "columns",
                        V::List(vec![column("A", literal("a")), column("DEAD", expression)]),
                    )],
                );
                assert_eq!(columns(&prune(&dead, Order::Declared).unwrap()), ["A"]);
            }
        }

        #[test]
        fn repeated_pruning_keeps_written_identifiers() {
            let input = spec(
                &["OUT"],
                vec![(
                    "columns",
                    V::List(vec![
                        column("OUT", compute("A + B")),
                        column("A", literal("a")),
                        column("B", literal("b")),
                    ]),
                )],
            );
            let original = input.clone();
            let first = prune(&input, Order::Dependencies).unwrap();
            for _ in 0..2 {
                assert_eq!(prune(&input, Order::Dependencies).unwrap(), first);
            }
            assert_eq!(columns(&first), ["A", "B", "OUT"]);
            assert_eq!(input, original);
        }

        #[test]
        fn prunes_dead_sources_follows_count_and_stably_orders_without_changing_output() {
            let fields = |columns: Vec<V>, inputs: Vec<(&'static str, V)>| {
                vec![
                    ("base", text("DM")),
                    ("input", V::map(inputs)),
                    ("columns", V::List(columns)),
                ]
            };
            let count = || aggregate(vec![("expr", text("COUNT(EX.*)"))]);
            let mut written = fields(
                vec![
                    column("OUT", compute("INNER + 1")),
                    column("N", count()),
                    column("INNER", source("DM.X")),
                    column("DEAD", source("GHOST")),
                ],
                vec![
                    ("DM", dataset("dm.csv")),
                    ("EX", dataset("ex.csv")),
                    ("DEAD", dataset("never.csv")),
                ],
            );
            // An intermediate list none of whose entries survives goes.
            written.push(("intermediates", V::List(vec![])));
            let input = spec(&["OUT", "N"], written);
            let result = prune(&input, Order::Dependencies).unwrap();
            let expected = spec(
                &["OUT", "N"],
                fields(
                    vec![
                        column("N", count()),
                        column("INNER", source("DM.X")),
                        column("OUT", compute("INNER + 1")),
                    ],
                    vec![("DM", dataset("dm.csv")), ("EX", dataset("ex.csv"))],
                ),
            );
            same(
                &result.document,
                result.document.root(),
                &expected,
                expected.root(),
            );
            assert_eq!(result.origins.len(), result.document.nodes().len());
            for (id, origin) in result.origins.iter().enumerate() {
                assert_eq!(origin.source, SchemaSource::Input);
                assert!(!origin.generated);
                if !matches!(result.document.nodes()[id], N::Sequence(_) | N::Mapping(_)) {
                    assert_eq!(result.document.nodes()[id], input.nodes()[origin.node]);
                }
            }
        }

        #[test]
        fn rows_and_assertions_are_roots_and_lookup_matching_inputs_precede_users() {
            let mut asserted = bare("ASSERT");
            asserted.set(
                &["verifications"],
                V::List(vec![op("not_missing", V::map(vec![]))]),
            );
            let input = spec(
                &["LIVE"],
                vec![
                    (
                        "input",
                        V::map(vec![
                            ("DM", dataset("dm")),
                            ("EX", dataset("ex")),
                            ("ROW", dataset("row")),
                            ("DEAD", dataset("dead")),
                        ]),
                    ),
                    ("base", text("DM")),
                    (
                        "verifications",
                        V::List(vec![op("unique", texts(&["CHECK"]))]),
                    ),
                    (
                        "columns",
                        V::List(vec![
                            column("LIVE", literal("fixed")),
                            column("KEY", source("DM.ID")),
                            column("CHECK", source("DM.X")),
                            asserted,
                            column("UNUSED", source("MISSING")),
                        ]),
                    ),
                    (
                        "intermediates",
                        V::List(vec![
                            V::map(vec![
                                ("id", text("LOOKUP")),
                                ("dataset", text("EX")),
                                ("key", texts(&["KEY"])),
                            ]),
                            V::map(vec![
                                ("id", text("UNUSED_LOOKUP")),
                                ("dataset", text("DEAD")),
                                ("require_match", V::boolean(true)),
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
                                        ("LIVE", derivation(source("LOOKUP.VALUE"))),
                                        ("UNUSED", derivation(source("DEAD.X"))),
                                    ]),
                                ),
                            ]),
                            V::map(vec![("id", text("R2")), ("derivations", V::map(vec![]))]),
                        ]),
                    ),
                ],
            );
            let result = prune(&input, Order::Dependencies).unwrap();
            let d = &result.document;
            assert_eq!(inputs(d), ["DM", "EX", "ROW"]);
            assert_eq!(columns(&result), ["KEY", "LIVE", "CHECK", "ASSERT"]);
            assert_eq!(names(d, "intermediates", "id"), ["LOOKUP"]);
            assert_eq!(names(d, "rows", "id"), ["R1", "R2"]);
            let N::Sequence(rows) = &d.nodes()[d.field(d.root(), "rows").unwrap()] else {
                panic!()
            };
            assert_eq!(keys(d, d.field(rows[0], "derivations").unwrap()), ["LIVE"]);
            assert!(keys(d, d.field(rows[1], "derivations").unwrap()).is_empty());
            // A standalone spec keeps the same declarations in declaration order.
            let declared = prune(&input, Order::Declared).unwrap();
            assert_eq!(columns(&declared), ["LIVE", "KEY", "CHECK", "ASSERT"]);
        }

        #[test]
        fn unknown_dependencies_are_lexical_and_precede_cycle_reporting() {
            // B also reads itself; the unknown names are reported first, in
            // name order, and only a layered spec is ordered (COLUMN-5).
            let input = spec(
                &["X"],
                vec![(
                    "columns",
                    V::List(vec![
                        column("X", compute("Z + B + A")),
                        column("B", source("B")),
                    ]),
                )],
            );
            let unknown = |dependency: &str| I::Unknown {
                column: "X".into(),
                dependency: dependency.into(),
            };
            assert_eq!(
                prune(&input, Order::Dependencies),
                Err(E::Invalid(vec![unknown("A"), unknown("Z")]))
            );
            assert_eq!(
                columns(&prune(&input, Order::Declared).unwrap()),
                ["X", "B"]
            );
            let Err(E::Invalid(issues)) = prune(&input, Order::Dependencies) else {
                panic!("expected completed dependency findings")
            };
            let diagnostics = yamaa_core::diagnostic::inheritance_dependency_diagnostics(&issues);
            assert_eq!(diagnostics.len(), 2);
            assert_eq!(diagnostics[0].spec_paths, ["columns.X.derivation"]);
            assert_eq!(diagnostics[1].spec_paths, ["columns.X.derivation"]);
            for (diagnostic, expected) in diagnostics.iter().zip(["A", "Z"]) {
                assert_eq!(diagnostic.definition().condition, "unknown_reference");
                assert_eq!(diagnostic.definition().rule, Some("COLUMN-5"));
                assert_eq!(
                    diagnostic.context["dependency"],
                    yamaa_core::json::Value::String(expected.into())
                );
            }
        }

        #[test]
        fn a_layered_cycle_names_its_closed_loop() {
            // One detector names the closed loop, as a single-file spec's
            // does, not every column the cycle leaves unscheduled (#2844,
            // decision 1).
            let cycle = I::Cycle {
                cycle: vec!["A".into(), "B".into(), "A".into()],
                paths: vec![
                    "columns.A.derivation.source".into(),
                    "columns.B.derivation.source".into(),
                ],
            };
            let input = spec(
                &["DOWNSTREAM", "FREE"],
                vec![(
                    "columns",
                    V::List(vec![
                        column("DOWNSTREAM", source("A")),
                        column("A", source("B")),
                        column("B", source("A")),
                        column("FREE", literal("no reference")),
                    ]),
                )],
            );
            assert_eq!(
                prune(&input, Order::Dependencies),
                Err(E::Invalid(vec![cycle.clone()]))
            );
            let diagnostics = yamaa_core::diagnostic::inheritance_dependency_diagnostics(
                std::slice::from_ref(&cycle),
            );
            assert_eq!(
                diagnostics[0].context["cycle"],
                yamaa_core::json::Value::Array(
                    ["A", "B", "A"]
                        .into_iter()
                        .map(|name| yamaa_core::json::Value::String(name.into()))
                        .collect()
                )
            );
            // A member only a row template derives cites its operation under
            // the column, as the single-file compiler does.
            let input = spec(
                &["A"],
                vec![
                    (
                        "columns",
                        V::List(vec![bare("A"), column("B", source("A"))]),
                    ),
                    (
                        "rows",
                        V::List(vec![V::map(vec![
                            ("id", text("R1")),
                            ("derivations", V::map(vec![("A", derivation(source("B")))])),
                        ])]),
                    ),
                ],
            );
            assert_eq!(
                prune(&input, Order::Dependencies),
                Err(E::Invalid(vec![cycle]))
            );
        }

        #[test]
        fn repeated_dependency_requests_keep_the_complete_document() {
            let input = spec(
                &["A"],
                vec![("columns", V::List(vec![column("A", literal("constant"))]))],
            );
            let first = prune(&input, Order::Dependencies).unwrap();
            let second = prune(&input, Order::Dependencies).unwrap();
            assert_eq!(first, second);
            assert_eq!(columns(&first), ["A"]);
        }

        #[test]
        fn unknown_dependency_retains_its_authored_column_and_name() {
            let input = spec(
                &["X"],
                vec![("columns", V::List(vec![column("X", source("MISSING"))]))],
            );
            assert_eq!(
                prune(&input, Order::Dependencies),
                Err(E::Invalid(vec![I::Unknown {
                    column: "X".into(),
                    dependency: "MISSING".into()
                }]))
            );
            // A predicate's own validation reports its unknown names (LAYER-5).
            let flag = op("flag", V::map(vec![("condition", text("MISSING = 'Y'"))]));
            let input = spec(&["X"], vec![("columns", V::List(vec![column("X", flag)]))]);
            assert_eq!(columns(&prune(&input, Order::Dependencies).unwrap()), ["X"]);
        }

        #[test]
        fn dependency_order_matches_the_shared_analysis() {
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
                let declared = graph
                    .iter()
                    .zip(&column_names)
                    .map(|(edges, name)| {
                        let expression = if edges.is_empty() {
                            literal("constant")
                        } else {
                            let expression = edges
                                .iter()
                                .map(|&index| column_names[index].as_str())
                                .collect::<Vec<_>>()
                                .join(" + ");
                            compute(&expression)
                        };
                        column(name, expression)
                    })
                    .collect();
                let output = column_names.iter().map(String::as_str).collect::<Vec<_>>();
                let input = spec(&output, vec![("columns", V::List(declared))]);
                let result = prune(&input, Order::Dependencies).unwrap();
                let expected_names: Vec<_> = expected
                    .iter()
                    .map(|&index| column_names[index].as_str())
                    .collect();
                assert_eq!(columns(&result), expected_names);
                let analyzed = analyze(&graph).unwrap();
                assert_eq!(analyzed.order, expected);
                assert_eq!(analyzed.cycle, None);
            }
        }

        #[test]
        fn the_order_names_only_the_first_cycle() {
            use yamaa_core::dependency_analysis::analyze;
            let input = spec(
                &["DOWNSTREAM", "A", "B", "FREE", "SELF", "FREE_READER"],
                vec![(
                    "columns",
                    V::List(vec![
                        column("DOWNSTREAM", source("A")),
                        column("A", source("B")),
                        column("B", source("A")),
                        column("FREE", literal("constant")),
                        column("SELF", source("SELF")),
                        column("FREE_READER", source("FREE")),
                    ]),
                )],
            );
            assert_eq!(
                prune(&input, Order::Dependencies),
                Err(E::Invalid(vec![I::Cycle {
                    cycle: vec!["A".into(), "B".into(), "A".into()],
                    paths: vec![
                        "columns.A.derivation.source".into(),
                        "columns.B.derivation.source".into()
                    ],
                }]))
            );
            let analyzed = analyze(&[vec![1], vec![2], vec![1], vec![], vec![4], vec![3]]).unwrap();
            assert_eq!(analyzed.order, vec![3, 5]);
            assert_eq!(analyzed.cycle, Some(vec![1, 2, 1]));
        }

        #[test]
        fn declared_order_and_unordered_input_are_kept() {
            let input = spec(
                &["A", "B"],
                vec![
                    (
                        "columns",
                        V::List(vec![column("A", source("B")), bare("B")]),
                    ),
                    ("keys", texts(&["A"])),
                ],
            );
            let original = input.clone();
            for _ in 0..2 {
                assert_eq!(
                    columns(&prune(&input, Order::Declared).unwrap()),
                    ["A", "B"]
                );
                assert_eq!(
                    columns(&prune(&input, Order::Dependencies).unwrap()),
                    ["B", "A"]
                );
            }
            assert_eq!(input, original);
        }

        #[test]
        fn aggregate_bindings_are_local_to_their_aggregate() {
            // The filter reads outside the bindings; the reducer and the
            // bindings read every binding of their aggregate locally.
            let total = aggregate(vec![
                ("filter", text("A > 0")),
                (
                    "derive",
                    V::List(vec![
                        binding("A", source("DM.X")),
                        binding("B", compute("A + OFFSET")),
                    ]),
                ),
                ("expr", text("SUM(B)")),
            ]);
            let fields = |expression: V| {
                vec![
                    (
                        "input",
                        V::map(vec![("DM", dataset("dm.csv")), ("EX", dataset("ex.csv"))]),
                    ),
                    (
                        "columns",
                        V::List(vec![
                            column("TOTAL", expression),
                            column("A", literal("a")),
                            column("B", literal("b")),
                            column("OFFSET", literal("o")),
                        ]),
                    ),
                ]
            };
            let result = prune(&spec(&["TOTAL"], fields(total)), Order::Dependencies).unwrap();
            assert_eq!(columns(&result), ["A", "OFFSET", "TOTAL"]);
            assert_eq!(inputs(&result.document), ["DM"]);
            // A later binding is the aggregate's too.
            let forward = aggregate(vec![
                (
                    "derive",
                    V::List(vec![
                        binding("A", source("B")),
                        binding("B", source("DM.X")),
                    ]),
                ),
                ("expr", text("SUM(A)")),
            ]);
            let result = prune(&spec(&["TOTAL"], fields(forward)), Order::Dependencies).unwrap();
            assert_eq!(columns(&result), ["TOTAL"]);
        }

        #[test]
        fn odm_items_name_their_input_and_range_bounds_name_donor_fields() {
            let read = op("odm", V::map(vec![("item", text("RAW.IT.WEIGHT"))]));
            let ranged = op(
                "source",
                V::map(vec![
                    ("variable", text("EX.DOSE")),
                    (
                        "between",
                        V::map(vec![
                            ("value", text("ADT")),
                            ("lower", text("START")),
                            ("upper", text("END")),
                        ]),
                    ),
                ]),
            );
            let input = spec(
                &["WEIGHT", "DOSE"],
                vec![
                    (
                        "input",
                        V::map(vec![
                            ("RAW", dataset("raw.csv")),
                            ("EX", dataset("ex.csv")),
                            ("UNUSED", dataset("unused.csv")),
                        ]),
                    ),
                    (
                        "columns",
                        V::List(vec![
                            column("WEIGHT", read),
                            column("DOSE", ranged),
                            column("ADT", literal("2026-01-01")),
                            column("START", literal("s")),
                            column("END", literal("e")),
                        ]),
                    ),
                ],
            );
            let result = prune(&input, Order::Dependencies).unwrap();
            assert_eq!(inputs(&result.document), ["RAW", "EX"]);
            assert_eq!(columns(&result), ["WEIGHT", "ADT", "DOSE"]);
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
            rule: &'static str,
            context: Vec<(&'static str, C)>,
        ) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: path.into(),
                code: match (condition, rule) {
                    ("schema_version_mismatch", "LAYER-4") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerVersionMismatch
                    }
                    ("duplicate_identifier", "LAYER-4") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerIdentifierRepeated
                    }
                    ("invalid_field_type", "SCHEMA-12") => {
                        yamaa_core::diagnostic::ConditionCode::SchemaLayerInvalidFieldType
                    }
                    ("invalid_clear", "LAYER-9") => {
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
                    "LAYER-4",
                    vec![("expected", text("1.0")), ("actual", C::Null)]
                )
            );
            assert_eq!(
                findings[5],
                finding(
                    "columns.X.name",
                    "duplicate_identifier",
                    "LAYER-4",
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
                        "SCHEMA-12",
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
                        "LAYER-9",
                        vec![("field", text("name"))]
                    ),
                    finding(
                        "columns[0].type",
                        "invalid_clear",
                        "LAYER-9",
                        vec![("field", text("type"))]
                    ),
                    finding(
                        "output",
                        "invalid_clear",
                        "LAYER-9",
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
                        "LAYER-9",
                        vec![("field", text("needed"))]
                    ),
                    finding(
                        "settings.needed",
                        "invalid_clear",
                        "LAYER-9",
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
            // A dataset written as a mapping says more than its path, so it is
            // not a restated path shorthand (SCHEMA-15).
            let dataset = |path: &str| {
                V::map(vec![
                    ("path", V::text(path)),
                    ("options", V::map(vec![("defaulted", V::text("SET"))])),
                ])
            };
            let input = layer(vec![(
                "input",
                V::Pairs(vec![
                    (V::integer("2"), dataset("first.csv")),
                    (V::text("KEEP"), dataset("middle.csv")),
                    (V::text("2"), dataset("last.csv")),
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
                        ("2", dataset("last.csv")),
                        ("KEEP", dataset("middle.csv")),
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
                    (V::text("2"), V::text("last.csv")),
                ]),
            )]);
            let Err(NormalizationError::Invalid(findings)) = schema.normalize_layer(&input) else {
                panic!("shadowed invalid member must fail")
            };
            assert_eq!(paths(&findings), [("input[2].path", "invalid_field_type")]);
        }

        #[test]
        fn a_dataset_mapping_holding_only_its_path_restates_the_path_shorthand() {
            let schema = schema();
            let input = layer(vec![(
                "input",
                V::map(vec![
                    ("DM", V::map(vec![("path", V::text("dm.csv"))])),
                    // A clear marker beside the path adds to it.
                    (
                        "AE",
                        V::map(vec![
                            ("path", V::text("ae.csv")),
                            ("options", V::Scalar(N::Null)),
                        ]),
                    ),
                    ("LB", V::text("lb.csv")),
                ]),
            )]);
            let Err(NormalizationError::Invalid(findings)) = schema.normalize_layer(&input) else {
                panic!("a restated path shorthand must fail")
            };
            assert_eq!(paths(&findings), [("input.DM", "redundant_long_form")]);
            assert_eq!(
                findings[0].context,
                [
                    ("field", C::Text("path".into())),
                    ("shorthand", C::InputValue(5)),
                ]
            );
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
            BundleError, BundleIssueKind, DescriptorIssue, Document, DocumentKind,
            DocumentNode as N, NormalizationError, NormalizationResource, NormalizedDocument,
            SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaSource, SchemaStructure,
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
                                V::text("tree_branch"),
                                V::text("str"),
                            ]),
                        )]),
                    ),
                    (
                        "tree_branch",
                        V::map(vec![("type", V::text("list[tree_value]"))]),
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
            // A type outside the closed list is VALUE-1's; the other
            // constraints are the schema language's.
            assert_eq!(
                result
                    .iter()
                    .map(|d| d.definition().rule)
                    .collect::<Vec<_>>(),
                [Some("VALUE-1"), Some("SCHEMA-12"), Some("SCHEMA-12")]
            );
            let result = validate(&schema, "kind", V::boolean(true), false);
            assert_eq!(findings(&result), [("kind", "invalid_field_type")]);
            assert_eq!(result[0].definition().rule, Some("SCHEMA-12"));
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
                V::map(vec![("literal", V::integer("4"))]),
                false
            )
            .is_empty());
            // The bare string is the only spelling of a plain source (EXPR-5).
            let restated = validate(
                &schema,
                "derive",
                V::map(vec![("source", V::text("DM.AGE"))]),
                false,
            );
            assert_eq!(findings(&restated), [("derive", "redundant_long_form")]);
            assert_eq!(
                restated[0].context,
                [
                    ("field", C::Text("source".into())),
                    ("shorthand", C::InputValue(1)),
                ]
            );
            assert_eq!(restated[0].definition().rule, Some("SCHEMA-15"));
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
            assert_eq!(result[0].definition().rule, Some("EXPR-5"));
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
                                (definition.phase, definition.condition, definition.rule),
                                ("validation", "invalid_field_type", Some("EXPR-7"))
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
                result.iter().all(|d| d.definition().rule != Some("EXPR-7")),
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
        fn single_or_list_unions_are_bundle_errors_and_lists_normalize_as_written() {
            // One spelling per meaning: a union may not offer a type and a list
            // of it, so no bare value is ever wrapped into a list (SCHEMA-10).
            let declaration = |members: Vec<&'static str>| {
                V::map(vec![(
                    "type",
                    V::List(members.into_iter().map(V::text).collect()),
                )])
            };
            let Err(BundleError::Invalid(issues)) = SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("schema_version", "str"),
                                V::map(vec![("simple", declaration(vec!["str", "list[str]"]))]),
                                V::map(vec![(
                                    "nested",
                                    declaration(vec!["list[str]", "list[list[str]]"]),
                                )]),
                                V::map(vec![(
                                    "larger",
                                    declaration(vec!["str", "list[str]", "null"]),
                                )]),
                                V::map(vec![("plain", declaration(vec!["str", "list[int]"]))]),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            ) else {
                panic!("a single-or-list union must be refused");
            };
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| (issue.path.as_str(), &issue.kind))
                    .collect::<Vec<_>>(),
                [
                    (
                        "root_class.simple",
                        &BundleIssueKind::Descriptor(DescriptorIssue::SingleOrList {
                            member: "str".into()
                        })
                    ),
                    (
                        "root_class.nested",
                        &BundleIssueKind::Descriptor(DescriptorIssue::SingleOrList {
                            member: "list[str]".into()
                        })
                    ),
                    (
                        "root_class.larger",
                        &BundleIssueKind::Descriptor(DescriptorIssue::SingleOrList {
                            member: "str".into()
                        })
                    ),
                ]
            );
            let schema = bundle(vec![field("names", "list[str]")], vec![]);
            let result = normalize(&schema, "names", V::List(vec![V::text("DM")]), false);
            assert_value(&result.document, V::List(vec![V::text("DM")]));
            assert!(result.origins.iter().all(|o| !o.generated));
            assert_eq!(
                findings(&validate(&schema, "names", V::text("DM"), false)),
                [("names", "invalid_field_type")]
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
                    V::map(vec![("name", V::text("DM")), ("optional", V::integer("1"))]),
                    false,
                )
                .document,
                V::map(vec![
                    ("enabled", V::boolean(true)),
                    ("name", V::text("DM")),
                    ("optional", V::integer("1")),
                ]),
            );
            assert_value(
                &normalize(&schema, "item", V::map(vec![]), true).document,
                V::map(vec![]),
            );
            // A written long form that holds only the shorthand's field restates
            // it (SCHEMA-15), in a complete value and in a fragment alike.
            for fragment in [false, true] {
                let input = document(V::map(vec![("name", V::text("DM"))]));
                let descriptor = schema.root_class().fields[1].descriptor;
                let Err(NormalizationError::Invalid(findings)) =
                    schema.normalize_descriptor(descriptor, &input, input.root(), fragment)
                else {
                    panic!("a restated shorthand must fail");
                };
                assert_eq!(
                    findings
                        .iter()
                        .map(|d| (d.path.as_str(), d.definition().condition))
                        .collect::<Vec<_>>(),
                    [("<normalization>", "redundant_long_form")]
                );
                assert_eq!(
                    findings[0].context,
                    [
                        ("field", C::Text("name".into())),
                        ("shorthand", C::InputValue(1)),
                    ]
                );
            }
        }

        #[test]
        fn bare_derivation_strings_are_source_references_never_calls() {
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
            // A string spelled like a call is still only a source reference
            // (EXPR-5); the source's own type then refuses it.
            for source in [
                "to_number(DM.AGE)",
                "  to_number( DM.AGE )  ",
                "nested(call(DM.AGE))",
                "1invalid(DM.AGE)",
                "op (DM.AGE)",
            ] {
                assert_value(
                    &normalize(&schema, "derive", V::text(source), false).document,
                    V::map(vec![
                        ("value", V::map(vec![("source", V::text(source))])),
                        ("fallback", V::integer("0")),
                    ]),
                );
                assert_value(
                    &normalize(&schema, "derive", V::text(source), true).document,
                    V::map(vec![("value", V::map(vec![("source", V::text(source))]))]),
                );
                assert_value(
                    &normalize(&schema, "result", V::text(source), false).document,
                    V::map(vec![("source", V::text(source))]),
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
                        ]),
                    )])]),
                )],
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
            assert_value(&first.document, V::map(vec![("name", V::text("DM"))]));
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
