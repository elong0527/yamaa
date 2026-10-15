//! Native dataset assertions.
#[allow(dead_code)]
mod support;
mod execution {
    mod dataset {
        use crate::support::dataset::*;

        /// Collection ambiguity keeps the selection owner through an absence wrapper;
        /// reached host failures and handlers retain their separate operand provenance.
        #[test]
        fn first_available_expression_owns_collection_failure_without_fallback() {
            use yamaa_core::dataset::{FirstAvailable, SelectionRead, SelectionSource};
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("A", ColumnType::Int),
                    ("B", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(1), Value::Int(11), Value::Int(20)],
                ],
            );
            let owner = "columns.V.derivation.first_available";
            let operand = "columns.V.derivation.first_available.sources[0]";
            let raw = "columns.V.derivation.first_available.sources[0].value";
            let make = |absent_variable| {
                let first = Assignment {
                    column: 1,
                    path: operand.into(),
                    expression: Expression::SourceBinding {
                        source: Box::new(Assignment {
                            column: 1,
                            path: raw.into(),
                            expression: if absent_variable {
                                Expression::Literal(Value::Missing)
                            } else {
                                Expression::Collect {
                                    column: 1,
                                    identifier: "SRC.A".into(),
                                    filter: None,
                                    selection: None,
                                }
                            },
                        }),
                        absent: Some(Value::Int(9)),
                        variable_absent: absent_variable,
                        selection_declared: false,
                        variable: if absent_variable {
                            "UNDECLARED"
                        } else {
                            "SRC.A"
                        }
                        .into(),
                        path: operand.into(),
                    },
                };
                key_plan(
                    &source,
                    schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
                    vec![0],
                    vec![assign(0, Expression::Source(0))],
                    vec![Assignment {
                        column: 1,
                        path: owner.into(),
                        expression: Expression::FirstAvailable(Box::new(FirstAvailable::new(
                            vec![
                                SelectionSource {
                                    path: operand.into(),
                                    read: SelectionRead::Expression(Box::new(first)),
                                },
                                SelectionSource {
                                    path: "columns.V.derivation.first_available.sources[1]".into(),
                                    read: SelectionRead::Collect {
                                        column: 2,
                                        identifier: "SRC.B".into(),
                                        filter: None,
                                    },
                                },
                            ],
                            Value::Int(99),
                        ))),
                    }],
                )
            };
            let plan = make(false);
            let absent = make(true);
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::MultipleValues {
                path,
                identifier,
                value_count,
                identity,
            } = *attempt.result.unwrap_err()
            else {
                panic!("distinct reached readings must fail the selection");
            };
            assert_eq!(path, owner);
            assert_eq!(identifier, "SRC.A");
            assert_eq!(value_count, 2);
            assert_eq!(
                identity,
                Some(yamaa_core::dataset::RowIdentity {
                    position: 0,
                    values: vec![Value::Int(1)],
                })
            );
            assert_eq!(
                attempt.handler_counts,
                vec![yamaa_core::numeric_lifecycle::HandlerCount {
                    spec_path: format!("{operand}.absent"),
                    handler: yamaa_core::numeric_lifecycle::HandlerKind::Absent,
                    count: 0,
                }],
            );
            assert!(source.reads.borrow().iter().all(|&(_, column)| column != 2));

            source.reads.borrow_mut().clear();
            source.fail = Some((1, 1));
            assert!(matches!(
                *plan.execute(&source, &[], &mut yamaa_core::function_invocation::NoFunctions::new(), None).result.unwrap_err(),
                ExecutionError::Cell {
                    path,
                    source_row: 1,
                    error: CellError::Access("source failure"),
                } if path == raw
            ));
            assert!(source.reads.borrow().iter().all(|&(_, column)| column != 2));

