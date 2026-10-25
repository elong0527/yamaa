//! Project contracts: the seams of a specification's row templates over its
//! column defaults that the diagnostic replay cannot reach, each named in its
//! test's first comment: the project calls a preparation activates, the
//! resolved plan, rows executed, and identifiers 200,000 characters long. What
//! these specifications refuse is `tests/fixtures/diagnostics/rows.json`'s,
//! and the order of str_pad's checkpoints is `text.json`'s (#3798).
mod project {
    mod specification_project_rows {
        use crate::support::id_source_schema as source;
        use crate::support::project_call_compiler as support;
        use crate::support::resolved;
        use crate::support::specification::{prepare, prepare_in};
        use support::{
            call, environment, file, project, specification, specification_with_rows, Tree,
        };
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            bound_expression::Read,
            dataset::{
                resolved::{Grain, Op, OperandRole},
                FunctionInput,
            },
            schema::DocumentNode as N,
            specification::{BindError, PrepareError},
            table::{Column, TableSchema},
            value::{ColumnType, Value},
        };
        /// A number is a literal as YAML writes it (D18).
        fn literal(value: i64) -> Tree {
            Tree::Scalar(N::Integer(value.to_string()))
        }
        /// Text is a literal quoted in the expression (D18).
        fn text_literal(value: &str) -> Tree {
            Tree::text(format!("'{}'", value.replace('\'', "''")))
        }
        fn row(id: &str, mut derivations: Vec<(&str, Tree)>, groups: Option<Vec<&str>>) -> Tree {
            derivations.insert(0, ("ID", Tree::text("SRC.ID")));
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
        /// A seam the replay cannot reach: the functions a preparation calls,
        /// each template's completion order and a bound call's argument read.
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
            let prepared = prepare_in(&spec, &environment(&["unused", "id"])).unwrap();
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

        /// A seam the replay cannot reach: the functions a preparation calls
        /// and the plans and slots of its project calls. That the replaced
        /// default is still checked is `rows.json`'s.
        #[test]
        fn fully_overridden_function_defaults_do_not_activate() {
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
            let prepared = prepare_in(&spec, &environment(&["id"])).unwrap();
            assert!(prepared.called_functions().is_empty());
            assert!(prepared.project_calls().unwrap().plans().is_empty());
            assert_eq!(prepared.project_calls().unwrap().calls()[0].slot(), None);
            assert!(prepared.bind(&source()).is_ok());
        }

        /// A seam the replay cannot reach: the first calling derivation a
        /// preparation returns, a value and not an issue.
        ///
        /// A build no host can run is refused at the first derivation, in
        /// authored order, that calls a project function the build runs
        /// (#3030 D221). Authored order is the order a formatted file writes
        /// derivations in (yaml/schema_spec.yaml): the intermediates', then
        /// the columns', then the row templates', each in declaration order
        /// and named by its derivation however deep the call sits in it. A
        /// column's derivation that every row template replaces calls nothing
        /// the build runs.
        #[test]
        fn the_first_calling_derivation_is_the_first_whose_call_the_build_runs() {
            let seven = || Tree::Scalar(N::Integer("7".into()));
            // An intermediate whose derivation calls `id`, read by a column
            // after a column that calls `id` too.
            let intermediate = {
                let column = |name, value| {
                    Tree::map(vec![
                        ("name", Tree::text(name)),
                        ("type", Tree::text("int")),
                        ("derivation", Tree::map(vec![("value", value)])),
                    ])
                };
                Tree::map(vec![
                    ("yamaa", Tree::text("1.0")),
                    ("type", Tree::text("spec")),
                    ("domain", Tree::text("TEST")),
                    (
                        "input",
                        Tree::map(vec![
                            ("SRC", Tree::map(vec![("path", Tree::text("input.csv"))])),
                            ("DONOR", Tree::map(vec![("path", Tree::text("donor.csv"))])),
                        ]),
                    ),
                    ("keys", Tree::List(vec![Tree::text("ID")])),
                    ("base", Tree::text("SRC")),
                    (
                        "intermediates",
                        Tree::List(vec![Tree::map(vec![
                            ("id", Tree::text("P")),
                            ("dataset", Tree::text("DONOR")),
                            ("key", Tree::map(vec![("K", Tree::text("ID"))])),
                            (
                                "derivations",
                                Tree::map(vec![("B", call("id", vec![("x", seven())]))]),
                            ),
                        ])]),
                    ),
                    (
                        "columns",
                        Tree::List(vec![
                            column("ID", Tree::text("SRC.ID")),
                            column("C", call("id", vec![("x", Tree::text("ID"))])),
                            column("D", Tree::text("P.B")),
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
                                    Tree::text("C"),
                                    Tree::text("D"),
                                ]),
                            ),
                        ]),
                    ),
                ])
                .document()
            };
            let nested = Tree::map(vec![(
                "case",
                Tree::List(vec![
                    Tree::map(vec![
                        ("when", Tree::text("ID > 0")),
                        ("then", call("id", vec![("x", Tree::text("A"))])),
                    ]),
                    Tree::map(vec![("otherwise", literal(0))]),
                ]),
            )]);
            let overridden = || ("A", "int", call("id", vec![("x", seven())]));
            let cases = [
                (
                    intermediate,
                    // P-CLR-2 addresses an intermediate by its `id`.
                    Some("intermediates.P.derivations.B"),
                    "an intermediate's call before a column's",
                ),
                (
                    specification(vec![
                        ("A", "int", literal(0)),
                        ("B", "int", nested),
                        ("C", "int", call("id", vec![("x", Tree::text("B"))])),
                    ]),
                    Some("columns.B.derivation"),
                    "a call nested in the second of two calling columns",
                ),
                (
                    specification_with_rows(
                        vec![overridden(), ("B", "int", literal(0))],
                        vec![
                            row("first", vec![("A", literal(8))], None),
                            row(
                                "second",
                                vec![("A", literal(9)), ("B", call("id", vec![("x", seven())]))],
                                None,
                            ),
                        ],
                    ),
                    // P-CLR-2 addresses a row template by its `id`.
                    Some("rows.second.derivations.B"),
                    "a row template's call after a replaced column call",
                ),
                (
                    specification_with_rows(
                        vec![overridden()],
                        vec![
                            row("first", vec![("A", literal(8))], None),
                            row("second", vec![("A", literal(9))], None),
                        ],
                    ),
                    None,
                    "only a column call every row template replaces",
                ),
            ];
            for (spec, expected, label) in cases {
                let prepared = prepare_in(&spec, &environment(&["id"])).unwrap();
                assert_eq!(prepared.first_calling_derivation(), expected, "{label}");
            }
        }

        /// A seam the replay cannot reach: a row template id 200,000
        /// characters long, generated, and the default-conflict finding each
        /// of 21 columns reports as its preflight record.
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
            let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &environment(&["id"]))
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

        /// A seam the replay cannot reach: the resolved plan's record read of
        /// a grouped call's argument. Its second half stays beside it: of
        /// binding's two findings for one argument the table compared the
        /// first's `row` alone, which a case's `compares` would state for
        /// both, and no authored record gives the second's.
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
            let prepared = prepare_in(&spec, &environment(&["id"])).unwrap();
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
            let prepared = prepare_in(&bad, &environment(&["id"])).unwrap();
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
                .all(|d| d.spec_paths == ["rows.group.derivations.A.function.args.x"]));
        }

        /// A seam the replay cannot reach: the resolved plan's row and column
        /// bindings, their completion order and a column call's record read.
        /// A call reading a later column is `rows.json`'s.
        #[test]
        fn unpromoted_column_calls_keep_record_reads_and_column_declaration_order() {
            let spec = specification_with_rows(
                vec![
                    ("A", "int", literal(7)),
                    ("B", "int", call("id", vec![("x", Tree::text("SRC.ID"))])),
                ],
                vec![row("record", vec![], None)],
            );
            let prepared = prepare_in(&spec, &environment(&["id"])).unwrap();
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
        }

        /// A seam the replay cannot reach: the resolved plan's column and row
        /// bindings, their completion order and a template's filter.
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
            let prepared = prepare_in(&spec, &environment(&["id"])).unwrap();
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
            let prepared = prepare_in(&spec, &environment(&["id"])).unwrap();
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

        /// A seam the replay cannot reach: a row template id 200,000
        /// characters long, generated, and the 63 project calls a preparation
        /// returns.
        #[test]
        fn every_function_argument_keeps_the_complete_long_row_context() {
            use crate::support::roots::{Case, Parameter};
            use yamaa_core::value::ValueType;
            let parameters = (0..8).map(|n| format!("p{n}")).collect::<Vec<_>>();
            let mut definition = file("id");
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
            let prepared = prepare_in(&spec, &project(vec![definition])).unwrap();
            assert_eq!(prepared.project_calls().unwrap().calls().len(), 63);
        }

        /// A seam the replay cannot reach: the resolved plan of a scalar
        /// `first_available` default, which stays row-local whether or not a
        /// row template replaces it. A window default stays in the column
        /// phase instead, as `rows.json`'s cases state.
        #[test]
        fn first_available_column_defaults_remain_row_local() {
            for overridden in [false, true] {
                let mut derivations = vec![("B", Tree::text("A"))];
                if overridden {
                    derivations.push(("A", literal(7)));
                }
                let payload = Tree::map(vec![("sources", Tree::List(vec![Tree::text("ID")]))]);
                let spec = specification_with_rows(
                    vec![
                        ("A", "int", Tree::map(vec![("first_available", payload)])),
                        ("B", "int", literal(0)),
                    ],
                    vec![row("record", derivations, None)],
                );
                let prepared = prepare_in(&spec, &environment(&[])).unwrap();
                let plan = prepared.bind(&source()).unwrap();
                assert!(resolved::columns(&plan).is_empty());
                assert_eq!(resolved::completed(&resolved::row(&plan, 0)), [0, 1, 2]);
                let a = resolved::row(&plan, 0)[1];
                if overridden {
                    assert_eq!(resolved::op(&plan, a.expr), &Op::Literal(Value::Int(7)));
                } else {
                    assert_eq!(&*a.path, "columns.A.derivation.first_available");
                    assert!(matches!(
                        resolved::op(&plan, a.expr),
                        Op::Window { .. } | Op::ContextWindow { .. } | Op::FirstAvailable { .. }
                    ));
                }
            }
        }

        /// A seam the replay cannot reach: the functions a preparation calls,
        /// the template's completion order and a nested call's argument read.
        #[test]
        fn nested_row_case_calls_share_activation_and_dependency_order() {
            let mut definition = file("format");
            definition.returns = ColumnType::Str;
            for case in &mut definition.tests {
                if let Value::Int(value) = case.result {
                    case.result = Value::Str(value.to_string());
                }
            }
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
            let prepared = prepare_in(
                &spec,
                &project(vec![file("id"), definition, file("unused")]),
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

        /// A seam the replay cannot reach: each template's bound assignments,
        /// their paths and grouping, over repeated binds.
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
            let prepared = prepare(&spec).unwrap();
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
                assert_eq!(&*big(0).path, "columns.BIG.derivation");
                assert_eq!(
                    literal(big(1)),
                    Op::Literal(Value::Str("big-override".into()))
                );
                assert_eq!(&*big(1).path, "rows.override.derivations.BIG");
                assert_eq!(literal(big(2)), Op::Literal(Value::Str(large.clone())));
                assert_eq!(&*big(2).path, "columns.BIG.derivation");
                assert!(matches!(
                    resolved::grain(resolved::templates(&plan)[2]),
                    Grain::Groups(_)
                ));
                let note = |template: usize| bound_assignment(&plan, template, 2);
                assert_eq!(literal(note(0)), Op::Literal(Value::Str("note-a".into())));
                assert_eq!(&*note(0).path, "rows.records.derivations.NOTE");
                assert_eq!(
                    literal(note(1)),
                    Op::Literal(Value::Str("note-default".into()))
                );
                assert_eq!(&*note(1).path, "columns.NOTE.derivation");
                assert_eq!(
                    literal(note(2)),
                    Op::Literal(Value::Str("note-default".into()))
                );
                assert_eq!(&*note(2).path, "columns.NOTE.derivation");
            }
        }

        /// A seam the replay cannot reach: the rows a bound plan executes to,
        /// a table core's readers do not return, over repeated runs.
        #[test]
        fn shared_defaults_execute_across_drivers_grouping_and_repeated_runs() {
            // Three admitted drivers: the second template reads ALT, the
            // third groups GRP; both inherit the same authored defaults.
            let large = "shared-authored-default ".repeat(256);
            let source_value = |variable: &str| Tree::text(variable);
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
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
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
            let prepared = prepare(&root.document()).unwrap();
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
                    &|| false,
                );
                assert_eq!(attempt.result.unwrap().dataset.rows(), expected.as_slice());
            }
        }
    }
}
