//! Independent ordering, admission, failure and committed benchmark observations.
use std::cell::RefCell;
use yamaa_core::{
    reduction::NumericReducer,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{
        Assignment, Check, DatasetPlan, ExecutionError, Expression, Limits, PlanError, RowMode,
        RowTemplate, Verification,
    },
    table_grouping::{partition, GroupingError},
};

struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    fail: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = &'static str;
    /// Expose the immutable test snapshot schema.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    /// Expose source cardinality without a cell read.
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    /// Observe reads and inject opaque access failures without altering values.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access("source failure"));
        }
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
/// Construct an ordered schema independently of plan compilation.
fn schema(columns: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
/// Build an observable normalized source port.
fn table(columns: &[(&str, ColumnType)], rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: schema(columns),
        rows,
        reads: RefCell::default(),
        fail: None,
    }
}
/// Give a bound assignment deterministic test provenance.
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
}
/// Keep fixtures below explicit source, output and partition budgets.
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 200,
        output_cells: 2000,
        key_cells: 2000,
        work_cells: 10000,
        scalar_text_bytes: 10000,
        output_text_bytes: 10000,
        identity_cells: 10000,
        identity_text_bytes: 10000,
    }
}
/// Attach deterministic declaration provenance to one check.
fn verification(check: Check) -> Verification {
    Verification {
        path: "verifications[0]".into(),
        check,
    }
}
/// Admit a record-driven plan with one output identity column.
fn record_plan(
    source: &Table,
    output: TableSchema,
    columns: Vec<Assignment>,
    mut checks: Vec<Verification>,
) -> DatasetPlan {
    for (index, check) in checks.iter_mut().enumerate() {
        check.path = format!("verifications[{index}]");
    }
    DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![],
        }],
        columns,
        vec![0],
        checks,
    )
    .unwrap()
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
                date(DatePrecision::Year),
            ],
            vec![Value::Missing, Value::float(2.0), date(DatePrecision::Day)],
            vec![
                Value::Str("a".into()),
                Value::float(0.0),
                date(DatePrecision::Day),
            ],
            vec![
                Value::Str("z\0\u{e9}".into()),
                Value::float(0.0),
                date(DatePrecision::Month),
            ],
            vec![
                Value::Missing,
                Value::float(2.0),
                date(DatePrecision::Month),
            ],
            vec![
                Value::Str("z\0e\u{301}".into()),
                Value::float(0.0),
                date(DatePrecision::Year),
            ],
        ],
    );
    assert_eq!(
        partition(&source, &[0, 1, 2], 6, 18).unwrap(),
        vec![vec![0, 3], vec![1, 4], vec![2], vec![5]]
    );
    assert_eq!(
        *source.reads.borrow(),
        (0..6)
            .flat_map(|row| (0..3).map(move |column| (row, column)))
            .collect::<Vec<_>>()
    );
}

/// Invalid declarations and insufficient budgets must not touch a single cell.
#[test]
fn grouping_preflight_and_port_failures_are_distinct() {
    let mut source = table(
        &[("x", ColumnType::Int)],
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
    );
    assert_eq!(
        partition(&source, &[], 2, 2),
        Err(GroupingError::EmptyColumns)
    );
    assert_eq!(
        partition(&source, &[0, 1], 2, 4),
        Err(GroupingError::InvalidColumn {
            position: 1,
            column: 1
        })
    );
    assert_eq!(
        partition(&source, &[0, 0], 2, 4),
        Err(GroupingError::DuplicateColumn {
            position: 1,
            column: 0
        })
    );
    assert_eq!(
        partition(&source, &[0], 1, 2),
        Err(GroupingError::RowLimit {
            limit: 1,
            required: 2
        })
    );
    assert_eq!(
        partition(&source, &[0], 2, 1),
        Err(GroupingError::KeyCellLimit { limit: 1 })
    );
    assert!(source.reads.borrow().is_empty());
    source.fail = Some((1, 0));
    assert_eq!(
        partition(&source, &[0], 2, 2),
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
    let actual = plan.execute(&source, limits()).unwrap();
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
    match *plan.execute(&source, limits()).unwrap_err() {
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
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    output_cells: 3,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Capacity
    ));
    assert!(source.reads.borrow().is_empty());
    let wrong = table(&[("wrong", ColumnType::Str)], vec![]);
    assert!(matches!(
        *plan.execute(&wrong, limits()).unwrap_err(),
        ExecutionError::SchemaMismatch
    ));
}

