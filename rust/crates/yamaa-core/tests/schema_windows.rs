//! Schema contracts: the seams of window expansion and of the specification
//! compiler that the diagnostic replay cannot reach, each named in its test's
//! first comment. The expanded document, the resolved plan and the compiler's
//! fixed limits stay here; the findings these families report, and the
//! documents they admit, are the cases of
//! `tests/fixtures/diagnostics/compiler.json` (#3798).
mod schema {
    mod schema_windows {
        use yamaa_core::schema::{
            Document, DocumentNode as N, ExpandedWindows, SchemaModule, SchemaStructure,
            WindowReference,
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
        fn schema() -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("windows", "dict[str, window_spec]"),
                                field("columns", "list[column_class]"),
                                field("metadata", "dict"),
                            ]),
                        ),
                        (
                            "column_class",
                            V::List(vec![
                                field("name", "str"),
                                field("derivation", "handled_expression_class"),
                            ]),
                        ),
                        (
                            "handled_expression_class",
                            V::List(vec![field("value", "expression")]),
                        ),
                        (
                            "expression",
                            V::map(vec![("registry", V::text("expressions"))]),
                        ),
                        (
                            "expressions",
                            V::map(vec![(
                                "row_number",
                                V::List(vec![field("window", "window_selection")]),
                            )]),
                        ),
                        (
                            "window_selection",
                            V::map(vec![(
                                "type",
                                V::List(vec![V::text("str"), V::text("window_spec")]),
                            )]),
                        ),
                        (
                            "window_spec",
                            V::List(vec![
                                field("group_by", "list[str]"),
                                field("order_by", "list[str]"),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        fn window() -> V {
            V::map(vec![("order_by", V::List(vec![V::text("SEQ")]))])
        }
        fn column(name: &'static str, selection: V) -> V {
            V::map(vec![
                ("name", V::text(name)),
                (
                    "derivation",
                    V::map(vec![(
                        "value",
                        V::map(vec![("row_number", V::map(vec![("window", selection)]))]),
                    )]),
                ),
            ])
        }
        fn input(columns: Vec<V>) -> V {
            V::map(vec![
                ("windows", V::map(vec![("VISITS", window())])),
                ("columns", V::List(columns)),
                ("metadata", V::map(vec![("window", V::text("UNKNOWN"))])),
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

        /// A seam the replay cannot reach: the expanded document, the origin
        /// of each of its nodes and the references the expansion records
        /// (WINDOW-2).
        #[test]
        fn expands_independent_copies_keeps_order_scope_and_definition_origins() {
            let input = document(input(vec![
                column("FIRST", V::text("VISITS")),
                column("SECOND", V::text("VISITS")),
            ]));
            let original = input.clone();
            let result = schema().expand_named_windows(&input, true).unwrap();
            let expected = document(V::map(vec![
                (
                    "columns",
                    V::List(vec![column("FIRST", window()), column("SECOND", window())]),
                ),
                ("metadata", V::map(vec![("window", V::text("UNKNOWN"))])),
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

        /// A seam the replay cannot reach: the document a non-strict
        /// expansion returns, which keeps root `windows` and an unknown
        /// reference as written for pruning to drop (WINDOW-2). What strict
        /// expansion reports of the surviving uses is `compiler.json`'s.
        #[test]
        fn non_strict_expansion_keeps_unknown_references_for_pruning() {
            let input = document(input(vec![
                column("DEAD", V::text("UNKNOWN")),
                column("LIVE", V::text("VISITS")),
            ]));
            let expanded = schema().expand_named_windows(&input, false).unwrap();
            assert!(expanded
                .document
                .field(expanded.document.root(), "windows")
                .is_some());
            assert_eq!(
                expanded.document.nodes()[selection(&expanded, 0)],
                N::Text("UNKNOWN".into())
            );
            assert_eq!(expanded.references.len(), 1);
        }
        fn input_value_live() -> V {
            input(vec![column("LIVE", V::text("VISITS"))])
        }

        /// A seam the replay cannot reach: two expansions of one input,
        /// compared whole with each other.
        #[test]
        fn repeated_window_expansion_preserves_complete_independent_results() {
            let schema = schema();
            let input = document(input_value_live());
            let first = schema.expand_named_windows(&input, true).unwrap();
            let second = schema.expand_named_windows(&input, true).unwrap();
            assert_eq!(first, second);
        }
    }
    mod specification_compiler {
        //! Compile 1.0 documents, lowered by the 1.0 front end, using only the
        //! core crate and immutable schemas. What the compiler prepares and
        //! binds is a resolved plan, a prepared source and fixed limits that
        //! no reader returns, so those seams stay here; what it refuses is
        //! `compiler.json`'s.
        use crate::support::id_source_schema as source;
        use crate::support::resolved;
        use crate::support::specification::{header, prepare};
        use yamaa_core::{
            dataset::{
                resolved::{Op, OperandRole},
                ResolvedPlan,
            },
            schema::{Document, DocumentNode as N},
            specification::PrepareError,
            value::{ColumnType, Value},
        };

        use crate::support::document::Tree;

        /// A column of type `int` that reads `read`, an expression field's text.
        fn reference(name: &str, read: &str) -> Tree {
            Tree::map(vec![
                ("name", Tree::text(name)),
                ("type", Tree::text("int")),
                ("derivation", Tree::map(vec![("value", Tree::text(read))])),
            ])
        }
        /// The expression a YAML scalar writes: text is quoted, and any other
        /// scalar is itself a literal (D18).
        fn literal(node: N) -> Tree {
            match node {
                N::Text(text) => Tree::text(format!("'{}'", text.replace('\'', "''"))),
                node => Tree::Scalar(node),
            }
        }
        /// A 1.0 specification over `SRC`, read from `input_path`, whose `VALUE`
        /// column derives `expression`, an expression field's text.
        fn document(expression: &str, input_path: &str) -> Document {
            value_document(Tree::text(expression), input_path, None, None)
        }
        /// [`document`], with `VALUE` deriving `operation` over `payload`.
        fn operation_document(operation: &str, payload: Tree, input_path: &str) -> Document {
            value_document(
                Tree::map(vec![(operation, payload)]),
                input_path,
                None,
                None,
            )
        }
        /// [`document`], with `VALUE` deriving `value` under the conversion
        /// handler `handler` and with the row templates `rows`, when given.
        fn value_document(
            value: Tree,
            input_path: &str,
            handler: Option<N>,
            rows: Option<Tree>,
        ) -> Document {
            value_tree(value, input_path, handler, rows).document()
        }
        /// The tree of [`value_document`].
        fn value_tree(
            value: Tree,
            input_path: &str,
            handler: Option<N>,
            rows: Option<Tree>,
        ) -> Tree {
            let mut wrapper = vec![("value", value)];
            if let Some(value) = handler {
                wrapper.push(("unconvertible", Tree::Scalar(value)));
            }
            let mut fields = header();
            fields.extend([
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text(input_path))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![
                        reference("ID", "SRC.ID"),
                        Tree::map(vec![
                            ("name", Tree::text("VALUE")),
                            ("type", Tree::text("int")),
                            ("derivation", Tree::map(wrapper)),
                        ]),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        (
                            "columns",
                            Tree::List(vec![Tree::text("ID"), Tree::text("VALUE")]),
                        ),
                    ]),
                ),
            ]);
            if let Some(rows) = rows {
                fields.push(("rows", rows));
            }
            Tree::map(fields)
        }

        fn case_document(when: &str, expression: &str) -> Document {
            operation_document(
                "case",
                Tree::List(vec![
                    Tree::map(vec![
                        ("when", Tree::text(when)),
                        ("then", Tree::text(expression)),
                    ]),
                    Tree::map(vec![("otherwise", Tree::Scalar(N::Integer("1".into())))]),
                ]),
                "input.csv",
            )
        }

        /// A seam the replay cannot reach: the resolved plan's case
        /// predicate, the reads it collects, its spec path and its binding.
        #[test]
        fn non_key_case_predicate_owns_key_grain_collection_without_study_authority() {
            let prepared = prepare(&case_document("SRC.ID > 0", "1")).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let case = resolved::columns(&plan)[0].expr;
            let Op::Case = resolved::op(&plan, case) else {
                panic!("ordered case");
            };
            let when = resolved::operands(&plan, case, OperandRole::When)[0];
            let Op::Predicate(when) = resolved::op(&plan, when) else {
                panic!("ordered case");
            };
            assert_eq!(when.collected(), ["SRC.ID"]);
            assert_eq!(
                when.plan().spec_path(),
                "columns.VALUE.derivation.case[0].when"
            );
            assert_eq!(
                when.bindings()[0].read,
                yamaa_core::bound_expression::Read::Source(0)
            );
        }

        /// A seam the replay cannot reach: the prepared source and
        /// projection, and the resolved plan's source, keys and compiled
        /// expression.
        #[test]
        fn compiler_owns_bound_plan_without_an_engine_or_source_port() {
            let prepared = prepare(&document("ID + 1 / 0", "input.csv")).unwrap();
            assert_eq!(prepared.source().path, "input.csv");
            assert_eq!(prepared.projection(), ["ID", "VALUE"]);
            let plan: ResolvedPlan = prepared.bind(&source()).unwrap();
            assert_eq!(resolved::source(&plan), &source());
            assert_eq!(resolved::keys(&plan), [0]);
            assert!(resolved::reads_source(
                &plan,
                resolved::row(&plan, 0)[0].expr,
                0
            ));
            let binding = resolved::columns(&plan)[0];
            let Op::Compute(compute) = resolved::op(&plan, binding.expr) else {
                panic!("compiled numeric declaration")
            };
            assert_eq!(compute.expression().spec_path(), "columns.VALUE.derivation");
            // Division by zero remains deferred until an application actually executes a row.
            assert_eq!(&*binding.path, "columns.VALUE.derivation");
        }

        /// A seam the replay cannot reach: the prepared source's profile and
        /// missing-text policy, and the resolved output's width.
        #[test]
        fn compiler_retains_admission_and_resource_boundaries() {
            let prepared = prepare(&document("ID + 1", "input.PARQUET")).unwrap();
            assert_eq!(
                prepared.source().profile,
                yamaa_core::specification::SourceProfile::Parquet
            );
            assert!(!prepared.source().empty_string_present);
            assert!(prepared.source().text_is_missing(""));
            assert!(!prepared.source().text_is_missing("NA"));
            let prepared = prepare(&document("ID + 1", "input.csv")).unwrap();
            assert_eq!(
                resolved::output(&prepared.bind(&source()).unwrap())
                    .columns()
                    .len(),
                2
            );
        }

        fn window_document(filter: &str) -> Document {
            window_settings(filter, "row_number", None, None, None)
        }
        fn window_settings(
            filter: &str,
            operation: &str,
            method: Option<&str>,
            direction: Option<&str>,
            nulls: Option<&str>,
        ) -> Document {
            let mut order = vec![("variable", Tree::text("ID"))];
            if let Some(value) = direction {
                order.push(("direction", Tree::text(value)));
            }
            if let Some(value) = nulls {
                order.push(("nulls", Tree::text(value)));
            }
            let mut options = vec![(
                "window",
                Tree::map(vec![
                    ("order_by", Tree::List(vec![Tree::map(order)])),
                    ("filter", Tree::text(filter)),
                ]),
            )];
            if let Some(value) = method {
                options.push(("method", Tree::text(value)));
            }
            operation_document(operation, Tree::map(options), "input.csv")
        }

        /// A seam the replay cannot reach: the resolved plan's window, its
        /// kind, partition, order term and filter, and a qualified filter's
        /// source reads.
        #[test]
        fn window_compiler_binds_output_dependencies_without_engine_or_records() {
            use yamaa_core::dataset::WindowKind;
            let prepared = prepare(&window_document("ID > 0")).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let Op::Window { window, .. } = resolved::op(&plan, resolved::columns(&plan)[0].expr)
            else {
                panic!("compiled window")
            };
            assert_eq!(window.kind, WindowKind::RowNumber);
            assert!(window.group_by.is_empty());
            assert_eq!(window.order_by[0].column, 0);
            assert!(!window.order_by[0].descending);
            // Missing sorts lowest, so an ascending term places it first.
            assert!(window.order_by[0].nulls_first);
            assert_eq!(
                window.filter.as_ref().unwrap().plan().spec_path(),
                "columns.VALUE.derivation.row_number.window.filter"
            );
            let qualified = prepare(&window_document("SRC.ID > 0"))
                .unwrap()
                .bind(&source())
                .unwrap();
            let Op::ContextWindow {
                context: window, ..
            } = resolved::op(&qualified, resolved::columns(&qualified)[0].expr)
            else {
                panic!("qualified source window");
            };
            assert_eq!(window.sources, vec![("SRC.ID".into(), 0)]);
            assert_eq!(
                window.window.filter.as_ref().unwrap().bindings()[0].read,
                yamaa_core::bound_expression::Read::Column(
                    resolved::output(&qualified).columns().len()
                )
            );
        }

        /// A seam the replay cannot reach: the resolved plan's rank kind and
        /// its order term's direction and missing placement.
        #[test]
        fn directly_admitted_windows_preserve_defaults_and_explicit_options() {
            use yamaa_core::dataset::WindowKind;
            for (method, kind) in [
                (None, WindowKind::Competition),
                (Some("competition"), WindowKind::Competition),
                (Some("dense"), WindowKind::Dense),
            ] {
                for direction in [None, Some("asc"), Some("desc")] {
                    for nulls in [None, Some("last"), Some("first")] {
                        let document = window_settings("ID > 0", "rank", method, direction, nulls);
                        let plan = prepare(&document).unwrap().bind(&source()).unwrap();
                        let Op::Window { window, .. } =
                            resolved::op(&plan, resolved::columns(&plan)[0].expr)
                        else {
                            panic!("compiled rank")
                        };
                        assert_eq!(window.kind, kind);
                        assert_eq!(window.order_by[0].descending, direction == Some("desc"));
                        // An omitted placement sorts missing lowest; a declared one
                        // is absolute whatever the direction.
                        let nulls_first = match nulls {
                            Some(placement) => placement == "first",
                            None => direction != Some("desc"),
                        };
                        assert_eq!(window.order_by[0].nulls_first, nulls_first);
                    }
                }
            }
        }

        fn literal_document(node: N) -> Document {
            value_document(literal(node), "input.csv", None, None)
        }

        /// A seam the replay cannot reach: the resolved plan's literal
        /// operation, output column type and keys.
        #[test]
        fn scalar_column_leaves_use_the_common_compiled_literal_expression() {
            use yamaa_core::value::Value;
            for (node, value) in [
                (N::Null, Value::Missing),
                (
                    N::Text("λ,\"quoted\"".into()),
                    Value::Str("λ,\"quoted\"".into()),
                ),
                (N::Float(1.5), Value::float(1.5)),
                (
                    N::Integer("9223372036854775807".into()),
                    Value::Int(i64::MAX),
                ),
                (
                    N::Integer("-9223372036854775808".into()),
                    Value::Int(i64::MIN),
                ),
            ] {
                let prepared = prepare(&literal_document(node)).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                let literal = resolved::columns(&plan)[0];
                assert_eq!(resolved::op(&plan, literal.expr), &Op::Literal(value));
                assert_eq!(&*literal.path, "columns.VALUE.derivation");
                assert_eq!(resolved::output(&plan).columns()[1].kind, ColumnType::Int);
                assert_eq!(resolved::keys(&plan), [0]);
            }
        }

        /// A seam the replay cannot reach: the resolved plan's conversion
        /// handlers, each at its assignment.
        #[test]
        fn column_recovery_compiles_present_null_and_text_handlers_without_dependencies() {
            use yamaa_core::value::Value;
            for (node, value) in [
                (N::Null, Value::Missing),
                (N::Integer("7".into()), Value::Int(7)),
                (
                    N::Text("NOT.A.REFERENCE".into()),
                    Value::Str("NOT.A.REFERENCE".into()),
                ),
            ] {
                let document = value_document(
                    Tree::Scalar(N::Boolean(true)),
                    "input.csv",
                    Some(node),
                    None,
                );
                let prepared = prepare(&document).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                let handlers = resolved::conversion_handlers(&plan);
                assert_eq!(handlers.len(), 1);
                let (assignment_path, handler) = handlers[0];
                assert_eq!(assignment_path, "columns.VALUE.derivation");
                assert_eq!(handler.spec_path, "columns.VALUE.derivation.unconvertible");
                assert_eq!(handler.value, value);
                assert_eq!(
                    resolved::conversion_handler(&plan, "columns.VALUE.derivation"),
                    Some(handler)
                );
                assert_eq!(
                    resolved::op(&plan, resolved::columns(&plan)[0].expr),
                    &Op::Literal(Value::Bool(true))
                );
                assert_eq!(resolved::keys(&plan), [0]);
            }
            let prepared = prepare(&literal_document(N::Null)).unwrap();
            assert!(resolved::conversion_handlers(&prepared.bind(&source()).unwrap()).is_empty());
        }

        fn recovery_rows(value: N, count: usize) -> Tree {
            let mut rows = vec![Tree::map(vec![
                ("id", Tree::text("override")),
                (
                    "derivations",
                    Tree::map(vec![(
                        "VALUE",
                        Tree::map(vec![
                            ("value", Tree::Scalar(N::Boolean(true))),
                            ("unconvertible", Tree::Scalar(value)),
                        ]),
                    )]),
                ),
            ])];
            for id in ["default-one", "default-two"].into_iter().take(count - 1) {
                rows.push(Tree::map(vec![("id", Tree::text(id))]));
            }
            Tree::List(rows)
        }

        /// A seam the replay cannot reach: the resolved plan's conversion
        /// handlers in row order, and the row assignments and column
        /// bindings of the templates that share the column default.
        #[test]
        fn row_recovery_binds_only_effective_handlers_in_row_order_and_shares_defaults() {
            use yamaa_core::value::Value;
            for count in [1, 3] {
                let document = value_document(
                    Tree::Scalar(N::Boolean(true)),
                    "input.csv",
                    Some(N::Null),
                    Some(recovery_rows(N::Integer("7".into()), count)),
                );
                let plan = prepare(&document).unwrap().bind(&source()).unwrap();
                let handlers = resolved::conversion_handlers(&plan);
                assert_eq!(handlers.len(), if count == 1 { 1 } else { 2 });
                assert_eq!(handlers[0].0, "rows.override.derivations.VALUE");
                assert_eq!(
                    handlers[0].1.spec_path,
                    "rows.override.derivations.VALUE.unconvertible"
                );
                assert_eq!(handlers[0].1.value, Value::Int(7));
                if count == 3 {
                    assert_eq!(handlers[1].1.value, Value::Missing);
                    assert_eq!(
                        handlers[1].1.spec_path,
                        "columns.VALUE.derivation.unconvertible"
                    );
                    // Unneeded defaults stay column-phase per template: the two
                    // inheriting templates carry no row assignments and share
                    // the single column-phase default.
                    assert!(resolved::row(&plan, 1).is_empty());
                    assert!(resolved::row(&plan, 2).is_empty());
                    assert_eq!(
                        &*resolved::columns(&plan)[1].path,
                        "columns.VALUE.derivation"
                    );
                }
            }
        }

        /// A seam the replay cannot reach: the resolved plan's row
        /// assignments, template filter and column bindings.
        #[test]
        fn record_and_group_filters_promote_their_complete_row_local_default() {
            for grouped in [false, true] {
                let mut fields = vec![
                    ("id", Tree::text("first")),
                    ("filter", Tree::text("VALUE = 2")),
                    (
                        "derivations",
                        Tree::map(vec![(
                            "ID",
                            Tree::map(vec![("value", Tree::text("SRC.ID"))]),
                        )]),
                    ),
                ];
                if grouped {
                    fields.push(("group_by", Tree::List(vec![Tree::text("SRC.ID")])));
                }
                let document = value_document(
                    Tree::Scalar(N::Integer("2".into())),
                    "input.csv",
                    None,
                    Some(Tree::List(vec![Tree::map(fields)])),
                );
                let plan = prepare(&document).unwrap().bind(&source()).unwrap();
                assert_eq!(resolved::row(&plan, 0).len(), 2);
                assert!(resolved::templates(&plan)[0].filter.is_some());
                assert!(resolved::columns(&plan).is_empty());
            }
        }

        /// A seam the replay cannot reach: the resolved plan's source
        /// binding, its path, absence answer and selection.
        #[test]
        fn nested_source_options_keep_authored_handlers_and_selection_after_binding() {
            for fields in [
                vec![("absent", Tree::Scalar(N::Integer("7".into())))],
                vec![
                    ("order_by", Tree::List(vec![Tree::text("SRC.ID")])),
                    ("keep", Tree::text("first")),
                ],
            ] {
                let mut fields = fields.clone();
                let absent_handler = fields.iter().any(|(name, _)| *name == "absent");
                fields.insert(0, ("variable", Tree::text("SRC.ID")));
                let value = Tree::map(vec![("source", Tree::map(fields))]);
                let payload = Tree::List(vec![
                    Tree::map(vec![("when", Tree::text("ID = 1")), ("then", value)]),
                    Tree::map(vec![("otherwise", Tree::Scalar(N::Integer("0".into())))]),
                ]);
                let document = operation_document("case", payload, "input.csv");
                let prepared = prepare(&document).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                let case = resolved::columns(&plan)[0].expr;
                let Op::Case = resolved::op(&plan, case) else {
                    panic!("nested source operation")
                };
                let binding = resolved::operands(&plan, case, OperandRole::Branch)[0];
                let Op::SourceBinding {
                    absent,
                    path: bound_path,
                    ..
                } = resolved::op(&plan, binding)
                else {
                    panic!("source options lost")
                };
                assert_eq!(bound_path, "columns.VALUE.derivation.case[0].then.source");
                if absent_handler {
                    assert_eq!(absent, &Some(Value::Int(7)));
                } else {
                    assert_eq!(absent, &None);
                    let source = resolved::operands(&plan, binding, OperandRole::Value)[0];
                    assert!(
                        matches!(resolved::op(&plan, source), Op::Collect { selection: Some(selection), .. } if selection.keep == yamaa_core::dataset::Keep::First && selection.order_by.len()==1)
                    );
                }
            }
        }

        /// A seam the replay cannot reach: the resolved plan's selection,
        /// its source paths and the operands it collects or binds.
        #[test]
        fn first_available_retains_collection_and_decorated_operand_paths() {
            let document = operation_document(
                "first_available",
                Tree::map(vec![(
                    "sources",
                    Tree::List(vec![
                        Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                        Tree::map(vec![
                            ("variable", Tree::text("SRC.ID")),
                            ("absent", Tree::Scalar(N::Integer("7".into()))),
                        ]),
                    ]),
                )]),
                "input.csv",
            );
            let prepared = prepare(&document).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let selection = resolved::columns(&plan)[0].expr;
            let Op::FirstAvailable { sources, .. } = resolved::op(&plan, selection) else {
                panic!("compiled first_available");
            };
            assert_eq!(
                sources[0],
                "columns.VALUE.derivation.first_available.sources[0]"
            );
            let operands = resolved::operands(&plan, selection, OperandRole::Selection);
            assert!(matches!(
                resolved::op(&plan, operands[0]),
                Op::Collect { identifier, .. } if identifier == "SRC.ID"
            ));
            let assignment = plan.expr(operands[1]);
            assert_eq!(
                &*assignment.path,
                "columns.VALUE.derivation.first_available.sources[1]"
            );
            assert!(matches!(&assignment.op,
        Op::SourceBinding { path, .. } if **path == *assignment.path));
        }

        /// A seam the replay cannot reach: the resolved plan's verification
        /// checks, a predicate declaration and a deferred grammar finding.
        #[test]
        fn assertion_keeps_both_predicates_and_the_deferred_grammar_finding() {
            let when = "ID = 1";
            let require = "ID >";
            let tree = Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text("input.csv"))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![Tree::map(vec![
                        ("name", Tree::text("ID")),
                        ("type", Tree::text("int")),
                        (
                            "derivation",
                            Tree::map(vec![("value", Tree::text("SRC.ID"))]),
                        ),
                    ])]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        ("columns", Tree::List(vec![Tree::text("ID")])),
                    ]),
                ),
                (
                    "verifications",
                    Tree::List(vec![Tree::map(vec![(
                        "assert",
                        Tree::map(vec![
                            ("when", Tree::text(when)),
                            ("require", Tree::text(require)),
                        ]),
                    )])]),
                ),
            ]);
            let plan = prepare(&tree.document()).unwrap().bind(&source()).unwrap();
            let verifications = resolved::verifications(&plan);
            assert!(matches!(
                verifications[0],
                yamaa_core::dataset::Check::PredicateDeclaration(_)
            ));
            assert!(
                matches!(verifications[1], yamaa_core::dataset::Check::InvalidDiagnostic(d)
        if d.code == yamaa_core::diagnostic::ConditionCode::ExpressionInvalid)
            );
        }

        fn wide_document(columns: usize, typed_fields: usize) -> Document {
            wide_document_with_projection(columns, typed_fields, columns)
        }
        fn wide_document_with_projection(
            columns: usize,
            typed_fields: usize,
            projected: usize,
        ) -> Document {
            let column_names: Vec<_> = (1..columns).map(|i| format!("C{i:04}")).collect();
            let field_names: Vec<_> = (1..=typed_fields).map(|i| format!("F{i:04}")).collect();
            let mut names = vec!["ID"];
            let mut column_trees = vec![Tree::map(vec![
                ("name", Tree::text("ID")),
                ("type", Tree::text("str")),
                (
                    "derivation",
                    Tree::map(vec![("value", Tree::text("SRC.ID"))]),
                ),
            ])];
            for name in &column_names {
                names.push(name.as_str());
                column_trees.push(Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("str")),
                    ("derivation", Tree::map(vec![("value", Tree::text("'x'"))])),
                ]));
            }
            let mut input = vec![("path", Tree::text("input.csv"))];
            if typed_fields > 0 {
                let types = field_names
                    .iter()
                    .map(|name| (name.as_str(), Tree::text("str")))
                    .collect::<Vec<_>>();
                input.push(("types", Tree::map(types)));
            }
            let fields = vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
                ("domain", Tree::text("TEST")),
                ("input", Tree::map(vec![("SRC", Tree::map(input))])),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                ("columns", Tree::List(column_trees)),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        (
                            "columns",
                            Tree::List(
                                names
                                    .iter()
                                    .cycle()
                                    .take(projected)
                                    .map(Tree::text)
                                    .collect(),
                            ),
                        ),
                    ]),
                ),
            ];
            Tree::map(fields).document()
        }

        /// A seam the replay cannot reach: the compiler's fixed limits, a
        /// `PrepareError::Limit` that no reader returns, over documents
        /// generated 1,024 and 1,025 wide.
        #[test]
        fn fixed_column_ceilings_admit_1024_and_refuse_1025() {
            prepare(&wide_document(1_024, 0)).unwrap();
            let Err(PrepareError::Limit(resource)) = prepare(&wide_document(1_025, 0)) else {
                panic!("1,025 declared columns must hit the fixed compiler bound");
            };
            assert_eq!(resource.resource, "columns");
            assert_eq!(resource.limit, 1024);
            assert_eq!(resource.actual, Some(1025));
            assert_eq!(resource.spec_paths, ["columns"]);
            let Err(PrepareError::Limit(resource)) =
                prepare(&wide_document_with_projection(1_024, 0, 1_025))
            else {
                panic!("1,025 projected columns must hit their fixed bound independently");
            };
            assert_eq!(resource.resource, "projected_columns");
            assert_eq!(resource.limit, 1024);
            assert_eq!(resource.actual, Some(1025));
            assert_eq!(resource.spec_paths, ["output.columns"]);
            prepare(&wide_document(2, 1_024)).unwrap();
            let Err(PrepareError::Limit(resource)) = prepare(&wide_document(2, 1_025)) else {
                panic!("1,025 typed source fields must hit the fixed compiler bound");
            };
            assert_eq!(resource.resource, "source_fields");
            assert_eq!(resource.limit, 1024);
            assert_eq!(resource.actual, Some(1025));
            assert_eq!(resource.spec_paths, ["input.SRC.types"]);
        }
    }
}
