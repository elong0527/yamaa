//! Independent ordering, admission, failure and committed benchmark observations.
use crate::support::table_schema as schema;
use yamaa_core::{
    table::{Column, TableAccess, TableSchema},
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value},
};

use yamaa_core::dataset::{
    DatasetPlan, ExecutionError, Expression, Limits, PlanError, RowMode, RowTemplate,
};

use super::dataset::*;
use super::dataset_filters::*;
use yamaa_core::bound_expression::{BoundPredicate as Filter, Read};
use yamaa_core::predicate::{self, Comparison, Node, Scalar};

/// Admit three unfiltered numbering operations over the same completed output relation.
pub(crate) fn numbering_plan(source: &Table, descending: bool, nulls_first: bool) -> DatasetPlan {
    numbering_with_filter(source, descending, nulls_first, None, ColumnType::Int)
}
/// Admit optional output-column eligibility and ordinary completed-result conversion.
pub(crate) fn numbering_with_filter(
    source: &Table,
    descending: bool,
    nulls_first: bool,
    predicate: Option<Filter>,
    output_kind: ColumnType,
) -> DatasetPlan {
    use yamaa_core::dataset::{OrderTerm, Window, WindowKind};
    let mut fields = source.schema.columns().to_vec();
    for name in ["SEQ", "RANK", "DENSE"] {
        fields.push(Column {
            name: name.into(),
            kind: output_kind,
        });
    }
    let mut columns = vec![
        assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "SRC.G".into(),
                filter: None,
                selection: None,
            },
        ),
        assign(
            2,
            Expression::Collect {
                column: 2,
                identifier: "SRC.V".into(),
                filter: None,
                selection: None,
            },
        ),
    ];
    for (index, kind) in [
        WindowKind::RowNumber,
        WindowKind::Competition,
        WindowKind::Dense,
    ]
    .into_iter()
    .enumerate()
    {
        columns.push(assign(
            index + 3,
            Expression::Window(Window {
                filter: predicate.clone(),
                kind,
                group_by: vec![1],
                order_by: vec![OrderTerm {
                    column: 2,
                    descending,
                    nulls_first,
                }],
            }),
        ));
    }
    DatasetPlan::new(
        source.schema.clone(),
        TableSchema::new(fields).unwrap(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        columns,
        vec![0],
        vec![],
    )
    .unwrap()
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
        .execute(&source, limits())
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
        .execute(&source, limits())
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
        .execute(&source, limits())
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
            .map(|(i, value)| vec![Value::Int(i as i64), Value::Missing, Value::Date(value)])
            .collect(),
    );
    let plan = numbering_plan(&source, false, false);
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(
        &result.dataset.rows()[0][3..],
        &[const { Value::Int(2) }; 3]
    );
    assert_eq!(
        &result.dataset.rows()[1][3..],
        &[Value::Int(3), Value::Int(2), Value::Int(2)]
    );
    source.rows.clear();
    let empty = plan.execute(&source, limits()).unwrap();
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
    let result = record_plan.execute(&records, limits()).unwrap();
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
fn numbering_accounts_comparison_work_text_and_recovers_with_fresh_budgets() {
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
    let mut bounded = limits();
    bounded.scalar_text_bytes = 3;
    assert!(matches!(
        *plan.execute(&source, bounded).unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    bounded = limits();
    bounded.work_cells = 20;
    assert!(matches!(
        *plan.execute(&source, bounded).unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::WorkCells,
            ..
        }
    ));
    let result = plan.execute(&source, limits()).unwrap();
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
    .execute(&source, limits())
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

/// Construct an eager predicate true on missing values but ill-typed on present integers.
pub(crate) fn missing_or_bad_window_predicate(and_false: bool) -> Filter {
    let nodes = vec![
        if and_false {
            Node::Boolean(false)
        } else {
            Node::IsNull {
                value: Scalar::Identifier("V".into()),
                negated: false,
            }
        },
        Node::Compare {
            operator: Comparison::Greater,
            left: Scalar::Identifier("V".into()),
            right: Scalar::Literal(Value::Str("x".into())),
        },
        if and_false {
            Node::And(0, 1)
        } else {
            Node::Or(0, 1)
        },
    ];
    Filter::new(
        predicate::Plan::new(
            nodes,
            2,
            "columns.SEQ.derivation.row_number".into(),
            "V IS NULL OR V > 'x'".into(),
            predicate::Limits::default(),
        )
        .unwrap(),
        vec![binding("V", Read::Column(2))],
    )
    .unwrap()
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Conversion { output_row: 0, .. }
    ));
    source.rows[1][1] = Value::Str("a".into());
    // In the same partition, the later filter failure precedes conversion of the first number.
    let ExecutionError::Predicate { error, .. } = *plan.execute(&source, limits()).unwrap_err()
    else {
        panic!("partition predicate must precede current conversion")
    };
    assert_eq!(error.spec_path, "columns.SEQ.derivation.row_number");
    source.rows.clear();
    assert!(plan
        .execute(&source, limits())
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Predicate { .. }
    ));
}

