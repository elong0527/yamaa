//! Schema contracts: resolution pruning, schema layer admission and schema
//! type expression.
mod schema {
    mod resolution_pruning {
        //! LAYER-11 pruning and the LAYER-13 order on the AST (#3134), over
        //! 1.0 documents in the normalized form, which the 1.0 front end
        //! lowers.
        use yamaa_core::expression::{CompileError, Context};
        use yamaa_core::resolution::{prune_lowered, Error as E, Issue as I, Order};
        use yamaa_core::schema::{Document, DocumentNode as N, NormalizedDocument, SchemaSource};

        use crate::support::document::Tree as V;

        /// Prune `document` as the 1.0 front end lowers it.
        fn prune(document: &Document, order: Order) -> Result<NormalizedDocument, E> {
            let spec = yamaa_core::front_end::v1::spec::lower(document, None).into_spec();
            prune_lowered(document, &spec, order)
        }

        fn text(s: &str) -> V {
            V::text(s)
        }
        fn texts(names: &[&str]) -> V {
            V::List(names.iter().map(|name| text(name)).collect())
        }
        fn op(name: &str, payload: V) -> V {
            V::map(vec![(name, payload)])
        }
        /// A read written as the name alone (D18).
        fn source(name: &str) -> V {
            text(name)
        }
        /// A formula, written as its text (D18).
        fn compute(expr: &str) -> V {
            text(expr)
        }
        /// Text, quoted in the expression (D18).
        fn literal(value: &str) -> V {
            text(&format!("'{value}'"))
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
        /// One entry of an aggregate's `derivations` map (P-BIND-1).
        fn binding(name: &str, expression: V) -> (&str, V) {
            (
                name,
                V::map(vec![
                    ("type", text("float")),
                    ("derivation", derivation(expression)),
                ]),
            )
        }
        fn dataset(path: &str) -> V {
            V::map(vec![("path", text(path))])
        }
        /// A spec whose output names `output`, with `fields` after it.
        fn tree(output: &[&str], fields: Vec<(&str, V)>) -> V {
            let mut all = vec![
                ("yamaa", text("1.0")),
                ("type", text("spec")),
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
                    ("source", text("CODE")),
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
            // A derivation's text is read in the one expression grammar (D18).
            for (expression, context) in [
                (compute(&deep), Context::Derivation),
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
            // A check that declares no field is its bare keyword (P-HND-4).
            asserted.set(&["verifications"], V::List(vec![text("not_missing")]));
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
                            // Every lookup declares its key (D23, D111).
                            V::map(vec![
                                ("id", text("UNUSED_LOOKUP")),
                                ("dataset", text("DEAD")),
                                ("key", texts(&["KEY"])),
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
                            // A template that overrides nothing writes no
                            // `derivations` (P-HND-4).
                            V::map(vec![("id", text("R2"))]),
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
            assert!(d.field(rows[1], "derivations").is_none());
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
                    "derivations",
                    V::map(vec![
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
                    "derivations",
                    V::map(vec![
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
        fn a_bare_reducer_name_reads_a_column_only_in_an_aggregate_without_bindings() {
            // LOOKUP-22: the reducer of an aggregate that declares bindings
            // names them, so a bare name that no binding names reads no
            // column, not even a declared one, and binding refuses it as
            // `unknown_derive_variable` whether or not the spec is layered.
            // Without bindings a bare reducer name reads a column, so an
            // undeclared one is an unknown dependency (COLUMN-5).
            let derived = || {
                aggregate(vec![
                    (
                        "derivations",
                        V::map(vec![binding("DOSE", source("EX.EXDOSE"))]),
                    ),
                    ("expr", text("SUM(DOSE_MG)")),
                ])
            };
            let plain = || aggregate(vec![("expr", text("SUM(DOSE_MG)"))]);
            let order = |names: &[&str]| -> Result<Vec<String>, E> {
                Ok(names.iter().map(|name| name.to_string()).collect())
            };
            let unknown = || -> Result<Vec<String>, E> {
                Err(E::Invalid(vec![I::Unknown {
                    column: "TOTAL".into(),
                    dependency: "DOSE_MG".into(),
                }]))
            };
            for (label, expression, declared, expected) in [
                (
                    "bindings, no such column",
                    derived(),
                    false,
                    order(&["TOTAL"]),
                ),
                (
                    "bindings, a same-named column",
                    derived(),
                    true,
                    order(&["TOTAL"]),
                ),
                (
                    "no bindings, a declared column",
                    plain(),
                    true,
                    order(&["DOSE_MG", "TOTAL"]),
                ),
                ("no bindings, no such column", plain(), false, unknown()),
            ] {
                let mut declarations = vec![column("TOTAL", expression)];
                if declared {
                    declarations.push(column("DOSE_MG", literal("1")));
                }
                let input = spec(
                    &["TOTAL"],
                    vec![
                        ("input", V::map(vec![("EX", dataset("ex.csv"))])),
                        ("columns", V::List(declarations)),
                    ],
                );
                assert_eq!(
                    prune(&input, Order::Dependencies).map(|result| columns(&result)),
                    expected,
                    "{label}"
                );
            }
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
        //! A 1.x layer is checked by `validate_layer`, against the view of the
        //! version it declares, and normalized by `normalize_versioned_layer`
        //! before it composes (LAYER-4).
        use yamaa_core::language::LanguageVersion;
        use yamaa_core::schema::{
            Document, DocumentKind as K, DocumentNode as N, NormalizedDocument, SchemaContext as C,
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

        /// The layers of a 1.x specification, headed by `yamaa` and `type`. An
        /// input member is a path or a dataset, so a bare path is shorthand for
        /// the dataset (SCHEMA-10).
        fn schema_with_identifier(identifier_type: V) -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("yamaa", "str", false),
                                field("type", "str", false),
                                field("parent", "str", false),
                                field("input", "dict[identifier, dataset_source]", false),
                                field("columns", "list[column_class]", false),
                                field("rows", "list[row_class]", false),
                                field("intermediates", "list[intermediate_class]", false),
                                field("settings", "options_class", false),
                                field("output", "str", true),
                            ]),
                        ),
                        ("identifier", V::map(vec![("type", identifier_type)])),
                        ("project_path", V::map(vec![("type", V::text("str"))])),
                        (
                            "dataset_source",
                            V::map(vec![(
                                "type",
                                V::List(vec![V::text("project_path"), V::text("dataset_class")]),
                            )]),
                        ),
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
                                field("path", "project_path", true),
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

        /// A 1.x layer: its header, then `fields`.
        fn layer(mut fields: Vec<(&'static str, V)>) -> Document {
            fields.splice(0..0, [("yamaa", V::text("1.0")), ("type", V::text("spec"))]);
            document(V::map(fields))
        }
        /// `schema` at language version 1.0, the version a [`layer`] declares.
        fn at_version(schema: &SchemaStructure) -> SchemaStructure {
            schema.at_version(LanguageVersion::new(0))
        }
        /// The findings of a 1.x layer, which admission reports before the
        /// layer is normalized.
        fn diagnostics(input: Document) -> Vec<SchemaDiagnostic> {
            let findings = at_version(&schema()).validate_layer(&input).unwrap();
            assert!(!findings.is_empty(), "expected ordered findings");
            findings
        }
        /// An admitted 1.x layer, normalized.
        fn normalized(schema: &SchemaStructure, input: &Document) -> NormalizedDocument {
            let view = at_version(schema);
            assert_eq!(view.validate_layer(input).unwrap(), []);
            view.normalize_versioned_layer(input).unwrap()
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
        fn normalization_defers_member_defaults_and_owns_shorthand() {
            let input = layer(vec![
                (
                    "rows",
                    V::List(vec![V::map(vec![
                        ("id", V::text("R")),
                        ("options", V::map(vec![("needed", V::text("yes"))])),
                    ])]),
                ),
                (
                    "columns",
                    V::List(vec![V::map(vec![
                        ("name", V::text("X")),
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
            let normalized = normalized(&schema(), &input);
            // Root fields follow schema order; a member's `defaulted` waits
            // for the resolved specification (LAYER-7).
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
                        ("options", V::map(vec![])),
                    ])]),
                ),
                (
                    "rows",
                    V::List(vec![V::map(vec![
                        ("id", V::text("R")),
                        ("options", V::map(vec![("needed", V::text("yes"))])),
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
            let output = normalized(&schema(), &input);
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
            let schema = at_version(&schema());
            let input = layer(vec![("output", V::integer("2"))]);
            let first = schema.validate_layer(&input).unwrap();
            assert!(!first.is_empty());
            assert_eq!(schema.validate_layer(&input).unwrap(), first);
        }

        #[test]
        fn independent_normalized_layers_keep_the_complete_authored_column() {
            let schema = schema();
            let input = layer(vec![(
                "columns",
                V::List(vec![V::map(vec![("name", V::text("X"))])]),
            )]);
            let first = normalized(&schema, &input);
            let second = normalized(&schema, &input);
            assert_eq!(first, second);
        }

        #[test]
        fn member_paths_distinguish_numeric_boolean_and_textual_keys() {
            let input = document(V::Pairs(vec![
                (V::text("yamaa"), V::text("1.0")),
                (V::text("type"), V::text("spec")),
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
                    V::List(vec![V::Pairs(vec![
                        (V::text("name"), V::text("2")),
                        (V::boolean(false), V::Scalar(N::Null)),
                        (V::integer("3"), V::Scalar(N::Null)),
                    ])]),
                ),
            ]));
            let findings = diagnostics(input);
            assert_eq!(
                paths(&findings),
                [
                    ("input.key(True)", "invalid_field_type"),
                    ("input[True]", "invalid_field_type"),
                    ("input.key(2)", "invalid_field_type"),
                    ("input[2]", "invalid_field_type"),
                    ("input.2", "invalid_field_type"),
                    ("input.key(None)", "invalid_field_type"),
                    ("input.None", "invalid_field_type"),
                    ("columns.2[False]", "unknown_field"),
                    ("columns.2[3]", "unknown_field"),
                ]
            );
        }

        #[test]
        fn a_dataset_mapping_holding_only_its_path_restates_the_path_shorthand() {
            let input = layer(vec![(
                "input",
                V::map(vec![
                    ("DM", V::map(vec![("path", V::text("dm.csv"))])),
                    // Another field beside the path adds to it.
                    (
                        "AE",
                        V::map(vec![
                            ("path", V::text("ae.csv")),
                            ("options", V::map(vec![("needed", V::text("yes"))])),
                        ]),
                    ),
                    ("LB", V::text("lb.csv")),
                ]),
            )]);
            let path = get(&input, get(&input, input.root(), "input"), "DM");
            let findings = diagnostics(input.clone());
            assert_eq!(paths(&findings), [("input.DM", "redundant_long_form")]);
            assert_eq!(
                findings[0].context,
                [
                    ("field", C::Text("path".into())),
                    ("shorthand", C::InputValue(get(&input, path, "path"))),
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
}