/// Admission rejects latent invalid paths even for an empty source snapshot.
#[test]
fn plan_admission_checks_every_template_and_dependency() {
    let source = schema(&[("id", ColumnType::Int), ("other", ColumnType::Int)]);
    let output = source.clone();
    let make = |templates, columns| {
        DatasetPlan::new(
            source.clone(),
            output.clone(),
            templates,
            columns,
            vec![0],
            vec![],
        )
    };
    let records = |assignments| RowTemplate {
        mode: RowMode::Records,
        assignments,
    };
    let grouped = |assignments| RowTemplate {
        mode: RowMode::Groups(vec![0]),
        assignments,
    };
    assert_eq!(make(vec![], vec![]), Err(PlanError::NoTemplates));
    assert_eq!(
        make(
            vec![records(vec![assign(0, Expression::Column(1))])],
            vec![]
        ),
        Err(PlanError::UnavailableColumn)
    );
    assert_eq!(
        make(
            vec![records(vec![assign(0, Expression::Source(2))])],
            vec![]
        ),
        Err(PlanError::InvalidSource)
    );
    assert_eq!(
        make(
            vec![grouped(vec![assign(0, Expression::Source(1))])],
            vec![]
        ),
        Err(PlanError::NonGroupSource)
    );
    let reduce = Expression::Reduce {
        column: 1,
        reducer: NumericReducer::Sum,
        text: "SUM(x.other)".into(),
    };
    assert_eq!(
        make(vec![records(vec![assign(0, reduce)])], vec![]),
        Err(PlanError::UngroupedReduction)
    );
    assert_eq!(
        make(
            vec![records(vec![
                assign(0, Expression::Source(0)),
                assign(0, Expression::Source(0))
            ])],
            vec![]
        ),
        Err(PlanError::DuplicateAssignment)
    );
    assert_eq!(
        make(
            vec![records(vec![assign(0, Expression::Source(0))])],
            vec![]
        ),
        Err(PlanError::IncompleteRow)
    );
    assert_eq!(
        make(
            vec![
                records(vec![]),
                grouped(vec![assign(0, Expression::Source(0))])
            ],
            vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Literal(Value::Missing))
            ]
        ),
        Err(PlanError::InconsistentRowColumns)
    );
    // A non-group field in the later column phase is invalid too.
    assert_eq!(
        make(
            vec![grouped(vec![assign(0, Expression::Source(0))])],
            vec![assign(1, Expression::Source(1))]
        ),
        Err(PlanError::NonGroupSource)
    );
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
    let ExecutionError::VerificationFailures(records) =
        *plan.execute(&source, limits()).unwrap_err()
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
    assert_eq!(records[1].condition, "row_count_failed");
    assert_eq!(records[1].requirement, "REQ-0385");
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
    let ExecutionError::KeyFailures(records) = *plan.execute(&source, limits()).unwrap_err() else {
        panic!("expected key failure")
    };
    assert_eq!(records.len(), 2);
    assert_eq!(
        (
            records[0].path.as_str(),
            records[0].condition,
            records[0].failed_count
        ),
        ("keys[0]", "missing_key", 2)
    );
    assert_eq!(
        (records[1].condition, records[1].failed_count),
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
    let result = plan.execute(&source, limits()).unwrap();
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::VerificationFailures(_)
    ));
    assert!(source.reads.borrow().is_empty());
}

