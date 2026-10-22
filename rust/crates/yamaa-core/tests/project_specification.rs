//! Project contracts.
mod project {
    mod specification_project_rows {
        use crate::support::id_source_schema as source;
        use crate::support::project_call_compiler as support;
        use crate::support::resolved;
        use support::{call, function, specification_with_rows, Tree};
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            bound_expression::Read,
            dataset::{
                resolved::{Grain, Op, OperandRole},
                FunctionInput,
            },
            schema::DocumentNode as N,
            specification::{BindError, PrepareError, PreparedSpecification},
            table::{Column, TableSchema},
            value::{ColumnType, Value},
        };
        fn literal(value: i64) -> Tree {
            Tree::map(vec![(
                "literal",
                Tree::Scalar(N::Integer(value.to_string())),
            )])
        }
        fn text_literal(value: &str) -> Tree {
            Tree::map(vec![("literal", Tree::text(value))])
        }
        fn row(id: &str, mut derivations: Vec<(&str, Tree)>, groups: Option<Vec<&str>>) -> Tree {
            derivations.insert(
                0,
                (
                    "ID",
                    Tree::map(vec![(
                        "source",
                        Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                    )]),
                ),
            );
            let mut fields = vec![
                ("id", Tree::text(id)),
                (
                    "derivations",
                    Tree::map(
                        derivations
                            .into_iter()
                            .map(|(name, value)| (name, Tree::map(vec![("value", value)])))
                            .collect(),
                    ),
                ),
            ];
            if let Some(groups) = groups {
                fields.push((
                    "group_by",
                    Tree::List(groups.into_iter().map(Tree::text).collect()),
                ));
            }
            Tree::map(fields)
        }
        #[test]
        fn grouped_filters_admit_completed_candidate_windows() {
            let mut template = row(
                "groups",
                vec![(
                    "SEQ",
                    Tree::map(vec![(
                        "row_number",
                        Tree::map(vec![(
                            "window",
                            Tree::map(vec![("order_by", Tree::List(vec![Tree::text("ID")]))]),
                        )]),
                    )]),
                )],
                Some(vec!["SRC.ID"]),
            );
            let Tree::Map(fields) = &mut template else {
                panic!("row mapping");
            };
            fields.push(("filter".into(), Tree::text("SEQ > 1")));
            let spec = specification_with_rows(vec![("SEQ", "int", literal(0))], vec![template]);
            let prepared = PreparedSpecification::prepare(&spec).unwrap();
            // Grouped filters gate completed groups under ROW-1.
            assert!(prepared.bind(&source()).is_ok());
        }

        #[test]
        fn row_reads_promote_transitive_defaults_and_schedule_each_override_graph() {
            let spec = specification_with_rows(
                vec![
                    ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                    ("B", "int", call("id", vec![("x", Tree::text("C"))])),
                    ("C", "int", literal(7)),
                    ("D", "int", literal(0)),
                ],
                vec![
                    row(
                        "first",
                        vec![("D", call("id", vec![("x", Tree::text("A"))]))],
                        None,
                    ),
                    row(
                        "second",
                        vec![
                            ("B", literal(9)),
                            ("D", call("id", vec![("x", Tree::text("A"))])),
                        ],
                        None,
                    ),
                ],
            );
            let prepared = PreparedSpecification::prepare_with_project(
                &spec,
                &[function("unused"), function("id")],
            )
            .unwrap();
            assert_eq!(prepared.called_functions(), [1]);
            let plan = prepared.bind(&source()).unwrap();
            assert!(resolved::columns(&plan).is_empty());
            assert_eq!(
                resolved::completed(&resolved::row(&plan, 0)),
                [0, 3, 2, 1, 4]
            );
            assert_eq!(
                resolved::completed(&resolved::row(&plan, 1)),
                [0, 2, 1, 3, 4]
            );
            let Op::ProjectFunction(bound) = resolved::op(&plan, resolved::row(&plan, 0)[2].expr)
            else {
                panic!("inherited B function")
            };
            assert_eq!(
                bound.arguments()[0].input,
                FunctionInput::Read(Read::Column(3))
            );
        }