            source.reads.borrow_mut().clear();
            let attempt = absent.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                &[vec![Value::Int(1), Value::Int(9)]]
            );
            assert_eq!(attempt.handler_counts.len(), 1);
            assert_eq!(
                attempt.handler_counts[0].spec_path,
                format!("{operand}.absent")
            );
            assert_eq!(
                attempt.handler_counts[0].handler,
                yamaa_core::numeric_lifecycle::HandlerKind::Absent
            );
            assert_eq!(attempt.handler_counts[0].count, 1);
            assert!(source.reads.borrow().iter().all(|&(_, column)| column == 0));
        }

        /// UNKNOWN continues; inactive branches cannot read a failing donor. Selected
        /// string literals are converted once by the owning output assignment.
        #[test]
        fn ordered_cases_are_lazy_and_convert_the_selected_raw_value() {
            use yamaa_core::dataset::CaseBranch;
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("FLAG", ColumnType::Int),
                    ("BAD", ColumnType::Str),
                ],
                vec![
                    vec![Value::Int(1), Value::Int(1), Value::Str("bad".into())],
                    vec![Value::Int(2), Value::Int(-1), Value::Str("bad".into())],
                    vec![Value::Int(3), Value::Missing, Value::Str("bad".into())],
                ],
            );
            source.fail = Some((0, 2));
            let never = filter(
                Node::Compare {
                    operator: Comparison::Equal,
                    left: Scalar::Literal(Value::Int(0)),
                    right: Scalar::Literal(Value::Int(1)),
                },
                vec![],
            );
            let branch = |when, expression| CaseBranch {
                when,
                assignment: Box::new(assign(2, expression)),
            };
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&[
                    ("ID", ColumnType::Int),
                    ("FLAG", ColumnType::Int),
                    ("V", ColumnType::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(1, Expression::Source(1)),
                    ],
                    filter: None,
                }],
                vec![assign(
                    2,
                    Expression::Case(vec![
                        branch(
                            Some(positive(Read::Column(1))),
                            Expression::Literal(Value::Str("7".into())),
                        ),
                        branch(Some(never), Expression::Source(2)),
                        branch(None, Expression::Literal(Value::Int(8))),
                    ]),
                )],
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
                    vec![Value::Int(1), Value::Int(1), Value::Int(7)],
                    vec![Value::Int(2), Value::Int(-1), Value::Int(8)],
                    vec![Value::Int(3), Value::Missing, Value::Int(8)]
                ]
            );
            assert!(source.reads.borrow().iter().all(|&(_, column)| column < 2));
        }

        /// Matching uses the driver's raw code even when the output code has another
        /// value. Conflicting raw readings are a collection failure, never first-row wins.
        #[test]
        fn named_record_matching_keeps_raw_driver_keys_independent_of_output_columns() {
            use yamaa_core::dataset::{IntermediateKey, MatchRead, MatchValue};
            let driver = table(
                &[("ID", ColumnType::Int), ("RAW", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Str("RAW".into())],
                    vec![Value::Int(1), Value::Str("RAW".into())],
                ],
            );
            let right = table(
                &[
                    ("ID", ColumnType::Int),
                    ("CODE", ColumnType::Str),
                    ("ORD", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("RAW".into()), Value::Int(1)],
                    vec![Value::Int(1), Value::Str("OUTPUT".into()), Value::Int(2)],
                ],
            );
            let mut item = named_item();
            item.keys = vec![IntermediateKey {
                source_column: 1,
                path: "intermediates[0].key".into(),
                value: MatchValue::Variable(MatchRead::Record {
                    column: 1,
                    identifier: "DRIVER.RAW".into(),
                }),
            }];
            use yamaa_core::dataset::{SecondarySource, SourceSchemas};
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: driver.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "OTHER".into(),
                        schema: right.schema.clone(),
                    }],
                },
                vec![item],
                schema(&[
                    ("ID", ColumnType::Int),
                    ("CODE", ColumnType::Str),
                    ("V", ColumnType::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![
                    assign(1, Expression::Literal(Value::Str("OUTPUT".into()))),
                    assign(
                        2,
                        Expression::Intermediate {
                            index: 0,
                            column: 2,
                        },
                    ),
                ],
                vec![0],
                vec![],
            )
            .unwrap();
            assert_eq!(
                plan.execute(
                    &driver,
                    &[&right],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[vec![
                    Value::Int(1),
                    Value::Str("OUTPUT".into()),
                    Value::Int(1)
                ]]
            );
            let conflict = table(
                &[("ID", ColumnType::Int), ("RAW", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Str("RAW".into())],
                    vec![Value::Int(1), Value::Str("OTHER".into())],
                ],
            );
            assert!(matches!(
                *plan
                    .execute(
                        &conflict,
                        &[&right],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::MultipleValues { .. }
            ));
        }

        /// Presentation admits every declared logical type, preserving stable ties and missing placement.
        #[test]
        fn presentation_order_preserves_all_types_ties_and_direction_independent_nulls() {
            use yamaa_core::dataset::OrderTerm;
            use yamaa_core::{
                temporal::{DateTime, DateTimePrecision},
                value::FiniteFloat,
            };
            let date = |year, precision| Value::Date(Date::new(year, 1, 1, precision).unwrap());
            let datetime = |year, precision| {
                Value::DateTime(
                    DateTime::new(
                        Date::new(year, 1, 1, DatePrecision::Day).unwrap(),
                        0,
                        0,
                        0,
                        precision,
                    )
                    .unwrap(),
                )
            };
            for (kind, low, high, tied) in [
                (
                    ColumnType::Str,
                    Value::Str("A\0z".into()),
                    Value::Str("\u{00e9}".into()),
                    Value::Str("A\0z".into()),
                ),
                (
                    ColumnType::Int,
                    Value::Int(9_007_199_254_740_992),
                    Value::Int(9_007_199_254_740_993),
                    Value::Int(9_007_199_254_740_992),
                ),
                (
                    ColumnType::Float,
                    Value::Float(FiniteFloat::new(-0.0).unwrap()),
                    Value::Float(FiniteFloat::new(1.0).unwrap()),
                    Value::Float(FiniteFloat::new(0.0).unwrap()),
                ),
                (
                    ColumnType::Date,
                    date(2020, DatePrecision::Year),
                    date(2021, DatePrecision::Day),
                    date(2020, DatePrecision::Month),
                ),
                (
                    ColumnType::DateTime,
                    datetime(2020, DateTimePrecision::Day),
                    datetime(2021, DateTimePrecision::Second),
                    datetime(2020, DateTimePrecision::Second),
                ),
            ] {
                let source = table(
                    &[("ID", ColumnType::Int), ("V", kind)],
                    vec![
                        vec![Value::Int(1), high],
                        vec![Value::Int(2), Value::Missing],
                        vec![Value::Int(3), low],
                        vec![Value::Int(4), tied],
                        vec![Value::Int(5), Value::Missing],
                    ],
                );
                for descending in [false, true] {
                    for nulls_first in [false, true] {
                        let plan = DatasetPlan::new(
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
                        .with_output_order(vec![OrderTerm {
                            column: 1,
                            descending,
                            nulls_first,
                        }])
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
                        let present = if descending {
                            vec![1, 3, 4]
                        } else {
                            vec![3, 4, 1]
                        };
                        let expected = if nulls_first {
                            [vec![2, 5], present].concat()
                        } else {
                            [present, vec![2, 5]].concat()
                        };
                        assert_eq!(
                            result
                                .dataset
                                .rows()
                                .iter()
                                .map(|r| match r[0] {
                                    Value::Int(v) => v,
                                    _ => panic!("ID"),
                                })
                                .collect::<Vec<_>>(),
                            expected,
                            "{kind:?} desc={descending} first={nulls_first}"
                        );
                        // Exact cell representations stay with their original row, including zero sign and precision.
                        for row in result.dataset.rows() {
                            let Value::Int(id) = row[0] else { panic!("ID") };
                            let original = &source.rows[usize::try_from(id - 1).unwrap()][1];
                            match (&row[1], original) {
                                (Value::Float(a), Value::Float(b)) => {
                                    assert_eq!(a.get().to_bits(), b.get().to_bits())
                                }
                                (Value::Date(a), Value::Date(b)) => {
                                    assert_eq!(a.collected_precision(), b.collected_precision())
                                }
                                (Value::DateTime(a), Value::DateTime(b)) => {
                                    assert_eq!(a.collected_precision(), b.collected_precision())
                                }
                                (actual, expected) => assert_eq!(actual, expected),
                            }
                        }
                    }
                }
            }
        }

        /// Ordering follows window evaluation and admits only distinct bound columns.
        #[test]
        fn presentation_keeps_window_neighbors_and_rejects_invalid_terms() {
            use yamaa_core::dataset::{OrderTerm, WindowKind};
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("a".into()), Value::Int(10)],
                    vec![Value::Int(2), Value::Str("a".into()), Value::Int(20)],
                    vec![Value::Int(3), Value::Str("a".into()), Value::Int(30)],
                ],
            );
            let plan = value_window_plan(
                &source,
                WindowKind::RowValue {
                    column: 2,
                    offset: -1,
                },
                None,
            )
            .unwrap()
            .with_output_order(vec![OrderTerm {
                column: 0,
                descending: true,
                nulls_first: false,
            }])
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
            assert_eq!(
                result
                    .dataset
                    .rows()
                    .iter()
                    .map(|r| (r[0].clone(), r[3].clone()))
                    .collect::<Vec<_>>(),
                [
                    (Value::Int(3), Value::Int(20)),
                    (Value::Int(2), Value::Int(10)),
                    (Value::Int(1), Value::Missing)
                ]
            );
            assert!(plan
                .clone()
                .with_output_order(vec![OrderTerm {
                    column: 99,
                    descending: false,
                    nulls_first: false
                }])
                .is_err());
            let duplicate = OrderTerm {
                column: 0,
                descending: false,
                nulls_first: false,
            };
            assert!(plan
                .with_output_order(vec![duplicate.clone(), duplicate])
                .is_err());
        }

        /// Exhaustion during presentation returns no partial artifact and retains completed checks.
        #[test]
        fn presentation_keeps_completed_checks_and_failed_verification_precedes_ordering() {
            use yamaa_core::dataset::OrderTerm;
            let source = table(
                &[("ID", ColumnType::Int)],
                vec![
                    vec![Value::Int(3)],
                    vec![Value::Int(1)],
                    vec![Value::Int(2)],
                ],
            );
            let base = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![Verification {
                    path: "verifications[0].row_count".into(),
                    check: Check::RowCount {
                        min: Some(3),
                        max: Some(3),
                    },
                }],
            )
            .unwrap();
            let ordered = base
                .clone()
                .with_output_order(vec![OrderTerm {
                    column: 0,
                    descending: false,
                    nulls_first: false,
                }])
                .unwrap();
            let recovered = ordered
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                recovered.dataset.rows(),
                [
                    vec![Value::Int(1)],
                    vec![Value::Int(2)],
                    vec![Value::Int(3)]
                ]
            );
            assert_eq!(recovered.verifications.len(), 1);
            // A failed verification prevents presentation from reordering the artifact.
            let fatal = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![Verification {
                    path: "verifications[0].row_count".into(),
                    check: Check::RowCount {
                        min: Some(4),
                        max: None,
                    },
                }],
            )
            .unwrap()
            .with_output_order(vec![OrderTerm {
                column: 0,
                descending: false,
                nulls_first: false,
            }])
            .unwrap();
            assert!(matches!(
                *fatal
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
        }

        #[test]
        fn grouped_row_count_failures_retain_first_output_positions_and_empty_artifacts() {
            use yamaa_core::dataset::{RowCount, RowCountBound};
            for (rows, grouped, expected_positions) in [
                (
                    vec![
                        vec![Value::Int(1), Value::Str("a".into())],
                        vec![Value::Int(2), Value::Str("a".into())],
                        vec![Value::Int(3), Value::Str("b".into())],
                        vec![Value::Int(4), Value::Str("b".into())],
                    ],
                    true,
                    vec![0, 2],
                ),
                (vec![], false, vec![0]),
            ] {
                let source = table(&[("ID", ColumnType::Int), ("G", ColumnType::Str)], rows);
                let check = RowCount {
                    min: Some(RowCountBound::new(if grouped { "3" } else { "1" }).unwrap()),
                    max: None,
                    min_fraction: None,
                    max_fraction: None,
                    group_by: grouped.then_some(vec![1]),
                    filter: None,
                    when: None,
                };
                let plan = DatasetPlan::new(
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
                    vec![Verification {
                        path: "verifications[0].row_count".into(),
                        check: Check::ScopedRowCount(check),
                    }],
                )
                .unwrap();
                let attempt = plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err()
                else {
                    panic!("expected complete failed row-count records")
                };
                assert_eq!(records.len(), 1);
                let record = &records[0];
                assert_eq!(
                    record
                        .offending_rows
                        .iter()
                        .map(|row| row.position)
                        .collect::<Vec<_>>(),
                    expected_positions
                );
                assert_eq!(record.failed_count, if grouped { 2 } else { 1 });
                assert_eq!(
                    record.row_count.as_ref().unwrap().counts,
                    if grouped { vec![2, 2] } else { vec![0] }
                );
            }
        }

        #[test]
        fn filtered_intermediate_grouping_uses_only_eligible_rows() {
            use yamaa_core::{
                bound_expression::{Binding, BoundPredicate, Read},
                dataset::{
                    Intermediate, IntermediateKey, IntermediateSource, MatchRead, MatchValue,
                    SourceSchemas,
                },
            };
            let names = (0..16)
                .map(|i| format!("K{i}"))
                .chain(["KEEP".into()])
                .collect::<Vec<_>>();
            let fields = names
                .iter()
                .map(|name| (name.as_str(), ColumnType::Int))
                .collect::<Vec<_>>();
            let mut rows = vec![vec![Value::Int(0); 17]; 100];
            rows[99][0] = Value::Int(1);
            rows[99][16] = Value::Int(1);
            let source = table(&fields, rows);
            let filter = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("KEEP = 1", "intermediates[0].filter")
                    .unwrap(),
                vec![Binding {
                    name: "KEEP".into(),
                    read: Read::Source(16),
                }],
            )
            .unwrap();
            let item = Intermediate {
                identifier: "KEPT".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Primary {
                    schema: source.schema.clone(),
                    name: "SRC".into(),
                },
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 0,
                        identifier: "ID".into(),
                    }),
                }],
                filter: Some(filter.into()),
                selection: None,
                no_match: None,
                range: None,
                columns: None,
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,
            };
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: source.schema.clone(),
                    secondary: vec![],
                },
                vec![item],
                schema(&[("ID", ColumnType::Int)]),
                vec![RowTemplate {
                    mode: RowMode::IntermediateDriver {
                        index: 0,
                        source: 0,
                        mode: Box::new(RowMode::Groups((0..16).collect())),
                    },
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
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
            assert_eq!(result.dataset.rows(), [vec![Value::Int(1)]]);
        }

        #[test]
        fn contains_fixed_match_boundary_precedes_later_source_reads() {
            let source = table(
                &[("id", ColumnType::Str)],
                vec![vec![Value::Str("a".repeat(30) + "!")]; 2],
            );
            let plan = DatasetPlan::new(
                source.schema.clone(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![],
                    filter: Some(filter(
                        Node::Contains {
                            value: Scalar::Identifier("x".into()),
                            pattern: "(a+)+$".into(),
                        },
                        vec![binding("x", Read::Source(0))],
                    )),
                }],
                vec![assign(0, Expression::Source(0))],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Limit {
                    resource: yamaa_core::dataset::Resource::RegexWork,
                    limit: 1_000_000,
                    ..
                }
            ));
            assert_eq!(*source.reads.borrow(), [(0, 0)]);
            assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
        }
    }
}
