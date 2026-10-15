//! Native dataset assertions.
#[allow(dead_code)]
mod support;
mod execution {
    mod dataset {
        use crate::support::dataset::*;

        /// Invalid declarations fail even for empty output, preserving earlier successful/failed checks.
        #[test]
        fn predicate_declaration_validation_is_sequential_on_empty_output() {
            for count in [0, 1] {
                let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]; count]);
                let invalid = check_predicate(
                    Node::Compare {
                        operator: Comparison::Equal,
                        left: Scalar::Identifier("id".into()),
                        right: Scalar::Literal(Value::Str("bad".into())),
                    },
                    vec![binding("id", Read::Column(0))],
                    "verifications[1].assert.require",
                );
                let plan = record_plan(
                    &source,
                    source.schema.clone(),
                    vec![assign(0, Expression::Source(0))],
                    vec![
                        verification(Check::RowCount {
                            min: Some(2),
                            max: None,
                        }),
                        verification(Check::Assert {
                            when: Some(check_predicate(
                                Node::Boolean(false),
                                vec![],
                                "verifications[1].assert.when",
                            )),
                            require: invalid,
                        }),
                        verification(Check::RowCount {
                            min: Some(0),
                            max: None,
                        }),
                    ],
                );
                let ExecutionError::VerificationPredicate { error, records } = *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap_err()
                else {
                    panic!("predicate declaration failure")
                };
                assert_eq!(error.spec_path, "verifications[1].assert.require");
                assert!(matches!(
                    error.kind,
                    predicate::ErrorKind::Condition(predicate::Condition::IncompatiblePair { .. })
                ));
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].failed_count, 1);
                assert_eq!(records[0].output_rows, count);
            }
        }

        /// A false `when` never masks a later dynamic `require` pattern failure.
        #[test]
        fn assert_when_evaluates_dynamic_require_and_keeps_prefix() {
            let source = table(
                &[("id", ColumnType::Int), ("p", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Str("ok".into())],
                    vec![Value::Int(2), Value::Str("tail!".into())],
                ],
            );
            let require = check_predicate(
                Node::Like {
                    value: Scalar::Literal(Value::Str("text".into())),
                    pattern: Scalar::Identifier("p".into()),
                    escape: Some('!'),
                    negated: false,
                },
                vec![binding("p", Read::Column(1))],
                "verifications[1].assert.require",
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                vec![
                    verification(Check::RowCount {
                        min: Some(2),
                        max: Some(2),
                    }),
                    verification(Check::Assert {
                        when: Some(check_predicate(
                            Node::Boolean(false),
                            vec![],
                            "verifications[1].assert.when",
                        )),
                        require,
                    }),
                ],
            );
            let ExecutionError::VerificationPredicate { error, records } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("dynamic require failure")
            };
            assert_eq!(
                error.kind,
                predicate::ErrorKind::Condition(predicate::Condition::DanglingEscape)
            );
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].failed_count, 0);
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
        }

        /// Check predicates cannot escape completed-output scope and read source values.
        #[test]
        fn predicate_checks_admit_only_completed_output_bindings() {
            let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
            for (read, error) in [
                (Read::Source(0), BindingError::GroupedSource),
                (Read::Column(1), BindingError::UnavailableColumn),
            ] {
                let plan = DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![],
                        filter: None,
                    }],
                    vec![assign(0, Expression::Source(0))],
                    vec![0],
                    vec![verification(Check::Assert {
                        when: None,
                        require: positive(read),
                    })],
                );
                assert_eq!(plan, Err(PlanError::Predicate(error)));
            }
            assert!(source.reads.borrow().is_empty());
        }

        /// A compiler checkpoint validates only a declaration, never data or a check record.
        #[test]
        fn predicate_declaration_checkpoint_does_not_evaluate_rows() {
            let source = table(
                &[("id", ColumnType::Int), ("p", ColumnType::Str)],
                vec![vec![Value::Int(1), Value::Str("tail!".into())]],
            );
            let predicate = check_predicate(
                Node::Like {
                    value: Scalar::Literal(Value::Str("text".into())),
                    pattern: Scalar::Identifier("p".into()),
                    escape: Some('!'),
                    negated: false,
                },
                vec![binding("p", Read::Column(1))],
                "verifications[1].assert.when",
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                vec![
                    verification(Check::RowCount {
                        min: Some(1),
                        max: Some(1),
                    }),
                    verification(Check::PredicateDeclaration(predicate)),
                ],
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
            assert_eq!(actual.verifications.len(), 1);
            assert_eq!(actual.dataset.rows(), source.rows);
        }

        /// Empty datasets still validate each authored verification declaration.
        #[test]
        fn predicate_declarations_on_empty_output_keep_each_record() {
            let source = table(&[("id", ColumnType::Str)], vec![]);
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![
                    verification(Check::Assert {
                        when: None,
                        require: check_predicate(
                            Node::Boolean(true),
                            vec![],
                            "verifications[0].assert.require",
                        ),
                    }),
                    verification(Check::Assert {
                        when: None,
                        require: check_predicate(
                            Node::Boolean(true),
                            vec![],
                            "verifications[1].assert.require",
                        ),
                    }),
                ],
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
                .verifications
                .len(),
                2
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![verification(Check::Assert {
                    when: None,
                    require: check_predicate(
                        Node::IsNull {
                            value: Scalar::Identifier("id".into()),
                            negated: true,
                        },
                        vec![binding("id", Read::Column(0))],
                        "verifications[0].assert.require",
                    ),
                })],
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
                .verifications[0]
                    .evaluated_count,
                0
            );
        }

        #[test]
        fn all_or_none_keeps_all_repeated_declared_reads() {
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Int),
                    ("W", ColumnType::Int),
                ],
                vec![vec![Value::Int(1), Value::Int(2), Value::Int(3)]],
            );
            let assignments = || {
                (0..3)
                    .map(|column| assign(column, Expression::Source(column)))
                    .collect()
            };
            let control = record_plan(&source, source.schema.clone(), assignments(), vec![]);
            assert!(control
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            let columns = (0..5001).map(|index| 1 + index % 2).collect();
            let plan = record_plan(
                &source,
                source.schema.clone(),
                assignments(),
                vec![verification(Check::AllOrNone(columns))],
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
            assert_eq!(result.verifications.len(), 1);
            assert_eq!(result.verifications[0].failed_count, 0);
            assert_eq!(result.verifications[0].evaluated_count, 1);
        }

        /// Converted identity collapses records in first appearance; source readings omit missing.
        #[test]
        fn key_grain_preserves_order_membership_and_completed_conversion() {
            let source = table(
                &[("id", ColumnType::Str), ("value", ColumnType::Str)],
                vec![
                    vec![Value::Str("02".into()), Value::Missing],
                    vec![Value::Str("01".into()), Value::Str("7".into())],
                    vec![Value::Str("2".into()), Value::Str("9".into())],
                    vec![Value::Str("2".into()), Value::Str("9".into())],
                ],
            );
            let plan = key_plan(
                &source,
                schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
                vec![0],
                vec![assign(0, Expression::Source(0))],
                vec![assign(1, collect(1))],
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
                &[
                    vec![Value::Int(2), Value::Int(9)],
                    vec![Value::Int(1), Value::Int(7)]
                ]
            );
            assert_eq!(
                *source.reads.borrow(),
                vec![
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (3, 0),
                    (0, 1),
                    (2, 1),
                    (3, 1),
                    (1, 1)
                ]
            );
            let reordered = key_plan(
                &source,
                schema(&[("ID", ColumnType::Int), ("OTHER", ColumnType::Int)]),
                vec![1, 0],
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Literal(Value::Int(99))),
                ],
                vec![],
            );
            assert_eq!(
                reordered
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
                &[
                    vec![Value::Int(2), Value::Int(99)],
                    vec![Value::Int(1), Value::Int(99)]
                ]
            );
        }

        /// Every raw distinct value is counted before target conversion; later port errors still win.
        #[test]
        fn key_grain_conflicts_count_all_raw_values_and_keep_identity() {
            let mut source = table(
                &[("id", ColumnType::Int), ("value", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Str("07".into())],
                    vec![Value::Int(1), Value::Str("7".into())],
                    vec![Value::Int(1), Value::Str("8".into())],
                    vec![Value::Int(1), Value::Missing],
                    vec![Value::Int(1), Value::Str("8".into())],
                ],
            );
            let plan = key_plan(
                &source,
                schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
                vec![0],
                vec![assign(0, Expression::Source(0))],
                vec![assign(1, collect(1))],
            );
            let error = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err();
            let ExecutionError::MultipleValues {
                path,
                identifier,
                value_count,
                identity,
            } = *error
            else {
                panic!("{error:?}")
            };
            assert_eq!(
                (path.as_str(), identifier.as_str(), value_count),
                ("columns.C1.derivation", "SRC.C1", 3)
            );
            let identity = identity.unwrap();
            assert_eq!(identity.position, 0);
            assert_eq!(identity.values, vec![Value::Int(1)]);
            source.fail = Some((4, 1));
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
                    source_row: 4,
                    error: CellError::Access("source failure"),
                    ..
                }
            ));
        }

        /// Key errors precede every non-key derivation, including on an earlier input record.
        #[test]
        fn key_grain_completes_all_key_probes_before_nonkeys() {
            let source = table(
                &[("id", ColumnType::Str), ("value", ColumnType::Str)],
                vec![
                    vec![Value::Str("1".into()), Value::Str("bad value".into())],
                    vec![Value::Str("bad key".into()), Value::Str("7".into())],
                ],
            );
            let plan = key_plan(
                &source,
                schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
                vec![0],
                vec![assign(0, Expression::Source(0))],
                vec![assign(1, collect(1))],
            );
            assert!(
                matches!(*plan.execute(&source, &[], &mut yamaa_core::function_invocation::NoFunctions::new(), None).result.unwrap_err(),ExecutionError::Conversion {path,identity:None,..} if path=="columns.ID")
            );
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
        }

        /// Missing records remain separate and cannot collide with sentinel-shaped user identities.
        #[test]
        fn key_grain_missing_records_reach_output_gate_without_collapsing() {
            let source = table(
                &[
                    ("id", ColumnType::Str),
                    ("tag", ColumnType::Str),
                    ("value", ColumnType::Str),
                ],
                vec![
                    vec![
                        Value::Str("1".into()),
                        Value::Str("__missing_key__".into()),
                        Value::Str("7".into()),
                    ],
                    vec![
                        Value::Missing,
                        Value::Str("present".into()),
                        Value::Str("8".into()),
                    ],
                    vec![
                        Value::Missing,
                        Value::Str("present".into()),
                        Value::Str("9".into()),
                    ],
                ],
            );
            let plan = key_plan(
                &source,
                schema(&[
                    ("ID", ColumnType::Int),
                    ("TAG", ColumnType::Str),
                    ("VALUE", ColumnType::Int),
                ]),
                vec![1, 0],
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                vec![assign(2, collect(2))],
            );
            let error = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err();
            let ExecutionError::KeyFailures(records) = *error else {
                panic!("{error:?}")
            };
            assert_eq!(records.len(), 2);
            assert_eq!(
                (
                    records[0].path.as_str(),
                    records[0].failed_count,
                    records[0].output_rows
                ),
                ("keys[1]", 2, 3)
            );
            assert_eq!(
                records[0]
                    .offending_rows
                    .iter()
                    .map(|r| r.position)
                    .collect::<Vec<_>>(),
                vec![1, 2]
            );
            assert_eq!(
                records[0].offending_rows[0].values,
                vec![Value::Str("present".into()), Value::Missing]
            );
            assert_eq!(
                (records[1].definition().condition, records[1].failed_count),
                ("duplicate_key", 1)
            );
        }

        /// Key storage is admitted before reads; collapsed output and released duplicate text have independent bounds.
        #[test]
        fn key_grain_collapses_duplicate_probes_and_handles_empty_source() {
            let source = table(
                &[("id", ColumnType::Str), ("value", ColumnType::Str)],
                vec![vec![Value::Str("same".into()), Value::Str("abcdefgh".into())]; 3],
            );
            let plan = key_plan(
                &source,
                schema(&[("ID", ColumnType::Str), ("VALUE", ColumnType::Str)]),
                vec![0],
                vec![assign(0, Expression::Source(0))],
                vec![assign(1, collect(1))],
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
                .rows()
                .len(),
                1
            );
            assert!(plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            let empty = table(
                &[("id", ColumnType::Str), ("value", ColumnType::Str)],
                vec![],
            );
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

        /// Collected values use exact typed equality and preserve the first present temporal precision/zero sign.
        #[test]
        fn collected_values_preserve_exact_identity_and_first_representation() {
            for (kind, values, expected) in [
                (
                    ColumnType::Int,
                    vec![Value::Int(9007199254740992), Value::Int(9007199254740993)],
                    None,
                ),
                (
                    ColumnType::Float,
                    vec![Value::float(-0.0), Value::float(0.0)],
                    Some(Value::float(-0.0)),
                ),
                (
                    ColumnType::Date,
                    vec![
                        Value::Date(Date::new(2024, 1, 1, DatePrecision::Year).unwrap()),
                        Value::Date(Date::new(2024, 1, 1, DatePrecision::Day).unwrap()),
                    ],
                    Some(Value::Date(
                        Date::new(2024, 1, 1, DatePrecision::Year).unwrap(),
                    )),
                ),
                (
                    ColumnType::Str,
                    vec![Value::Missing, Value::Missing],
                    Some(Value::Missing),
                ),
            ] {
                let source = table(
                    &[("value", kind)],
                    values.into_iter().map(|v| vec![v]).collect(),
                );
                let plan = key_plan(
                    &source,
                    schema(&[("ID", ColumnType::Int), ("VALUE", kind)]),
                    vec![0],
                    vec![assign(0, Expression::Literal(Value::Int(1)))],
                    vec![assign(1, collect(0))],
                );
                match expected {
                    None => assert!(matches!(
                        *plan
                            .execute(
                                &source,
                                &[],
                                &mut yamaa_core::function_invocation::NoFunctions::new(),
                                None
                            )
                            .result
                            .unwrap_err(),
                        ExecutionError::MultipleValues { value_count: 2, .. }
                    )),
                    Some(expected) => {
                        let result = plan
                            .execute(
                                &source,
                                &[],
                                &mut yamaa_core::function_invocation::NoFunctions::new(),
                                None,
                            )
                            .result
                            .unwrap();
                        assert_eq!(result.dataset.rows()[0][1], expected);
                        if let Value::Float(v) = &result.dataset.rows()[0][1] {
                            assert_eq!(v.get().to_bits(), (-0.0_f64).to_bits());
                        }
                        if let Value::Date(v) = &result.dataset.rows()[0][1] {
                            assert_eq!(v.collected_precision(), DatePrecision::Year);
                        }
                    }
                }
            }
        }

        /// Typed admission rejects incomplete key sets and collection-only reducers in key phase.
        #[test]
        fn key_grain_rejects_incomplete_or_wrong_phase_assignments() {
            let source = schema(&[("id", ColumnType::Int)]);
            let output = schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]);
            for (assignments, columns) in [
                (
                    vec![],
                    vec![assign(0, Expression::Source(0)), assign(1, collect(0))],
                ),
                (
                    vec![assign(
                        0,
                        Expression::Reduce {
                            column: 0,
                            reducer: NumericReducer::Sum,
                            text: "SUM(SRC.id)".into(),
                            identifier: Some("SRC.id".into()),
                        },
                    )],
                    vec![assign(1, collect(0))],
                ),
                (
                    vec![
                        assign(0, Expression::Source(0)),
                        assign(1, Expression::Literal(Value::Int(1))),
                    ],
                    vec![],
                ),
                (
                    vec![assign(0, Expression::Source(0))],
                    vec![assign(1, Expression::Source(0))],
                ),
            ] {
                assert_eq!(
                    DatasetPlan::new(
                        source.clone(),
                        output.clone(),
                        vec![RowTemplate {
                            mode: RowMode::Keys,
                            assignments,
                            filter: None
                        }],
                        columns,
                        vec![0],
                        vec![]
                    )
                    .unwrap_err(),
                    PlanError::InvalidKeyMode
                );
            }
        }

        #[test]
        fn numbering_preserves_output_order_exact_integers_nulls_and_partition_ties() {
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Int(1),
                        Value::Str("a".into()),
                        Value::Int(9_007_199_254_740_992),
                    ],
                    vec![Value::Int(2), Value::Missing, Value::Missing],
                    vec![
                        Value::Int(3),
                        Value::Str("a".into()),
                        Value::Int(9_007_199_254_740_993),
                    ],
                    vec![Value::Int(4), Value::Str("a".into()), Value::Missing],
                    vec![Value::Int(5), Value::Missing, Value::Missing],
                    vec![
                        Value::Int(6),
                        Value::Str("a".into()),
                        Value::Int(9_007_199_254_740_993),
                    ],
                ],
            );
            let result = numbering_plan(&source, true, false)
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            let expected = [
                [3, 3, 2],
                [1, 1, 1],
                [1, 1, 1],
                [4, 4, 3],
                [2, 1, 1],
                [2, 1, 1],
            ];
            for (row, numbers) in result.dataset.rows().iter().zip(expected) {
                assert_eq!(&row[3..], numbers.map(Value::Int));
            }
            // Null placement never reverses with descending direction.
            let first = numbering_plan(&source, true, true)
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(&first.dataset.rows()[3][3..], &[const { Value::Int(1) }; 3]);
            assert_eq!(source.reads.borrow().len(), 36); // No window revisits source data.
        }

        #[test]
        fn numbering_orders_floats_numerically_and_ties_signed_zero() {
            use yamaa_core::value::FiniteFloat;
            let float = |value| Value::Float(FiniteFloat::new(value).unwrap());
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Float),
                ],
                [float(-0.0), float(-3.0), float(0.0), float(2.0)]
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| vec![Value::Int(i as i64), Value::Str("x".into()), value])
                    .collect(),
            );
            let result = numbering_plan(&source, false, false)
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            let expected = [[2, 2, 2], [1, 1, 1], [3, 2, 2], [4, 4, 3]];
            for (row, numbers) in result.dataset.rows().iter().zip(expected) {
                assert_eq!(&row[3..], numbers.map(Value::Int));
            }
        }

        #[test]
        fn numbering_ignores_temporal_precision_for_ties_and_handles_empty_scope() {
            let a = Date::new(2024, 1, 1, DatePrecision::Year).unwrap();
            let b = Date::new(2024, 1, 1, DatePrecision::Day).unwrap();
            let c = Date::new(2023, 1, 1, DatePrecision::Day).unwrap();
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Date),
                ],
                [a, b, c]
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| {
                        vec![Value::Int(i as i64), Value::Missing, Value::Date(value)]
                    })
                    .collect(),
            );
            let plan = numbering_plan(&source, false, false);
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
                &result.dataset.rows()[0][3..],
                &[const { Value::Int(2) }; 3]
            );
            assert_eq!(
                &result.dataset.rows()[1][3..],
                &[Value::Int(3), Value::Int(2), Value::Int(2)]
            );
            source.rows.clear();
            let empty = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(empty.dataset.row_count(), 0);
            assert_eq!(empty.dataset.schema().columns().len(), 6);
        }

        #[test]
        fn numbering_admits_complete_dependencies_in_completed_column_phase() {
            use yamaa_core::dataset::{OrderTerm, Window, WindowKind};
            let source = table(&[("ID", ColumnType::Int)], vec![]);
            let window = Window {
                filter: None,
                kind: WindowKind::RowNumber,
                group_by: vec![],
                order_by: vec![OrderTerm {
                    column: 0,
                    descending: false,
                    nulls_first: false,
                }],
            };
            let output = schema(&[("ID", ColumnType::Int), ("SEQ", ColumnType::Int)]);
            let build = |window, mode, row_window| {
                let mut row = vec![assign(0, Expression::Source(0))];
                let mut columns = vec![assign(1, Expression::Window(window))];
                if row_window {
                    row.append(&mut columns);
                }
                DatasetPlan::new(
                    source.schema.clone(),
                    output.clone(),
                    vec![RowTemplate {
                        mode,
                        assignments: row,
                        filter: None,
                    }],
                    columns,
                    vec![0],
                    vec![],
                )
            };
            assert!(build(window.clone(), RowMode::Keys, false).is_ok());
            let record_plan = build(window.clone(), RowMode::Records, false).unwrap();
            assert_eq!(
                build(window.clone(), RowMode::Keys, true),
                Err(PlanError::InvalidKeyMode)
            );
            let mut invalid = window.clone();
            invalid.order_by.clear();
            assert_eq!(
                build(invalid, RowMode::Keys, false),
                Err(PlanError::InvalidWindow)
            );
            for column in [1, 2] {
                let mut invalid = window.clone();
                invalid.order_by[0].column = column;
                assert_eq!(
                    build(invalid, RowMode::Keys, false),
                    Err(PlanError::InvalidWindow)
                );
            }
            let mut invalid = window;
            invalid.group_by = vec![0, 0];
            assert_eq!(
                build(invalid, RowMode::Keys, false),
                Err(PlanError::InvalidWindow)
            );
            assert!(source.reads.borrow().is_empty());
            let records = table(
                &[("ID", ColumnType::Int)],
                vec![
                    vec![Value::Int(9)],
                    vec![Value::Int(2)],
                    vec![Value::Int(7)],
                ],
            );
            let result = record_plan
                .execute(
                    &records,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[
                    vec![Value::Int(9), Value::Int(3)],
                    vec![Value::Int(2), Value::Int(1)],
                    vec![Value::Int(7), Value::Int(2)],
                ]
            );
            assert_eq!(*records.reads.borrow(), vec![(0, 0), (1, 0), (2, 0)]);
        }

        #[test]
        fn numbering_preserves_original_text_ties() {
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Str),
                ],
                ["z", "a", "a"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| vec![Value::Int(i as i64), Value::Missing, Value::Str(v.into())])
                    .collect(),
            );
            let plan = numbering_plan(&source, false, false);
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
                &result.dataset.rows()[0][3..],
                &[Value::Int(3), Value::Int(3), Value::Int(2)]
            );
        }

        /// Excluded rows remain present, while all three numbering operations count only eligible rows.
        #[test]
        fn window_filters_preserve_rows_and_number_only_true_in_sorted_partitions() {
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                [Some(2), None, Some(-1), Some(2), Some(1)]
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        vec![
                            Value::Int(i as i64),
                            Value::Str("g".into()),
                            v.map_or(Value::Missing, Value::Int),
                        ]
                    })
                    .collect(),
            );
            let result = numbering_with_filter(
                &source,
                true,
                false,
                Some(positive(Read::Column(2))),
                ColumnType::Int,
            )
            .execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            )
            .result
            .unwrap();
            let expected = [
                Some([1, 1, 1]),
                None,
                None,
                Some([2, 1, 1]),
                Some([3, 3, 2]),
            ];
            for (row, numbers) in result.dataset.rows().iter().zip(expected) {
                let expected = numbers.map_or(vec![Value::Missing; 3], |numbers| {
                    numbers.map(Value::Int).to_vec()
                });
                assert_eq!(&row[3..], &expected);
            }
            assert_eq!(source.reads.borrow().len(), 15);
        }

        /// A partition completes eligibility before its first conversion, but later partitions remain lazy.
        #[test]
        fn window_filter_conditions_follow_partition_then_current_conversion_order() {
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("a".into()), Value::Missing],
                    vec![Value::Int(2), Value::Str("b".into()), Value::Int(1)],
                ],
            );
            let plan = numbering_with_filter(
                &source,
                false,
                true,
                Some(missing_or_bad_window_predicate(false)),
                ColumnType::Date,
            );
            // The first partition returns 1; conversion fails before a later group's invalid comparison.
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
                ExecutionError::Conversion { output_row: 0, .. }
            ));
            source.rows[1][1] = Value::Str("a".into());
            // In the same partition, the later filter failure precedes conversion of the first number.
            let ExecutionError::Predicate { error, .. } = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("partition predicate must precede current conversion")
            };
            assert_eq!(error.spec_path, "columns.SEQ.derivation.row_number");
            source.rows.clear();
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

        /// Eager filter evaluation cannot hide a bad operand behind a false left side.
        #[test]
        fn window_filter_boolean_operands_remain_eager() {
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                vec![vec![Value::Int(1), Value::Missing, Value::Int(1)]],
            );
            let plan = numbering_with_filter(
                &source,
                false,
                false,
                Some(missing_or_bad_window_predicate(true)),
                ColumnType::Int,
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
                ExecutionError::Predicate { .. }
            ));
        }

        /// The window filter numbers the eligible rows in order.
        #[test]
        fn window_filter_numbers_the_original_second_eligible_row() {
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Missing, Value::Int(1)],
                    vec![Value::Int(2), Value::Missing, Value::Int(2)],
                ],
            );
            let plan = numbering_with_filter(
                &source,
                false,
                false,
                Some(positive(Read::Column(2))),
                ColumnType::Int,
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
                .rows()[1][3],
                Value::Int(2)
            );
        }

        /// Offsets include missing donors; previous/LOCF cross gaps while retaining every output position.
        #[test]
        fn value_windows_distinguish_neighbor_reads_previous_present_and_current_present() {
            use yamaa_core::dataset::WindowKind;
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                [Some(7), None, None, Some(9), None]
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        vec![
                            Value::Int(i as i64 + 1),
                            Value::Missing,
                            v.map_or(Value::Missing, Value::Int),
                        ]
                    })
                    .collect(),
            );
            for (kind, expected) in [
                (
                    WindowKind::RowValue {
                        column: 2,
                        offset: -1,
                    },
                    vec![None, Some(7), None, None, Some(9)],
                ),
                (
                    WindowKind::RowValue {
                        column: 2,
                        offset: 1,
                    },
                    vec![None, None, Some(9), None, None],
                ),
                (
                    WindowKind::PreviousNonMissing { column: 2 },
                    vec![None, Some(7), Some(7), Some(7), Some(9)],
                ),
                (
                    WindowKind::Locf { column: 2 },
                    vec![Some(7), Some(7), Some(7), Some(9), Some(9)],
                ),
            ] {
                let result = value_window_plan(&source, kind, None)
                    .unwrap()
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
                        .map(|row| row[3].clone())
                        .collect::<Vec<_>>(),
                    expected
                        .into_iter()
                        .map(|v| v.map_or(Value::Missing, Value::Int))
                        .collect::<Vec<_>>()
                );
            }
            assert_eq!(source.reads.borrow().len(), 60); // Value windows only read completed output cells.
        }

        /// Filtering removes donor positions from offsets, without dropping excluded output rows.
        #[test]
        fn value_window_offsets_use_only_eligible_positions_and_keep_excluded_results_missing() {
            use yamaa_core::dataset::WindowKind;
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                [Some(7), Some(8), None, Some(9)]
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        vec![
                            Value::Int(i as i64 + 1),
                            Value::Missing,
                            v.map_or(Value::Missing, Value::Int),
                        ]
                    })
                    .collect(),
            );
            let predicate = check_predicate(
                Node::Compare {
                    operator: Comparison::NotEqual,
                    left: Scalar::Identifier("ID".into()),
                    right: Scalar::Literal(Value::Int(2)),
                },
                vec![binding("ID", Read::Column(0))],
                "columns.RESULT.derivation.row_value",
            );
            let result = value_window_plan(
                &source,
                WindowKind::RowValue {
                    column: 2,
                    offset: -1,
                },
                Some(predicate),
            )
            .unwrap()
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
                    .map(|row| row[3].clone())
                    .collect::<Vec<_>>(),
                vec![
                    Value::Missing,
                    Value::Missing,
                    Value::Int(7),
                    Value::Missing
                ]
            );
        }

        /// Signed offset extremes safely read missing, while zero and unavailable donors fail admission.
        #[test]
        fn value_windows_bound_offsets_and_admit_completed_donors_before_io() {
            use yamaa_core::dataset::WindowKind;
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                vec![vec![Value::Int(1), Value::Missing, Value::Int(7)]],
            );
            for kind in [
                WindowKind::RowValue {
                    column: 2,
                    offset: 0,
                },
                WindowKind::Locf { column: 3 },
                WindowKind::PreviousNonMissing { column: 99 },
            ] {
                assert_eq!(
                    value_window_plan(&source, kind, None),
                    Err(PlanError::InvalidWindow)
                );
            }
            assert!(source.reads.borrow().is_empty());
            for offset in [i64::MIN, i64::MAX] {
                let plan =
                    value_window_plan(&source, WindowKind::RowValue { column: 2, offset }, None)
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
                    .rows()[0][3],
                    Value::Missing
                );
            }
        }

        /// A donor keeps its temporal precision and exact text.
        #[test]
        fn value_window_donors_preserve_temporal_and_unicode_representation() {
            use yamaa_core::dataset::WindowKind;
            let date = Date::new(2024, 1, 1, DatePrecision::Year).unwrap();
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Date),
                ],
                vec![
                    vec![Value::Int(1), Value::Missing, Value::Date(date)],
                    vec![Value::Int(2), Value::Missing, Value::Missing],
                ],
            );
            let result = value_window_plan(&source, WindowKind::Locf { column: 2 }, None)
                .unwrap()
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            let Value::Date(donor) = result.dataset.rows()[1][3] else {
                panic!("retained date donor")
            };
            assert_eq!(donor.collected_precision(), DatePrecision::Year);
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Str),
                ],
                vec![
                    vec![Value::Int(1), Value::Missing, Value::Str("🌱".into())],
                    vec![Value::Int(2), Value::Missing, Value::Missing],
                ],
            );
            let plan = value_window_plan(&source, WindowKind::Locf { column: 2 }, None).unwrap();
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
                .rows()[1][3],
                Value::Str("🌱".into())
            );
        }

        /// Previous-present selection crosses a long run of missing values.
        #[test]
        fn previous_non_missing_crosses_long_gaps() {
            use yamaa_core::dataset::WindowKind;
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("V", ColumnType::Int),
                ],
                (0..5000)
                    .map(|i| {
                        vec![
                            Value::Int(i),
                            Value::Missing,
                            if i == 0 {
                                Value::Int(7)
                            } else {
                                Value::Missing
                            },
                        ]
                    })
                    .collect(),
            );
            let result =
                value_window_plan(&source, WindowKind::PreviousNonMissing { column: 2 }, None)
                    .unwrap()
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap();
            assert_eq!(result.dataset.rows()[0][3], Value::Missing);
            assert!(result.dataset.rows()[1..]
                .iter()
                .all(|row| row[3] == Value::Int(7)));
        }

        /// Baseline compares each candidate to its own reference and skips either missing operand.
        #[test]
        fn baseline_uses_row_specific_reference_and_missing_candidates() {
            let date = |day| Value::Date(Date::new(2025, 1, day, DatePrecision::Day).unwrap());
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("D", ColumnType::Date),
                    ("R", ColumnType::Date),
                ],
                vec![
                    vec![Value::Int(1), Value::Missing, date(1), date(3)],
                    vec![Value::Int(2), Value::Missing, date(3), date(2)],
                    vec![Value::Int(3), Value::Missing, Value::Missing, date(3)],
                    vec![
                        Value::Int(4),
                        Value::Str("b".into()),
                        date(2),
                        Value::Missing,
                    ],
                    vec![Value::Int(5), Value::Str("b".into()), date(2), date(2)],
                    vec![Value::Int(6), Value::Str("b".into()), date(3), date(3)],
                ],
            );
            let result = baseline_plan(&source, vec![1], None)
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
                    .map(|r| r[4].clone())
                    .collect::<Vec<_>>(),
                vec![
                    Value::Str("Y".into()),
                    Value::Missing,
                    Value::Missing,
                    Value::Missing,
                    Value::Missing,
                    Value::Str("Y".into())
                ]
            );
            assert_eq!(source.reads.borrow().len(), 24);
        }

        /// Excluded later rows and excluded ties cannot take a baseline or make it ambiguous.
        #[test]
        fn baseline_filters_exclude_later_rows_and_ties_without_dropping_output() {
            let date = |day| Value::Date(Date::new(2025, 1, day, DatePrecision::Day).unwrap());
            // Original expressions/test_windows.py inputs: excluding January 6 must
            // leave January 2 flagged; excluding its January 6 twin must remove the tie.
            for first in [2, 6] {
                let source = table(
                    &[
                        ("ID", ColumnType::Int),
                        ("ELIGIBLE", ColumnType::Int),
                        ("ADT", ColumnType::Date),
                        ("TRTSDT", ColumnType::Date),
                    ],
                    vec![
                        vec![Value::Int(1), Value::Int(1), date(first), date(10)],
                        vec![Value::Int(2), Value::Int(0), date(6), date(10)],
                    ],
                );
                let result = baseline_plan(&source, vec![], Some(positive(Read::Column(1))))
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
                    &[
                        vec![
                            Value::Int(1),
                            Value::Int(1),
                            date(first),
                            date(10),
                            Value::Str("Y".into()),
                        ],
                        vec![
                            Value::Int(2),
                            Value::Int(0),
                            date(6),
                            date(10),
                            Value::Missing,
                        ],
                    ]
                );
            }
        }

        /// Missing, post-reference and filter-excluded rows never end the partition scan.
        #[test]
        fn baseline_search_keeps_original_candidate_and_row_specific_reference_rules() {
            let date = |day| Value::Date(Date::new(2025, 1, day, DatePrecision::Day).unwrap());
            // Original expressions/test_windows.py cases preserve their January dates
            // and expected flags, including equality with a row's own reference.
            for (pairs, eligible, expected) in [
                (
                    vec![(Some(20), 10), (None, 10)],
                    vec![1, 1],
                    vec![Value::Missing, Value::Missing],
                ),
                (
                    vec![(Some(6), 10), (Some(2), 10)],
                    vec![1, 1],
                    vec![Value::Str("Y".into()), Value::Missing],
                ),
                (
                    vec![(Some(6), 10), (Some(6), 2)],
                    vec![1, 1],
                    vec![Value::Str("Y".into()), Value::Missing],
                ),
                (
                    vec![(Some(20), 10), (None, 10), (Some(6), 10)],
                    vec![1, 1, 1],
                    vec![Value::Missing, Value::Missing, Value::Str("Y".into())],
                ),
                (
                    vec![(Some(20), 10), (None, 10), (Some(6), 10)],
                    vec![0, 1, 1],
                    vec![Value::Missing, Value::Missing, Value::Str("Y".into())],
                ),
                (vec![(Some(6), 6)], vec![1], vec![Value::Str("Y".into())]),
            ] {
                let source = table(
                    &[
                        ("ID", ColumnType::Int),
                        ("ELIGIBLE", ColumnType::Int),
                        ("ADT", ColumnType::Date),
                        ("TRTSDT", ColumnType::Date),
                    ],
                    pairs
                        .into_iter()
                        .zip(eligible)
                        .enumerate()
                        .map(|(index, ((day, reference), eligible))| {
                            vec![
                                Value::Int(index as i64 + 1),
                                Value::Int(eligible),
                                day.map_or(Value::Missing, date),
                                date(reference),
                            ]
                        })
                        .collect(),
                );
                let result = baseline_plan(&source, vec![], Some(positive(Read::Column(1))))
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap();
                assert_eq!(result.dataset.row_count(), expected.len());
                assert_eq!(
                    result
                        .dataset
                        .rows()
                        .iter()
                        .map(|row| row[4].clone())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
        }

        /// Equal represented dates tie despite collected precision; diagnostics identify partitions.
        #[test]
        fn baseline_ties_count_all_matches_and_keep_partition_identity() {
            let year = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
            let day = Date::new(2025, 1, 1, DatePrecision::Day).unwrap();
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("G", ColumnType::Str),
                    ("D", ColumnType::Date),
                    ("R", ColumnType::Date),
                ],
                (1..=3)
                    .map(|id| {
                        vec![
                            Value::Int(id),
                            Value::Str("partition".into()),
                            Value::Date(if id == 1 { year } else { day }),
                            Value::Date(day),
                        ]
                    })
                    .collect(),
            );
            let plan = baseline_plan(&source, vec![1], None);
            let failure = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err();
            match *failure {
                ExecutionError::BaselineAmbiguity {
                    path,
                    column,
                    date: Value::Date(date),
                    match_count,
                    partition,
                } => {
                    assert_eq!(path, "columns.C4.derivation");
                    assert_eq!(column, "BLFL");
                    assert_eq!(date.collected_precision(), DatePrecision::Year);
                    assert_eq!(match_count, 3);
                    assert_eq!(
                        partition,
                        vec![("G".into(), Value::Str("partition".into()))]
                    );
                }
                other => panic!("unexpected {other:?}"),
            }
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
                ExecutionError::BaselineAmbiguity { match_count: 3, .. }
            ));
            assert!(
                matches!(*baseline_plan(&source, vec![], None).execute(&source, &[], &mut yamaa_core::function_invocation::NoFunctions::new(), None).result.unwrap_err(), ExecutionError::BaselineAmbiguity { partition, .. } if partition.is_empty())
            );
        }

        /// Filtering removes bad keys and conflicting values while preserving source memberships.
        #[test]
        fn root_filter_precedes_keys_and_retains_only_qualifying_feeders() {
            let source = table(
                &[
                    ("ID", ColumnType::Str),
                    ("V", ColumnType::Str),
                    ("KEEP", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Str("bad".into()),
                        Value::Str("bad".into()),
                        Value::Int(0),
                    ],
                    vec![
                        Value::Str("02".into()),
                        Value::Str("seven".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Str("conflict".into()),
                        Value::Missing,
                    ],
                    vec![
                        Value::Str("1".into()),
                        Value::Str("eight".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Str("seven".into()),
                        Value::Int(1),
                    ],
                ],
            );
            let plan = root_filtered_plan(&source, positive(Read::Source(2))).unwrap();
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
                &[
                    vec![Value::Int(2), Value::Str("seven".into())],
                    vec![Value::Int(1), Value::Str("eight".into())]
                ]
            );
            let reads = source.reads.borrow();
            assert_eq!(&reads[..5], &[(0, 2), (1, 2), (2, 2), (3, 2), (4, 2)]);
            assert!(!reads.contains(&(0, 0)) && !reads.contains(&(2, 0)));
            assert!(reads.contains(&(1, 1)) && reads.contains(&(3, 1)) && reads.contains(&(4, 1)));
            assert!(!reads.contains(&(2, 1)));
        }

        /// A later predicate error precedes an earlier bad key; false/empty filters skip conversion.
        #[test]
        fn root_filter_completes_before_key_failures_and_recovers() {
            let mut source = table(
                &[
                    ("ID", ColumnType::Str),
                    ("V", ColumnType::Str),
                    ("KEEP", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Str("bad".into()),
                        Value::Str("value".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Str("value".into()),
                        Value::Int(1),
                    ],
                ],
            );
            let plan = root_filtered_plan(&source, positive(Read::Source(2))).unwrap();
            source.fail = Some((1, 2));
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
                ExecutionError::Predicate { source_row: 1, .. }
            ));
            assert_eq!(*source.reads.borrow(), vec![(0, 2), (1, 2)]);
            source.fail = None;
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
                ExecutionError::Conversion { identity: None, .. }
            ));
            let excluded =
                root_filtered_plan(&source, filter(Node::Boolean(false), vec![])).unwrap();
            assert!(excluded
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
            source.rows.clear();
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
            assert!(matches!(
                root_filtered_plan(&source, positive(Read::Column(0))),
                Err(PlanError::Filter(BindingError::UnavailableColumn))
            ));
        }

        /// Source filters retain output keys, exclude conflicting values and finish before reads.
        #[test]
        fn selected_sources_narrow_feeders_without_dropping_rows() {
            let source = table(
                &[
                    ("ID", ColumnType::Str),
                    ("V", ColumnType::Str),
                    ("KEEP", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Str("02".into()),
                        Value::Str("seven".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Str("eight".into()),
                        Value::Int(0),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Str("seven".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("1".into()),
                        Value::Str("nine".into()),
                        Value::Missing,
                    ],
                ],
            );
            let result = selected_source_plan(&source, positive(Read::Source(2)))
                .unwrap()
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
                &[
                    vec![Value::Int(2), Value::Str("seven".into())],
                    vec![Value::Int(1), Value::Missing]
                ]
            );
            assert_eq!(
                *source.reads.borrow(),
                vec![
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (3, 0),
                    (0, 2),
                    (1, 2),
                    (2, 2),
                    (0, 1),
                    (2, 1),
                    (3, 2)
                ]
            );
            let excluded = selected_source_plan(&source, filter(Node::Boolean(false), vec![]))
                .unwrap()
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                excluded.dataset.rows(),
                &[
                    vec![Value::Int(2), Value::Missing],
                    vec![Value::Int(1), Value::Missing]
                ]
            );
            assert!(matches!(
                *selected_source_plan(&source, filter(Node::Boolean(true), vec![]))
                    .unwrap()
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::MultipleValues { value_count: 2, .. }
            ));
            assert!(matches!(
                selected_source_plan(&source, positive(Read::Column(0))),
                Err(PlanError::Filter(BindingError::UnavailableColumn))
            ));
        }

        /// All key conversions precede source predicates; empty inputs do not evaluate them.
        #[test]
        fn selected_source_filter_follows_keys_and_handles_empty_source() {
            let mut source = table(
                &[
                    ("ID", ColumnType::Str),
                    ("V", ColumnType::Str),
                    ("KEEP", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Str("2".into()),
                        Value::Str("seven".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("bad".into()),
                        Value::Str("seven".into()),
                        Value::Int(1),
                    ],
                ],
            );
            let plan = selected_source_plan(&source, positive(Read::Source(2))).unwrap();
            source.fail = Some((0, 2));
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
                ExecutionError::Conversion { identity: None, .. }
            ));
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
            source.fail = None;
            source.rows[1][0] = Value::Str("2".into());
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
                .rows()
                .len(),
                1
            );
            source.rows.clear();
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

        /// A later eligibility condition outranks an earlier donor access failure.
        #[test]
        fn selected_source_finishes_predicates_before_any_donor_access() {
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("P", ColumnType::Str),
                ],
                vec![
                    vec![
                        Value::Int(1),
                        Value::Str("unread".into()),
                        Value::Str("%".into()),
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("unread".into()),
                        Value::Str("!".into()),
                    ],
                ],
            );
            source.fail = Some((0, 1));
            let predicate = filter(
                Node::Like {
                    value: Scalar::Literal(Value::Str("x".into())),
                    pattern: Scalar::Identifier("P".into()),
                    escape: Some('!'),
                    negated: false,
                },
                vec![binding("P", Read::Source(2))],
            );
            let plan = selected_source_plan(&source, predicate).unwrap();
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
                ExecutionError::Predicate { source_row: 1, .. }
            ));
            assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (0, 2), (1, 2)]);
            source.rows[1][2] = Value::Str("%".into());
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
                ExecutionError::Cell { source_row: 0, .. }
            ));
        }

        /// Missing order placement is independent of direction; ties use original source order.
        #[test]
        fn source_order_selects_present_donors_stably_and_counts_actual_choices() {
            use yamaa_core::{dataset::Keep, numeric_lifecycle::HandlerKind};
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Int(1),
                        Value::Str("7".into()),
                        Value::Int(9_007_199_254_740_993),
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("8".into()),
                        Value::Int(9_007_199_254_740_992),
                    ],
                    vec![Value::Int(1), Value::Str("9".into()), Value::Missing],
                    vec![Value::Int(1), Value::Missing, Value::Missing],
                ],
            );
            for (descending, nulls_first, first, last) in [
                (false, false, 8, 9),
                (true, false, 7, 9),
                (false, true, 9, 7),
                (true, true, 9, 8),
            ] {
                for (keep, expected) in [(Keep::First, first), (Keep::Last, last)] {
                    let attempt =
                        ordered_source_plan(&source, keep, descending, nulls_first, false).execute(
                            &source,
                            &[],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None,
                        );
                    assert_eq!(
                        attempt.result.unwrap().dataset.rows(),
                        &[vec![Value::Int(1), Value::Int(expected)]]
                    );
                    assert_eq!(attempt.handler_counts.len(), 1);
                    assert_eq!(
                        attempt.handler_counts[0].spec_path,
                        "columns.C1.derivation.multiple_matches"
                    );
                    assert_eq!(
                        attempt.handler_counts[0].handler,
                        HandlerKind::MultipleMatches
                    );
                    assert_eq!(attempt.handler_counts[0].handler.name(), "multiple_matches");
                    assert_eq!(attempt.handler_counts[0].count, 1);
                }
            }
            let mut repeated = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                source.rows.clone(),
            );
            repeated.rows.extend([
                vec![Value::Int(2), Value::Str("10".into()), Value::Int(0)],
                vec![Value::Int(2), Value::Str("11".into()), Value::Int(1)],
            ]);
            let attempt = ordered_source_plan(&repeated, Keep::Last, false, false, false).execute(
                &repeated,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(attempt.result.unwrap().dataset.rows().len(), 2);
            assert_eq!(attempt.handler_counts.len(), 1);
            assert_eq!(attempt.handler_counts[0].count, 2);
            let tied = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("7".into()), Value::Int(0)],
                    vec![Value::Int(1), Value::Str("8".into()), Value::Int(0)],
                    vec![Value::Int(1), Value::Str("9".into()), Value::Int(0)],
                ],
            );
            for (keep, expected) in [(Keep::First, 7), (Keep::Last, 9)] {
                assert_eq!(
                    ordered_source_plan(&tied, keep, true, true, false)
                        .execute(
                            &tied,
                            &[],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None
                        )
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    &[vec![Value::Int(1), Value::Int(expected)]]
                );
            }
        }
    }
}