        #[test]
        fn fully_overridden_function_defaults_do_not_activate_but_remain_statically_checked() {
            let spec = specification_with_rows(
                vec![(
                    "A",
                    "int",
                    call("id", vec![("x", Tree::Scalar(N::Integer("7".into())))]),
                )],
                vec![
                    row("first", vec![("A", literal(8))], None),
                    row("second", vec![("A", literal(9))], None),
                ],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            assert!(prepared.called_functions().is_empty());
            assert!(prepared.project_calls().unwrap().plans().is_empty());
            assert_eq!(prepared.project_calls().unwrap().calls()[0].slot(), None);
            assert!(prepared.bind(&source()).is_ok());
            let invalid = specification_with_rows(
                vec![(
                    "A",
                    "int",
                    call("id", vec![("x", Tree::Scalar(N::Float(7.0)))]),
                )],
                vec![
                    row("first", vec![("A", literal(8))], None),
                    row("second", vec![("A", literal(9))], None),
                ],
            );
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&invalid, &[function("id")])
            else {
                panic!("unused default still checked")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].diagnostic().spec_paths,
                ["columns.A.derivation.function.args.x"]
            );
        }

        #[test]
        fn row_function_cycles_report_authored_paths_without_column_forward_rules() {
            let spec = specification_with_rows(
                vec![
                    ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                    ("B", "int", call("id", vec![("x", Tree::text("A"))])),
                    ("D", "int", literal(0)),
                ],
                vec![row(
                    "record",
                    vec![("D", call("id", vec![("x", Tree::text("A"))]))],
                    None,
                )],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("cycle")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].definition().condition, "dependency_cycle");
            assert_eq!(
                diagnostics[0].spec_paths,
                [
                    "columns.A.derivation.function",
                    "columns.B.derivation.function"
                ]
            );
            assert_eq!(
                diagnostics[0].context["cycle"],
                JsonValue::Array(
                    vec!["A", "B", "A"]
                        .into_iter()
                        .map(|s| JsonValue::String(s.into()))
                        .collect()
                )
            );
        }

        #[test]
        fn transitive_nonlocal_defaults_report_one_issue_per_column_with_all_overriding_rows() {
            use yamaa_core::json::Value as JsonValue;
            let spec = specification_with_rows(
                vec![
                    (
                        "A",
                        "int",
                        Tree::map(vec![(
                            "aggregate",
                            Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
                        )]),
                    ),
                    ("B", "int", call("id", vec![("x", Tree::text("A"))])),
                    ("C", "int", call("id", vec![("x", Tree::text("B"))])),
                ],
                vec![
                    row("first", vec![("C", literal(1)), ("B", literal(2))], None),
                    row("second", vec![("B", literal(3)), ("C", literal(4))], None),
                ],
            );
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("duplicate defaults")
            };
            assert_eq!(findings.len(), 2);
            for (finding, column) in findings.iter().zip(["B", "C"]) {
                let diagnostic = finding.diagnostic();
                assert_eq!(
                    diagnostic.spec_paths,
                    [format!("columns.{column}.derivation")]
                );
                assert_eq!(diagnostic.definition().condition, "duplicate_derivation");
                assert_eq!(diagnostic.definition().rule, Some("SPEC-6"));
                assert_eq!(
                    diagnostic.context["rows"],
                    JsonValue::Array(vec![
                        JsonValue::String("first".into()),
                        JsonValue::String("second".into()),
                    ])
                );
            }
        }

        #[test]
        fn inherited_cycles_are_reported_once_while_distinct_row_override_cycles_keep_their_paths()
        {
            for local_override in [false, true] {
                let mut rows = vec![
                    row(
                        "first",
                        vec![("D", call("id", vec![("x", Tree::text("A"))]))],
                        None,
                    ),
                    row(
                        "second",
                        vec![("D", call("id", vec![("x", Tree::text("A"))]))],
                        None,
                    ),
                ];
                if local_override {
                    rows.push(row(
                        "local",
                        vec![
                            ("A", call("id", vec![("x", Tree::text("B"))])),
                            ("D", call("id", vec![("x", Tree::text("A"))])),
                        ],
                        None,
                    ));
                }
                let spec = specification_with_rows(
                    vec![
                        ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                        ("B", "int", call("id", vec![("x", Tree::text("A"))])),
                        ("D", "int", literal(0)),
                    ],
                    rows,
                );
                let prepared =
                    PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
                let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                    panic!("cycles")
                };
                let diagnostics = findings
                    .iter()
                    .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(diagnostics.len(), if local_override { 2 } else { 1 });
                assert_eq!(diagnostics[0].definition().condition, "dependency_cycle");
                assert_eq!(
                    diagnostics[0].spec_paths,
                    [
                        "columns.A.derivation.function",
                        "columns.B.derivation.function"
                    ]
                );
                if local_override {
                    assert_eq!(diagnostics[1].definition().condition, "dependency_cycle");
                    assert_eq!(
                        diagnostics[1].spec_paths,
                        [
                            "rows[2].derivations.A.function",
                            "columns.B.derivation.function"
                        ]
                    );
                }
            }
        }

        #[test]
        fn grouped_nonlocal_defaults_keep_complete_long_conflict_context() {
            let names = (0..21).map(|n| format!("C{n}")).collect::<Vec<_>>();
            let mut columns = vec![(
                "A",
                "int",
                Tree::map(vec![(
                    "aggregate",
                    Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
                )]),
            )];
            columns.extend(names.iter().map(|name| {
                (
                    name.as_str(),
                    "int",
                    call("id", vec![("x", Tree::text("A"))]),
                )
            }));
            let overrides = names
                .iter()
                .map(|name| (name.as_str(), literal(1)))
                .collect();
            let id = "r".repeat(200_000);
            let spec = specification_with_rows(columns, vec![row(&id, overrides, None)]);
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("complete semantic default conflicts");
            };
            assert_eq!(findings.len(), names.len());
            for (finding, name) in findings.iter().zip(&names) {
                let yamaa_core::specification::PreflightFinding::DuplicateRowDefault {
                    column,
                    rows,
                    not_row_local,
                } = finding
                else {
                    panic!("independently authored default conflict");
                };
                assert_eq!(column, name);
                assert_eq!(not_row_local, "aggregate");
                assert!(rows.len() == 1 && rows[0] == id);
            }
        }

        #[test]
        fn grouped_function_arguments_keep_record_reads_and_complete_scope_and_type_findings() {
            let spec = specification_with_rows(
                vec![("A", "int", literal(0))],
                vec![row(
                    "group",
                    vec![("A", call("id", vec![("x", Tree::text("SRC.ID"))]))],
                    Some(vec!["SRC.ID"]),
                )],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let Op::ProjectFunction(bound) = resolved::op(&plan, resolved::row(&plan, 0)[1].expr)
            else {
                panic!("group function")
            };
            assert_eq!(
                bound.arguments()[0].input,
                FunctionInput::Read(Read::Source(0))
            );
            let bad = specification_with_rows(
                vec![("A", "int", literal(0))],
                vec![row(
                    "group",
                    vec![("A", call("id", vec![("x", Tree::text("SRC.X"))]))],
                    Some(vec!["SRC.ID"]),
                )],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&bad, &[function("id")]).unwrap();
            let source = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "X".into(),
                    kind: ColumnType::Float,
                },
            ])
            .unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
                panic!("group and type findings")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 2);
            assert_eq!(
                diagnostics[0].definition().condition,
                "ungrouped_driver_field"
            );
            assert_eq!(
                diagnostics[0].context["row"],
                JsonValue::String("group".into())
            );
            assert_eq!(
                diagnostics[1].definition().condition,
                "invalid_function_argument"
            );
            assert!(diagnostics
                .iter()
                .all(|d| d.spec_paths == ["rows[0].derivations.A.function.args.x"]));
        }

        #[test]
        fn unpromoted_column_calls_keep_record_reads_and_column_declaration_order() {
            let spec = specification_with_rows(
                vec![
                    ("A", "int", literal(7)),
                    ("B", "int", call("id", vec![("x", Tree::text("SRC.ID"))])),
                ],
                vec![row("record", vec![], None)],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            assert_eq!(resolved::row(&plan, 0).len(), 1);
            let columns = resolved::columns(&plan);
            assert_eq!(resolved::completed(&columns), [1, 2]);
            let Op::ProjectFunction(bound) = resolved::op(&plan, columns[1].expr) else {
                panic!("column call")
            };
            assert_eq!(
                bound.arguments()[0].input,
                FunctionInput::Read(Read::Source(0))
            );
            let forward = specification_with_rows(
                vec![
                    ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                    ("B", "int", literal(7)),
                ],
                vec![row("record", vec![], None)],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&forward, &[function("id")]).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("column forward reference")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].definition().condition, "forward_reference");
            assert_eq!(diagnostics[0].spec_paths, ["columns.A.derivation.function"]);
        }

        #[test]
        fn row_reads_of_nonlocal_columns_return_complete_phase_findings_before_lowering() {
            let spec = specification_with_rows(
                vec![
                    (
                        "A",
                        "int",
                        Tree::map(vec![(
                            "aggregate",
                            Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
                        )]),
                    ),
                    ("B", "int", literal(0)),
                    ("C", "int", literal(0)),
                ],
                vec![row(
                    "record",
                    vec![
                        ("B", call("id", vec![("x", Tree::text("A"))])),
                        ("C", call("id", vec![("x", Tree::text("A"))])),
                    ],
                    None,
                )],
            );
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("row phase findings")
            };
            assert_eq!(findings.len(), 2);
            for (finding, column) in findings.iter().zip(["B", "C"]) {
                let diagnostic = finding.diagnostic();
                assert_eq!(diagnostic.definition().condition, "phase_boundary");
                assert_eq!(diagnostic.definition().rule, Some("COLUMN-4"));
                assert_eq!(
                    diagnostic.spec_paths,
                    [format!("rows[0].derivations.{column}.function.args.x")]
                );
            }
        }

        #[test]
        fn grouped_filters_promote_function_defaults_and_qualified_fields_do_not_promote_outputs() {
            let mut grouped = row("group", vec![], Some(vec!["SRC.ID"]));
            let Tree::Map(fields) = &mut grouped else {
                unreachable!()
            };
            fields.push(("filter".into(), Tree::text("B >= 7")));
            let spec = specification_with_rows(
                vec![
                    ("A", "int", literal(7)),
                    ("B", "int", call("id", vec![("x", Tree::text("A"))])),
                ],
                vec![grouped],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            assert!(resolved::columns(&plan).is_empty());
            assert_eq!(resolved::completed(&resolved::row(&plan, 0)), [0, 1, 2]);
            assert!(resolved::templates(&plan)[0].filter.is_some());
            let spec = specification_with_rows(
                vec![("A", "int", literal(7)), ("B", "int", literal(0))],
                vec![row(
                    "record",
                    vec![("B", call("id", vec![("x", Tree::text("SRC.A"))]))],
                    None,
                )],
            );
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let source = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "A".into(),
                    kind: ColumnType::Int,
                },
            ])
            .unwrap();
            let plan = prepared.bind(&source).unwrap();
            assert_eq!(resolved::completed(&resolved::row(&plan, 0)), [0, 2]);
            assert_eq!(resolved::columns(&plan)[0].column.index(), 1);
        }

        #[test]
        fn every_function_argument_keeps_the_complete_long_row_context() {
            use crate::support::roots::{Case, Parameter};
            use yamaa_core::project_function::{Function, Language};
            use yamaa_core::value::ValueType;
            let parameters = (0..8).map(|n| format!("p{n}")).collect::<Vec<_>>();
            let mut definition = function("id").definition().clone();
            definition.params = parameters
                .iter()
                .map(|name| {
                    Parameter {
                        name: name.clone(),
                        kind: ValueType::Int,
                        required: false,
                        default: Some(Value::Int(1)),
                        accepts_missing: false,
                    }
                    .root()
                })
                .collect();
            definition.tests = vec![
                Case {
                    id: "ordinary".into(),
                    covers: std::iter::once("normal".into())
                        .chain(parameters.iter().map(|name| format!("default:{name}")))
                        .collect(),
                    args: vec![],
                    result: Value::Int(1),
                }
                .root(),
                Case {
                    id: "boundary".into(),
                    covers: vec!["boundary".into()],
                    args: vec![("p0".into(), Value::Int(i64::MIN))],
                    result: Value::Int(1),
                }
                .root(),
            ];
            definition.tests.extend(parameters.iter().map(|name| {
                Case {
                    id: format!("missing-{name}"),
                    covers: vec![format!("short-circuit-missing:{name}")],
                    args: vec![(name.clone(), Value::Missing)],
                    result: Value::Missing,
                }
                .root()
            }));
            let function = Function::admit(Language::Python, definition).unwrap();
            let names = (0..63).map(|n| format!("C{n}")).collect::<Vec<_>>();
            let columns = names
                .iter()
                .map(|name| (name.as_str(), "int", literal(0)))
                .collect();
            let derivations = names
                .iter()
                .map(|name| {
                    (
                        name.as_str(),
                        call(
                            "id",
                            parameters
                                .iter()
                                .map(|parameter| (parameter.as_str(), Tree::text("SRC.X")))
                                .collect(),
                        ),
                    )
                })
                .collect();
            let id = "r".repeat(200_000);
            let spec =
                specification_with_rows(columns, vec![row(&id, derivations, Some(vec!["SRC.ID"]))]);
            let prepared = PreparedSpecification::prepare_with_project(&spec, &[function]).unwrap();
            assert_eq!(prepared.project_calls().unwrap().calls().len(), 63);
        }

        #[test]
        fn column_windows_stay_global_and_scalar_row_defaults_remain_local() {
            for operation in ["row_number", "rank", "locf", "first_available"] {
                for overridden in [false, true] {
                    let payload = if operation == "first_available" {
                        Tree::map(vec![("sources", Tree::List(vec![Tree::text("ID")]))])
                    } else {
                        let mut fields = vec![(
                            "window",
                            Tree::map(vec![("order_by", Tree::List(vec![Tree::text("ID")]))]),
                        )];
                        if operation == "locf" {
                            fields.push(("source", Tree::text("ID")));
                        }
                        Tree::map(fields)
                    };
                    let mut derivations = vec![(
                        "B",
                        Tree::map(vec![(
                            "source",
                            Tree::map(vec![("variable", Tree::text("A"))]),
                        )]),
                    )];
                    if overridden {
                        derivations.push(("A", literal(7)));
                    }
                    let spec = specification_with_rows(
                        vec![
                            ("A", "int", Tree::map(vec![(operation, payload)])),
                            ("B", "int", literal(0)),
                        ],
                        vec![row("record", derivations, None)],
                    );
                    let prepared = PreparedSpecification::prepare_with_project(&spec, &[]);
                    if operation != "first_available" {
                        let Err(PrepareError::Invalid(findings)) = prepared else {
                            panic!("column windows stay in the column phase");
                        };
                        let condition = if overridden {
                            "duplicate_derivation"
                        } else {
                            "phase_boundary"
                        };
                        assert!(
                            findings.iter().any(|finding| finding
                                .diagnostic()
                                .definition()
                                .condition
                                == condition),
                            "{findings:?}"
                        );
                        if overridden {
                            // SPEC-6 names why the column-level derivation cannot
                            // be a row default: it is a window derivation.
                            let duplicate = findings
                                .iter()
                                .find(|finding| {
                                    finding.diagnostic().definition().condition
                                        == "duplicate_derivation"
                                })
                                .expect("duplicate finding");
                            assert_eq!(
                                duplicate.diagnostic().context["not_row_local"],
                                JsonValue::String("window".into())
                            );
                        }
                        continue;
                    }
                    let prepared = prepared.unwrap();
                    let plan = prepared.bind(&source()).unwrap();
                    assert!(resolved::columns(&plan).is_empty());
                    assert_eq!(resolved::completed(&resolved::row(&plan, 0)), [0, 1, 2]);
                    let a = resolved::row(&plan, 0)[1];
                    if overridden {
                        assert_eq!(resolved::op(&plan, a.expr), &Op::Literal(Value::Int(7)));
                    } else {
                        assert_eq!(&*a.path, format!("columns.A.derivation.{operation}"));
                        assert!(matches!(
                            resolved::op(&plan, a.expr),
                            Op::Window { .. }
                                | Op::ContextWindow { .. }
                                | Op::FirstAvailable { .. }
                        ));
                    }
                }
            }
        }

        #[test]
        fn bare_row_reads_of_nonlocal_outputs_keep_complete_phase_diagnostics_without_function_context(
        ) {
            let bare = || {
                Tree::map(vec![(
                    "source",
                    Tree::map(vec![("variable", Tree::text("A"))]),
                )])
            };
            let spec = specification_with_rows(
                vec![
                    (
                        "A",
                        "int",
                        Tree::map(vec![(
                            "aggregate",
                            Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
                        )]),
                    ),
                    ("B", "int", literal(0)),
                    ("C", "int", literal(0)),
                ],
                vec![row("record", vec![("B", bare()), ("C", bare())], None)],
            );
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[])
            else {
                panic!("complete bare row phase findings before lowering")
            };
            assert_eq!(findings.len(), 2);
            for (finding, column) in findings.iter().zip(["B", "C"]) {
                let diagnostic = finding.diagnostic();
                assert_eq!(
                    (
                        diagnostic.definition().phase,
                        diagnostic.definition().condition,
                        diagnostic.definition().rule
                    ),
                    ("validation", "phase_boundary", Some("COLUMN-4"))
                );
                assert_eq!(
                    diagnostic.spec_paths,
                    [format!("rows[0].derivations.{column}.source")]
                );
                assert_eq!(
                    diagnostic.context,
                    [
                        (
                            "available_phase".into(),
                            JsonValue::String("column_derivation".into())
                        ),
                        ("identifier".into(), JsonValue::String("A".into())),
                        (
                            "required_phase".into(),
                            JsonValue::String("row_construction".into())
                        ),
                        ("row".into(), JsonValue::String("record".into()))
                    ]
                    .into_iter()
                    .collect()
                );
                assert!(diagnostic.source_span.is_none() && diagnostic.operand_route.is_none());
            }
        }

        #[test]
        fn nested_row_case_calls_share_activation_and_dependency_order() {
            use yamaa_core::project_function::{Function, Language};
            let mut definition = function("format").definition().clone();
            definition.returns = ColumnType::Str;
            for case in &mut definition.tests {
                if let Value::Int(value) = case.result {
                    case.result = Value::Str(value.to_string());
                }
            }
            let formatter = Function::admit(Language::Python, definition).unwrap();
            let case = |then: Tree, otherwise: Tree| {
                Tree::map(vec![(
                    "case",
                    Tree::List(vec![
                        Tree::map(vec![("when", Tree::text("ID > 0")), ("then", then)]),
                        Tree::map(vec![("otherwise", otherwise)]),
                    ]),
                )])
            };
            let identity = case(call("id", vec![("x", Tree::text("C"))]), literal(0));
            let formatted = case(
                call("format", vec![("x", Tree::text("A"))]),
                text_literal("prefix:"),
            );
            let spec = specification_with_rows(
                vec![
                    ("A", "int", literal(0)),
                    ("C", "int", literal(7)),
                    ("D", "str", text_literal("unused")),
                ],
                vec![row("one", vec![("D", formatted), ("A", identity)], None)],
            );
            let prepared = PreparedSpecification::prepare_with_project(
                &spec,
                &[function("id"), formatter, function("unused")],
            )
            .unwrap();
            assert_eq!(prepared.called_functions(), [0, 1]);
            let plan = prepared.bind(&source()).unwrap();
            assert!(resolved::columns(&plan).is_empty());
            assert_eq!(resolved::completed(&resolved::row(&plan, 0)), [0, 2, 1, 3]);
            let case = resolved::row(&plan, 0)[3].expr;
            let Op::Case = resolved::op(&plan, case) else {
                panic!("case")
            };
            let branch = resolved::operands(&plan, case, OperandRole::Branch)[0];
            let Op::ProjectFunction(function) = resolved::op(&plan, branch) else {
                panic!("nested function")
            };
            assert_eq!(
                function.arguments()[0].input,
                FunctionInput::Read(Read::Column(1))
            );
        }

        fn bound_assignment(
            plan: &yamaa_core::dataset::ResolvedPlan,
            template: usize,
            column: usize,
        ) -> &yamaa_core::dataset::resolved::Binding {
            resolved::row(plan, template)
                .into_iter()
                .find(|binding| binding.column.index() == column)
                .unwrap()
        }

        #[test]
        fn shared_defaults_bind_per_template_with_own_paths_and_repeated_binds() {
            // One large authored default is inherited by two of three
            // templates; the remaining template overrides it, and NOTE is
            // overridden by a different template than BIG. Every template
            // filters on the two bare defaults, so each needs all three
            // columns row-phase and the inherited defaults stay shared.
            let large = "shared-authored-default ".repeat(256);
            let mut templates = vec![
                row("records", vec![("NOTE", text_literal("note-a"))], None),
                row(
                    "override",
                    vec![("BIG", text_literal("big-override"))],
                    None,
                ),
                row("grouped", vec![], Some(vec!["SRC.ID"])),
            ];
            for template in &mut templates {
                let Tree::Map(fields) = template else {
                    panic!("row mapping");
                };
                fields.push((
                    "filter".into(),
                    Tree::text("BIG IS NOT NULL AND NOTE IS NOT NULL"),
                ));
            }
            let spec = specification_with_rows(
                vec![
                    ("BIG", "str", text_literal(&large)),
                    ("NOTE", "str", text_literal("note-default")),
                ],
                templates,
            );
            let prepared = PreparedSpecification::prepare(&spec).unwrap();
            // Binding is per template and per bind call: repeated binds of
            // the shared preparation must reproduce the same independently
            // expected assignments, paths and grouping modes.
            for _ in 0..2 {
                let plan = prepared.bind(&source()).unwrap();
                assert_eq!(resolved::templates(&plan).len(), 3);
                let literal = |binding: &yamaa_core::dataset::resolved::Binding| {
                    resolved::op(&plan, binding.expr).clone()
                };
                let big = |template: usize| bound_assignment(&plan, template, 1);
                assert_eq!(literal(big(0)), Op::Literal(Value::Str(large.clone())));
                assert_eq!(&*big(0).path, "columns.BIG.derivation.literal");
                assert_eq!(
                    literal(big(1)),
                    Op::Literal(Value::Str("big-override".into()))
                );
                assert_eq!(&*big(1).path, "rows[1].derivations.BIG.literal");
                assert_eq!(literal(big(2)), Op::Literal(Value::Str(large.clone())));
                assert_eq!(&*big(2).path, "columns.BIG.derivation.literal");
                assert!(matches!(
                    resolved::grain(resolved::templates(&plan)[2]),
                    Grain::Groups(_)
                ));
                let note = |template: usize| bound_assignment(&plan, template, 2);
                assert_eq!(literal(note(0)), Op::Literal(Value::Str("note-a".into())));
                assert_eq!(&*note(0).path, "rows[0].derivations.NOTE.literal");
                assert_eq!(
                    literal(note(1)),
                    Op::Literal(Value::Str("note-default".into()))
                );
                assert_eq!(&*note(1).path, "columns.NOTE.derivation.literal");
                assert_eq!(
                    literal(note(2)),
                    Op::Literal(Value::Str("note-default".into()))
                );
                assert_eq!(&*note(2).path, "columns.NOTE.derivation.literal");
            }
        }

        #[test]
        fn shared_defaults_execute_across_drivers_grouping_and_repeated_runs() {
            // Three admitted drivers: the second template reads ALT, the
            // third groups GRP; both inherit the same authored defaults.
            let large = "shared-authored-default ".repeat(256);
            let source_value = |variable: &str| {
                Tree::map(vec![(
                    "source",
                    Tree::map(vec![("variable", Tree::text(variable))]),
                )])
            };
            let derivation = |value: Tree| Tree::map(vec![("value", value)]);
            let column = |name: &str, kind: &str, value: Tree| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text(kind)),
                    ("derivation", derivation(value)),
                ])
            };
            let row = |id: &str,
                       dataset: Option<&str>,
                       groups: Option<Vec<&str>>,
                       derivations: Vec<(&str, Tree)>| {
                let mut fields = vec![
                    ("id", Tree::text(id)),
                    (
                        "derivations",
                        Tree::map(
                            derivations
                                .into_iter()
                                .map(|(name, value)| (name, derivation(value)))
                                .collect(),
                        ),
                    ),
                ];
                if let Some(dataset) = dataset {
                    fields.push(("dataset", Tree::text(dataset)));
                }
                if let Some(groups) = groups {
                    fields.push((
                        "group_by",
                        Tree::List(groups.into_iter().map(Tree::text).collect()),
                    ));
                }
                Tree::map(fields)
            };
            let root = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(vec![
                        ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                        ("ALT", Tree::map(vec![("path", Tree::text("alt.csv"))])),
                        ("GRP", Tree::map(vec![("path", Tree::text("grp.csv"))])),
                    ]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![
                        column("ID", "int", source_value("SRC.ID")),
                        column("BIG", "str", text_literal(&large)),
                        column("NOTE", "str", text_literal("note-default")),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("output.csv")),
                        (
                            "columns",
                            Tree::List(vec![
                                Tree::text("ID"),
                                Tree::text("BIG"),
                                Tree::text("NOTE"),
                            ]),
                        ),
                    ]),
                ),
                (
                    "rows",
                    Tree::List(vec![
                        row(
                            "src-records",
                            Some("SRC"),
                            None,
                            vec![
                                ("ID", source_value("SRC.ID")),
                                ("NOTE", text_literal("note-a")),
                            ],
                        ),
                        row(
                            "alt-records",
                            Some("ALT"),
                            None,
                            vec![
                                ("ID", source_value("ALT.ID")),
                                ("BIG", text_literal("big-override")),
                            ],
                        ),
                        row(
                            "grp-grouped",
                            Some("GRP"),
                            Some(vec!["GRP.ID"]),
                            vec![("ID", source_value("GRP.ID"))],
                        ),
                    ]),
                ),
            ]);
            let document = root.document();
            let spec = yamaa_core::schema::SpecificationDocument::admit(document)
                .unwrap()
                .unwrap();
            let prepared = PreparedSpecification::prepare(&spec).unwrap();
            let schema = crate::support::table_schema(&[("ID", ColumnType::Int)]);
            let plan = prepared.bind_sources(&[&schema, &schema, &schema]).unwrap();
            let src_table = crate::support::dataset::table(
                &[("ID", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            let alt_table = crate::support::dataset::table(
                &[("ID", ColumnType::Int)],
                vec![vec![Value::Int(10)]],
            );
            let grp_table = crate::support::dataset::table(
                &[("ID", ColumnType::Int)],
                vec![vec![Value::Int(7)], vec![Value::Int(8)]],
            );
            let expected = vec![
                vec![
                    Value::Int(1),
                    Value::Str(large.clone()),
                    Value::Str("note-a".into()),
                ],
                vec![
                    Value::Int(2),
                    Value::Str(large.clone()),
                    Value::Str("note-a".into()),
                ],
                vec![
                    Value::Int(10),
                    Value::Str("big-override".into()),
                    Value::Str("note-default".into()),
                ],
                vec![
                    Value::Int(7),
                    Value::Str(large.clone()),
                    Value::Str("note-default".into()),
                ],
                vec![
                    Value::Int(8),
                    Value::Str(large.clone()),
                    Value::Str("note-default".into()),
                ],
            ];
            // Repeated runs over one bound plan stay identical: sharing the
            // prepared payload introduces no per-run cross-template state.
            for _ in 0..2 {
                let attempt = plan.execute(
                    &src_table,
                    &[&alt_table, &grp_table],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(attempt.result.unwrap().dataset.rows(), expected.as_slice());
            }
        }
    }
    mod specification_str_pad {
        //! Independent captured reference ordering for positive-width admission failures.
        use crate::support::project_call_compiler as support;
        use support::{specification, specification_with_rows, Tree};
        use yamaa_core::{
            diagnostic::ConditionCode,
            schema::DocumentNode as N,
            specification::{BindError, PreparedSpecification},
            table::{Column, TableSchema},
            value::ColumnType,
        };

        fn pad(source: &str, width: &str) -> Tree {
            Tree::map(vec![(
                "str_pad",
                Tree::map(vec![
                    ("source", Tree::text(source)),
                    ("width", Tree::Scalar(N::Integer(width.into()))),
                ]),
            )])
        }
        fn cases(when: &str, first: Tree, otherwise: Tree) -> Tree {
            Tree::map(vec![(
                "case",
                Tree::List(vec![
                    Tree::map(vec![("when", Tree::text(when)), ("then", first)]),
                    Tree::map(vec![("otherwise", otherwise)]),
                ]),
            )])
        }
        fn findings(
            spec: &yamaa_core::schema::SpecificationDocument,
        ) -> Vec<(ConditionCode, String)> {
            let prepared = PreparedSpecification::prepare(spec).unwrap();
            let source = TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            }])
            .unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
                panic!("retained width and reference findings")
            };
            findings
                .iter()
                .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
                .map(|diagnostic| (diagnostic.code, diagnostic.spec_paths[0].clone()))
                .collect()
        }
        fn widths_and_sources(paths: &[&str]) -> Vec<String> {
            paths.iter().map(|path| (*path).into()).collect()
        }

        #[test]
        fn all_nested_case_widths_precede_that_derivations_reference_findings() {
            let spec = specification(vec![(
                "A",
                "str",
                cases(
                    "ID = 1",
                    cases("ID = 1", pad("SRC.ABSENT1", "0"), pad("SRC.ABSENT2", "-1")),
                    pad("SRC.ABSENT3", "0"),
                ),
            )]);
            let actual = findings(&spec);
            assert_eq!(
                actual
                    .iter()
                    .map(|(_, path)| path.clone())
                    .collect::<Vec<_>>(),
                widths_and_sources(&[
                    "columns.A.derivation.case[0].then.case[0].then.str_pad.width",
                    "columns.A.derivation.case[0].then.case[1].otherwise.str_pad.width",
                    "columns.A.derivation.case[1].otherwise.str_pad.width",
                    "columns.A.derivation.case[0].then.case[0].then.str_pad.source",
                    "columns.A.derivation.case[0].then.case[1].otherwise.str_pad.source",
                    "columns.A.derivation.case[1].otherwise.str_pad.source",
                ])
            );
            assert!(actual[..3]
                .iter()
                .all(|(code, _)| *code == ConditionCode::StrPadWidth));
        }

        #[test]
        fn width_checkpoint_precedes_case_predicate_reference_binding_without_duplication() {
            let spec = specification(vec![(
                "A",
                "str",
                cases("ABSENT = 1", pad("SRC.ABSENT1", "0"), pad("SRC.ID", "2")),
            )]);
            let actual = findings(&spec);
            assert_eq!(actual[0].0, ConditionCode::StrPadWidth);
            assert_eq!(actual[1].0, ConditionCode::PredicateUnknownField);
            assert_eq!(
                actual.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
                widths_and_sources(&[
                    "columns.A.derivation.case[0].then.str_pad.width",
                    "columns.A.derivation.case[0].when",
                    "columns.A.derivation.case[0].then.str_pad.source",
                ])
            );
            // Predicate ownership is retained without moving the earlier width failure.
        }

        #[test]
        fn separate_column_derivations_keep_width_then_reference_interleaving() {
            let spec = specification(vec![
                ("A", "str", pad("SRC.ABSENT1", "0")),
                ("B", "str", pad("SRC.ABSENT2", "-1")),
            ]);
            assert_eq!(
                findings(&spec)
                    .into_iter()
                    .map(|(_, path)| path)
                    .collect::<Vec<_>>(),
                widths_and_sources(&[
                    "columns.A.derivation.str_pad.width",
                    "columns.A.derivation.str_pad.source",
                    "columns.B.derivation.str_pad.width",
                    "columns.B.derivation.str_pad.source",
                ])
            );
        }

        #[test]
        fn row_substitutions_report_all_widths_before_reference_binding() {
            let spec = specification_with_rows(
                vec![
                    (
                        "A",
                        "str",
                        Tree::map(vec![("literal", Tree::text("unused"))]),
                    ),
                    (
                        "B",
                        "str",
                        Tree::map(vec![("literal", Tree::text("unused"))]),
                    ),
                ],
                vec![Tree::map(vec![
                    ("id", Tree::text("records")),
                    (
                        "derivations",
                        Tree::map(vec![
                            ("A", Tree::map(vec![("value", pad("ABSENT1", "0"))])),
                            ("B", Tree::map(vec![("value", pad("ABSENT2", "-1"))])),
                        ]),
                    ),
                ])],
            );
            assert_eq!(
                findings(&spec)
                    .into_iter()
                    .map(|(_, path)| path)
                    .collect::<Vec<_>>(),
                widths_and_sources(&[
                    "rows[0].derivations.A.str_pad.width",
                    "rows[0].derivations.B.str_pad.width",
                    "rows[0].derivations.A.str_pad.source",
                    "rows[0].derivations.B.str_pad.source",
                ])
            );
        }

        #[test]
        fn predicate_grammar_stays_before_its_branch_width_validation() {
            let spec = specification(vec![(
                "A",
                "str",
                cases("ID >", pad("SRC.ABSENT", "0"), pad("SRC.ID", "2")),
            )]);
            let actual = findings(&spec);
            assert_eq!(actual[0].0.definition().condition, "invalid_expression");
            assert_eq!(actual[1].0, ConditionCode::StrPadWidth);
            assert_eq!(
                actual.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
                widths_and_sources(&[
                    "columns.A.derivation.case[0].when",
                    "columns.A.derivation.case[0].then.str_pad.width",
                    "columns.A.derivation.case[0].then.str_pad.source",
                ])
            );
        }

        #[test]
        fn numeric_grammar_and_widths_keep_authored_order_before_reference_binding() {
            let invalid_numeric = || {
                Tree::map(vec![(
                    "compute",
                    Tree::map(vec![("expr", Tree::text("ID +"))]),
                )])
            };
            for (first, otherwise, expected) in [
                (
                    invalid_numeric(),
                    pad("SRC.ABSENT", "0"),
                    widths_and_sources(&[
                        "columns.A.derivation.case[0].then.compute.expr",
                        "columns.A.derivation.case[1].otherwise.str_pad.width",
                        "columns.A.derivation.case[0].when",
                        "columns.A.derivation.case[1].otherwise.str_pad.source",
                    ]),
                ),
                (
                    pad("SRC.ABSENT", "0"),
                    invalid_numeric(),
                    widths_and_sources(&[
                        "columns.A.derivation.case[0].then.str_pad.width",
                        "columns.A.derivation.case[1].otherwise.compute.expr",
                        "columns.A.derivation.case[0].when",
                        "columns.A.derivation.case[0].then.str_pad.source",
                    ]),
                ),
            ] {
                let spec = specification(vec![("A", "str", cases("ABSENT = 1", first, otherwise))]);
                let actual = findings(&spec);
                assert!(actual[..2]
                    .iter()
                    .any(|(code, _)| *code == ConditionCode::StrPadWidth));
                assert!(actual[..2]
                    .iter()
                    .any(|(code, _)| code.definition().condition == "invalid_expression"));
                assert_eq!(
                    actual.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
}
