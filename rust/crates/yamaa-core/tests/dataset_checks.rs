//! Native dataset assertions.
#[allow(dead_code)]
mod support;
mod execution {
    mod dataset {
        use crate::support::dataset::*;

        /// A completed later column waits for an earlier unchecked declaration; retained
        /// records survive the earlier column's conversion failure without reading again.
        #[test]
        fn column_checks_wait_for_the_declared_prefix_and_retain_prior_records() {
            use yamaa_core::dataset::ColumnVerifications;
            let source = table(
                &[("ID", ColumnType::Int), ("V", ColumnType::Int)],
                vec![vec![Value::Int(1), Value::Missing]],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&[
                    ("ID", ColumnType::Int),
                    ("EARLIER", ColumnType::Int),
                    ("LATER", ColumnType::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![
                    assign(2, Expression::Source(1)),
                    assign(1, Expression::Literal(Value::Str("bad".into()))),
                ],
                vec![0],
                vec![],
            )
            .unwrap()
            .with_column_verifications(vec![
                ColumnVerifications {
                    column: 0,
                    checks: vec![Verification {
                        path: "columns.ID.verifications[0].not_missing".into(),
                        check: Check::NotMissing,
                    }],
                },
                ColumnVerifications {
                    column: 2,
                    checks: vec![Verification {
                        path: "columns.LATER.verifications[0].not_missing".into(),
                        check: Check::NotMissing,
                    }],
                },
            ])
            .unwrap();
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Conversion { .. }
            ));
            assert_eq!(attempt.retained_verifications.len(), 1);
            assert_eq!(
                (
                    attempt.retained_verifications[0].path.as_str(),
                    attempt.retained_verifications[0].failed_count
                ),
                ("columns.ID.verifications[0].not_missing", 0)
            );
            assert_eq!(*source.reads.borrow(), [(0, 0), (0, 1)]);
            let empty = table(&[("ID", ColumnType::Int), ("V", ColumnType::Int)], vec![]);
            let result = plan
                .execute(
                    &empty,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                result
                    .verifications
                    .iter()
                    .map(|r| (r.path.as_str(), r.evaluated_count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.ID.verifications[0].not_missing", 0),
                    ("columns.LATER.verifications[0].not_missing", 0)
                ]
            );
        }

        #[test]
        fn repeated_column_patterns_keep_all_completed_checks() {
            use yamaa_core::dataset::ColumnVerifications;
            let source = table(
                &[("ID", ColumnType::Str), ("V", ColumnType::Str)],
                vec![
                    vec![Value::Str("a".into()), Value::Str("bbbbbbbb".into())],
                    vec![Value::Str("b".into()), Value::Str("bbbbbbbb".into())],
                ],
            );
            let make = |count| {
                DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1)),
                        ],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap()
                .with_column_verifications(vec![
                    ColumnVerifications {
                        column: 0,
                        checks: vec![Verification {
                            path: "columns.ID.verifications[0].not_missing".into(),
                            check: Check::NotMissing,
                        }],
                    },
                    ColumnVerifications {
                        column: 1,
                        checks: (0..count)
                            .map(|index| Verification {
                                path: format!("columns.V.verifications[{index}].matches"),
                                check: Check::Matches(
                                    yamaa_core::portable_regex::Pattern::compile("b").unwrap(),
                                ),
                            })
                            .collect(),
                    },
                ])
                .unwrap()
            };
            assert!(make(1)
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            let plan = make(10);
            for _ in 0..2 {
                source.reads.borrow_mut().clear();
                let records = plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap()
                    .verifications;
                assert_eq!(records.len(), 11);
                assert!(records.iter().all(|record| record.failed_count == 0));
                assert_eq!(records[0].path, "columns.ID.verifications[0].not_missing");
                assert_eq!(records[4].path, "columns.V.verifications[3].matches");
                assert_eq!(*source.reads.borrow(), [(0, 0), (0, 1), (1, 0), (1, 1)]);
            }
        }

