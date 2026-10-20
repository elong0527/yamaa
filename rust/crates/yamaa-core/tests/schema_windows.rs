//! Schema contracts.
#[allow(dead_code)]
mod support;
mod schema {
    mod schema_windows {
        use yamaa_core::schema::{
            Document, DocumentNode as N, ExpandedWindows, NormalizationError, SchemaContext,
            SchemaModule, SchemaStructure, WindowReference,
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
                                field("schema_version", "str"),
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

        #[test]
        fn non_strict_expansion_precedes_pruning_and_strict_failure_names_surviving_use() {
            let input = document(input(vec![
                column("DEAD", V::text("UNKNOWN")),
                column("LIVE", V::text("VISITS")),
            ]));
            let schema = schema();
            let expanded = schema.expand_named_windows(&input, false).unwrap();
            assert!(expanded
                .document
                .field(expanded.document.root(), "windows")
                .is_some());
            assert_eq!(
                expanded.document.nodes()[selection(&expanded, 0)],
                N::Text("UNKNOWN".into())
            );
            assert_eq!(expanded.references.len(), 1);
            let NormalizationError::Invalid(findings) =
                schema.expand_named_windows(&input, true).unwrap_err()
            else {
                panic!()
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].definition().condition, "unknown_window");
            assert_eq!(findings[0].definition().rule, Some("WINDOW-1"));
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
            assert!(schema.expand_named_windows(&alive, true).is_ok());
        }
        fn input_value_live() -> V {
            input(vec![column("LIVE", V::text("VISITS"))])
        }

        #[test]
        fn unused_malformed_definition_precedes_reference_errors() {
            let input = document(V::map(vec![
                ("windows", V::map(vec![("UNUSED", V::text("VISITS"))])),
                ("columns", V::List(vec![column("A", V::text("UNKNOWN"))])),
            ]));
            for strict in [false, true] {
                let NormalizationError::Invalid(findings) =
                    schema().expand_named_windows(&input, strict).unwrap_err()
                else {
                    panic!()
                };
                assert_eq!(findings[0].path, "windows.UNUSED");
                assert_eq!(findings[0].definition().condition, "invalid_field_type");
            }
        }

        #[test]
        fn strict_unknown_diagnostics_keep_column_declaration_order() {
            let input = document(V::map(vec![(
                "columns",
                V::List(vec![
                    column("Z", V::text("SECOND")),
                    column("A", V::text("FIRST")),
                ]),
            )]));
            let NormalizationError::Invalid(findings) =
                schema().expand_named_windows(&input, true).unwrap_err()
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
        fn repeated_window_expansion_preserves_complete_independent_results() {
            let schema = schema();
            let input = document(input_value_live());
            let first = schema.expand_named_windows(&input, true).unwrap();
            let second = schema.expand_named_windows(&input, true).unwrap();
            assert_eq!(first, second);
        }
    }
    mod specification_compiler {
        //! Compile normalized documents using only the core crate and immutable schemas.
        use crate::support::id_source_schema as source;
        use crate::support::resolved;
        use yamaa_core::{
            dataset::{
                resolved::{Op, OperandRole},
                ResolvedPlan,
            },
            schema::{Document, DocumentNode as N, SpecificationDocument},
            specification::{
                BindError, BindFinding, PreflightFinding, PrepareError, PreparedSpecification,
            },
            value::{ColumnType, Value},
        };

        use crate::support::document::Tree;