/// Predicate and window accounting share one run-local budget, including excluded rows.
#[test]
fn window_filter_limits_share_budget_and_fresh_execution_recovers() {
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
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    work_cells: 30,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit { .. }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows()[1][3],
        Value::Int(2)
    );
}

/// Build a value-window plan with a separate completed donor column and stable ordinal order.
pub(crate) fn value_window_plan(
    source: &Table,
    kind: yamaa_core::dataset::WindowKind,
    filter: Option<Filter>,
) -> Result<DatasetPlan, PlanError> {
    use yamaa_core::dataset::{OrderTerm, Window};
    let mut fields = source.schema.columns().to_vec();
    fields.push(Column {
        name: "RESULT".into(),
        kind: fields[2].kind,
    });
    DatasetPlan::new(
        source.schema.clone(),
        TableSchema::new(fields).unwrap(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(1, collect(1)),
            assign(2, collect(2)),
            assign(
                3,
                Expression::Window(Window {
                    kind,
                    group_by: vec![1],
                    order_by: vec![OrderTerm {
                        column: 0,
                        descending: false,
                        nulls_first: false,
                    }],
                    filter,
                }),
            ),
        ],
        vec![0],
        vec![],
    )
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
            .execute(&source, limits())
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
    .execute(&source, limits())
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
            value_window_plan(&source, WindowKind::RowValue { column: 2, offset }, None).unwrap();
        assert_eq!(
            plan.execute(&source, limits()).unwrap().dataset.rows()[0][3],
            Value::Missing
        );
    }
}

/// A donor keeps temporal precision and text is charged before any result clone.
#[test]
fn value_window_donors_preserve_representation_and_obey_copy_budgets() {
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
        .execute(&source, limits())
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
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 4,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows()[1][3],
        Value::Str("🌱".into())
    );
}

/// A long missing run is resolved within a linear-scan plus sorting work budget.
#[test]
fn previous_non_missing_crosses_long_gaps_without_rescanning_each_prefix() {
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
    let budget = Limits {
        source_rows: 5000,
        output_rows: 5000,
        output_cells: 20000,
        key_cells: 20000,
        work_cells: 300000,
        ..limits()
    };
    let result = value_window_plan(&source, WindowKind::PreviousNonMissing { column: 2 }, None)
        .unwrap()
        .execute(&source, budget)
        .unwrap();
    assert_eq!(result.dataset.rows()[0][3], Value::Missing);
    assert!(result.dataset.rows()[1..]
        .iter()
        .all(|row| row[3] == Value::Int(7)));
}

/// Admit temporal baseline selection over completed dates and partition keys.
pub(crate) fn baseline_plan(
    source: &Table,
    groups: Vec<usize>,
    filter: Option<Filter>,
) -> DatasetPlan {
    use yamaa_core::dataset::{Window, WindowKind};
    let mut fields = source.schema.columns().to_vec();
    fields.push(Column {
        name: "BLFL".into(),
        kind: ColumnType::Str,
    });
    DatasetPlan::new(
        source.schema.clone(),
        TableSchema::new(fields).unwrap(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(1, collect(1)),
            assign(2, collect(2)),
            assign(3, collect(3)),
            assign(
                4,
                Expression::Window(Window {
                    kind: WindowKind::BaselineFlag {
                        date: 2,
                        reference_date: 3,
                    },
                    group_by: groups,
                    order_by: vec![],
                    filter,
                }),
            ),
        ],
        vec![0],
        vec![],
    )
    .unwrap()
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
        .execute(&source, limits())
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
            .execute(&source, limits())
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
            .execute(&source, limits())
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
fn baseline_ties_count_all_matches_and_charge_partition_identity() {
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
    let failure = plan.execute(&source, limits()).unwrap_err();
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
    for constrained in [
        Limits {
            identity_cells: 0,
            ..limits()
        },
        Limits {
            identity_text_bytes: 8,
            ..limits()
        },
    ] {
        assert!(matches!(
            *plan.execute(&source, constrained).unwrap_err(),
            ExecutionError::Limit { .. }
        ));
    }
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::BaselineAmbiguity { match_count: 3, .. }
    ));
    assert!(
        matches!(*baseline_plan(&source, vec![], None).execute(&source, Limits { identity_cells: 0, ..limits() }).unwrap_err(), ExecutionError::BaselineAmbiguity { partition, .. } if partition.is_empty())
    );
}
