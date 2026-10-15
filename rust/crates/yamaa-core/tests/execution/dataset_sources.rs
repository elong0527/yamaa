//! Independent ordering, admission, failure and committed benchmark observations.
use crate::support::table_schema as schema;
use yamaa_core::{
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value},
};

use yamaa_core::dataset::{
    DatasetPlan, ExecutionError, Expression, Limits, PlanError, RowMode, RowTemplate,
};

use super::dataset::*;
use super::dataset_filters::*;
use yamaa_core::bound_expression::{BindingError, BoundPredicate as Filter, Read};
use yamaa_core::predicate::{Node, Scalar};

/// Bind a source-only root predicate before standalone key construction.
pub(crate) fn root_filtered_plan(
    source: &Table,
    predicate: Filter,
) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        source.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Str)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: Some(predicate),
        }],
        vec![assign(1, collect(1))],
        vec![0],
        vec![],
    )
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
    let result = plan.execute(&source, limits()).unwrap();
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Predicate { source_row: 1, .. }
    ));
    assert_eq!(*source.reads.borrow(), vec![(0, 2), (1, 2)]);
    source.fail = None;
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Conversion { identity: None, .. }
    ));
    let excluded = root_filtered_plan(&source, filter(Node::Boolean(false), vec![])).unwrap();
    assert!(excluded
        .execute(&source, limits())
        .unwrap()
        .dataset
        .rows()
        .is_empty());
    source.rows.clear();
    assert!(plan
        .execute(&source, limits())
        .unwrap()
        .dataset
        .rows()
        .is_empty());
    assert!(matches!(
        root_filtered_plan(&source, positive(Read::Column(0))),
        Err(PlanError::Filter(BindingError::UnavailableColumn))
    ));
}

/// Admit a source-only filter inside one collected non-key reading.
pub(crate) fn selected_source_plan(
    source: &Table,
    predicate: Filter,
) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        source.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Str)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "SRC.V".into(),
                filter: Some(predicate),
                selection: None,
            },
        )],
        vec![0],
        vec![],
    )
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
        .execute(&source, limits())
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
        .execute(&source, limits())
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
            .execute(&source, limits())
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
fn selected_source_filter_follows_keys_and_reuses_fresh_limits() {
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Conversion { identity: None, .. }
    ));
    assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
    source.fail = None;
    source.rows[1][0] = Value::Str("2".into());
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    work_cells: 1,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit { .. }
    ));
    assert_eq!(
        plan.execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .len(),
        1
    );
    source.rows.clear();
    assert!(plan
        .execute(&source, limits())
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Predicate { source_row: 1, .. }
    ));
    assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (0, 2), (1, 2)]);
    source.rows[1][2] = Value::Str("%".into());
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Cell { source_row: 0, .. }
    ));
}

/// Bind one source choice without changing output identity or conversion ownership.
pub(crate) fn ordered_source_plan(
    source: &Table,
    keep: yamaa_core::dataset::Keep,
    descending: bool,
    nulls_first: bool,
    later_failure: bool,
) -> DatasetPlan {
    use yamaa_core::dataset::{OrderTerm, SourceSelection};
    let mut columns = vec![assign(
        1,
        Expression::Collect {
            column: 1,
            identifier: "SRC.V".into(),
            filter: None,
            selection: Some(SourceSelection {
                order_by: vec![OrderTerm {
                    column: 2,
                    descending,
                    nulls_first,
                }],
                keep,
            }),
        },
    )];
    let mut fields = vec![("ID", ColumnType::Int), ("V", ColumnType::Int)];
    if later_failure {
        fields.push(("LATER", ColumnType::Int));
        columns.push(assign(2, Expression::Literal(Value::Str("bad".into()))));
    }
    key_plan(
        source,
        schema(&fields),
        vec![0],
        vec![assign(0, Expression::Source(0))],
        columns,
    )
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
            let attempt = ordered_source_plan(&source, keep, descending, nulls_first, false)
                .execute_observed(&source, limits());
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
    let attempt = ordered_source_plan(&repeated, Keep::Last, false, false, false)
        .execute_observed(&repeated, limits());
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
                .execute(&tied, limits())
                .unwrap()
                .dataset
                .rows(),
            &[vec![Value::Int(1), Value::Int(expected)]]
        );
    }
}