/// This reader accepts only the deliberately unquoted committed CSV fixture.
/// It is test setup, not a production ingestion or specification compiler path.
fn fixture(text: &str, float_column: usize) -> Table {
    assert!(!text.contains('"'));
    let mut lines = text.lines();
    let names: Vec<_> = lines.next().unwrap().split(',').collect();
    let columns: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                *name,
                if index == float_column {
                    ColumnType::Float
                } else {
                    ColumnType::Str
                },
            )
        })
        .collect();
    let rows = lines
        .map(|line| {
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(fields.len(), names.len());
            fields
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    if value.is_empty() {
                        Value::Missing
                    } else if index == float_column {
                        Value::float(value.parse().unwrap())
                    } else {
                        Value::Str((*value).into())
                    }
                })
                .collect()
        })
        .collect();
    table(&columns, rows)
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
                mode: RowMode::Records,
                assignments: vec![
                    assign(3, Expression::Source(3)),
                    assign(4, Expression::Source(4)),
                    assign(5, Expression::Source(6)),
                    assign(6, Expression::Literal(Value::Missing)),
                ],
            },
            RowTemplate {
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
    let actual = plan.execute(&source, limits()).unwrap();
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
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                Assignment {
                    column: 1,
                    expression: Expression::Reduce {
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
    let ExecutionError::Reduction { path, error, .. } =
        *plan.execute(&source, limits()).unwrap_err()
    else {
        panic!("expected reduction port error")
    };
    assert_eq!(path, "rows[0].derivations.x.aggregate");
    assert!(matches!(
        error,
        yamaa_engine::table_reduction::TableReductionError::Cell {
            position: 2,
            error: CellError::Access("source failure")
        }
    ));
    source.fail = None;
    let ExecutionError::Reduction { error, .. } = *plan.execute(&source, limits()).unwrap_err()
    else {
        panic!("expected arithmetic overflow")
    };
    assert!(matches!(
        error,
        yamaa_engine::table_reduction::TableReductionError::Reduction(
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
            vec![date(DatePrecision::Year), Value::Int(1)],
            vec![date(DatePrecision::Day), Value::Missing],
            vec![date(DatePrecision::Month), Value::Int(2)],
        ],
    );
    let output = schema(&[("id", ColumnType::Date), ("mean", ColumnType::Float)]);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Reduce {
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
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(result.dataset.rows().len(), 1);
    let Value::Date(actual) = result.dataset.rows()[0][0] else {
        panic!("expected date")
    };
    assert_eq!(actual.collected_precision(), DatePrecision::Year);
    assert_eq!(result.dataset.rows()[0][1], Value::float(1.5));
}

/// Reduction input types and verification declarations are static checks, even on empty input.
#[test]
fn nonnumeric_reductions_and_invalid_verifications_are_rejected() {
    let source = schema(&[("id", ColumnType::Str)]);
    let grouped = vec![RowTemplate {
        mode: RowMode::Groups(vec![0]),
        assignments: vec![assign(
            0,
            Expression::Reduce {
                column: 0,
                reducer: NumericReducer::Sum,
                text: "SUM(T.id)".into(),
            },
        )],
    }];
    assert_eq!(
        DatasetPlan::new(
            source.clone(),
            source.clone(),
            grouped,
            vec![],
            vec![0],
            vec![]
        ),
        Err(PlanError::NonNumericReduction)
    );
    let template = || {
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0, Expression::Source(0))],
        }]
    };
    for check in [
        Check::RowCount {
            min: None,
            max: None,
        },
        Check::RowCount {
            min: Some(2),
            max: Some(1),
        },
    ] {
        assert_eq!(
            DatasetPlan::new(
                source.clone(),
                source.clone(),
                template(),
                vec![],
                vec![0],
                vec![verification(check)]
            ),
            Err(PlanError::InvalidBounds)
        );
    }
    for columns in [vec![], vec![1]] {
        assert_eq!(
            DatasetPlan::new(
                source.clone(),
                source.clone(),
                template(),
                vec![],
                vec![0],
                vec![verification(Check::Unique(columns))]
            ),
            Err(PlanError::InvalidColumns)
        );
    }
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
        partition(&source, &[0], 5, 5).unwrap(),
        vec![vec![0, 3], vec![1], vec![2], vec![4]]
    );
    let empty = table(&[("id", ColumnType::Int)], vec![]);
    assert!(partition(&empty, &[0], 0, 0).unwrap().is_empty());
    let plan = DatasetPlan::new(
        empty.schema.clone(),
        empty.schema.clone(),
        vec![RowTemplate {
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
            Limits {
                source_rows: 0,
                output_rows: 0,
                output_cells: 0,
                key_cells: 0,
                ..limits()
            },
        )
        .unwrap();
    assert_eq!(actual.dataset.schema(), &empty.schema);
    assert_eq!(actual.dataset.row_count(), 0);
    assert!(empty.reads.borrow().is_empty());
}

/// Checked budget multiplication must reject overflow before a port can be read.
#[test]
fn grouping_resource_arithmetic_cannot_wrap() {
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
        /// Any read is an admission bug for this impossible budget.
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<()>> {
            panic!("must not read")
        }
    }
    let source = Huge(schema(&[("a", ColumnType::Int), ("b", ColumnType::Int)]));
    assert_eq!(
        partition(&source, &[0, 1], usize::MAX, usize::MAX),
        Err(GroupingError::KeyCellLimit { limit: usize::MAX })
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
    let actual = plan.execute(&source, limits()).unwrap();
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
    } = *plan.execute(&source, limits()).unwrap_err()
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
    let ExecutionError::VerificationFailures(records) =
        *plan.execute(&source, limits()).unwrap_err()
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
                let output = schema(&[("id", ColumnType::Str), ("value", ColumnType::Float)]);
                let value = assign(1, Expression::Literal(literal.clone()));
                let plan = DatasetPlan::new(
                    empty.schema.clone(),
                    output,
                    vec![RowTemplate {
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
                    plan.execute(&empty, limits()).unwrap().dataset.row_count(),
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
                } = *plan.execute(&populated, limits()).unwrap_err()
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
                        value: yamaa_core::conversion::ConversionValue::Runtime(literal.clone()),
                        reason: yamaa_core::conversion::ConversionReason::IncompatibleOrInvalidText,
                    }
                );
            }
        }
    }
}