        fn document(expression: &str, input_path: &str) -> SpecificationDocument {
            operation_document(
                "compute",
                Tree::map(vec![("expr", Tree::text(expression))]),
                input_path,
            )
        }
        fn operation_document(
            operation: &str,
            payload: Tree,
            input_path: &str,
        ) -> SpecificationDocument {
            operation_document_with_handler(operation, payload, input_path, None)
        }
        fn operation_document_with_handler(
            operation: &str,
            payload: Tree,
            input_path: &str,
            handler: Option<N>,
        ) -> SpecificationDocument {
            operation_document_with_rows(operation, payload, input_path, handler, None)
        }
        fn operation_document_with_rows(
            operation: &str,
            payload: Tree,
            input_path: &str,
            handler: Option<N>,
            rows: Option<Tree>,
        ) -> SpecificationDocument {
            let column = |name, op, field, value| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("int")),
                    (
                        "derivation",
                        Tree::map(vec![(
                            "value",
                            Tree::map(vec![(op, Tree::map(vec![(field, Tree::text(value))]))]),
                        )]),
                    ),
                ])
            };
            let mut wrapper = vec![("value", Tree::map(vec![(operation, payload)]))];
            if let Some(value) = handler {
                wrapper.push(("unconvertible", Tree::Scalar(value)));
            }
            let mut fields = vec![
                ("schema_version", Tree::text("1.0")),
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
                        column("ID", "source", "variable", "SRC.ID"),
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
            ];
            if let Some(rows) = rows {
                fields.push(("rows", rows));
            }
            let tree = Tree::map(fields);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            SpecificationDocument::admit(document).unwrap().unwrap()
        }

        #[test]
        fn aggregate_derive_windows_fail_preflight_at_the_nested_operation_path() {
            use yamaa_core::json::Value as JsonValue;
            use yamaa_core::{diagnostic::ConditionCode, specification::PreflightFinding};

            for operation in ["row_number", "rank", "row_value", "locf", "baseline_flag"] {
                for variable in ["SRC.ID", "ID"] {
                    let mut fields = vec![(
                        "window",
                        Tree::map(vec![("order_by", Tree::List(vec![Tree::text(variable)]))]),
                    )];
                    match operation {
                        "row_value" => {
                            fields.push(("source", Tree::text(variable)));
                            fields.push(("offset", Tree::Scalar(N::Integer("-1".into()))));
                        }
                        "locf" => fields.push(("source", Tree::text(variable))),
                        "baseline_flag" => {
                            fields.push(("date", Tree::text(variable)));
                            fields.push(("reference_date", Tree::text(variable)));
                            fields.push(("value", Tree::text(variable)));
                        }
                        _ => {}
                    }
                    let window = Tree::map(vec![(operation, Tree::map(fields))]);
                    let literal =
                        Tree::map(vec![("literal", Tree::Scalar(N::Integer("0".into())))]);
                    for (derivation, suffix) in [
                        (window.clone(), format!(".{operation}")),
                        (
                            Tree::map(vec![(
                                "case",
                                Tree::List(vec![
                                    Tree::map(vec![
                                        ("when", Tree::text("SRC.ID >= 0")),
                                        ("then", window.clone()),
                                    ]),
                                    Tree::map(vec![("otherwise", literal.clone())]),
                                ]),
                            )]),
                            format!(".case[0].then.{operation}"),
                        ),
                        (
                            Tree::map(vec![(
                                "case",
                                Tree::List(vec![
                                    Tree::map(vec![
                                        ("when", Tree::text("SRC.ID >= 0")),
                                        ("then", literal.clone()),
                                    ]),
                                    Tree::map(vec![("otherwise", window.clone())]),
                                ]),
                            )]),
                            format!(".case[1].otherwise.{operation}"),
                        ),
                    ] {
                        let document = operation_document(
                            "aggregate",
                            Tree::map(vec![
                                (
                                    "derive",
                                    Tree::List(vec![Tree::map(vec![
                                        ("name", Tree::text("V")),
                                        ("type", Tree::text("int")),
                                        ("derivation", Tree::map(vec![("value", derivation)])),
                                    ])]),
                                ),
                                ("expr", Tree::text("SUM(V)")),
                            ]),
                            "input.csv",
                        );
                        let outcome = PreparedSpecification::prepare(&document);
                        let Err(PrepareError::Invalid(findings)) = outcome else {
                            panic!("unexpected aggregate derive window outcome: {operation} {variable}: {outcome:?}");
                        };
                        let [PreflightFinding::Scalar(diagnostic)] = findings.as_slice() else {
                            panic!("unexpected window findings: {findings:?}");
                        };
                        assert_eq!(diagnostic.code, ConditionCode::AggregateDeriveDeclaration);
                        assert_eq!(diagnostic.definition().phase, "validation");
                        assert_eq!(diagnostic.definition().condition, "invalid_derive_step");
                        assert_eq!(diagnostic.definition().rule, Some("LOOKUP-22"));
                        assert_eq!(
                            diagnostic.spec_paths,
                            [format!(
                                "columns.VALUE.derivation.aggregate.derive[0].derivation{suffix}"
                            )]
                        );
                        assert_eq!(
                            diagnostic.context["reason"],
                            JsonValue::String(
                                "a derive binding cannot use a window operation".into()
                            )
                        );
                    }
                }
            }
        }

        #[test]
        fn aggregate_derive_window_names_in_literals_remain_literals() {
            let binding = |name, kind, value| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text(kind)),
                    ("derivation", Tree::map(vec![("value", value)])),
                ])
            };
            let document = operation_document(
                "aggregate",
                Tree::map(vec![
                    (
                        "derive",
                        Tree::List(vec![
                            binding(
                                "V",
                                "int",
                                Tree::map(vec![(
                                    "source",
                                    Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                )]),
                            ),
                            binding(
                                "TAG",
                                "str",
                                Tree::map(vec![("literal", Tree::text("row_value"))]),
                            ),
                        ]),
                    ),
                    ("expr", Tree::text("SUM(V)")),
                ]),
                "input.csv",
            );
            PreparedSpecification::prepare(&document).unwrap();
        }

        fn case_document(when: &str, expression: &str) -> SpecificationDocument {
            operation_document(
                "case",
                Tree::List(vec![
                    Tree::map(vec![
                        ("when", Tree::text(when)),
                        (
                            "then",
                            Tree::map(vec![(
                                "compute",
                                Tree::map(vec![("expr", Tree::text(expression))]),
                            )]),
                        ),
                    ]),
                    Tree::map(vec![(
                        "otherwise",
                        Tree::map(vec![("literal", Tree::Scalar(N::Integer("1".into())))]),
                    )]),
                ]),
                "input.csv",
            )
        }

        #[test]
        fn case_grammar_findings_retain_their_original_nested_paths() {
            let prepared = PreparedSpecification::prepare(&case_document("ID >", "1")).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("a predicate expression finding")
            };
            let diagnostics = findings[0].diagnostics(prepared.source()).unwrap();
            assert_eq!(
                diagnostics[0].spec_paths,
                ["columns.VALUE.derivation.case[0].when"]
            );
            assert!(matches!(findings[0], BindFinding::Predicate(_)));
            let prepared =
                PreparedSpecification::prepare(&case_document("ID > 0", "ID +")).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("a compute expression finding")
            };
            let diagnostics = findings[0].diagnostics(prepared.source()).unwrap();
            assert_eq!(
                diagnostics[0].spec_paths,
                ["columns.VALUE.derivation.case[0].then.compute.expr"]
            );
            assert!(matches!(findings[0], BindFinding::Numeric { .. }));
        }

        #[test]
        fn non_key_case_predicate_owns_key_grain_collection_without_study_authority() {
            let prepared =
                PreparedSpecification::prepare(&case_document("SRC.ID > 0", "1")).unwrap();
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

        #[test]
        fn compiler_owns_bound_plan_without_an_engine_or_source_port() {
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1 / 0", "input.csv")).unwrap();
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
            assert_eq!(
                compute.expression().spec_path(),
                "columns.VALUE.derivation.compute"
            );
            // Division by zero remains deferred until an application actually executes a row.
            assert_eq!(&*binding.path, "columns.VALUE.derivation.compute");
        }

        #[test]
        fn compiler_retains_admission_binding_and_resource_boundaries() {
            let prepared = PreparedSpecification::prepare(&document("ID +", "input.csv")).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("binding diagnostic")
            };
            assert!(
                matches!(&findings[..], [BindFinding::Numeric { path, expression, .. }] if path == "columns.VALUE.derivation.compute.expr" && expression == "ID +")
            );
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare(&document("ID + 1", "input.bad"))
            else {
                panic!("unknown source profile admission")
            };
            assert_eq!(findings.len(), 1);
            let diagnostic = findings[0].diagnostic();
            assert_eq!(
                diagnostic.code,
                yamaa_core::diagnostic::ConditionCode::SourceUnknownProfile
            );
            assert_eq!(diagnostic.spec_paths, ["input.SRC.path"]);
            assert_eq!(
                diagnostic.context,
                [("dataset", "SRC"), ("path", "input.bad")]
                    .into_iter()
                    .map(|(key, value)| (key.into(), yamaa_core::json::Value::String(value.into())))
                    .collect()
            );
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.PARQUET")).unwrap();
            assert_eq!(
                prepared.source().profile,
                yamaa_core::specification::SourceProfile::Parquet
            );
            assert!(!prepared.source().empty_string_present);
            assert!(prepared.source().text_is_missing(""));
            assert!(!prepared.source().text_is_missing("NA"));
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            assert_eq!(
                resolved::output(&prepared.bind(&source()).unwrap())
                    .columns()
                    .len(),
                2
            );
        }

        #[test]
        fn parquet_type_declarations_keep_all_semantic_findings() {
            let original = document("ID + 1", "source.parquet");
            let d = original.document();
            let inputs = d.field(d.root(), "input").unwrap();
            let source = d.field(inputs, "SRC").unwrap();
            let mut nodes = d.nodes().to_vec();
            let types = Tree::map(vec![
                ("ID", Tree::text("int")),
                ("VALUE", Tree::text("float")),
            ])
            .append(&mut nodes);
            let name = Tree::text("types").append(&mut nodes);
            let N::Mapping(fields) = &mut nodes[source] else {
                unreachable!()
            };
            fields.push((name, types));
            // Reorder the arena after adding fields, preserving the model's ownership contract.
            fn copy(input: &[N], id: usize, out: &mut Vec<N>) -> usize {
                let value = match &input[id] {
                    N::Sequence(items) => {
                        N::Sequence(items.iter().map(|&id| copy(input, id, out)).collect())
                    }
                    N::Mapping(items) => N::Mapping(
                        items
                            .iter()
                            .map(|&(k, v)| (copy(input, k, out), copy(input, v, out)))
                            .collect(),
                    ),
                    value => value.clone(),
                };
                let index = out.len();
                out.push(value);
                index
            }
            let mut ordered = Vec::new();
            let root = copy(&nodes, d.root(), &mut ordered);
            let model = SpecificationDocument::admit(Document::new(ordered, root).unwrap())
                .unwrap()
                .unwrap();
            let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&model)
            else {
                panic!("all redundant parquet type declarations remain semantic findings");
            };
            assert_eq!(findings.len(), 2);
            assert!(findings.iter().all(|f| matches!(
                f,
                yamaa_core::specification::PreflightFinding::RedundantSourceType { .. }
            )));
        }

        fn window_document(filter: &str) -> SpecificationDocument {
            window_settings(filter, "row_number", None, None, None)
        }
        fn window_settings(
            filter: &str,
            operation: &str,
            method: Option<&str>,
            direction: Option<&str>,
            nulls: Option<&str>,
        ) -> SpecificationDocument {
            let original = document("0", "input.csv");
            let input = original.document();
            let N::Sequence(columns) =
                &input.nodes()[input.field(input.root(), "columns").unwrap()]
            else {
                unreachable!()
            };
            let derivation = input.field(columns[1], "derivation").unwrap();
            let value = input.field(derivation, "value").unwrap();
            let payload = input.field(value, "compute").unwrap();
            let mut nodes = input.nodes().to_vec();
            let mut order = vec![("variable", Tree::text("ID"))];
            if let Some(value) = direction {
                order.push(("direction", Tree::text(value)));
            }
            if let Some(value) = nulls {
                order.push(("nulls", Tree::text(value)));
            }
            let window = Tree::map(vec![
                ("order_by", Tree::List(vec![Tree::map(order)])),
                ("filter", Tree::text(filter)),
            ])
            .append(&mut nodes);
            let window_name = Tree::text("window").append(&mut nodes);
            let operation = Tree::text(operation).append(&mut nodes);
            let mut options = vec![(window_name, window)];
            if let Some(value) = method {
                options.push((
                    Tree::text("method").append(&mut nodes),
                    Tree::text(value).append(&mut nodes),
                ));
            }
            nodes[payload] = N::Mapping(options);
            nodes[value] = N::Mapping(vec![(operation, payload)]);
            fn copy_node(input: &[N], id: usize, output: &mut Vec<N>) -> usize {
                let node = match &input[id] {
                    N::Sequence(items) => N::Sequence(
                        items
                            .iter()
                            .map(|&id| copy_node(input, id, output))
                            .collect(),
                    ),
                    N::Mapping(items) => N::Mapping(
                        items
                            .iter()
                            .map(|&(a, b)| {
                                (copy_node(input, a, output), copy_node(input, b, output))
                            })
                            .collect(),
                    ),
                    node => node.clone(),
                };
                let id = output.len();
                output.push(node);
                id
            }
            let mut ordered = Vec::new();
            let root = copy_node(&nodes, input.root(), &mut ordered);
            let doc = Document::new(ordered, root).unwrap();
            SpecificationDocument::admit(doc).unwrap().unwrap()
        }

        #[test]
        fn window_compiler_binds_output_dependencies_without_engine_or_records() {
            use yamaa_core::dataset::WindowKind;
            let prepared = PreparedSpecification::prepare(&window_document("ID > 0")).unwrap();
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
            let qualified = PreparedSpecification::prepare(&window_document("SRC.ID > 0"))
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
                        let plan = PreparedSpecification::prepare(&document)
                            .unwrap()
                            .bind(&source())
                            .unwrap();
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

        fn literal_document(node: N) -> SpecificationDocument {
            operation_document("literal", Tree::Scalar(node), "input.csv")
        }

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
                let prepared = PreparedSpecification::prepare(&literal_document(node)).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                let literal = resolved::columns(&plan)[0];
                assert_eq!(resolved::op(&plan, literal.expr), &Op::Literal(value));
                assert_eq!(&*literal.path, "columns.VALUE.derivation.literal");
                assert_eq!(resolved::output(&plan).columns()[1].kind, ColumnType::Int);
                assert_eq!(resolved::keys(&plan), [0]);
            }
            // A Boolean converts to no column type (VALUE-3), so its int column
            // is refused before any record is read (VALUE-24).
            let prepared =
                PreparedSpecification::prepare(&literal_document(N::Boolean(true))).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("a Boolean literal cannot fill an int column")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(diagnostics, prepared.check_declaration_diagnostics());
            assert_eq!(
                diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.definition().condition,
                        diagnostic.spec_paths.as_slice()
                    ))
                    .collect::<Vec<_>>(),
                [(
                    "conversion_failed",
                    ["columns.VALUE".to_string()].as_slice()
                )]
            );
        }

        #[test]
        fn out_of_range_literal_integers_fail_validation_before_binding() {
            let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(
                &literal_document(N::Integer("9223372036854775808".into())),
            ) else {
                panic!("wide integer must not narrow")
            };
            assert_eq!(
                findings,
                [PreflightFinding::IntegerLiteralOverflow {
                    path: "columns.VALUE.derivation.literal".into(),
                    value: "9223372036854775808".into(),
                }]
            );
        }

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
                let document = operation_document_with_handler(
                    "literal",
                    Tree::Scalar(N::Boolean(true)),
                    "input.csv",
                    Some(node),
                );
                let prepared = PreparedSpecification::prepare(&document).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                let handlers = resolved::conversion_handlers(&plan);
                assert_eq!(handlers.len(), 1);
                let (assignment_path, handler) = handlers[0];
                assert_eq!(assignment_path, "columns.VALUE.derivation.literal");
                assert_eq!(handler.spec_path, "columns.VALUE.derivation.unconvertible");
                assert_eq!(handler.value, value);
                assert_eq!(
                    resolved::conversion_handler(&plan, "columns.VALUE.derivation.literal"),
                    Some(handler)
                );
                assert_eq!(
                    resolved::op(&plan, resolved::columns(&plan)[0].expr),
                    &Op::Literal(Value::Bool(true))
                );
                assert_eq!(resolved::keys(&plan), [0]);
            }
            let prepared = PreparedSpecification::prepare(&literal_document(N::Null)).unwrap();
            assert!(resolved::conversion_handlers(&prepared.bind(&source()).unwrap()).is_empty());
        }

        #[test]
        fn wide_recovery_literal_fails_validation_before_source_binding() {
            let document = operation_document_with_handler(
                "literal",
                Tree::Scalar(N::Boolean(true)),
                "input.csv",
                Some(N::Integer("9223372036854775808".into())),
            );
            let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&document)
            else {
                panic!("wide handler must not narrow")
            };
            assert_eq!(
                findings,
                [PreflightFinding::IntegerLiteralOverflow {
                    path: "columns.VALUE.derivation.unconvertible".into(),
                    value: "9223372036854775808".into(),
                }]
            );
        }

        fn recovery_rows(value: N, count: usize) -> Tree {
            let mut rows = vec![Tree::map(vec![
                ("id", Tree::text("override")),
                (
                    "derivations",
                    Tree::map(vec![(
                        "VALUE",
                        Tree::map(vec![
                            (
                                "value",
                                Tree::map(vec![("literal", Tree::Scalar(N::Boolean(true)))]),
                            ),
                            ("unconvertible", Tree::Scalar(value)),
                        ]),
                    )]),
                ),
            ])];
            for id in ["default-one", "default-two"].into_iter().take(count - 1) {
                rows.push(Tree::map(vec![
                    ("id", Tree::text(id)),
                    ("derivations", Tree::map(vec![])),
                ]));
            }
            Tree::List(rows)
        }

        #[test]
        fn row_recovery_binds_only_effective_handlers_in_row_order_and_shares_defaults() {
            use yamaa_core::value::Value;
            for count in [1, 3] {
                let document = operation_document_with_rows(
                    "literal",
                    Tree::Scalar(N::Boolean(true)),
                    "input.csv",
                    Some(N::Null),
                    Some(recovery_rows(N::Integer("7".into()), count)),
                );
                let plan = PreparedSpecification::prepare(&document)
                    .unwrap()
                    .bind(&source())
                    .unwrap();
                let handlers = resolved::conversion_handlers(&plan);
                assert_eq!(handlers.len(), if count == 1 { 1 } else { 2 });
                assert_eq!(handlers[0].0, "rows[0].derivations.VALUE.literal");
                assert_eq!(
                    handlers[0].1.spec_path,
                    "rows[0].derivations.VALUE.unconvertible"
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
                        "columns.VALUE.derivation.literal"
                    );
                }
            }
        }

        #[test]
        fn null_rows_prepare_without_internal_error() {
            // #2428: `rows: null` is the natural YAML spelling of "no row
            // derivations" and the rest of preparation treats a Null node as
            // absent. Imputation declaration findings ran unconditionally and
            // passed the Null node to `sequence()`, aborting preparation with
            // `PrepareError::Internal` even for specifications with no imputation.
            // The same unfiltered pattern in the other unconditional declaration
            // passes (match values, scalar, str_case, str_extract, str_pad and
            // str_template) aborted preparation immediately afterwards, so those
            // passes skip a Null `rows` node too.
            let document = operation_document_with_rows(
                "compute",
                Tree::map(vec![("expr", Tree::text("ID + 1"))]),
                "input.csv",
                None,
                Some(Tree::Scalar(N::Null)),
            );
            PreparedSpecification::prepare(&document).unwrap();
        }

        #[test]
        fn wide_row_recovery_fails_validation_at_its_authored_path_before_binding() {
            let document = operation_document_with_rows(
                "literal",
                Tree::Scalar(N::Boolean(true)),
                "input.csv",
                None,
                Some(recovery_rows(N::Integer("9223372036854775808".into()), 1)),
            );
            let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&document)
            else {
                panic!("wide row handler must not narrow")
            };
            assert_eq!(
                findings,
                [PreflightFinding::IntegerLiteralOverflow {
                    path: "rows[0].derivations.VALUE.unconvertible".into(),
                    value: "9223372036854775808".into(),
                }]
            );
        }

        #[test]
        fn row_filter_carrier_findings_remain_normative_including_dead_branches() {
            for (filter, position) in [
                ("FALSE AND ID = 9223372036854775808", "15"),
                ("TRUE OR ID = -9223372036854775809", "13"),
            ] {
                let rows = Tree::List(vec![Tree::map(vec![
                    ("id", Tree::text("first")),
                    ("filter", Tree::text(filter)),
                    (
                        "derivations",
                        Tree::map(vec![
                            (
                                "ID",
                                Tree::map(vec![(
                                    "value",
                                    Tree::map(vec![(
                                        "source",
                                        Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                    )]),
                                )]),
                            ),
                            (
                                "VALUE",
                                Tree::map(vec![(
                                    "value",
                                    Tree::map(vec![(
                                        "literal",
                                        Tree::Scalar(N::Integer("2".into())),
                                    )]),
                                )]),
                            ),
                        ]),
                    ),
                ])]);
                let document = operation_document_with_rows(
                    "literal",
                    Tree::Scalar(N::Integer("2".into())),
                    "input.csv",
                    None,
                    Some(rows),
                );
                let prepared = PreparedSpecification::prepare(&document).unwrap();
                let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                    panic!("predicate overflow must remain a deferred normative finding");
                };
                let [BindFinding::Lookup(finding)] = findings.as_slice() else {
                    panic!("unexpected carrier findings: {findings:?}");
                };
                let diagnostic = finding.diagnostic();
                assert_eq!(
                    diagnostic.code,
                    yamaa_core::diagnostic::ConditionCode::LiteralIntegerOverflow
                );
                assert_eq!(diagnostic.spec_paths, ["rows[0].filter"]);
                assert_eq!(diagnostic.definition().phase, "validation");
                assert_eq!(diagnostic.definition().rule, Some("VALUE-5"));
                assert_eq!(diagnostic.context.len(), 2);
                assert_eq!(
                    diagnostic.context["predicate"],
                    yamaa_core::json::Value::String(filter.into())
                );
                assert_eq!(
                    diagnostic.context["position"],
                    yamaa_core::json::Value::Number((position).parse().expect("canonical integer"))
                );
            }
        }

        #[test]
        fn row_filters_retain_both_deferred_grammar_findings() {
            let rows = Tree::List(
                ["first", "second"]
                    .into_iter()
                    .map(|id| {
                        Tree::map(vec![
                            ("id", Tree::text(id)),
                            ("filter", Tree::text("ID >")),
                            (
                                "derivations",
                                Tree::map(vec![
                                    (
                                        "ID",
                                        Tree::map(vec![(
                                            "value",
                                            Tree::map(vec![(
                                                "source",
                                                Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                            )]),
                                        )]),
                                    ),
                                    (
                                        "VALUE",
                                        Tree::map(vec![(
                                            "value",
                                            Tree::map(vec![(
                                                "literal",
                                                Tree::Scalar(N::Integer("2".into())),
                                            )]),
                                        )]),
                                    ),
                                ]),
                            ),
                        ])
                    })
                    .collect(),
            );
            let document = operation_document_with_rows(
                "literal",
                Tree::Scalar(N::Integer("2".into())),
                "input.csv",
                None,
                Some(rows),
            );
            let prepared = PreparedSpecification::prepare(&document).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("both authored row filters must retain their grammar failures");
            };
            assert_eq!(findings.len(), 2);
            for (finding, path) in findings.iter().zip(["rows[0].filter", "rows[1].filter"]) {
                let BindFinding::Lookup(finding) = finding else {
                    panic!("row predicate finding")
                };
                let diagnostic = finding.diagnostic();
                assert_eq!(
                    diagnostic.code,
                    yamaa_core::diagnostic::ConditionCode::ExpressionInvalid
                );
                assert_eq!(diagnostic.spec_paths, [path]);
                assert_eq!(
                    diagnostic.context["predicate"],
                    yamaa_core::json::Value::from("ID >")
                );
            }
        }

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
                            Tree::map(vec![(
                                "value",
                                Tree::map(vec![(
                                    "source",
                                    Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                )]),
                            )]),
                        )]),
                    ),
                ];
                if grouped {
                    fields.push(("group_by", Tree::List(vec![Tree::text("SRC.ID")])));
                }
                let document = operation_document_with_rows(
                    "literal",
                    Tree::Scalar(N::Integer("2".into())),
                    "input.csv",
                    None,
                    Some(Tree::List(vec![Tree::map(fields)])),
                );
                let plan = PreparedSpecification::prepare(&document)
                    .unwrap()
                    .bind(&source())
                    .unwrap();
                assert_eq!(resolved::row(&plan, 0).len(), 2);
                assert!(resolved::templates(&plan)[0].filter.is_some());
                assert!(resolved::columns(&plan).is_empty());
            }
        }

        #[test]
        fn source_filters_bind_valid_text_and_retain_carrier_findings() {
            let document = operation_document(
                "source",
                Tree::map(vec![
                    ("variable", Tree::text("SRC.ID")),
                    ("filter", Tree::text("SRC.ID = 1")),
                ]),
                "input.csv",
            );
            PreparedSpecification::prepare(&document)
                .unwrap()
                .bind(&source())
                .unwrap();
            let document = operation_document(
                "source",
                Tree::map(vec![
                    ("variable", Tree::text("SRC.ID")),
                    ("filter", Tree::text("SRC.ID = 9223372036854775808")),
                ]),
                "input.csv",
            );
            let prepared = PreparedSpecification::prepare(&document).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("source predicate overflow must remain a deferred normative finding");
            };
            let [BindFinding::Source(finding)] = findings.as_slice() else {
                panic!("unexpected source carrier findings: {findings:?}");
            };
            let diagnostic = finding.diagnostic();
            assert_eq!(
                diagnostic.code,
                yamaa_core::diagnostic::ConditionCode::LiteralIntegerOverflow
            );
            assert_eq!(diagnostic.definition().phase, "validation");
            assert_eq!(diagnostic.definition().rule, Some("VALUE-5"));
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.VALUE.derivation.source.filter"]
            );
            assert_eq!(diagnostic.context.len(), 2);
            assert_eq!(
                diagnostic.context["predicate"],
                yamaa_core::json::Value::String("SRC.ID = 9223372036854775808".into())
            );
            assert_eq!(
                diagnostic.context["position"],
                yamaa_core::json::Value::Number(("9").parse().expect("canonical integer"))
            );
        }

        #[test]
        fn selection_filters_retain_their_decorated_operand_path() {
            let payload = Tree::map(vec![(
                "sources",
                Tree::List(vec![Tree::map(vec![
                    ("variable", Tree::text("SRC.ID")),
                    ("filter", Tree::text("SRC.ID =")),
                ])]),
            )]);
            let document = operation_document("first_available", payload, "input.csv");
            let prepared = PreparedSpecification::prepare(&document).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("deferred selection grammar")
            };
            assert_eq!(findings.len(), 1);
            let BindFinding::Source(finding) = &findings[0] else {
                panic!("source grammar")
            };
            let diagnostic = finding.diagnostic();
            assert_eq!(
                diagnostic.code,
                yamaa_core::diagnostic::ConditionCode::ExpressionInvalid
            );
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.VALUE.derivation.first_available.sources[0].filter"]
            );
            assert_eq!(
                diagnostic.context["predicate"],
                yamaa_core::json::Value::from("SRC.ID =")
            );
        }

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
                    Tree::map(vec![(
                        "otherwise",
                        Tree::map(vec![("literal", Tree::Scalar(N::Integer("0".into())))]),
                    )]),
                ]);
                let document = operation_document("case", payload, "input.csv");
                let prepared = PreparedSpecification::prepare(&document).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                let case = resolved::columns(&plan)[0].expr;
                let Op::Case = resolved::op(&plan, case) else {
                    panic!("nested source operation")
                };
                let binding = resolved::operands(&plan, case, OperandRole::Branch)[0];
                let Op::SourceBinding {
                    absent,
                    variable_absent,
                    path: bound_path,
                    ..
                } = resolved::op(&plan, binding)
                else {
                    panic!("source options lost")
                };
                assert_eq!(bound_path, "columns.VALUE.derivation.case[0].then.source");
                assert!(!variable_absent);
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

        #[test]
        fn unqualified_key_source_absence_is_an_admitted_key_expression() {
            let document = operation_document(
                "source",
                Tree::map(vec![
                    ("variable", Tree::text("SOME_NAME")),
                    ("absent", Tree::Scalar(N::Integer("7".into()))),
                ]),
                "input.csv",
            );
            let raw = document.document();
            let mut nodes = raw.nodes().to_vec();
            let N::Sequence(keys) = &nodes[raw.field(raw.root(), "keys").unwrap()] else {
                panic!("key sequence");
            };
            let key = keys[0];
            nodes[key] = N::Text("VALUE".into());
            let document = SpecificationDocument::admit(Document::new(nodes, raw.root()).unwrap())
                .unwrap()
                .unwrap();
            let prepared = PreparedSpecification::prepare(&document).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let binding = resolved::row(&plan, 0)[0].expr;
            let source = resolved::operands(&plan, binding, OperandRole::Value)[0];
            assert!(matches!(resolved::op(&plan, binding),
        Op::SourceBinding { absent: Some(Value::Int(7)), variable_absent: true, .. }
        if matches!(resolved::op(&plan, source), Op::Literal(Value::Missing))));
        }

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
            let prepared = PreparedSpecification::prepare(&document).unwrap();
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

        #[test]
        fn assertion_keeps_both_predicates_and_the_deferred_grammar_finding() {
            let when = "ID = 1";
            let require = "ID >";
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
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
                            Tree::map(vec![(
                                "value",
                                Tree::map(vec![(
                                    "source",
                                    Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                )]),
                            )]),
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
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
                .unwrap()
                .unwrap();
            let plan = PreparedSpecification::prepare(&document)
                .unwrap()
                .bind(&source())
                .unwrap();
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

        fn verification_document(kind: &str, check: Tree, column: bool) -> SpecificationDocument {
            let mut value = vec![
                ("name", Tree::text("VALUE")),
                ("type", Tree::text(kind)),
                (
                    "derivation",
                    Tree::map(vec![(
                        "value",
                        Tree::map(vec![("literal", Tree::Scalar(N::Integer("1".into())))]),
                    )]),
                ),
            ];
            if column {
                value.push(("verifications", Tree::List(vec![check.clone()])));
            }
            let mut tree = vec![
                ("schema_version", Tree::text("1.0")),
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
                    Tree::List(vec![
                        Tree::map(vec![
                            ("name", Tree::text("ID")),
                            ("type", Tree::text("int")),
                            (
                                "derivation",
                                Tree::map(vec![(
                                    "value",
                                    Tree::map(vec![(
                                        "source",
                                        Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                    )]),
                                )]),
                            ),
                        ]),
                        Tree::map(value),
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
            ];
            if !column {
                tree.push(("verifications", Tree::List(vec![check])));
            }
            let mut nodes = Vec::new();
            let root = Tree::map(tree).append(&mut nodes);
            SpecificationDocument::admit(Document::new(nodes, root).unwrap())
                .unwrap()
                .unwrap()
        }
        #[test]
        fn malformed_assert_carriers_and_numeric_bounds_are_typed_declaration_findings() {
            use yamaa_core::diagnostic::ConditionCode as C;
            use yamaa_core::json::Value as V;

            let checks = [
                (
                    "int",
                    false,
                    Tree::map(vec![(
                        "assert",
                        Tree::map(vec![
                            ("when", Tree::Scalar(N::Boolean(true))),
                            ("require", Tree::text("VALUE > 0")),
                        ]),
                    )]),
                    C::VerificationDeclarationRequired,
                    "verifications[0].assert.when",
                    "a predicate must be text",
                ),
                (
                    "int",
                    true,
                    Tree::map(vec![(
                        "range",
                        Tree::map(vec![
                            ("min", Tree::Scalar(N::Boolean(false))),
                            ("max", Tree::Scalar(N::Integer("7".into()))),
                        ]),
                    )]),
                    C::VerificationDeclarationRequired,
                    "columns.VALUE.verifications[0].range",
                    "a range bound is numeric",
                ),
                (
                    "str",
                    true,
                    Tree::map(vec![(
                        "range",
                        Tree::map(vec![("min", Tree::Scalar(N::Boolean(false)))]),
                    )]),
                    C::VerificationColumnType,
                    "columns.VALUE.verifications[0].range",
                    "a str column does not admit this verification",
                ),
                (
                    "int",
                    true,
                    Tree::map(vec![(
                        "allowed_values",
                        Tree::map(vec![(
                            "values",
                            Tree::List(vec![
                                Tree::Scalar(N::Null),
                                Tree::Scalar(N::Boolean(false)),
                            ]),
                        )]),
                    )]),
                    C::VerificationColumnType,
                    "columns.VALUE.verifications[0].allowed_values",
                    "listed value None is not a int value",
                ),
            ];
            for (kind, column, check, code, path, reason) in checks {
                let prepared =
                    PreparedSpecification::prepare(&verification_document(kind, check, column))
                        .unwrap();
                let findings = prepared.verification_declaration_diagnostics();
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].code, code);
                assert_eq!(findings[0].spec_paths, [path]);
                assert_eq!(findings[0].definition().phase, "validation");
                assert_eq!(
                    findings[0].context,
                    [("reason".into(), V::String(reason.into()))].into()
                );
                assert!(
                    prepared.bind(&source()).is_ok(),
                    "declarations retain their runtime checkpoint"
                );
            }
        }

        fn wide_document(columns: usize, typed_fields: usize) -> SpecificationDocument {
            wide_document_with_projection(columns, typed_fields, columns)
        }
        fn wide_document_with_projection(
            columns: usize,
            typed_fields: usize,
            projected: usize,
        ) -> SpecificationDocument {
            let column_names: Vec<_> = (1..columns).map(|i| format!("C{i:04}")).collect();
            let field_names: Vec<_> = (1..=typed_fields).map(|i| format!("F{i:04}")).collect();
            let mut names = vec!["ID"];
            let mut column_trees = vec![Tree::map(vec![
                ("name", Tree::text("ID")),
                ("type", Tree::text("str")),
                (
                    "derivation",
                    Tree::map(vec![(
                        "value",
                        Tree::map(vec![(
                            "source",
                            Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                        )]),
                    )]),
                ),
            ])];
            for name in &column_names {
                names.push(name.as_str());
                column_trees.push(Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("str")),
                    (
                        "derivation",
                        Tree::map(vec![(
                            "value",
                            Tree::map(vec![("literal", Tree::Scalar(N::Text("x".into())))]),
                        )]),
                    ),
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
                ("schema_version", Tree::text("1.0")),
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
            let tree = Tree::map(fields);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            SpecificationDocument::admit(document).unwrap().unwrap()
        }

        #[test]
        fn fixed_column_ceilings_admit_1024_and_refuse_1025() {
            PreparedSpecification::prepare(&wide_document(1_024, 0)).unwrap();
            let Err(PrepareError::Limit(resource)) =
                PreparedSpecification::prepare(&wide_document(1_025, 0))
            else {
                panic!("1,025 declared columns must hit the fixed compiler bound");
            };
            assert_eq!(resource.resource, "columns");
            assert_eq!(resource.limit, 1024);
            assert_eq!(resource.actual, Some(1025));
            assert_eq!(resource.spec_paths, ["columns"]);
            let Err(PrepareError::Limit(resource)) =
                PreparedSpecification::prepare(&wide_document_with_projection(1_024, 0, 1_025))
            else {
                panic!("1,025 projected columns must hit their fixed bound independently");
            };
            assert_eq!(resource.resource, "projected_columns");
            assert_eq!(resource.limit, 1024);
            assert_eq!(resource.actual, Some(1025));
            assert_eq!(resource.spec_paths, ["output.columns"]);
            PreparedSpecification::prepare(&wide_document(2, 1_024)).unwrap();
            let Err(PrepareError::Limit(resource)) =
                PreparedSpecification::prepare(&wide_document(2, 1_025))
            else {
                panic!("1,025 typed source fields must hit the fixed compiler bound");
            };
            assert_eq!(resource.resource, "source_fields");
            assert_eq!(resource.limit, 1024);
            assert_eq!(resource.actual, Some(1025));
            assert_eq!(resource.spec_paths, ["input.SRC.types"]);
        }

        #[test]
        fn qualified_aggregate_group_by_must_be_an_output_key_req_0470() {
            use yamaa_core::{
                diagnostic::ConditionCode,
                table::{Column, TableSchema},
            };

            // Donor SRC has ID (the output/join key), PARAM (a second grouping
            // column) and V. Two donor groups share the key ID=1 but differ in
            // PARAM, so a group_by on SRC.PARAM that is not an output key would
            // silently merge those groups in the Matched executor, which matches
            // on keys only.
            let donor = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "PARAM".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "V".into(),
                    kind: ColumnType::Int,
                },
            ])
            .unwrap();
            let document_for = |group: &str| {
                operation_document(
                    "aggregate",
                    Tree::map(vec![
                        ("expr", Tree::text("SUM(SRC.V)")),
                        ("group_by", Tree::List(vec![Tree::text(group)])),
                    ]),
                    "input.csv",
                )
            };
            // Non-compliant: PARAM is not an output key (spec keys are [ID]).
            let prepared = PreparedSpecification::prepare(&document_for("SRC.PARAM")).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&donor) else {
                panic!("qualified aggregate grouping by a non-output-key column must be refused");
            };
            let diagnostics = findings[0].diagnostics(prepared.source()).unwrap();
            let diagnostic = diagnostics
                .iter()
                .find(|d| d.definition().rule == Some("LOOKUP-19"))
                .unwrap_or_else(|| panic!("expected a LOOKUP-19 diagnostic, got {diagnostics:?}"));
            assert_eq!(diagnostic.code, ConditionCode::AggregateGroupNotOutputKey);
            assert_eq!(diagnostic.definition().phase, "validation");
            assert_eq!(
                diagnostic.definition().condition,
                "invalid_aggregate_context"
            );
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.VALUE.derivation.aggregate.group_by"]
            );
            // Positive control: grouping by the output key itself is compliant and
            // the same aggregate still binds (and so can execute).
            let prepared = PreparedSpecification::prepare(&document_for("SRC.ID")).unwrap();
            prepared.bind(&donor).unwrap();
        }
    }
    mod specification_model {
        use yamaa_core::schema::{Document, DocumentNode as N, SpecificationDocument};

        fn document() -> Document {
            let mut nodes = Vec::new();
            let mut fields = Vec::new();
            for (name, value) in [
                ("schema_version", N::Text("1.0".into())),
                ("domain", N::Text("X".into())),
                ("input", N::Mapping(vec![])),
                ("keys", N::Sequence(vec![])),
                ("columns", N::Sequence(vec![])),
            ] {
                let key = nodes.len();
                nodes.push(N::Text(name.into()));
                let value_id = nodes.len();
                nodes.push(value);
                fields.push((key, value_id));
            }
            let path = nodes.len();
            nodes.push(N::Text("path".into()));
            let value = nodes.len();
            nodes.push(N::Text("output.csv".into()));
            let columns = nodes.len();
            nodes.push(N::Text("columns".into()));
            let empty = nodes.len();
            nodes.push(N::Sequence(vec![]));
            let output = nodes.len();
            nodes.push(N::Mapping(vec![(path, value), (columns, empty)]));
            let key = nodes.len();
            nodes.push(N::Text("output".into()));
            fields.push((key, output));
            let root = nodes.len();
            nodes.push(N::Mapping(fields));
            Document::new(nodes, root).unwrap()
        }

        #[test]
        fn admitted_model_owns_unchanged_occurrences_without_claiming_execution() {
            let document = document();
            let expected = document.clone();
            let model = SpecificationDocument::admit(document).unwrap().unwrap();
            assert_eq!(model.document(), &expected);
            assert_eq!(
                model.document().field(model.document().root(), "base"),
                None
            );
        }

        #[test]
        fn invalid_models_keep_all_authored_findings_without_a_partial_model() {
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let findings = SpecificationDocument::admit(empty).unwrap().unwrap_err();
            assert!(!findings.is_empty());
            assert!(findings.iter().all(|finding| !finding.path.is_empty()));
            assert!(SpecificationDocument::admit(document()).unwrap().is_ok());
        }

        #[test]
        fn independent_admissions_preserve_the_same_occurrences() {
            let input = document();
            let first = SpecificationDocument::admit(input.clone())
                .unwrap()
                .unwrap();
            let second = SpecificationDocument::admit(input.clone())
                .unwrap()
                .unwrap();
            assert_eq!(first.document(), &input);
            assert_eq!(second.document(), &input);
        }
    }
}