/// Equal or absent values bypass all order reads and do not register a zero count.
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
    let attempt = plan.execute_observed(&source, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        &[vec![Value::Int(1), Value::Int(7)]]
    );
    assert!(attempt.handler_counts.is_empty());
    assert!(!source.reads.borrow().iter().any(|(_, column)| *column == 2));
    source.rows[1][1] = Value::Str("8".into());
    let attempt = plan.execute_observed(&source, limits());
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Cell { source_row: 0, .. }
    ));
    assert!(attempt.handler_counts.is_empty());
    source.fail = None;
    assert_eq!(
        plan.execute_observed(&source, limits()).handler_counts[0].count,
        1
    );
    assert_eq!(
        plan.execute_observed(&source, limits()).handler_counts[0].count,
        1
    );
    for row in &mut source.rows {
        row[1] = Value::Missing;
    }
    let attempt = plan.execute_observed(&source, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        &[vec![Value::Int(1), Value::Missing]]
    );
    assert!(attempt.handler_counts.is_empty());
    source.rows.clear();
    let attempt = plan.execute_observed(&source, limits());
    assert!(attempt.result.unwrap().dataset.rows().is_empty());
    assert!(attempt.handler_counts.is_empty());
}

/// A completed choice remains observable after its conversion or later execution fails.
#[test]
fn source_order_counts_survive_conversion_and_resource_failures() {
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
    let attempt = ordered_source_plan(&source, Keep::Last, false, false, false)
        .execute_observed(&source, limits());
    assert!(
        matches!(*attempt.result.unwrap_err(), ExecutionError::Conversion {path,identity:Some(_),..} if path=="columns.V")
    );
    assert_eq!(attempt.handler_counts[0].count, 1);
    source.rows[1][1] = Value::Str("8".into());
    let plan = ordered_source_plan(&source, Keep::Last, false, false, true);
    let attempt = plan.execute_observed(&source, limits());
    assert!(
        matches!(*attempt.result.unwrap_err(), ExecutionError::Conversion {path,..} if path=="columns.LATER")
    );
    assert_eq!(attempt.handler_counts[0].count, 1);
    let mut observed_later_limit = false;
    for work_cells in 0..100 {
        let attempt = plan.execute_observed(
            &source,
            Limits {
                work_cells,
                ..limits()
            },
        );
        if matches!(attempt.result,Err(error) if matches!(*error,ExecutionError::Limit {..}))
            && !attempt.handler_counts.is_empty()
        {
            assert_eq!(attempt.handler_counts[0].count, 1);
            observed_later_limit = true;
        }
    }
    assert!(observed_later_limit);
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
                Value::Date(Date::new(2024, 1, 1, DatePrecision::Year).unwrap()),
                Value::Date(Date::new(2024, 1, 1, DatePrecision::Day).unwrap()),
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
        let attempt = plan.execute_observed(&source, limits());
        assert!(attempt.handler_counts.is_empty());
        let result = attempt.result.unwrap();
        match &result.dataset.rows()[0][1] {
            Value::Float(v) => assert_eq!(v.get().to_bits(), (-0.0_f64).to_bits()),
            Value::Date(v) => assert_eq!(v.collected_precision(), DatePrecision::Year),
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
                Value::Date(Date::new(2024, 1, 1, DatePrecision::Year).unwrap()),
                Value::Str("é".into()),
            ],
            vec![
                Value::Int(1),
                Value::Str("8".into()),
                Value::Date(Date::new(2024, 1, 1, DatePrecision::Day).unwrap()),
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
    let attempt = plan.execute_observed(&source, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        &[vec![Value::Int(1), Value::Int(8)]]
    );
    assert_eq!(attempt.handler_counts[0].count, 1);
}

/// Bind an implicit many-to-one read against a separately owned immutable relation.
pub(crate) fn secondary_lookup_plan(
    source: &Table,
    secondary: &Table,
) -> Result<DatasetPlan, PlanError> {
    use yamaa_core::dataset::{Lookup, MatchKey, SecondarySource};
    DatasetPlan::new_with_sources(
        source.schema.clone(),
        vec![SecondarySource {
            name: "OTHER".into(),
            schema: secondary.schema.clone(),
        }],
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(
            1,
            Expression::Lookup(Lookup {
                source: 0,
                column: 1,
                keys: vec![MatchKey {
                    source_column: 0,
                    output_column: 0,
                }],
            }),
        )],
        vec![0],
        vec![],
    )
}

/// Reuse one named record selection for two independently converted readings.
pub(crate) fn named_plan(
    source: &Table,
    secondary: &Table,
    item: yamaa_core::dataset::Intermediate,
) -> Result<DatasetPlan, PlanError> {
    use yamaa_core::dataset::{SecondarySource, SourceSchemas};
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: source.schema.clone(),
            secondary: vec![SecondarySource {
                name: "OTHER".into(),
                schema: secondary.schema.clone(),
            }],
        },
        vec![item],
        schema(&[
            ("ID", ColumnType::Int),
            ("V", ColumnType::Int),
            ("W", ColumnType::Str),
        ]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(
                1,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            ),
            assign(
                2,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            ),
        ],
        vec![0],
        vec![],
    )
}

