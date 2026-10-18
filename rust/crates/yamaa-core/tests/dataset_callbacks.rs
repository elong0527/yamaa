//! Native dataset assertions.
#[allow(dead_code)]
mod support;
mod execution {
    mod dataset {
        use crate::support::dataset::*;

        /// Equal or absent values bypass all order reads; the declared selection
        /// still reports its zero count, registered before any row.
        #[test]
        fn source_order_is_lazy_for_one_distinct_reading_and_restarts_observations() {
            use yamaa_core::dataset::Keep;
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("7".into()), Value::Int(0)],
                    vec![Value::Int(1), Value::Str("7".into()), Value::Int(1)],
                ],
            );
            let plan = ordered_source_plan(&source, Keep::Last, false, false, false);
            source.fail = Some((0, 2));
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                &[vec![Value::Int(1), Value::Int(7)]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.multiple_matches", 0)]
            );
            assert!(!source.reads.borrow().iter().any(|(_, column)| *column == 2));
            source.rows[1][1] = Value::Str("8".into());
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Cell { source_row: 0, .. }
            ));
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.multiple_matches", 0)]
            );
            source.fail = None;
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .handler_counts[0]
                    .count,
                1
            );
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .handler_counts[0]
                    .count,
                1
            );
            for row in &mut source.rows {
                row[1] = Value::Missing;
            }
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                &[vec![Value::Int(1), Value::Missing]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.multiple_matches", 0)]
            );
            source.rows.clear();
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(attempt.result.unwrap().dataset.rows().is_empty());
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.multiple_matches", 0)]
            );
        }

        /// A completed choice remains observable after its conversion or later execution fails.
        #[test]
        fn source_order_counts_survive_current_and_later_conversion_failures() {
            use yamaa_core::dataset::Keep;
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("7".into()), Value::Int(0)],
                    vec![Value::Int(1), Value::Str("bad".into()), Value::Int(1)],
                ],
            );
            let attempt = ordered_source_plan(&source, Keep::Last, false, false, false).execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(
                matches!(*attempt.result.unwrap_err(), ExecutionError::Conversion {path,identity:Some(_),..} if path=="columns.V")
            );
            assert_eq!(attempt.handler_counts[0].count, 1);
            source.rows[1][1] = Value::Str("8".into());
            let plan = ordered_source_plan(&source, Keep::Last, false, false, true);
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(
                matches!(*attempt.result.unwrap_err(), ExecutionError::Conversion {path,..} if path=="columns.LATER")
            );
            assert_eq!(attempt.handler_counts[0].count, 1);
        }

        /// Ordering ignores representational precision and preserves equal donor representations.
        #[test]
        fn source_order_uses_exact_typed_comparison_and_lazy_representation() {
            use yamaa_core::dataset::{Keep, OrderTerm, SourceSelection};
            for (kind, values) in [
                (
                    ColumnType::Float,
                    vec![Value::float(-0.0), Value::float(0.0)],
                ),
                (
                    ColumnType::Date,
                    vec![
                        Value::Date(Date::new(2024, 1, 1, Precision::Year).unwrap()),
                        Value::Date(Date::new(2024, 1, 1, Precision::Day).unwrap()),
                    ],
                ),
            ] {
                let mut source = table(
                    &[("V", kind), ("ORDER", ColumnType::Int)],
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| vec![v, Value::Int(i as i64)])
                        .collect(),
                );
                source.fail = Some((0, 1));
                let plan = key_plan(
                    &source,
                    schema(&[("ID", ColumnType::Int), ("V", kind)]),
                    vec![0],
                    vec![assign(0, Expression::Literal(Value::Int(1)))],
                    vec![assign(
                        1,
                        Expression::Collect {
                            column: 0,
                            identifier: "SRC.V".into(),
                            filter: None,
                            selection: Some(SourceSelection {
                                order_by: vec![OrderTerm {
                                    column: 1,
                                    descending: false,
                                    nulls_first: false,
                                }],
                                keep: Keep::Last,
                            }),
                        },
                    )],
                );
                let attempt = plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|count| (count.spec_path.as_str(), count.count))
                        .collect::<Vec<_>>(),
                    [("columns.C1.derivation.multiple_matches", 0)]
                );
                let result = attempt.result.unwrap();
                match &result.dataset.rows()[0][1] {
                    Value::Float(v) => assert_eq!(v.get().to_bits(), (-0.0_f64).to_bits()),
                    Value::Date(v) => assert_eq!(v.collected_precision(), Precision::Year),
                    _ => panic!("wrong collected type"),
                }
            }
            // A temporal tie advances to a second Unicode text term, then source position.
            let source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Date),
                    ("TEXT", ColumnType::Str),
                ],
                vec![
                    vec![
                        Value::Int(1),
                        Value::Str("7".into()),
                        Value::Date(Date::new(2024, 1, 1, Precision::Year).unwrap()),
                        Value::Str("é".into()),
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("8".into()),
                        Value::Date(Date::new(2024, 1, 1, Precision::Day).unwrap()),
                        Value::Str("Ω".into()),
                    ],
                ],
            );
            let expression = Expression::Collect {
                column: 1,
                identifier: "SRC.V".into(),
                filter: None,
                selection: Some(SourceSelection {
                    order_by: vec![
                        OrderTerm {
                            column: 2,
                            descending: false,
                            nulls_first: false,
                        },
                        OrderTerm {
                            column: 3,
                            descending: false,
                            nulls_first: false,
                        },
                    ],
                    keep: Keep::Last,
                }),
            };
            let plan = key_plan(
                &source,
                schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
                vec![0],
                vec![assign(0, Expression::Source(0))],
                vec![assign(1, expression)],
            );
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                &[vec![Value::Int(1), Value::Int(8)]]
            );
            assert_eq!(attempt.handler_counts[0].count, 1);
        }

        /// The cached row stays stable, but each reader inherits its own handler accounting.
        #[test]
        fn named_intermediates_cache_records_and_repeat_reading_handlers() {
            let source = table(
                &[("ID", ColumnType::Int)],
                vec![
                    vec![Value::Int(1)],
                    vec![Value::Int(2)],
                    vec![Value::Int(3)],
                ],
            );
            let secondary = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Int(1),
                        Value::Str("7".into()),
                        Value::Int(9007199254740992),
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("8".into()),
                        Value::Int(9007199254740993),
                    ],
                    vec![Value::Int(2), Value::Missing, Value::Int(0)],
                ],
            );
            let mut item = named_item();
            item.filter = Some(positive(Read::Source(0)).into());
            let plan = named_plan(&source, &secondary, item).unwrap();
            for _ in 0..2 {
                secondary.reads.borrow_mut().clear();
                let attempt = plan.execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    &[
                        vec![Value::Int(1), Value::Int(8), Value::Str("8".into())],
                        vec![Value::Int(2), Value::Missing, Value::Missing],
                        vec![Value::Int(3), Value::Missing, Value::Missing],
                    ]
                );
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|count| (count.spec_path.as_str(), count.handler.name(), count.count))
                        .collect::<Vec<_>>(),
                    vec![
                        (
                            "columns.C1.derivation.multiple_matches",
                            "multiple_matches",
                            1
                        ),
                        ("columns.C1.derivation.no_match", "no_match", 1),
                        (
                            "columns.C2.derivation.multiple_matches",
                            "multiple_matches",
                            1
                        ),
                        ("columns.C2.derivation.no_match", "no_match", 1),
                    ]
                );
                let reads = secondary.reads.borrow();
                assert_eq!(reads.iter().filter(|(_, column)| *column == 0).count(), 12);
                assert_eq!(reads.iter().filter(|(_, column)| *column == 2).count(), 2);
                assert_eq!(reads.iter().filter(|(_, column)| *column == 1).count(), 4);
            }
        }

        /// Duplicate identical payloads are records, and every failure retains its prior ledger.
        #[test]
        fn named_intermediates_preserve_join_and_conversion_failure_priority() {
            let source = table(
                &[("ID", ColumnType::Int)],
                vec![vec![Value::Int(1)], vec![Value::Int(2)]],
            );
            let mut secondary = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![
                    vec![Value::Int(1), Value::Str("7".into()), Value::Int(0)],
                    vec![Value::Int(1), Value::Str("7".into()), Value::Int(0)],
                ],
            );
            let mut item = named_item();
            item.selection = None;
            let attempt = named_plan(&source, &secondary, item).unwrap().execute(
                &source,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(
                matches!(*attempt.result.unwrap_err(), ExecutionError::MultipleMatches { intermediate, path, match_count: 2, identity: Some(_), .. } if intermediate == "SELECTED" && path == "intermediates[0]")
            );
            // Both reads registered their zero before the first match failed.
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.no_match", 0),
                    ("columns.C2.derivation.no_match", 0)
                ]
            );
            assert!(secondary
                .reads
                .borrow()
                .iter()
                .all(|(_, column)| *column == 0));
            let mut item = named_item();
            item.no_match = None;
            item.require_match = true;
            let attempt = named_plan(&source, &secondary, item).unwrap().execute(
                &source,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(
                matches!(*attempt.result.unwrap_err(), ExecutionError::UnmatchedKey { matched_key, identity: Some(_), .. } if matched_key == vec![("ID".into(),Value::Int(2))])
            );
            // A read that requires a match registers no `no_match`.
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.multiple_matches", 1),
                    ("columns.C2.derivation.multiple_matches", 0)
                ]
            );
            let mut item = named_item();
            item.no_match = Some(Value::Str("bad".into()));
            let attempt = named_plan(&source, &secondary, item).unwrap().execute(
                &source,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Conversion { output_row: 1, .. }
            ));
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.multiple_matches", 1),
                    ("columns.C1.derivation.no_match", 1),
                    ("columns.C2.derivation.multiple_matches", 0),
                    ("columns.C2.derivation.no_match", 0)
                ]
            );
            secondary.rows[1][1] = Value::Str("bad".into());
            let attempt = named_plan(&source, &secondary, named_item())
                .unwrap()
                .execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Conversion { output_row: 0, .. }
            ));
            assert_eq!(attempt.handler_counts[0].count, 1);
        }

        /// Filtering is lazy globally, finishes before matching and never executes for empty output.
        #[test]
        fn named_intermediate_filter_scope_and_handlers_are_attempt_local() {
            let mut source = table(&[("ID", ColumnType::Int)], vec![vec![Value::Int(99)]]);
            let secondary = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![vec![Value::Int(1), Value::Str("bad".into()), Value::Int(0)]],
            );
            let mut item = named_item();
            item.filter = Some(positive(Read::Source(1)).into());
            let plan = named_plan(&source, &secondary, item).unwrap();
            // The filter narrows matched records only (LOOKUP-8): a record whose
            // key differs is never asked, and a matched one fails the comparison.
            assert!(plan
                .execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            let matched = table(
                &[
                    ("ID", ColumnType::Int),
                    ("V", ColumnType::Str),
                    ("ORDER", ColumnType::Int),
                ],
                vec![vec![
                    Value::Int(99),
                    Value::Str("bad".into()),
                    Value::Int(0),
                ]],
            );
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[&matched],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Predicate { .. }
            ));
            source.rows.clear();
            secondary.reads.borrow_mut().clear();
            let empty = plan.execute(
                &source,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(empty.result.unwrap().dataset.rows().is_empty());
            // An empty output still reports every registered read at zero.
            assert_eq!(
                empty
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.multiple_matches", 0),
                    ("columns.C1.derivation.no_match", 0),
                    ("columns.C2.derivation.multiple_matches", 0),
                    ("columns.C2.derivation.no_match", 0)
                ]
            );
            assert!(secondary.reads.borrow().is_empty());
            source.rows.push(vec![Value::Int(99)]);
            let plan = named_plan(&source, &secondary, named_item()).unwrap();
            assert_eq!(
                plan.execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .handler_counts
                .iter()
                .map(|count| (count.spec_path.as_str(), count.count))
                .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.multiple_matches", 0),
                    ("columns.C1.derivation.no_match", 1),
                    ("columns.C2.derivation.multiple_matches", 0),
                    ("columns.C2.derivation.no_match", 1)
                ]
            );
            let mut item = named_item();
            item.keys[0].value =
                yamaa_core::dataset::MatchValue::Variable(yamaa_core::dataset::MatchRead::Output {
                    column: 1,
                    identifier: "V".into(),
                });
            assert_eq!(
                named_plan(&source, &secondary, item),
                Err(PlanError::InvalidLookup)
            );
            let mut item = named_item();
            item.filter = Some(positive(Read::Column(0)).into());
            assert!(matches!(
                named_plan(&source, &secondary, item),
                Err(PlanError::Filter(_))
            ));
        }

        /// Converted keys reach a secondary relation; no matching record answers missing.
        #[test]
        fn secondary_lookup_uses_completed_keys_and_preserves_missing_reads() {
            let source = table(
                &[("ID", ColumnType::Str)],
                vec![
                    vec![Value::Str("02".into())],
                    vec![Value::Str("1".into())],
                    vec![Value::Str("3".into())],
                ],
            );
            let mut secondary = table(
                &[("ID", ColumnType::Int), ("V", ColumnType::Str)],
                vec![
                    vec![Value::Int(2), Value::Str("7".into())],
                    vec![Value::Int(1), Value::Missing],
                    vec![Value::Missing, Value::Str("99".into())],
                ],
            );
            let plan = secondary_lookup_plan(&source, &secondary).unwrap();
            let attempt = plan.execute(
                &source,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(attempt.handler_counts.is_empty());
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                &[
                    vec![Value::Int(2), Value::Int(7)],
                    vec![Value::Int(1), Value::Missing],
                    vec![Value::Int(3), Value::Missing]
                ]
            );
            assert!(!secondary.reads.borrow().contains(&(2, 1)));
            secondary.rows.clear();
            assert!(plan
                .execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows()
                .iter()
                .all(|row| row[1] == Value::Missing));
        }

        /// Duplicate secondary records disagree about cardinality even when their values agree.
        #[test]
        fn secondary_lookup_counts_records_before_reading_donors() {
            let source = table(
                &[("ID", ColumnType::Str)],
                vec![vec![Value::Str("02".into())]],
            );
            let mut secondary = table(
                &[("ID", ColumnType::Int), ("V", ColumnType::Str)],
                vec![
                    vec![Value::Int(2), Value::Str("7".into())],
                    vec![Value::Int(2), Value::Str("7".into())],
                    vec![Value::Int(2), Value::Missing],
                ],
            );
            secondary.fail = Some((0, 1));
            let plan = secondary_lookup_plan(&source, &secondary).unwrap();
            let attempt = plan.execute(
                &source,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            match *attempt.result.unwrap_err() {
                ExecutionError::MultipleMatches {
                    dataset,
                    match_count,
                    matched_key,
                    identity,
                    ..
                } => {
                    assert_eq!(dataset, "OTHER");
                    assert_eq!(match_count, 3);
                    assert_eq!(matched_key, vec![("ID".into(), Value::Int(2))]);
                    assert_eq!(identity.unwrap().values, vec![Value::Int(2)]);
                }
                error => panic!("wrong failure {error:?}"),
            }
            assert_eq!(*secondary.reads.borrow(), vec![(0, 0), (1, 0), (2, 0)]);
            secondary.rows.truncate(1);
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[&secondary],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::SecondaryCell {
                    source: 0,
                    source_row: 0,
                    ..
                }
            ));
            secondary.fail = None;
            secondary.rows[0][1] = Value::Str("bad".into());
            assert!(
                matches!(*plan.execute(&source, &[&secondary], &mut yamaa_core::function_invocation::NoFunctions::new(), None).result.unwrap_err(),ExecutionError::Conversion {path,identity:Some(_),..} if path=="columns.V")
            );
        }

        /// Source schemas and completed base keys precede secondary field reads.
        #[test]
        fn secondary_lookup_validates_catalog_and_failure_order() {
            let mut source = table(
                &[("ID", ColumnType::Str)],
                vec![
                    vec![Value::Str("02".into())],
                    vec![Value::Str("bad".into())],
                ],
            );
            let mut secondary = table(
                &[("ID", ColumnType::Int), ("V", ColumnType::Str)],
                vec![vec![Value::Int(2), Value::Str("7".into())]],
            );
            secondary.fail = Some((0, 0));
            let plan = secondary_lookup_plan(&source, &secondary).unwrap();
            assert!(matches!(
                *plan
                    .execute(
                        &source,
                        &[&secondary],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Conversion { identity: None, .. }
            ));
            assert!(secondary.reads.borrow().is_empty());
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
                ExecutionError::SchemaMismatch
            ));
            source.rows.pop();
            secondary.fail = None;
            assert!(plan
                .execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            source.rows.clear();
            secondary.fail = Some((0, 0));
            assert!(plan
                .execute(
                    &source,
                    &[&secondary],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows()
                .is_empty());
            let wrong = table(&[("ID", ColumnType::Float), ("V", ColumnType::Str)], vec![]);
            assert!(matches!(
                secondary_lookup_plan(&source, &wrong),
                Err(PlanError::InvalidLookup)
            ));
        }

        /// Replacement finishes before dependents read it, with zero counts and fresh ledgers.
        #[test]
        fn completed_conversion_handlers_preserve_counts_values_and_failures() {
            let source = table(
                &[("ID", ColumnType::Int), ("X", ColumnType::Str)],
                vec![
                    vec![Value::Int(1), Value::Str("bad".into())],
                    vec![Value::Int(2), Value::Str("4".into())],
                ],
            );
            let plan = record_plan(
                &source,
                schema(&[
                    ("ID", ColumnType::Int),
                    ("X", ColumnType::Int),
                    ("COPY", ColumnType::Int),
                ]),
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                    assign(2, Expression::Column(1)),
                ],
                vec![],
            );
            let declarations = vec![
                conversion_handler(2, Value::Int(99)),
                conversion_handler(1, Value::Int(7)),
                conversion_handler(0, Value::Int(0)),
            ];
            let bound = plan
                .clone()
                .with_conversion_handlers(declarations.clone())
                .unwrap();
            for _ in 0..2 {
                let attempt = bound.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    vec![
                        vec![Value::Int(1), Value::Int(7), Value::Int(7)],
                        vec![Value::Int(2), Value::Int(4), Value::Int(4)]
                    ]
                );
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|entry| entry.count)
                        .collect::<Vec<_>>(),
                    vec![0, 1, 0]
                );
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|entry| &entry.spec_path)
                        .collect::<Vec<_>>(),
                    declarations
                        .iter()
                        .map(|entry| &entry.handler.spec_path)
                        .collect::<Vec<_>>()
                );
            }
            let empty = table(&[("ID", ColumnType::Int), ("X", ColumnType::Str)], vec![]);
            let attempt = bound.execute(
                &empty,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(attempt.result.unwrap().dataset.rows().is_empty());
            assert!(attempt.handler_counts.iter().all(|entry| entry.count == 0));
            let missing = plan
                .clone()
                .with_conversion_handlers(vec![conversion_handler(1, Value::Missing)])
                .unwrap()
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            assert_eq!(
                missing.result.unwrap().dataset.rows()[0],
                vec![Value::Int(1), Value::Missing, Value::Missing]
            );
            let bad = plan
                .clone()
                .with_conversion_handlers(vec![conversion_handler(
                    1,
                    Value::Str("still bad".into()),
                )])
                .unwrap()
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            assert!(
                matches!(*bad.result.unwrap_err(),ExecutionError::Conversion { path, identity:Some(_), .. } if path=="columns.C1.derivation.unconvertible")
            );
            assert_eq!(bad.handler_counts[0].count, 1);
            let mut broken = source;
            broken.fail = Some((0, 1));
            let attempt = bound.execute(
                &broken,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Cell { .. }
            ));
            assert!(attempt.handler_counts.iter().all(|entry| entry.count == 0));
        }

        /// Unused handler literals stay lazy; invalid bindings read no source cells.
        #[test]
        fn conversion_handlers_validate_declarations_and_only_recover_reached_values() {
            let source = table(&[("ID", ColumnType::Int)], vec![vec![Value::Int(1)]]);
            let plan = record_plan(
                &source,
                schema(&[("ID", ColumnType::Int)]),
                vec![assign(0, Expression::Source(0))],
                vec![],
            );
            for declarations in [
                vec![conversion_handler(9, Value::Int(0))],
                vec![conversion_handler(0, Value::Int(0)); 2],
            ] {
                assert_eq!(
                    plan.clone().with_conversion_handlers(declarations),
                    Err(PlanError::InvalidConversionHandler)
                );
            }
            assert!(source.reads.borrow().is_empty());

            let bound = plan
                .with_conversion_handlers(vec![conversion_handler(
                    0,
                    Value::Str("large unused replacement".into()),
                )])
                .unwrap();
            assert!(bound
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            let bad = record_plan(
                &source,
                schema(&[("ID", ColumnType::Int)]),
                vec![assign(
                    0,
                    Expression::Literal(Value::Float(
                        yamaa_core::value::FiniteFloat::new(1.5).unwrap(),
                    )),
                )],
                vec![],
            )
            .with_conversion_handlers(vec![conversion_handler(0, Value::Str("0".into()))])
            .unwrap()
            .execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(bad.result.unwrap().dataset.rows(), [vec![Value::Int(0)]]);
            assert_eq!(bad.handler_counts[0].count, 1);
        }

        /// Raw record/group match values remain independent of already converted output keys.
        #[test]
        fn row_lookups_match_driver_fields_and_count_records_before_donor_reads() {
            use yamaa_core::dataset::{RowLookup, RowMatchKey, SecondarySource, SourceSchemas};
            let driver = table(
                &[("ID", ColumnType::Str)],
                vec![vec![Value::Str("02".into())]],
            );
            let mut right = table(
                &[("ID", ColumnType::Str), ("V", ColumnType::Str)],
                vec![
                    vec![Value::Str("02".into()), Value::Str("7".into())],
                    vec![Value::Str("2".into()), Value::Str("99".into())],
                ],
            );
            let binding = RowLookup {
                source: 0,
                column: 1,
                keys: vec![RowMatchKey {
                    source_column: 0,
                    driver_column: 0,
                }],
            };
            let make = |mode, binding| {
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: driver.schema.clone(),
                        secondary: vec![SecondarySource {
                            name: "OTHER".into(),
                            schema: right.schema.clone(),
                        }],
                    },
                    vec![],
                    schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
                    vec![RowTemplate {
                        mode,
                        filter: None,
                        assignments: vec![
                            assign(0, Expression::Literal(Value::Int(99))),
                            assign(1, Expression::RowLookup(binding)),
                        ],
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
            };
            for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
                let plan = make(mode, binding.clone()).unwrap();
                for _ in 0..2 {
                    driver.reads.borrow_mut().clear();
                    right.reads.borrow_mut().clear();
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
                        &[vec![Value::Int(99), Value::Int(7)]]
                    );
                    assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (0, 1)]);
                }
            }
            assert_eq!(
                make(RowMode::Keys, binding.clone()),
                Err(PlanError::InvalidKeyMode)
            );
            let mut invalid = binding.clone();
            invalid.keys[0].driver_column = 9;
            assert_eq!(
                make(RowMode::Records, invalid),
                Err(PlanError::InvalidLookup)
            );
            let plan = make(RowMode::Records, binding).unwrap();
            right.rows.push(right.rows[0].clone());
            right.fail = Some((0, 1));
            right.reads.borrow_mut().clear();
            let failed = plan.execute(
                &driver,
                &[&right],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            match *failed.result.unwrap_err() {
                ExecutionError::MultipleMatches {
                    match_count,
                    matched_key,
                    identity,
                    ..
                } => {
                    assert_eq!(match_count, 2);
                    assert_eq!(matched_key, vec![("ID".into(), Value::Str("02".into()))]);
                    assert_eq!(identity.unwrap().values, vec![Value::Int(99)]);
                }
                error => panic!("unexpected {error:?}"),
            }
            assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0)]);
            right.reads.borrow_mut().clear();
            let missing = table(&[("ID", ColumnType::Str)], vec![vec![Value::Missing]]);
            assert_eq!(
                plan.execute(
                    &missing,
                    &[&right],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[vec![Value::Int(99), Value::Missing]]
            );
            assert!(right.reads.borrow().is_empty());
            let mut failing = driver;
            failing.fail = Some((0, 0));
            assert!(matches!(
                *plan
                    .execute(
                        &failing,
                        &[&right],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::Cell { source_row: 0, .. }
            ));
        }

        /// Placement preserves filtering order without changing raw-key matching or duplicate priority.
        #[test]
        fn row_lookup_assignment_phase_controls_reads_for_discarded_candidates() {
            use yamaa_core::dataset::{RowLookup, RowMatchKey, SecondarySource};
            for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
                let driver = table(
                    &[("ID", ColumnType::Int)],
                    vec![vec![Value::Int(1)], vec![Value::Int(2)]],
                );
                let mut right = table(
                    &[("ID", ColumnType::Int), ("V", ColumnType::Int)],
                    vec![
                        vec![Value::Int(1), Value::Int(10)],
                        vec![Value::Int(1), Value::Int(10)],
                        vec![Value::Int(2), Value::Int(20)],
                    ],
                );
                // A donor access error must not mask ambiguity or reach a discarded row.
                right.fail = Some((0, 1));
                let lookup = assign(
                    1,
                    Expression::RowLookup(RowLookup {
                        source: 0,
                        column: 1,
                        keys: vec![RowMatchKey {
                            source_column: 0,
                            driver_column: 0,
                        }],
                    }),
                );
                for in_template in [true, false] {
                    let mut assignments = vec![assign(0, Expression::Source(0))];
                    let mut columns = vec![];
                    if in_template {
                        assignments.push(lookup.clone());
                    } else {
                        columns.push(lookup.clone());
                    }
                    let plan = DatasetPlan::new_with_sources(
                        driver.schema.clone(),
                        vec![SecondarySource {
                            name: "OTHER".into(),
                            schema: right.schema.clone(),
                        }],
                        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
                        vec![RowTemplate {
                            mode: mode.clone(),
                            assignments,
                            filter: Some(filter(
                                Node::Compare {
                                    operator: Comparison::Greater,
                                    left: Scalar::Identifier("id".into()),
                                    right: Scalar::Literal(Value::Int(1)),
                                },
                                vec![binding("id", Read::Column(0))],
                            )),
                        }],
                        columns,
                        vec![0],
                        vec![],
                    )
                    .unwrap();
                    for _ in 0..2 {
                        right.reads.borrow_mut().clear();
                        let attempt = plan.execute(
                            &driver,
                            &[&right],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None,
                        );
                        if in_template {
                            assert!(matches!(
                                *attempt.result.unwrap_err(),
                                ExecutionError::MultipleMatches { match_count: 2, .. }
                            ));
                            assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0)]);
                        } else {
                            assert_eq!(
                                attempt.result.unwrap().dataset.rows(),
                                &[vec![Value::Int(2), Value::Int(20)]]
                            );
                            // Only the retained candidate scans; its donor is read once.
                            assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0), (2, 1)]);
                        }
                    }
                    if !in_template {
                        // The later phase still rejects ambiguity in a retained candidate.
                        right.rows.push(vec![Value::Int(2), Value::Int(20)]);
                        right.fail = Some((2, 1));
                        right.reads.borrow_mut().clear();
                        let attempt = plan.execute(
                            &driver,
                            &[&right],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None,
                        );
                        assert!(matches!(
                            *attempt.result.unwrap_err(),
                            ExecutionError::MultipleMatches { match_count: 2, .. }
                        ));
                        assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
                    }
                }
            }
        }

        /// Row and template matching uses an independent per-match boundary.
        #[test]
        fn contains_filters_keep_original_rows_and_templates_independent() {
            let subject = "a".repeat(100);
            for (rows, templates) in [(2, 1), (1, 2)] {
                let source = table(
                    &[("id", ColumnType::Str)],
                    vec![vec![Value::Str(subject.clone())]; rows],
                );
                let template = RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![],
                    filter: Some(filter(
                        Node::Contains {
                            value: Scalar::Identifier("x".into()),
                            pattern: "z".into(),
                        },
                        vec![binding("x", Read::Source(0))],
                    )),
                };
                let plan = DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![template; templates],
                    vec![assign(0, Expression::Source(0))],
                    vec![0],
                    vec![],
                )
                .unwrap();
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

        /// Empty-output declarations inspect each literal match independently.
        #[test]
        fn contains_declarations_each_start_a_fresh_match() {
            let source = table(&[("id", ColumnType::Int)], vec![]);
            let checks: Vec<_> = (0..2)
                .map(|index| {
                    verification(Check::Assert {
                        when: None,
                        require: check_predicate(
                            Node::Contains {
                                value: Scalar::Literal(Value::Str("a".repeat(100))),
                                pattern: "z".into(),
                            },
                            vec![],
                            &format!("verifications[{index}].assert.require"),
                        ),
                    })
                })
                .collect();
            let single = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![checks[0].clone()],
            );
            assert_eq!(
                single
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap()
                    .verifications
                    .len(),
                1
            );
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                checks,
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
        }

        #[test]
        fn numeric_reductions_check_present_values_after_collecting_source_arguments() {
            use yamaa_core::table_reduction::TableReductionError;
            use yamaa_core::value::ValueType;
            for reducer in [NumericReducer::Sum, NumericReducer::Mean] {
                let mut source = table(
                    &[("id", ColumnType::Str), ("x", ColumnType::Str)],
                    vec![
                        vec![Value::Str("A".into()), Value::Missing],
                        vec![Value::Str("A".into()), Value::Missing],
                    ],
                );
                let make = |identifier| {
                    DatasetPlan::new(
                        source.schema.clone(),
                        schema(&[("id", ColumnType::Str), ("total", ColumnType::Float)]),
                        vec![RowTemplate {
                            mode: RowMode::Groups(vec![0]),
                            filter: None,
                            assignments: vec![
                                assign(0, Expression::Source(0)),
                                assign(
                                    1,
                                    Expression::Reduce {
                                        identifier,
                                        column: 1,
                                        reducer,
                                        text: format!("{}(T.x)", reducer.name()),
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
                let plan = make(Some("T.x".into()));
                let fallback = make(None);
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
                    &[vec![Value::Str("A".into()), Value::Missing]]
                );
                source.rows[1][1] = Value::Str("2".into());
                // Numeric-looking text is not coerced. The authored expression/operand
                // remain attached to the runtime type finding, with no output identity.
                for (plan, name) in [(&plan, "T.x"), (&fallback, "x")] {
                    assert_eq!(
                        *plan
                            .execute(
                                &source,
                                &[],
                                &mut yamaa_core::function_invocation::NoFunctions::new(),
                                None
                            )
                            .result
                            .unwrap_err(),
                        ExecutionError::ReductionType {
                            path: "columns.C1.derivation".into(),
                            expression: format!("{}(T.x)", reducer.name()),
                            reducer,
                            source: name.into(),
                            actual: ValueType::Str,
                        }
                    );
                }
                source.rows[0][1] = Value::Str("first bad value".into());
                source.fail = Some((1, 1));
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
                    ExecutionError::Reduction {
                        error: TableReductionError::Cell {
                            position: 1,
                            error: CellError::Access("source failure")
                        },
                        ..
                    }
                ));
                source.fail = None;
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
        }

        #[test]
        fn first_available_skips_unselected_reads_and_does_not_fallback_after_conversion() {
            use yamaa_core::dataset::{FirstAvailable, SelectionRead, SelectionSource};
            let mut source = table(
                &[
                    ("ID", ColumnType::Int),
                    ("A", ColumnType::Str),
                    ("B", ColumnType::Str),
                ],
                vec![
                    vec![
                        Value::Int(1),
                        Value::Str("bad".into()),
                        Value::Str("20".into()),
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("other".into()),
                        Value::Str("20".into()),
                    ],
                ],
            );
            let make = |columns: &[usize]| {
                key_plan(
                    &source,
                    schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
                    vec![0],
                    vec![assign(0, Expression::Source(0))],
                    vec![assign(
                        1,
                        Expression::FirstAvailable(Box::new(FirstAvailable::new(
                            "columns.V.derivation.first_available".into(),
                            columns
                                .iter()
                                .map(|&column| SelectionSource {
                                    path: format!("operand.{column}"),
                                    read: SelectionRead::Collect {
                                        column,
                                        identifier: format!("SRC.C{column}"),
                                        filter: None,
                                    },
                                })
                                .collect(),
                            Value::Int(99),
                        ))),
                    )],
                )
            };
            let skipped = make(&[2, 1]);
            let chosen = make(&[1, 2]);
            source.fail = Some((0, 1));
            assert_eq!(
                skipped
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
                &[vec![Value::Int(1), Value::Int(20)]]
            );
            assert!(source.reads.borrow().iter().all(|&(_, column)| column != 1));
            source.fail = None;
            source.rows[1][1] = Value::Str("bad".into());
            source.reads.borrow_mut().clear();
            assert!(
                matches!(*chosen.execute(&source, &[], &mut yamaa_core::function_invocation::NoFunctions::new(), None).result.unwrap_err(),ExecutionError::Conversion { ref path, output_row:0, .. } if path=="columns.V")
            );
            assert!(source.reads.borrow().iter().all(|&(_, column)| column != 2));
        }
        mod dataset_functions {
            use crate::support::dataset::dataset_functions::*;

            /// Phase observation must not repeat source reads or callbacks or expose partial output.
            #[test]
            fn phase_observation_preserves_execution_and_effects() {
                use yamaa_core::dataset::ExecutionPhase::*;
                let source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(2), Value::Missing, Value::Int(30)],
                ]);
                let plan = plan(
                    &source,
                    RowMode::Records,
                    true,
                    None,
                    signature(ColumnType::Int),
                    arguments(),
                )
                .unwrap();
                let mut calls = callbacks();
                let mut phases = Vec::new();
                let measured = plan.execute(
                    &source,
                    &[],
                    &mut calls,
                    Some(&mut |event| {
                        if let yamaa_core::dataset::ExecutionEvent::Phase(phase) = event {
                            phases.push((phase, source.reads.borrow().len()));
                        }
                    }),
                );
                assert_eq!(
                    phases,
                    vec![
                        (Admission, 0),
                        (Derivation, 0),
                        (OutputKeys, 6),
                        (Verification, 6),
                        (Finished, 6)
                    ]
                );
                assert_eq!(
                    measured.result.as_ref().unwrap().dataset.rows(),
                    &[
                        vec![Value::Int(1), Value::Int(11)],
                        vec![Value::Int(2), Value::Missing]
                    ]
                );
                assert_eq!(calls.calls.len(), 1);
                let reads = source.reads.borrow().clone();
                assert_eq!(reads, vec![(0, 0), (0, 2), (0, 1), (1, 0), (1, 2), (1, 1)]);
                source.reads.borrow_mut().clear();
                let mut plain_calls = callbacks();
                let plain = plan.execute(&source, &[], &mut plain_calls, None);
                assert_eq!(measured, plain);
                assert_eq!(calls.calls, plain_calls.calls);
                assert_eq!(*source.reads.borrow(), reads);
            }

            #[test]
            fn typed_execution_exits_close_actual_success_and_each_failed_stage_before_finished() {
                use yamaa_core::application_observer::Outcome;
                use yamaa_core::dataset::{ExecutionEvent, ExecutionPhase::*};
                for failure in [
                    None,
                    Some(Admission),
                    Some(Derivation),
                    Some(OutputKeys),
                    Some(Verification),
                ] {
                    let mut source =
                        source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
                    let plan = record_plan(
                        &source,
                        schema(&[("ID", ColumnType::Int)]),
                        vec![assign(0, Expression::Source(0))],
                        vec![verification(Check::RowCount {
                            min: None,
                            max: Some(if failure == Some(Verification) { 0 } else { 1 }),
                        })],
                    );
                    match failure {
                        Some(Admission) => source.schema = schema(&[("OTHER", ColumnType::Int)]),
                        Some(Derivation) => source.fail = Some((0, 0)),
                        Some(OutputKeys) => source.rows[0][0] = Value::Missing,
                        _ => (),
                    }
                    let mut events = Vec::new();
                    let attempt = plan.execute(
                        &source,
                        &[],
                        &mut callbacks(),
                        Some(&mut |event| events.push(event)),
                    );
                    assert_eq!(attempt.result.is_ok(), failure.is_none());
                    let mut expected = Vec::new();
                    for phase in [Admission, Derivation, OutputKeys, Verification] {
                        expected.push(ExecutionEvent::Phase(phase));
                        expected.push(ExecutionEvent::Exit {
                            phase,
                            outcome: if failure == Some(phase) {
                                Outcome::Failed
                            } else {
                                Outcome::Succeeded
                            },
                        });
                        if failure == Some(phase) {
                            break;
                        }
                    }
                    expected.push(ExecutionEvent::Phase(Finished));
                    assert_eq!(events, expected);
                    assert_eq!(source.reads.borrow().is_empty(), failure == Some(Admission));
                }
            }

            /// Every returned failure closes the reached phase without inventing later work.
            #[test]
            fn phase_observation_finishes_at_the_actual_failure_boundary() {
                use yamaa_core::dataset::ExecutionPhase::*;
                for failure in [Admission, Derivation, OutputKeys, Verification] {
                    let mut source =
                        source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
                    let plan = record_plan(
                        &source,
                        schema(&[("ID", ColumnType::Int)]),
                        vec![assign(0, Expression::Source(0))],
                        vec![verification(Check::RowCount {
                            min: None,
                            max: Some(0),
                        })],
                    );
                    match failure {
                        Admission => source.schema = schema(&[("OTHER", ColumnType::Int)]),
                        Derivation => source.fail = Some((0, 0)),
                        OutputKeys => source.rows[0][0] = Value::Missing,
                        Verification => (),
                        Finished => unreachable!(),
                    }
                    let mut phases = Vec::new();
                    let measured = plan.execute(
                        &source,
                        &[],
                        &mut callbacks(),
                        Some(&mut |event| {
                            if let yamaa_core::dataset::ExecutionEvent::Phase(phase) = event {
                                phases.push(phase);
                            }
                        }),
                    );
                    let error = measured.result.unwrap_err();
                    assert!(match failure {
                        Admission => matches!(*error, ExecutionError::SchemaMismatch),
                        Derivation => matches!(
                            *error,
                            ExecutionError::Cell {
                                error: CellError::Access("source failure"),
                                ..
                            }
                        ),
                        OutputKeys => matches!(*error, ExecutionError::KeyFailures(_)),
                        Verification => matches!(*error, ExecutionError::VerificationFailures(_)),
                        Finished => false,
                    });
                    let mut expected = vec![Admission, Derivation, OutputKeys, Verification];
                    expected
                        .truncate(expected.iter().position(|phase| *phase == failure).unwrap() + 1);
                    expected.push(Finished);
                    assert_eq!(phases, expected);
                }
            }

            /// Authored reads precede declaration-order checks/defaults and each attempt is fresh.
            #[test]
            fn authored_reads_and_mapped_calls_preserve_distinct_orders() {
                let source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(2), Value::Missing, Value::Int(30)],
                ]);
                let plan = plan(
                    &source,
                    RowMode::Records,
                    true,
                    None,
                    signature(ColumnType::Int),
                    arguments(),
                )
                .unwrap();
                let mut callbacks = callbacks();
                for count in 1..=2 {
                    source.reads.borrow_mut().clear();
                    let result = plan
                        .execute(&source, &[], &mut callbacks, None)
                        .result
                        .unwrap();
                    assert_eq!(
                        result.dataset.rows(),
                        &[
                            vec![Value::Int(1), Value::Int(11)],
                            vec![Value::Int(2), Value::Missing]
                        ]
                    );
                    assert_eq!(
                        *source.reads.borrow(),
                        vec![(0, 0), (0, 2), (0, 1), (1, 0), (1, 2), (1, 1)]
                    );
                    assert_eq!(callbacks.calls.len(), count);
                    assert_eq!(
                        callbacks.calls[count - 1],
                        vec![
                            ("lhs".into(), "Int(10)".into()),
                            ("rhs".into(), "Int(20)".into()),
                            ("scale".into(), "Int(5)".into())
                        ]
                    );
                }
            }

            /// Missing arguments never hide a later authored resolution error or trigger a callback.
            #[test]
            fn missing_does_not_skip_remaining_argument_resolution() {
                let mut source = source(vec![vec![Value::Int(1), Value::Missing, Value::Int(20)]]);
                source.fail = Some((0, 2));
                let mut args = arguments();
                args.reverse();
                let plan = plan(
                    &source,
                    RowMode::Records,
                    true,
                    None,
                    signature(ColumnType::Int),
                    args,
                )
                .unwrap();
                let mut callbacks = callbacks();
                let error = plan
                    .execute(&source, &[], &mut callbacks, None)
                    .result
                    .unwrap_err();
                assert!(matches!(
                    *error,
                    ExecutionError::Cell {
                        source_row: 0,
                        error: CellError::Access("source failure"),
                        ..
                    }
                ));
                assert_eq!(*source.reads.borrow(), vec![(0, 0), (0, 1), (0, 2)]);
                assert!(callbacks.calls.is_empty());
            }

            /// Row callbacks precede filtering, whereas column callbacks run only on retained rows.
            #[test]
            fn callback_assignment_phase_and_empty_input_control_effects() {
                let source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
                for row_call in [true, false] {
                    let plan = plan(
                        &source,
                        RowMode::Records,
                        row_call,
                        Some(filter(Node::Boolean(false), vec![])),
                        signature(ColumnType::Int),
                        arguments(),
                    )
                    .unwrap();
                    let mut callbacks = callbacks();
                    assert!(plan
                        .execute(&source, &[], &mut callbacks, None)
                        .result
                        .unwrap()
                        .dataset
                        .rows()
                        .is_empty());
                    assert_eq!(callbacks.calls.len(), usize::from(row_call));
                    let empty = table(
                        &[
                            ("ID", ColumnType::Int),
                            ("A", ColumnType::Int),
                            ("B", ColumnType::Int),
                        ],
                        vec![],
                    );
                    assert!(plan
                        .execute(&empty, &[], &mut callbacks, None)
                        .result
                        .unwrap()
                        .dataset
                        .rows()
                        .is_empty());
                    assert_eq!(callbacks.calls.len(), usize::from(row_call));
                }
            }

            /// Signature mismatches and legacy entrypoints refuse functions before any table access.
            #[test]
            fn all_function_bindings_are_required_before_table_access() {
                struct Unreadable;
                impl TableAccess for Unreadable {
                    type Error = &'static str;
                    fn schema(&self) -> &TableSchema {
                        panic!("schema read before binding admission")
                    }
                    fn row_count(&self) -> usize {
                        panic!("row count read before binding admission")
                    }
                    fn cell(
                        &self,
                        _: usize,
                        _: usize,
                    ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                        panic!("cell read before binding admission")
                    }
                }
                let source = source(vec![]);
                let plan = plan(
                    &source,
                    RowMode::Records,
                    false,
                    None,
                    signature(ColumnType::Int),
                    arguments(),
                )
                .unwrap();
                assert!(matches!(
                    *plan
                        .execute(
                            &Unreadable,
                            &[],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None
                        )
                        .result
                        .unwrap_err(),
                    ExecutionError::FunctionBinding { slot: 0 }
                ));
                let mut callbacks = callbacks();
                callbacks.signature = signature(ColumnType::Str);
                assert!(matches!(
                    *plan
                        .execute(&Unreadable, &[], &mut callbacks, None)
                        .result
                        .unwrap_err(),
                    ExecutionError::FunctionBinding { slot: 0 }
                ));
                assert!(callbacks.calls.is_empty());
            }

            /// Fatal callback and result failures retain provenance and bypass conversion handlers.
            #[test]
            fn invocation_failures_are_fatal_and_do_not_retry_or_recover() {
                let source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(2), Value::Int(30), Value::Int(40)],
                ]);
                let plan = plan(
                    &source,
                    RowMode::Records,
                    false,
                    None,
                    signature(ColumnType::Int),
                    arguments(),
                )
                .unwrap()
                .with_conversion_handlers(vec![conversion_handler(1, Value::Int(9))])
                .unwrap();
                for (result, expected) in [
                    (Err("callback payload"), "function_call_failed"),
                    (Ok(Value::Bool(true)), "invalid_function_result"),
                    (Ok(Value::Missing), "invalid_function_result"),
                ] {
                    let mut callbacks = callbacks();
                    callbacks.result = result;
                    let attempt = plan.execute(&source, &[], &mut callbacks, None);
                    match *attempt.result.unwrap_err() {
                        ExecutionError::Function {
                            path,
                            error,
                            identity,
                        } => {
                            assert_eq!(path, "columns.C1.derivation");
                            assert_eq!(error.condition(), expected);
                            assert_eq!(error.identity, callbacks.signature.identity().clone());
                            assert_eq!(identity.unwrap().values, vec![Value::Int(1)]);
                            if expected == "function_call_failed" {
                                assert_eq!(error.kind, FailureKind::CallFailed("callback payload"));
                            }
                        }
                        other => panic!("unexpected {other:?}"),
                    }
                    assert_eq!(callbacks.calls.len(), 1);
                    assert_eq!(attempt.handler_counts[0].count, 0);
                }
                let mut callbacks = callbacks();
                callbacks.signature = signature(ColumnType::Str);
                callbacks.result = Ok(Value::Str("bad".into()));
                let converted = super::dataset_functions::plan(
                    &source,
                    RowMode::Records,
                    false,
                    None,
                    signature(ColumnType::Str),
                    arguments(),
                )
                .unwrap()
                .with_conversion_handlers(vec![conversion_handler(1, Value::Int(9))])
                .unwrap();
                let attempt = converted.execute(&source, &[], &mut callbacks, None);
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    &[
                        vec![Value::Int(1), Value::Int(9)],
                        vec![Value::Int(2), Value::Int(9)]
                    ]
                );
                assert_eq!(attempt.handler_counts[0].count, 2);
                assert_eq!(callbacks.calls.len(), 2);
            }

            /// Phase and name admission are independent of data cardinality and callback presence.
            #[test]
            fn function_bindings_validate_names_dependencies_and_source_scope() {
                let source = source(vec![]);
                assert_eq!(
                    BoundFunction::new(0, signature(ColumnType::Int), vec![]),
                    Err(PlanError::InvalidFunction)
                );
                let mut args = arguments();
                args.push(args[0].clone());
                assert_eq!(
                    BoundFunction::new(0, signature(ColumnType::Int), args),
                    Err(PlanError::InvalidFunction)
                );
                let mut args = arguments();
                args[0].name = "unknown".into();
                assert_eq!(
                    BoundFunction::new(0, signature(ColumnType::Int), args),
                    Err(PlanError::InvalidFunction)
                );
                assert_eq!(
                    plan(
                        &source,
                        RowMode::Groups(vec![0]),
                        true,
                        None,
                        signature(ColumnType::Int),
                        arguments()
                    ),
                    Err(PlanError::NonGroupSource)
                );
                assert_eq!(
                    plan(
                        &source,
                        RowMode::Keys,
                        true,
                        None,
                        signature(ColumnType::Int),
                        arguments()
                    ),
                    Err(PlanError::InvalidKeyMode)
                );
                assert_eq!(
                    plan(
                        &source,
                        RowMode::Keys,
                        false,
                        None,
                        signature(ColumnType::Int),
                        arguments()
                    ),
                    Err(PlanError::InvalidKeyMode)
                );
                let mut args = arguments();
                args[0].input = FunctionInput::Read(Read::Column(1));
                assert_eq!(
                    plan(
                        &source,
                        RowMode::Records,
                        true,
                        None,
                        signature(ColumnType::Int),
                        args
                    ),
                    Err(PlanError::UnavailableColumn)
                );
            }

            /// Grouped calls run once per group; key-grain calls consume completed outputs only.
            #[test]
            fn group_and_key_grain_calls_use_logical_candidates() {
                let source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(2), Value::Int(30), Value::Int(40)],
                ]);
                let grouped = plan(
                    &source,
                    RowMode::Groups(vec![0, 1, 2]),
                    true,
                    None,
                    signature(ColumnType::Int),
                    arguments(),
                )
                .unwrap();
                let mut calls = callbacks();
                assert_eq!(
                    grouped
                        .execute(&source, &[], &mut calls, None)
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    &[
                        vec![Value::Int(1), Value::Int(11)],
                        vec![Value::Int(2), Value::Int(11)]
                    ]
                );
                assert_eq!(calls.calls.len(), 2);
                let args = vec![
                    FunctionArgument {
                        name: "first".into(),
                        input: FunctionInput::Read(Read::Column(0)),
                    },
                    FunctionArgument {
                        name: "second".into(),
                        input: FunctionInput::Literal(Value::Int(3)),
                    },
                ];
                let keyed = plan(
                    &source,
                    RowMode::Keys,
                    false,
                    None,
                    signature(ColumnType::Int),
                    args,
                )
                .unwrap();
                calls.calls.clear();
                assert_eq!(
                    keyed
                        .execute(&source, &[], &mut calls, None)
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    &[
                        vec![Value::Int(1), Value::Int(11)],
                        vec![Value::Int(2), Value::Int(11)]
                    ]
                );
                assert_eq!(calls.calls.len(), 2);
                assert_eq!(calls.calls[0][0].1, "Int(1)");
                assert_eq!(calls.calls[1][0].1, "Int(2)");
            }

            /// Whole-column traversal finishes conversion before dependent callbacks see a value.
            #[test]
            fn later_function_columns_observe_converted_values_in_column_order() {
                let source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(2), Value::Int(30), Value::Int(40)],
                ]);
                let args = |first, second| {
                    vec![
                        FunctionArgument {
                            name: "first".into(),
                            input: FunctionInput::Read(Read::Column(first)),
                        },
                        FunctionArgument {
                            name: "second".into(),
                            input: FunctionInput::Read(Read::Column(second)),
                        },
                    ]
                };
                let plan = DatasetPlan::new(
                    source.schema.clone(),
                    schema(&[
                        ("ID", ColumnType::Int),
                        ("A", ColumnType::Int),
                        ("B", ColumnType::Int),
                    ]),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![assign(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![
                        assign(
                            1,
                            Expression::Function(
                                BoundFunction::new(0, signature(ColumnType::Str), args(0, 0))
                                    .unwrap(),
                            ),
                        ),
                        assign(
                            2,
                            Expression::Function(
                                BoundFunction::new(0, signature(ColumnType::Str), args(1, 0))
                                    .unwrap(),
                            ),
                        ),
                    ],
                    vec![0],
                    vec![],
                )
                .unwrap();
                let mut calls = callbacks();
                calls.signature = signature(ColumnType::Str);
                calls.result = Ok(Value::Str("17".into()));
                assert_eq!(
                    plan.execute(&source, &[], &mut calls, None)
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    &[
                        vec![Value::Int(1), Value::Int(17), Value::Int(17)],
                        vec![Value::Int(2), Value::Int(17), Value::Int(17)]
                    ]
                );
                let observed: Vec<_> = calls
                    .calls
                    .iter()
                    .map(|a| (a[0].1.as_str(), a[1].1.as_str()))
                    .collect();
                assert_eq!(
                    observed,
                    vec![
                        ("Int(1)", "Int(1)"),
                        ("Int(2)", "Int(2)"),
                        ("Int(17)", "Int(1)"),
                        ("Int(17)", "Int(2)")
                    ]
                );
            }

            /// Equal donors collapse without changing the first representation or callback order.
            #[test]
            fn collected_arguments_use_all_feeders_and_preserve_missing() {
                let source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(2), Value::Missing, Value::Int(30)],
                ]);
                let plan = plan(
                    &source,
                    RowMode::Keys,
                    false,
                    None,
                    signature(ColumnType::Int),
                    collected_arguments(),
                )
                .unwrap();
                for _ in 0..2 {
                    source.reads.borrow_mut().clear();
                    let mut calls = callbacks();
                    let result = plan.execute(&source, &[], &mut calls, None).result.unwrap();
                    assert_eq!(
                        result.dataset.rows(),
                        &[
                            vec![Value::Int(1), Value::Int(11)],
                            vec![Value::Int(2), Value::Missing]
                        ]
                    );
                    assert_eq!(
                        calls.calls,
                        vec![vec![
                            ("lhs".into(), "Int(10)".into()),
                            ("rhs".into(), "Int(20)".into()),
                            ("scale".into(), "Int(5)".into())
                        ]]
                    );
                    assert_eq!(
                        *source.reads.borrow(),
                        vec![
                            (0, 0),
                            (1, 0),
                            (2, 0),
                            (0, 2),
                            (1, 2),
                            (0, 1),
                            (1, 1),
                            (2, 2),
                            (2, 1)
                        ]
                    );
                }
            }

            /// Missing earlier arguments cannot suppress later ambiguity or opaque source failures.
            #[test]
            fn collected_argument_resolution_finishes_before_missing_short_circuit() {
                let mut source = source(vec![
                    vec![Value::Int(1), Value::Int(10), Value::Missing],
                    vec![Value::Int(1), Value::Int(11), Value::Missing],
                    vec![Value::Int(1), Value::Int(12), Value::Missing],
                ]);
                let plan = plan(
                    &source,
                    RowMode::Keys,
                    false,
                    None,
                    signature(ColumnType::Int),
                    collected_arguments(),
                )
                .unwrap();
                let mut calls = callbacks();
                let error = plan
                    .execute(&source, &[], &mut calls, None)
                    .result
                    .unwrap_err();
                match *error {
                    ExecutionError::MultipleValues {
                        path,
                        identifier,
                        value_count,
                        identity,
                    } => {
                        assert_eq!(path, "columns.C1.derivation");
                        assert_eq!(identifier, "SOURCE.A");
                        assert_eq!(value_count, 3);
                        assert_eq!(identity.unwrap().values, vec![Value::Int(1)]);
                    }
                    other => panic!("unexpected error {other:?}"),
                }
                assert!(calls.calls.is_empty());
                assert_eq!(
                    *source.reads.borrow(),
                    vec![
                        (0, 0),
                        (1, 0),
                        (2, 0),
                        (0, 2),
                        (1, 2),
                        (2, 2),
                        (0, 1),
                        (1, 1),
                        (2, 1)
                    ]
                );
                source.fail = Some((2, 1));
                source.reads.borrow_mut().clear();
                assert!(matches!(
                    *plan
                        .execute(&source, &[], &mut calls, None)
                        .result
                        .unwrap_err(),
                    ExecutionError::Cell {
                        source_row: 2,
                        error: CellError::Access("source failure"),
                        ..
                    }
                ));
                assert!(calls.calls.is_empty());
            }

            /// Collected leaves are only legal after key construction, with bounded valid metadata.
            #[test]
            fn collected_argument_admission_precedes_source_effects() {
                let source = source(vec![]);
                for mode in [RowMode::Records, RowMode::Groups(vec![0, 1, 2])] {
                    assert_eq!(
                        plan(
                            &source,
                            mode,
                            false,
                            None,
                            signature(ColumnType::Int),
                            collected_arguments()
                        ),
                        Err(PlanError::InvalidKeyMode)
                    );
                }
                for (column, identifier, expected) in [
                    (99, "SOURCE.X", PlanError::InvalidSource),
                    (1, "", PlanError::InvalidKeyMode),
                ] {
                    let mut args = collected_arguments();
                    args[0].input = FunctionInput::Collect {
                        column,
                        identifier: identifier.into(),
                    };
                    assert_eq!(
                        plan(
                            &source,
                            RowMode::Keys,
                            false,
                            None,
                            signature(ColumnType::Int),
                            args
                        ),
                        Err(expected)
                    );
                }
                assert!(source.reads.borrow().is_empty());
            }

            /// Collected arguments call once; an empty source creates no callback attempt.
            #[test]
            fn collected_arguments_call_once_and_empty_sources_do_not_call() {
                let source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]; 2]);
                let plan = plan(
                    &source,
                    RowMode::Keys,
                    false,
                    None,
                    signature(ColumnType::Int),
                    collected_arguments(),
                )
                .unwrap();
                let mut calls = callbacks();
                assert!(plan.execute(&source, &[], &mut calls, None).result.is_ok());
                assert_eq!(calls.calls.len(), 1);

                let mut text_source = table(
                    &[
                        ("ID", ColumnType::Int),
                        ("A", ColumnType::Int),
                        ("B", ColumnType::Str),
                    ],
                    vec![vec![Value::Int(1), Value::Int(10), Value::Str("large".into())]; 2],
                );
                let text_plan = super::dataset_functions::plan(
                    &text_source,
                    RowMode::Keys,
                    false,
                    None,
                    signature(ColumnType::Int),
                    collected_arguments(),
                )
                .unwrap();
                calls.calls.clear();
                text_source.rows.clear();
                assert!(text_plan
                    .execute(&text_source, &[], &mut calls, None)
                    .result
                    .is_ok());
                assert!(calls.calls.is_empty());
            }
        }
    }
}