        #[test]
        fn large_allowed_value_sets_keep_the_prior_check() {
            use yamaa_core::dataset::ColumnVerifications;
            let source = table(
                &[("ID", ColumnType::Int), ("V", ColumnType::Int)],
                vec![vec![Value::Int(1), Value::Int(10)]],
            );
            let make = |wide| {
                let groups = if wide {
                    vec![
                        ColumnVerifications {
                            column: 0,
                            checks: vec![Verification {
                                path: "columns.ID.verifications[0].not_missing".into(),
                                check: Check::NotMissing,
                            }],
                        },
                        ColumnVerifications {
                            column: 1,
                            checks: vec![Verification {
                                path: "columns.V.verifications[0].allowed_values".into(),
                                check: Check::AllowedValues((0..5001).map(Value::Int).collect()),
                            }],
                        },
                    ]
                } else {
                    vec![]
                };
                DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1)),
                        ],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap()
                .with_column_verifications(groups)
                .unwrap()
            };
            assert!(make(false)
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            source.reads.borrow_mut().clear();
            let records = make(true)
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap()
                .verifications;
            assert_eq!(records.len(), 2);
            assert!(records.iter().all(|record| record.failed_count == 0));
            assert_eq!(records[0].path, "columns.ID.verifications[0].not_missing");
            assert_eq!(*source.reads.borrow(), [(0, 0), (0, 1)]);
        }

        /// COUNT uses its owning group, skips unused fields and preserves source failures.
        #[test]
        fn grouped_count_avoids_unused_fields_and_withholds_output_on_read_failure() {
            use yamaa_core::table_reduction::TableReductionError;
            let mut source = table(
                &[("K", ColumnType::Int), ("V", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Missing],
                    vec![Value::Int(1), Value::Str("".into())],
                    vec![Value::Int(1), Value::Missing],
                ],
            );
            source.fail = Some((1, 1));
            let plan = |column| {
                DatasetPlan::new(
                    source.schema.clone(),
                    schema(&[("K", ColumnType::Int), ("N", ColumnType::Int)]),
                    vec![RowTemplate {
                        mode: RowMode::Groups(vec![0]),
                        filter: None,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(
                                1,
                                Expression::Count {
                                    column,
                                    text: "COUNT(SRC.*)".into(),
                                },
                            ),
                        ],
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap()
            };
            assert_eq!(
                plan(None)
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                &[vec![Value::Int(1), Value::Int(3)]]
            );
            assert!(source.reads.borrow().iter().all(|(_, column)| *column == 0));
            source.reads.borrow_mut().clear();
            assert_eq!(
                *plan(Some(1))
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Reduction {
                    path: "columns.C1.derivation".into(),
                    error: TableReductionError::Cell {
                        position: 1,
                        error: CellError::Access("source failure")
                    },
                    identity: None,
                }
            );
            assert_eq!(
                *source.reads.borrow(),
                [(0, 0), (1, 0), (2, 0), (0, 0), (0, 1), (1, 1)]
            );
        }

        /// Binding is complete before execution, while each written occurrence still reads once.
        #[test]
        fn dataset_numeric_bindings_and_occurrence_reads_are_explicit() {
            use yamaa_core::{
                bound_expression::{Binding, BindingError},
                dataset::BoundNumeric,
                expression::{compile, Context},
            };
            let compiled = compile("X + X * 2", Context::Compute, "compute").unwrap();
            assert_eq!(compiled.identifiers().collect::<Vec<_>>(), vec!["X"]);
            assert_eq!(
                BoundNumeric::new(compiled.clone(), vec![]),
                Err(BindingError::MissingName)
            );
            let binding = Binding {
                name: "X".into(),
                read: Read::Source(1),
            };
            assert_eq!(
                BoundNumeric::new(compiled.clone(), vec![binding.clone(), binding.clone()]),
                Err(BindingError::DuplicateName)
            );
            assert_eq!(
                BoundNumeric::new(
                    compiled,
                    vec![
                        binding,
                        Binding {
                            name: "Y".into(),
                            read: Read::Source(1)
                        }
                    ]
                ),
                Err(BindingError::UnusedName)
            );
            let source = table(
                &[("ID", ColumnType::Int), ("X", ColumnType::Int)],
                vec![vec![Value::Int(1), Value::Int(7)]],
            );
            let plan = record_plan(
                &source,
                schema(&[("ID", ColumnType::Int), ("N", ColumnType::Int)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, computed("X + X * 2", &[("X", Read::Source(1))])),
                ],
                vec![],
            );
            let result = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[vec![Value::Int(1), Value::Int(21)]]
            );
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (0, 1), (0, 1)]);
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                result.dataset.rows()
            );
        }

        /// Numeric validation has no output identity; reached arithmetic and conversion do.
        #[test]
        fn dataset_numeric_failure_priority_preserves_ports_missingness_and_identity() {
            use yamaa_core::expression::{Condition, ErrorKind};
            let mut source = table(
                &[("ID", ColumnType::Int), ("X", ColumnType::Str)],
                vec![vec![Value::Int(1), Value::Str("7".into())]],
            );
            let make = |source: &Table, text: &str, bindings: &[(&str, Read)]| {
                record_plan(
                    source,
                    schema(&[("ID", ColumnType::Int), ("N", ColumnType::Int)]),
                    vec![
                        assign(0, Expression::Source(0)),
                        assign(1, computed(text, bindings)),
                    ],
                    vec![],
                )
            };
            source.fail = Some((0, 1));
            let plan = make(&source, "1 / 0 + X", &[("X", Read::Source(1))]);
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Numeric {
                    identity: Some(_),
                    ..
                }
            ));
            assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
            let plan = make(&source, "NULL + X", &[("X", Read::Source(1))]);
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Cell {
                    error: CellError::Access("source failure"),
                    ..
                }
            ));
            source.fail = None;
            let plan = make(&source, "X + 1", &[("X", Read::Source(1))]);
            assert!(
                matches!(*plan.execute(&source, &[], &mut yamaa_core::function_invocation::NoFunctions::new(), None).result.unwrap_err(),ExecutionError::Numeric { error,identity:None } if matches!(error.kind,ErrorKind::Condition(Condition::NumericInput { .. })))
            );
            // A literal outside the int range is refused before any record is read.
            assert!(matches!(
                yamaa_core::expression::compile(
                    "9223372036854775808",
                    yamaa_core::expression::Context::Compute,
                    "columns.N.derivation.compute"
                ),
                Err(yamaa_core::expression::CompileError::Invalid {
                    failure: yamaa_core::expression::Failure::IntegerOverflow,
                    ..
                })
            ));
            source.rows.clear();
            let plan = make(&source, "-9223372036854775808", &[]);
            assert!(plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows()
                .is_empty());
        }

        /// Computation consumes converted dependencies and cannot pick a key group's first donor.
        #[test]
        fn key_grain_numeric_dependencies_are_completed_values() {
            let source = table(
                &[("ID", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(1)]],
            );
            let output = schema(&[
                ("ID", ColumnType::Int),
                ("X", ColumnType::Int),
                ("N", ColumnType::Float),
            ]);
            let template = RowTemplate {
                mode: RowMode::Keys,
                assignments: vec![assign(0, Expression::Source(0))],
                filter: None,
            };
            let assignments = vec![
                assign(1, Expression::Literal(Value::Str("0007".into()))),
                assign(2, computed("X / 2", &[("X", Read::Column(1))])),
            ];
            let plan = DatasetPlan::new(
                source.schema.clone(),
                output.clone(),
                vec![template.clone()],
                assignments.clone(),
                vec![0],
                vec![],
            )
            .unwrap();
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[vec![Value::Int(1), Value::Int(7), Value::float(3.5)]]
            );
            for (read, error) in [
                (Read::Column(2), PlanError::UnavailableColumn),
                (Read::Source(0), PlanError::InvalidKeyMode),
            ] {
                let mut assignments = assignments.clone();
                assignments[1].expression = computed("X / 2", &[("X", read)]);
                assert_eq!(
                    DatasetPlan::new(
                        source.schema.clone(),
                        output.clone(),
                        vec![template.clone()],
                        assignments,
                        vec![0],
                        vec![]
                    ),
                    Err(error)
                );
            }
            assert_eq!(
                source
                    .reads
                    .borrow()
                    .iter()
                    .filter(|(_, column)| *column == 0)
                    .count(),
                2
            );
        }

        /// First occurrence determines group order; source order determines member order.
        #[test]
        fn grouping_uses_exact_logical_equality_without_sorting() {
            let date = |precision| Value::Date(Date::new(2024, 2, 29, precision).unwrap());
            let source = table(
                &[
                    ("text", ColumnType::Str),
                    ("number", ColumnType::Float),
                    ("day", ColumnType::Date),
                ],
                vec![
                    vec![
                        Value::Str("z\0\u{e9}".into()),
                        Value::float(-0.0),
                        date(Precision::Year),
                    ],
                    vec![Value::Missing, Value::float(2.0), date(Precision::Day)],
                    vec![
                        Value::Str("a".into()),
                        Value::float(0.0),
                        date(Precision::Day),
                    ],
                    vec![
                        Value::Str("z\0\u{e9}".into()),
                        Value::float(0.0),
                        date(Precision::Month),
                    ],
                    vec![Value::Missing, Value::float(2.0), date(Precision::Month)],
                    vec![
                        Value::Str("z\0e\u{301}".into()),
                        Value::float(0.0),
                        date(Precision::Year),
                    ],
                ],
            );
            assert_eq!(
                partition(&source, &[0, 1, 2]).unwrap(),
                vec![vec![0, 3], vec![1, 4], vec![2], vec![5]]
            );
            assert_eq!(
                *source.reads.borrow(),
                (0..6)
                    .flat_map(|row| (0..3).map(move |column| (row, column)))
                    .collect::<Vec<_>>()
            );
        }

        /// Invalid grouping declarations precede every source cell read.
        #[test]
        fn grouping_preflight_and_port_failures_are_distinct() {
            let mut source = table(
                &[("x", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            assert_eq!(partition(&source, &[]), Err(GroupingError::EmptyColumns));
            assert_eq!(
                partition(&source, &[0, 1]),
                Err(GroupingError::InvalidColumn {
                    position: 1,
                    column: 1
                })
            );
            assert_eq!(
                partition(&source, &[0, 0]),
                Err(GroupingError::DuplicateColumn {
                    position: 1,
                    column: 0
                })
            );
            assert!(source.reads.borrow().is_empty());
            source.fail = Some((1, 0));
            assert_eq!(
                partition(&source, &[0]),
                Err(GroupingError::Cell {
                    row: 1,
                    column: 0,
                    error: CellError::Access("source failure")
                })
            );
        }

        /// Dependencies observe converted values; a complete column precedes the next.
        #[test]
        fn completed_values_and_whole_column_order() {
            let source = table(
                &[("id", ColumnType::Str), ("text", ColumnType::Str)],
                vec![
                    vec![Value::Str("01".into()), Value::Str("0007".into())],
                    vec![Value::Str("02".into()), Value::Str("0008".into())],
                ],
            );
            let output = schema(&[
                ("id", ColumnType::Int),
                ("number", ColumnType::Int),
                ("text", ColumnType::Str),
            ]);
            let plan = record_plan(
                &source,
                output,
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                    assign(2, Expression::Column(1)),
                ],
                vec![],
            );
            let actual = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                actual.dataset.rows(),
                &[
                    vec![Value::Int(1), Value::Int(7), Value::Str("7".into())],
                    vec![Value::Int(2), Value::Int(8), Value::Str("8".into())]
                ]
            );
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
            assert_eq!(
                actual.dataset.cell(2, 0),
                Err(CellError::OutOfBounds { row: 2, column: 0 })
            );
        }

        /// Later data errors bypass later columns and never become missing or accepted output.
        #[test]
        fn failures_keep_provenance_and_stop_later_columns() {
            let source = table(
                &[("id", ColumnType::Str), ("other", ColumnType::Int)],
                vec![
                    vec![Value::Str("1".into()), Value::Int(9)],
                    vec![Value::Str("bad".into()), Value::Int(8)],
                ],
            );
            let plan = record_plan(
                &source,
                schema(&[("id", ColumnType::Int), ("other", ColumnType::Int)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                vec![],
            );
            match *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            {
                ExecutionError::Conversion {
                    path, output_row, ..
                } => {
                    assert_eq!(path, "columns.id");
                    assert_eq!(output_row, 1);
                }
                other => panic!("{other:?}"),
            }
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
            source.reads.borrow_mut().clear();
            let wrong = table(&[("wrong", ColumnType::Str)], vec![]);
            assert!(matches!(
                *plan
                    .execute(
                        &wrong,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::SchemaMismatch
            ));
        }

        /// Failed checks retain declaration order, full identities and the artifact count.
        #[test]
        fn verification_failure_observations_survive_discarded_output() {
            let source = table(
                &[("id", ColumnType::Int), ("x", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Str("z".into())],
                    vec![Value::Int(2), Value::Str("a".into())],
                    vec![Value::Int(3), Value::Str("z".into())],
                ],
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                vec![
                    verification(Check::Unique(vec![1])),
                    Verification {
                        path: "verifications[1]".into(),
                        check: Check::RowCount {
                            min: None,
                            max: Some(2),
                        },
                    },
                    Verification {
                        path: "verifications[2]".into(),
                        check: Check::RowCount {
                            min: Some(3),
                            max: Some(3),
                        },
                    },
                ],
            );
            let ExecutionError::VerificationFailures(records) = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected verification failure")
            };
            assert_eq!(
                records
                    .iter()
                    .map(|r| (r.evaluated_count, r.failed_count, r.output_rows))
                    .collect::<Vec<_>>(),
                vec![(2, 1, 3), (1, 1, 3), (1, 0, 3)]
            );
            assert_eq!(
                records[0]
                    .offending_rows
                    .iter()
                    .map(|r| (r.position, r.values.clone()))
                    .collect::<Vec<_>>(),
                vec![(0, vec![Value::Int(1)]), (2, vec![Value::Int(3)])]
            );
            assert_eq!(records[1].definition().condition, "row_count_failed");
            assert_eq!(records[1].definition().requirement.unwrap(), "REQ-0385");
            assert_eq!(records[2].path, "verifications[2]");
        }

        /// Missing and duplicate keys precede dataset verification, including missing groups.
        #[test]
        fn output_identity_checks_are_fatal_before_verification() {
            let source = table(
                &[("id", ColumnType::Int)],
                vec![
                    vec![Value::Missing],
                    vec![Value::Int(2)],
                    vec![Value::Missing],
                    vec![Value::Int(2)],
                ],
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![verification(Check::RowCount {
                    min: Some(99),
                    max: None,
                })],
            );
            let ExecutionError::KeyFailures(records) = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected key failure")
            };
            assert_eq!(records.len(), 2);
            assert_eq!(
                (
                    records[0].path.as_str(),
                    records[0].definition().condition,
                    records[0].failed_count
                ),
                ("keys[0]", "missing_key", 2)
            );
            assert_eq!(
                (records[1].definition().condition, records[1].failed_count),
                ("duplicate_key", 2)
            );
            assert_eq!(
                records[1]
                    .offending_rows
                    .iter()
                    .map(|r| r.position)
                    .collect::<Vec<_>>(),
                vec![0, 1]
            );
            assert_eq!(records[1].offending_rows[0].values, vec![Value::Missing]);
        }

        /// Empty artifacts still have a schema and one ungrouped row-count evaluated unit.
        #[test]
        fn empty_tables_keep_schema_and_verify_zero_rows() {
            let source = table(&[("id", ColumnType::Int)], vec![]);
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![
                    verification(Check::Unique(vec![0])),
                    verification(Check::RowCount {
                        min: Some(0),
                        max: Some(0),
                    }),
                ],
            );
            let result = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(result.dataset.schema(), &source.schema);
            assert_eq!(result.dataset.row_count(), 0);
            assert_eq!(
                result
                    .verifications
                    .iter()
                    .map(|r| r.evaluated_count)
                    .collect::<Vec<_>>(),
                vec![0, 1]
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![verification(Check::RowCount {
                    min: Some(1),
                    max: None,
                })],
            );
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::VerificationFailures(_)
            ));
            assert!(source.reads.borrow().is_empty());
        }

        /// Replay real source and independent committed expected values through the Rust
        /// application plan. This intentionally does not claim YAML or host dispatch yet.
        #[test]
        fn adlb_ordered_sum_matches_committed_expected_artifact() {
            let source = fixture(
                include_str!("../../../../benchmarks/adam-adlb-ordered-sum/input/lb.csv"),
                6,
            );
            let expected = fixture(
                include_str!("../../../../benchmarks/adam-adlb-ordered-sum/expected/adlb.csv"),
                5,
            );
            let output = schema(&[
                ("STUDYID", ColumnType::Str),
                ("USUBJID", ColumnType::Str),
                ("AVISIT", ColumnType::Str),
                ("PARAMCD", ColumnType::Str),
                ("PARAM", ColumnType::Str),
                ("AVAL", ColumnType::Float),
                ("DTYPE", ColumnType::Str),
            ]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                output,
                vec![
                    RowTemplate {
                        filter: None,
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(3, Expression::Source(3)),
                            assign(4, Expression::Source(4)),
                            assign(5, Expression::Source(6)),
                            assign(6, Expression::Literal(Value::Missing)),
                        ],
                    },
                    RowTemplate {
                        filter: None,
                        mode: RowMode::Groups(vec![0, 1, 5]),
                        assignments: vec![
                            assign(3, Expression::Literal(Value::Str("TOTAL".into()))),
                            assign(
                                4,
                                Expression::Literal(Value::Str("Total of Components".into())),
                            ),
                            assign(
                                5,
                                Expression::Reduce {
                                    identifier: None,
                                    column: 6,
                                    reducer: NumericReducer::Sum,
                                    text: "SUM(LB.LBSTRESN)".into(),
                                },
                            ),
                            assign(6, Expression::Literal(Value::Str("CALCULATION".into()))),
                        ],
                    },
                ],
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                    assign(2, Expression::Source(5)),
                ],
                vec![0, 1, 3, 2],
                vec![
                    verification(Check::Unique(vec![0, 1, 3, 2])),
                    Verification {
                        path: "verifications[1]".into(),
                        check: Check::RowCount {
                            min: Some(17),
                            max: Some(17),
                        },
                    },
                ],
            )
            .unwrap();
            let actual = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            let projection = [0, 1, 3, 4, 2, 5, 6];
            assert_eq!(
                projection
                    .iter()
                    .map(|&column| actual.dataset.schema().columns()[column].clone())
                    .collect::<Vec<_>>(),
                expected.schema.columns()
            );
            let projected: Vec<Vec<_>> = actual
                .dataset
                .rows()
                .iter()
                .map(|row| {
                    projection
                        .iter()
                        .map(|&column| row[column].clone())
                        .collect()
                })
                .collect();
            assert_eq!(projected, expected.rows);
            assert_eq!(
                actual
                    .verifications
                    .iter()
                    .map(|record| (record.evaluated_count, record.failed_count))
                    .collect::<Vec<_>>(),
                vec![(17, 0), (1, 0)]
            );
            assert_eq!(
                actual.dataset.rows()[12][5],
                Value::float(0.6000000000000001)
            );
            assert_eq!(actual.dataset.rows()[13][5], Value::float(0.6));
            assert_eq!(actual.dataset.rows()[16][5], Value::Missing);
        }

        /// Group reductions collect selected inputs before folding; port errors have priority.
        #[test]
        fn grouped_reduction_preserves_error_order_and_paths() {
            let mut source = table(
                &[("id", ColumnType::Int), ("x", ColumnType::Int)],
                vec![
                    vec![Value::Int(1), Value::Int(i64::MAX)],
                    vec![Value::Int(1), Value::Int(1)],
                    vec![Value::Int(1), Value::Int(2)],
                ],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    filter: None,
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        Assignment {
                            column: 1,
                            expression: Expression::Reduce {
                                identifier: None,
                                column: 1,
                                reducer: NumericReducer::Sum,
                                text: "SUM(T.x)".into(),
                            },
                            path: "rows[0].derivations.x.aggregate".into(),
                        },
                    ],
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            source.fail = Some((2, 1));
            let ExecutionError::Reduction { path, error, .. } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected reduction port error")
            };
            assert_eq!(path, "rows[0].derivations.x.aggregate");
            assert!(matches!(
                error,
                yamaa_core::table_reduction::TableReductionError::Cell {
                    position: 2,
                    error: CellError::Access("source failure")
                }
            ));
            source.fail = None;
            let ExecutionError::Reduction { error, .. } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected arithmetic overflow")
            };
            assert!(matches!(
                error,
                yamaa_core::table_reduction::TableReductionError::Reduction(
                    yamaa_core::reduction::ReductionError::Arithmetic { argument: 1, .. }
                )
            ));
        }

        /// Group keys compare civil fields but the first key's collected precision survives.
        #[test]
        fn grouped_mean_preserves_temporal_metadata_and_missing_values() {
            let date = |precision| Value::Date(Date::new(2024, 1, 2, precision).unwrap());
            let source = table(
                &[("id", ColumnType::Date), ("x", ColumnType::Int)],
                vec![
                    vec![date(Precision::Year), Value::Int(1)],
                    vec![date(Precision::Day), Value::Missing],
                    vec![date(Precision::Month), Value::Int(2)],
                ],
            );
            let output = schema(&[("id", ColumnType::Date), ("mean", ColumnType::Float)]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                output,
                vec![RowTemplate {
                    filter: None,
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            Expression::Reduce {
                                identifier: None,
                                column: 1,
                                reducer: NumericReducer::Mean,
                                text: "MEAN(T.x)".into(),
                            },
                        ),
                    ],
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let result = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(result.dataset.rows().len(), 1);
            let Value::Date(actual) = result.dataset.rows()[0][0] else {
                panic!("expected date")
            };
            assert_eq!(actual.collected_precision(), Precision::Year);
            assert_eq!(result.dataset.rows()[0][1], Value::float(1.5));
        }

        /// Full-width integer keys are never converted to floating point for grouping.
        #[test]
        fn grouping_distinguishes_adjacent_large_integers_and_empty_snapshots() {
            let source = table(
                &[("id", ColumnType::Int)],
                vec![
                    vec![Value::Int(9_007_199_254_740_993)],
                    vec![Value::Int(9_007_199_254_740_992)],
                    vec![Value::Int(i64::MIN)],
                    vec![Value::Int(9_007_199_254_740_993)],
                    vec![Value::Int(i64::MAX)],
                ],
            );
            assert_eq!(
                partition(&source, &[0]).unwrap(),
                vec![vec![0, 3], vec![1], vec![2], vec![4]]
            );
            let empty = table(&[("id", ColumnType::Int)], vec![]);
            assert!(partition(&empty, &[0]).unwrap().is_empty());
            let plan = DatasetPlan::new(
                empty.schema.clone(),
                empty.schema.clone(),
                vec![RowTemplate {
                    filter: None,
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![assign(0, Expression::Source(0))],
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let actual = plan
                .execute(
                    &empty,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(actual.dataset.schema(), &empty.schema);
            assert_eq!(actual.dataset.row_count(), 0);
            assert!(empty.reads.borrow().is_empty());
        }

        /// Impossible table geometry is rejected before a port can be read.
        #[test]
        fn grouping_refuses_impossible_row_geometry_before_reads() {
            struct Huge(TableSchema);
            impl TableAccess for Huge {
                type Error = ();
                /// Describe a virtual source without allocating its records.
                fn schema(&self) -> &TableSchema {
                    &self.0
                }
                /// Exercise usize multiplication overflow at preflight.
                fn row_count(&self) -> usize {
                    usize::MAX
                }
                /// Any read is an admission bug for this impossible table geometry.
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<()>> {
                    panic!("must not read")
                }
            }
            let source = Huge(schema(&[("a", ColumnType::Int), ("b", ColumnType::Int)]));
            assert_eq!(
                partition(&source, &[0, 1]),
                Err(GroupingError::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "table_rows",
                    limit: 10_000_000,
                    actual: Some(usize::MAX)
                }))
            );
        }

        /// The row phase completes each candidate before the later whole-column phase.
        #[test]
        fn row_dependency_conversion_and_direct_access_failure_are_explicit() {
            let mut source = table(
                &[("id", ColumnType::Str)],
                vec![
                    vec![Value::Str("0001".into())],
                    vec![Value::Str("0002".into())],
                ],
            );
            let output = schema(&[
                ("id", ColumnType::Int),
                ("text", ColumnType::Str),
                ("later", ColumnType::Str),
            ]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                output,
                vec![RowTemplate {
                    filter: None,
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(1, Expression::Column(0)),
                    ],
                }],
                vec![assign(2, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            let actual = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                actual.dataset.rows()[0],
                vec![
                    Value::Int(1),
                    Value::Str("1".into()),
                    Value::Str("0001".into())
                ]
            );
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (0, 0), (1, 0)]);
            source.reads.borrow_mut().clear();
            source.fail = Some((1, 0));
            let ExecutionError::Cell {
                path,
                source_row,
                error,
            } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected source error")
            };
            assert_eq!(path, "columns.C0.derivation");
            assert_eq!(source_row, 1);
            assert_eq!(error, CellError::Access("source failure"));
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
        }

        /// Unique permits repeated column references and treats missing as a value.
        #[test]
        fn repeated_unique_columns_keep_missing_combinations_and_paths_are_unique() {
            let source = table(
                &[("id", ColumnType::Int), ("x", ColumnType::Int)],
                vec![
                    vec![Value::Int(1), Value::Missing],
                    vec![Value::Int(2), Value::Missing],
                ],
            );
            let assignments = vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
            ];
            let plan = record_plan(
                &source,
                source.schema.clone(),
                assignments.clone(),
                vec![verification(Check::Unique(vec![1, 1]))],
            );
            let ExecutionError::VerificationFailures(records) = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected duplicate missing combination")
            };
            assert_eq!(
                (
                    records[0].evaluated_count,
                    records[0].failed_count,
                    records[0].offending_rows.len()
                ),
                (1, 1, 2)
            );
            let check = verification(Check::Unique(vec![0]));
            assert_eq!(
                DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![RowTemplate {
                        filter: None,
                        mode: RowMode::Records,
                        assignments
                    }],
                    vec![],
                    vec![0],
                    vec![check.clone(), check]
                ),
                Err(PlanError::DuplicateVerificationPath)
            );
        }

        /// Conversion is a runtime lifecycle, not static literal/type compatibility.
        /// Eager rejection would change empty-template behavior and the diagnostic phase.
        #[test]
        fn literal_conversion_only_occurs_for_constructed_values() {
            for literal in [Value::Str("TOTAL".into()), Value::Bool(true)] {
                for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
                    for row_phase in [true, false] {
                        let empty = table(&[("id", ColumnType::Str)], vec![]);
                        let output =
                            schema(&[("id", ColumnType::Str), ("value", ColumnType::Float)]);
                        let value = assign(1, Expression::Literal(literal.clone()));
                        let plan = DatasetPlan::new(
                            empty.schema.clone(),
                            output,
                            vec![RowTemplate {
                                filter: None,
                                mode: mode.clone(),
                                assignments: if row_phase {
                                    vec![value.clone()]
                                } else {
                                    vec![]
                                },
                            }],
                            if row_phase {
                                vec![assign(0, Expression::Source(0))]
                            } else {
                                vec![assign(0, Expression::Source(0)), value]
                            },
                            vec![0],
                            vec![],
                        )
                        .unwrap();
                        assert_eq!(
                            plan.execute(
                                &empty,
                                &[],
                                &mut yamaa_core::function_invocation::NoFunctions::new(),
                                None
                            )
                            .result
                            .unwrap()
                            .dataset
                            .row_count(),
                            0
                        );
                        let populated = table(
                            &[("id", ColumnType::Str)],
                            vec![vec![Value::Str("a".into())]],
                        );
                        let ExecutionError::Conversion {
                            path,
                            output_row,
                            error,
                            identity,
                        } = *plan
                            .execute(
                                &populated,
                                &[],
                                &mut yamaa_core::function_invocation::NoFunctions::new(),
                                None,
                            )
                            .result
                            .unwrap_err()
                        else {
                            panic!("expected per-value conversion error")
                        };
                        assert_eq!(path, "columns.value");
                        assert_eq!(output_row, 0);
                        assert_eq!(identity.is_some(), !row_phase);
                        assert_eq!(
                    error,
                    yamaa_core::conversion::ConversionError {
                        target: ColumnType::Float,
                        value: literal.clone().into(),
                        source_type: literal.value_type(),
                        reason: yamaa_core::conversion::ConversionReason::IncompatibleOrInvalidText,
                    }
                );
                    }
                }
            }
        }

        /// Repeated text-to-number conversion retains an integer for each candidate.
        #[test]
        fn repeated_literal_text_is_converted_for_each_constructed_value() {
            let source = table(
                &[("id", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            let plan = record_plan(
                &source,
                schema(&[("id", ColumnType::Int), ("number", ColumnType::Int)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Literal(Value::Str("0007".into()))),
                ],
                vec![],
            );
            let actual = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                actual.dataset.rows(),
                &[
                    vec![Value::Int(1), Value::Int(7)],
                    vec![Value::Int(2), Value::Int(7)]
                ]
            );
        }

        /// Mapping and cut keep every successfully selected row (#2429).
        #[test]
        fn mapping_and_cut_rows_keep_all_successful_selected_values() {
            use yamaa_core::mapping_cut::{Cut, Mapping, MappingCut};
            let mapping_source = table(
                &[("id", ColumnType::Int), ("v", ColumnType::Str)],
                (0..20_i64)
                    .map(|id| vec![Value::Int(id), Value::Str("Y".into())])
                    .collect(),
            );
            let mapping_plan = record_plan(
                &mapping_source,
                schema(&[("id", ColumnType::Int), ("code", ColumnType::Int)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(
                        1,
                        Expression::MappingCut {
                            source: Box::new(assign(1, Expression::Source(1))),
                            operation: MappingCut::Mapping(
                                Mapping::new(
                                    "V".into(),
                                    "columns.C1.derivation.mapping".into(),
                                    vec![("Y".into(), Value::Int(1)), ("N".into(), Value::Int(0))],
                                    true,
                                    None,
                                    None,
                                )
                                .unwrap(),
                            ),
                        },
                    ),
                ],
                vec![],
            );
            let mapped = mapping_plan
                .execute(
                    &mapping_source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(mapped.dataset.rows().len(), 20);
            assert!(mapped
                .dataset
                .rows()
                .iter()
                .all(|row| row[1] == Value::Int(1)));

            let cut_source = table(
                &[("id", ColumnType::Int), ("age", ColumnType::Int)],
                (0..20_i64)
                    .map(|id| vec![Value::Int(id), Value::Int(30)])
                    .collect(),
            );
            let cut_plan = record_plan(
                &cut_source,
                schema(&[("id", ColumnType::Int), ("band", ColumnType::Str)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(
                        1,
                        Expression::MappingCut {
                            source: Box::new(assign(1, Expression::Source(1))),
                            operation: MappingCut::Cut(
                                Cut::new(
                                    "AGE".into(),
                                    "columns.C1.derivation.cut".into(),
                                    vec![18.0],
                                    vec!["a".into(), "b".into()],
                                    true,
                                    None,
                                )
                                .unwrap(),
                            ),
                        },
                    ),
                ],
                vec![],
            );
            let cut = cut_plan
                .execute(
                    &cut_source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(cut.dataset.rows().len(), 20);
            assert!(cut
                .dataset
                .rows()
                .iter()
                .all(|row| row[1] == Value::Str("b".into())));
        }

        /// A selection failure retains its own mapping diagnostic.
        #[test]
        fn mapping_selection_failure_retains_the_original_semantic_diagnostic() {
            use yamaa_core::mapping_cut::{Mapping, MappingCut};
            let source = table(
                &[("id", ColumnType::Int), ("v", ColumnType::Str)],
                vec![vec![Value::Int(1), Value::Str("Z".into())]],
            );
            let plan = record_plan(
                &source,
                schema(&[("id", ColumnType::Int), ("code", ColumnType::Int)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(
                        1,
                        Expression::MappingCut {
                            source: Box::new(assign(1, Expression::Source(1))),
                            operation: MappingCut::Mapping(
                                Mapping::new(
                                    "V".into(),
                                    "columns.C1.derivation.mapping".into(),
                                    vec![("Y".into(), Value::Int(1))],
                                    true,
                                    None,
                                    None,
                                )
                                .unwrap(),
                            ),
                        },
                    ),
                ],
                vec![],
            );
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::SemanticDiagnostic { .. }
            ));
        }

        /// Conversion of a reached mapping handler retains the mapping failure owner.
        #[test]
        fn mapping_handler_conversion_failure_retains_the_original_semantic_diagnostic() {
            use yamaa_core::mapping_cut::{Mapping, MappingCut};
            let source = table(
                &[("id", ColumnType::Int), ("v", ColumnType::Str)],
                vec![vec![Value::Int(1), Value::Missing]],
            );
            let plan = record_plan(
                &source,
                schema(&[("id", ColumnType::Int), ("code", ColumnType::Int)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(
                        1,
                        Expression::MappingCut {
                            source: Box::new(assign(1, Expression::Source(1))),
                            operation: MappingCut::Mapping(
                                Mapping::new(
                                    "V".into(),
                                    "columns.C1.derivation.mapping".into(),
                                    vec![("Y".into(), Value::Int(1))],
                                    true,
                                    Some(Value::Str("bad".into())),
                                    None,
                                )
                                .unwrap(),
                            ),
                        },
                    ),
                ],
                vec![],
            );
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::SemanticDiagnostic { .. }
            ));
        }

        /// Dependent columns preserve the exact authored Unicode text.
        #[test]
        fn dependent_unicode_text_columns_keep_both_original_rows() {
            let source = table(
                &[("id", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            let plan = record_plan(
                &source,
                schema(&[
                    ("id", ColumnType::Int),
                    ("a", ColumnType::Str),
                    ("b", ColumnType::Str),
                ]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Literal(Value::Str("\u{e9}".into()))),
                    assign(2, Expression::Column(1)),
                ],
                vec![],
            );
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .row_count(),
                2
            );
        }

        /// Repeated failed checks each retain complete row identities.
        #[test]
        fn repeated_failed_checks_keep_each_complete_identity() {
            let source = table(
                &[("id", ColumnType::Str), ("x", ColumnType::Int)],
                vec![
                    vec![Value::Str("a".into()), Value::Int(1)],
                    vec![Value::Str("b".into()), Value::Int(1)],
                ],
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                vec![
                    verification(Check::Unique(vec![1])),
                    verification(Check::Unique(vec![1])),
                ],
            );
            let ExecutionError::VerificationFailures(records) = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected complete failed checks")
            };
            assert_eq!(records.len(), 2);
            assert!(records
                .iter()
                .all(|record| record.offending_rows.len() == 2));
        }

        /// Completed missing keys are reportable; unconstructed key slots are not.
        #[test]
        fn conversion_failure_retains_completed_missing_identity() {
            use yamaa_core::dataset::RowIdentity;
            let source = table(&[("id", ColumnType::Str)], vec![vec![Value::Missing]]);
            let plan = record_plan(
                &source,
                schema(&[("id", ColumnType::Str), ("x", ColumnType::Float)]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Literal(Value::Bool(true))),
                ],
                vec![],
            );
            let ExecutionError::Conversion { identity, .. } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("expected conversion condition")
            };
            assert_eq!(
                identity,
                Some(RowIdentity {
                    position: 0,
                    values: vec![Value::Missing]
                })
            );
        }

        /// Filters preserve source membership through the later whole-column phase.
        #[test]
        fn row_filters_keep_true_in_original_order_and_drop_unknown() {
            let source = table(
                &[("id", ColumnType::Int), ("x", ColumnType::Int)],
                vec![
                    vec![Value::Int(8), Value::Int(-1)],
                    vec![Value::Int(3), Value::Int(2)],
                    vec![Value::Int(7), Value::Missing],
                    vec![Value::Int(1), Value::Int(4)],
                ],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(1, Expression::Source(1))],
                    filter: Some(positive(Read::Column(1))),
                }],
                vec![assign(0, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[
                    vec![Value::Int(3), Value::Int(2)],
                    vec![Value::Int(1), Value::Int(4)]
                ]
            );
            assert_eq!(
                *source.reads.borrow(),
                vec![(0, 1), (1, 1), (2, 1), (3, 1), (1, 0), (3, 0)]
            );
        }

        /// Group filters see the finished reduction and preserve the surviving group's first member.
        #[test]
        fn grouped_filters_follow_complete_reductions() {
            let source = table(
                &[("id", ColumnType::Int), ("x", ColumnType::Int)],
                vec![
                    vec![Value::Int(8), Value::Int(-3)],
                    vec![Value::Int(3), Value::Int(2)],
                    vec![Value::Int(8), Value::Int(1)],
                    vec![Value::Int(3), Value::Int(4)],
                ],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![assign(
                        1,
                        Expression::Reduce {
                            identifier: None,
                            column: 1,
                            reducer: NumericReducer::Sum,
                            text: "SUM(T.x)".into(),
                        },
                    )],
                    filter: Some(positive(Read::Column(1))),
                }],
                vec![assign(0, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[vec![Value::Int(3), Value::Int(6)]]
            );
        }

        /// A false filter cannot hide earlier conversion failures; empty input invents no evaluations.
        #[test]
        fn filter_timing_preserves_conversion_and_empty_input_behavior() {
            let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Literal(Value::Bool(true)))],
                    filter: Some(filter(Node::Boolean(false), vec![])),
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Conversion { .. }
            ));
            let empty = table(&[("id", ColumnType::Int)], vec![]);
            assert!(plan
                .execute(
                    &empty,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows()
                .is_empty());
        }

        /// Whole-plan binding errors are caught before a source cell is accessed.
        #[test]
        fn filter_bindings_enforce_complete_names_and_phase_scope() {
            for (bindings, expected) in [
                (vec![], BindingError::MissingName),
                (
                    vec![binding("x", Read::Source(0)), binding("x", Read::Source(0))],
                    BindingError::DuplicateName,
                ),
                (
                    vec![binding("y", Read::Source(0))],
                    BindingError::UnusedName,
                ),
            ] {
                let predicate = predicate_plan(
                    vec![Node::IsNull {
                        value: Scalar::Identifier("x".into()),
                        negated: false,
                    }],
                    0,
                    "rows[0].filter",
                );
                assert_eq!(Filter::new(predicate, bindings), Err(expected));
            }
            let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
            for (mode, read, expected) in [
                (
                    RowMode::Records,
                    Read::Column(0),
                    BindingError::UnavailableColumn,
                ),
                (
                    RowMode::Records,
                    Read::Source(1),
                    BindingError::InvalidSource,
                ),
                (
                    RowMode::Groups(vec![0]),
                    Read::Source(0),
                    BindingError::GroupedSource,
                ),
            ] {
                let result = DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![RowTemplate {
                        mode,
                        assignments: vec![],
                        filter: Some(positive(read)),
                    }],
                    vec![assign(0, Expression::Source(0))],
                    vec![0],
                    vec![],
                );
                assert_eq!(result, Err(PlanError::Filter(expected)));
            }
            assert!(source.reads.borrow().is_empty());
        }

        /// Resolved source errors remain opaque and stop at the first written occurrence.
        #[test]
        fn filter_source_errors_keep_occurrence_and_provenance() {
            let mut source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
            source.fail = Some((0, 0));
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![],
                    filter: Some(positive(Read::Source(0))),
                }],
                vec![assign(0, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            let ExecutionError::Predicate { source_row, error } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("predicate port failure")
            };
            assert_eq!(source_row, 0);
            assert_eq!(error.spec_path, "rows[0].filter");
            assert!(matches!(
                error.kind,
                expression::ErrorKind::Resolution {
                    error: CellError::Access("source failure"),
                    ..
                }
            ));
            assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
        }

        /// Discarding values refunds retained output bytes, never cumulative operand bytes.
        #[test]
        fn rejected_candidates_keep_no_retained_output() {
            let source = table(
                &[("id", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&[("id", ColumnType::Int), ("x", ColumnType::Str)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(1, Expression::Literal(Value::Str("abc".into())))],
                    filter: Some(filter(Node::Boolean(false), vec![])),
                }],
                vec![assign(0, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            for _ in 0..2 {
                assert!(plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap()
                    .dataset
                    .rows()
                    .is_empty());
            }
        }

        /// LIKE filters preserve the original rejected-row behavior on repeated attempts.
        #[test]
        fn like_filters_repeat_without_retaining_rejected_rows() {
            let source = table(
                &[("id", ColumnType::Str)],
                vec![vec![Value::Str("abc".into())]; 2],
            );
            let predicate = filter(
                Node::Like {
                    value: Scalar::Identifier("x".into()),
                    pattern: Scalar::Literal(Value::Str("z".into())),
                    escape: None,
                    negated: false,
                },
                vec![binding("x", Read::Source(0))],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![],
                    filter: Some(predicate),
                }],
                vec![assign(0, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            for _ in 0..2 {
                assert!(plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap()
                    .dataset
                    .rows()
                    .is_empty());
            }
        }

        /// Asserts fail on false and unknown; with `when` they fail only for true -> nontrue.
        #[test]
        fn predicate_checks_follow_truth_tables_and_keep_every_record() {
            let source = table(
                &[("id", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            let truths = [
                Node::Boolean(true),
                Node::Boolean(false),
                Node::Compare {
                    operator: Comparison::Equal,
                    left: Scalar::Literal(Value::Missing),
                    right: Scalar::Literal(Value::Int(1)),
                },
            ];
            for (index, node) in truths.iter().enumerate() {
                let plan = record_plan(
                    &source,
                    source.schema.clone(),
                    vec![assign(0, Expression::Source(0))],
                    vec![verification(Check::Assert {
                        when: None,
                        require: check_predicate(
                            node.clone(),
                            vec![],
                            "verifications[0].assert.require",
                        ),
                    })],
                );
                let result = plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result;
                if index == 0 {
                    let records = result.unwrap().verifications;
                    assert_eq!(records[0].evaluated_count, 2);
                    assert_eq!(records[0].failed_count, 0);
                } else {
                    let ExecutionError::VerificationFailures(records) = *result.unwrap_err() else {
                        panic!("assert data failure")
                    };
                    assert_eq!(records[0].definition().condition, "assert_failed");
                    assert_eq!(records[0].failed_count, 2);
                    assert_eq!(
                        records[0]
                            .offending_rows
                            .iter()
                            .map(|row| row.position)
                            .collect::<Vec<_>>(),
                        vec![0, 1]
                    );
                }
            }
            for (a, when) in truths.iter().enumerate() {
                for (b, require) in truths.iter().enumerate() {
                    let plan = record_plan(
                        &source,
                        source.schema.clone(),
                        vec![assign(0, Expression::Source(0))],
                        vec![
                            verification(Check::Assert {
                                when: Some(check_predicate(
                                    when.clone(),
                                    vec![],
                                    "verifications[0].assert.when",
                                )),
                                require: check_predicate(
                                    require.clone(),
                                    vec![],
                                    "verifications[0].assert.require",
                                ),
                            }),
                            verification(Check::RowCount {
                                min: Some(2),
                                max: Some(2),
                            }),
                        ],
                    );
                    let result = plan
                        .execute(
                            &source,
                            &[],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None,
                        )
                        .result;
                    if a == 0 && b != 0 {
                        let ExecutionError::VerificationFailures(records) = *result.unwrap_err()
                        else {
                            panic!("guarded assert data failure")
                        };
                        assert_eq!(records.len(), 2);
                        assert_eq!(records[0].definition().condition, "assert_failed");
                        assert_eq!(records[0].definition().requirement.unwrap(), "REQ-0384");
                        assert_eq!(records[0].failed_count, 2);
                        assert_eq!(records[1].failed_count, 0);
                    } else {
                        assert_eq!(result.unwrap().verifications[0].failed_count, 0);
                    }
                }
            }
        }
    }
}