/// Repeated text-to-number conversion consumes work even when output stores no strings.
#[test]
fn scalar_text_work_is_cumulative_and_separate_from_retained_text() {
    use yamaa_engine::dataset::Resource;
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
    let policy = Limits {
        scalar_text_bytes: 7,
        output_text_bytes: 0,
        ..limits()
    };
    assert_eq!(
        *plan.execute(&source, policy).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: 7,
            required: Some(8)
        }
    );
    let actual = plan
        .execute(
            &source,
            Limits {
                scalar_text_bytes: 8,
                ..policy
            },
        )
        .unwrap();
    assert_eq!(
        actual.dataset.rows(),
        &[
            vec![Value::Int(1), Value::Int(7)],
            vec![Value::Int(2), Value::Int(7)]
        ]
    );
}

/// UTF-8 bytes, including copies into dependent columns, are counted before retention.
#[test]
fn output_text_limit_counts_bytes_and_fresh_runs_reset_counters() {
    use yamaa_engine::dataset::Resource;
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
    let policy = Limits {
        output_text_bytes: 7,
        ..limits()
    };
    for _ in 0..2 {
        assert_eq!(
            *plan.execute(&source, policy).unwrap_err(),
            ExecutionError::Limit {
                resource: Resource::OutputTextBytes,
                limit: 7,
                required: Some(8)
            }
        );
    }
    assert_eq!(
        plan.execute(
            &source,
            Limits {
                output_text_bytes: 8,
                ..policy
            }
        )
        .unwrap()
        .dataset
        .row_count(),
        2
    );
}

/// Repeated failed checks cannot multiply retained identities past the caller's budget.
#[test]
fn verification_identity_budgets_accumulate_across_checks() {
    use yamaa_engine::dataset::Resource;
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
    for (policy, resource) in [
        (
            Limits {
                identity_cells: 3,
                ..limits()
            },
            Resource::IdentityCells,
        ),
        (
            Limits {
                identity_text_bytes: 3,
                ..limits()
            },
            Resource::IdentityTextBytes,
        ),
    ] {
        assert_eq!(
            *plan.execute(&source, policy).unwrap_err(),
            ExecutionError::Limit {
                resource,
                limit: 3,
                required: Some(4)
            }
        );
    }
    let ExecutionError::VerificationFailures(records) = *plan
        .execute(
            &source,
            Limits {
                identity_cells: 4,
                identity_text_bytes: 4,
                ..limits()
            },
        )
        .unwrap_err()
    else {
        panic!("expected complete failed checks")
    };
    assert_eq!(records.len(), 2);
    assert!(records
        .iter()
        .all(|record| record.offending_rows.len() == 2));
}

/// Work admission precedes grouped reads and the aggregate's eager argument collection.
#[test]
fn work_limits_fail_before_the_next_source_operation() {
    use yamaa_engine::dataset::Resource;
    let source = table(
        &[("id", ColumnType::Int), ("x", ColumnType::Int)],
        vec![
            vec![Value::Int(1), Value::Int(1)],
            vec![Value::Int(1), Value::Int(2)],
            vec![Value::Int(1), Value::Int(3)],
        ],
    );
    let plan = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Reduce {
                        column: 1,
                        reducer: NumericReducer::Sum,
                        text: "SUM(T.x)".into(),
                    },
                ),
            ],
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        *plan
            .execute(
                &source,
                Limits {
                    work_cells: 2,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: 2,
            required: Some(3)
        }
    );
    assert!(source.reads.borrow().is_empty());
    assert_eq!(
        *plan
            .execute(
                &source,
                Limits {
                    work_cells: 8,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: 8,
            required: Some(9)
        }
    );
    assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (2, 0), (0, 0)]);
}

/// Completed missing keys are reportable; unconstructed key slots are not.
#[test]
fn conversion_failure_retains_completed_missing_identity_with_a_budget() {
    use yamaa_engine::dataset::{Resource, RowIdentity};
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
    let ExecutionError::Conversion { identity, .. } = *plan.execute(&source, limits()).unwrap_err()
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
    assert_eq!(
        *plan
            .execute(
                &source,
                Limits {
                    identity_cells: 0,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::IdentityCells,
            limit: 0,
            required: Some(1)
        }
    );
}