/// A declaration independent of source payloads, including explicit missing absence handling.
pub(crate) fn named_item() -> yamaa_core::dataset::Intermediate {
    use yamaa_core::dataset::{
        Intermediate, IntermediateKey, Keep, MatchRead, MatchValue, OrderTerm, SourceSelection,
    };
    Intermediate {
        identifier: "SELECTED".into(),
        path: "intermediates[0]".into(),
        source: yamaa_core::dataset::IntermediateSource::Input(0),
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        filter: None,
        selection: Some(SourceSelection {
            order_by: vec![OrderTerm {
                column: 2,
                descending: false,
                nulls_first: false,
            }],
            keep: Keep::Last,
        }),
        no_match: Some(Value::Missing),
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,

        range: None,
        columns: None,
    }
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
        let attempt = plan.execute_observed_sources(&source, &[&secondary], limits());
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
    let attempt = named_plan(&source, &secondary, item)
        .unwrap()
        .execute_observed_sources(&source, &[&secondary], limits());
    assert!(
        matches!(*attempt.result.unwrap_err(), ExecutionError::MultipleMatches { intermediate, path, match_count: 2, identity: Some(_), .. } if intermediate == "SELECTED" && path == "intermediates[0]")
    );
    assert!(attempt.handler_counts.is_empty());
    assert!(secondary
        .reads
        .borrow()
        .iter()
        .all(|(_, column)| *column == 0));
    let mut item = named_item();
    item.no_match = None;
    let attempt = named_plan(&source, &secondary, item)
        .unwrap()
        .execute_observed_sources(&source, &[&secondary], limits());
    assert!(
        matches!(*attempt.result.unwrap_err(), ExecutionError::UnmatchedKey { matched_key, identity: Some(_), .. } if matched_key == vec![("ID".into(),Value::Int(2))])
    );
    assert_eq!(attempt.handler_counts.len(), 1);
    let mut item = named_item();
    item.no_match = Some(Value::Str("bad".into()));
    let attempt = named_plan(&source, &secondary, item)
        .unwrap()
        .execute_observed_sources(&source, &[&secondary], limits());
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Conversion { output_row: 1, .. }
    ));
    assert_eq!(attempt.handler_counts.len(), 2);
    secondary.rows[1][1] = Value::Str("bad".into());
    let attempt = named_plan(&source, &secondary, named_item())
        .unwrap()
        .execute_observed_sources(&source, &[&secondary], limits());
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Conversion { output_row: 0, .. }
    ));
    assert_eq!(attempt.handler_counts[0].count, 1);
}

/// Filtering is lazy globally, finishes before matching and never executes for empty output.
#[test]
fn named_intermediate_filter_scope_and_resources_are_attempt_local() {
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
    assert!(matches!(
        *plan
            .execute_observed_sources(&source, &[&secondary], limits())
            .result
            .unwrap_err(),
        ExecutionError::Predicate { .. }
    ));
    source.rows.clear();
    secondary.reads.borrow_mut().clear();
    let empty = plan.execute_observed_sources(&source, &[&secondary], limits());
    assert!(empty.result.unwrap().dataset.rows().is_empty());
    assert!(empty.handler_counts.is_empty());
    assert!(secondary.reads.borrow().is_empty());
    source.rows.push(vec![Value::Int(99)]);
    let plan = named_plan(&source, &secondary, named_item()).unwrap();
    assert!(matches!(
        *plan
            .execute_observed_sources(
                &source,
                &[&secondary],
                Limits {
                    work_cells: 1,
                    ..limits()
                }
            )
            .result
            .unwrap_err(),
        ExecutionError::Limit { .. }
    ));
    assert_eq!(
        plan.execute_observed_sources(&source, &[&secondary], limits())
            .handler_counts
            .len(),
        2
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
    let attempt = plan.execute_observed_sources(&source, &[&secondary], limits());
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
        .execute_observed_sources(&source, &[&secondary], limits())
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
    let attempt = plan.execute_observed_sources(&source, &[&secondary], limits());
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
            .execute_observed_sources(&source, &[&secondary], limits())
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
        matches!(*plan.execute_observed_sources(&source,&[&secondary],limits()).result.unwrap_err(),ExecutionError::Conversion {path,identity:Some(_),..} if path=="columns.V")
    );
}

/// Admission and base keys precede secondary reads; total input capacity and fresh budgets apply.
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
            .execute_observed_sources(&source, &[&secondary], limits())
            .result
            .unwrap_err(),
        ExecutionError::Conversion { identity: None, .. }
    ));
    assert!(secondary.reads.borrow().is_empty());
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::SchemaMismatch
    ));
    source.rows.pop();
    secondary.fail = None;
    assert!(matches!(
        *plan
            .execute_observed_sources(
                &source,
                &[&secondary],
                Limits {
                    source_rows: 1,
                    ..limits()
                }
            )
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::SourceRows,
            limit: 1,
            required: Some(2)
        }
    ));
    assert!(plan
        .execute_observed_sources(&source, &[&secondary], limits())
        .result
        .is_ok());
    source.rows.clear();
    secondary.fail = Some((0, 0));
    assert!(plan
        .execute_observed_sources(&source, &[&secondary], limits())
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
