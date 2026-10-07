//! Independent ordering, admission, failure and committed benchmark observations.
use std::cell::RefCell;
use yamaa_core::{
    reduction::NumericReducer,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::DatasetExecution;
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

/// COUNT uses its owning group and preserves failures and cumulative work admission.
#[test]
fn grouped_count_avoids_unused_fields_and_withholds_output_on_read_failure() {
    use yamaa_engine::{dataset::Resource, table_reduction::TableReductionError};
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
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows(),
        &[vec![Value::Int(1), Value::Int(3)]]
    );
    assert!(source.reads.borrow().iter().all(|(_, column)| *column == 0));
    for column in [None, Some(1)] {
        source.reads.borrow_mut().clear();
        assert_eq!(
            *plan(column)
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
                required: Some(9),
            }
        );
        assert_eq!(*source.reads.borrow(), [(0, 0), (1, 0), (2, 0), (0, 0)]);
    }
    source.reads.borrow_mut().clear();
    assert_eq!(
        *plan(Some(1)).execute(&source, limits()).unwrap_err(),
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
            filter: None,
            mode: RowMode::Records,
            assignments: vec![],
        }],
        columns,
        vec![0],
        checks,
    )
    .unwrap()
}

/// Compile authored arithmetic with a closed static name-to-read map.
fn computed(text: &str, bindings: &[(&str, Read)]) -> Expression {
    use yamaa_core::numeric_compiler::{compile_numeric, CompileLimits};
    use yamaa_engine::{dataset::BoundNumeric, dataset_predicate::Binding};
    Expression::Compute(
        BoundNumeric::new(
            compile_numeric(
                text,
                "columns.N.derivation.compute",
                CompileLimits::default(),
            )
            .unwrap(),
            bindings
                .iter()
                .map(|(name, read)| Binding {
                    name: (*name).into(),
                    read: *read,
                })
                .collect(),
        )
        .unwrap(),
    )
}

/// Binding is complete before execution, while each written occurrence still reads once.
#[test]
fn dataset_numeric_bindings_and_occurrence_budgets_are_explicit() {
    use yamaa_core::numeric_compiler::{compile_numeric, CompileLimits};
    use yamaa_engine::{
        dataset::BoundNumeric,
        dataset_predicate::{Binding, BindingError},
    };
    let compiled = compile_numeric("X + X * 2", "compute", CompileLimits::default()).unwrap();
    assert_eq!(compiled.identifiers().collect::<Vec<_>>(), vec!["X"]);
    assert_eq!(compiled.resolution_count(), 2);
    assert_eq!(compiled.node_count(), 5);
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
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(
        result.dataset.rows(),
        &[vec![Value::Int(1), Value::Int(21)]]
    );
    assert_eq!(*source.reads.borrow(), vec![(0, 0), (0, 1), (0, 1)]);
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    work_cells: 3,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit { .. }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows(),
        result.dataset.rows()
    );
}

/// Numeric validation has no output identity; reached arithmetic and conversion do.
#[test]
fn dataset_numeric_failure_priority_preserves_ports_missingness_and_identity() {
    use yamaa_core::evaluation::{EvaluationErrorKind, NumericCondition};
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Numeric {
            identity: Some(_),
            ..
        }
    ));
    assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
    let plan = make(&source, "NULL + X", &[("X", Read::Source(1))]);
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Cell {
            error: CellError::Access("source failure"),
            ..
        }
    ));
    source.fail = None;
    let plan = make(&source, "X + 1", &[("X", Read::Source(1))]);
    assert!(
        matches!(*plan.execute(&source,limits()).unwrap_err(),ExecutionError::Numeric { error,identity:None } if matches!(error.evaluation.kind,EvaluationErrorKind::Numeric(NumericCondition::IncompatibleInput { .. })))
    );
    let plan = make(&source, "9223372036854775808", &[]);
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Numeric {
            identity: Some(_),
            ..
        }
    ));
    source.rows.clear();
    assert!(plan
        .execute(&source, limits())
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
        plan.execute(&source, limits()).unwrap().dataset.rows(),
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
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(result.dataset.rows().len(), 1);
    let Value::Date(actual) = result.dataset.rows()[0][0] else {
        panic!("expected date")
    };
    assert_eq!(actual.collected_precision(), DatePrecision::Year);
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
        partition(&source, &[0], 5, 5).unwrap(),
        vec![vec![0, 3], vec![1], vec![2], vec![4]]
    );
    let empty = table(&[("id", ColumnType::Int)], vec![]);
    assert!(partition(&empty, &[0], 0, 0).unwrap().is_empty());
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
                let output = schema(&[("id", ColumnType::Str), ("value", ColumnType::Float)]);
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
            filter: None,
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Reduce {
                        identifier: None,
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

use yamaa_core::predicate::{self, Comparison, Node, Scalar};
use yamaa_engine::dataset_predicate::{Binding, BindingError, BoundPredicate as Filter, Read};

/// Admit a single authored predicate and its explicit scope bindings.
fn filter(node: Node, bindings: Vec<Binding>) -> Filter {
    Filter::new(
        predicate::Plan::new(
            vec![node],
            0,
            "rows[0].filter".into(),
            "authored predicate".into(),
            predicate::Limits::default(),
        )
        .unwrap(),
        bindings,
    )
    .unwrap()
}
/// Bind an occurrence without guessing whether it belongs to source or candidate scope.
fn binding(name: &str, read: Read) -> Binding {
    Binding {
        name: name.into(),
        read,
    }
}
/// Keep rows whose completed value exceeds zero; missing values yield unknown.
fn positive(read: Read) -> Filter {
    filter(
        Node::Compare {
            operator: Comparison::Greater,
            left: Scalar::Identifier("x".into()),
            right: Scalar::Literal(Value::Int(0)),
        },
        vec![binding("x", read)],
    )
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
        plan.execute(&source, limits()).unwrap().dataset.rows(),
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
        plan.execute(&source, limits()).unwrap().dataset.rows(),
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
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Conversion { .. }
    ));
    let empty = table(&[("id", ColumnType::Int)], vec![]);
    assert!(plan
        .execute(&empty, limits())
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
        let predicate = predicate::Plan::new(
            vec![Node::IsNull {
                value: Scalar::Identifier("x".into()),
                negated: false,
            }],
            0,
            "rows[0].filter".into(),
            "x IS NULL".into(),
            predicate::Limits::default(),
        )
        .unwrap();
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
    let ExecutionError::Predicate { source_row, error } =
        *plan.execute(&source, limits()).unwrap_err()
    else {
        panic!("predicate port failure")
    };
    assert_eq!(source_row, 0);
    assert_eq!(error.spec_path, "rows[0].filter");
    assert!(matches!(
        error.kind,
        predicate::ErrorKind::Resolution {
            error: CellError::Access("source failure"),
            ..
        }
    ));
    assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
}
/// Discarding values refunds retained output bytes, never cumulative operand bytes.
#[test]
fn rejected_candidates_release_retained_text_but_not_work() {
    use yamaa_engine::dataset::Resource;
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
    let policy = Limits {
        output_text_bytes: 3,
        scalar_text_bytes: 6,
        ..limits()
    };
    for _ in 0..2 {
        assert!(plan
            .execute(&source, policy)
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 5,
                    ..policy
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: 5,
            required: Some(6)
        }
    ));
}
/// Predicate operands and LIKE cells accumulate across rows and restart only for a new run.
#[test]
fn filter_limits_are_cumulative_and_recover_on_new_execution() {
    use yamaa_engine::dataset::Resource;
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
    for (policy, resource) in [
        (
            Limits {
                scalar_text_bytes: 7,
                ..limits()
            },
            Resource::PredicateTextBytes,
        ),
        (
            Limits {
                work_cells: 10,
                ..limits()
            },
            Resource::PredicateLikeWork,
        ),
    ] {
        assert!(
            matches!(*plan.execute(&source, policy).unwrap_err(), ExecutionError::Limit { resource: r, .. } if r == resource)
        );
        assert!(plan
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
}

/// Supply a verification predicate with its actual owning declaration path.
fn check_predicate(node: Node, bindings: Vec<Binding>, path: &str) -> Filter {
    Filter::new(
        predicate::Plan::new(
            vec![node],
            0,
            path.into(),
            "authored verification predicate".into(),
            predicate::Limits::default(),
        )
        .unwrap(),
        bindings,
    )
    .unwrap()
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
                require: check_predicate(node.clone(), vec![], "verifications[0].assert.require"),
            })],
        );
        let result = plan.execute(&source, limits());
        if index == 0 {
            let records = result.unwrap().verifications;
            assert_eq!(records[0].evaluated_count, 2);
            assert_eq!(records[0].failed_count, 0);
        } else {
            let ExecutionError::VerificationFailures(records) = *result.unwrap_err() else {
                panic!("assert data failure")
            };
            assert_eq!(records[0].condition, "assert_failed");
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
            let result = plan.execute(&source, limits());
            if a == 0 && b != 0 {
                let ExecutionError::VerificationFailures(records) = *result.unwrap_err() else {
                    panic!("guarded assert data failure")
                };
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].condition, "assert_failed");
                assert_eq!(records[0].requirement, "REQ-0384");
                assert_eq!(records[0].failed_count, 2);
                assert_eq!(records[1].failed_count, 0);
            } else {
                assert_eq!(result.unwrap().verifications[0].failed_count, 0);
            }
        }
    }
}
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
        let ExecutionError::VerificationPredicate { error, records } =
            *plan.execute(&source, limits()).unwrap_err()
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
    let ExecutionError::VerificationPredicate { error, records } =
        *plan.execute(&source, limits()).unwrap_err()
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
    let actual = plan.execute(&source, limits()).unwrap();
    assert_eq!(actual.verifications.len(), 1);
    assert_eq!(actual.dataset.rows(), source.rows);
}

/// Sample validation consumes the same cumulative budget, including on empty datasets.
#[test]
fn predicate_check_samples_share_budgets_and_fresh_runs_recover() {
    use yamaa_engine::dataset::Resource;
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
        ExecutionError::Limit {
            resource: Resource::PredicateWork,
            limit: 1,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().verifications.len(),
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
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 5,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::PredicateTextBytes,
            limit: 5,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().verifications[0].evaluated_count,
        0
    );
}

/// Bind no-template keys separately from later whole-column derivations.
fn key_plan(
    source: &Table,
    output: TableSchema,
    keys: Vec<usize>,
    key_assignments: Vec<Assignment>,
    columns: Vec<Assignment>,
) -> DatasetPlan {
    DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: key_assignments,
            filter: None,
        }],
        columns,
        keys,
        vec![],
    )
    .unwrap()
}
/// Read one distinct present raw source value across the complete feeding group.
fn collect(column: usize) -> Expression {
    Expression::Collect {
        column,
        identifier: format!("SRC.C{column}"),
        filter: None,
        selection: None,
    }
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
    let result = plan.execute(&source, limits()).unwrap();
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
        reordered.execute(&source, limits()).unwrap().dataset.rows(),
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
    let error = plan.execute(&source, limits()).unwrap_err();
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
        *plan.execute(&source, limits()).unwrap_err(),
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
        matches!(*plan.execute(&source,limits()).unwrap_err(),ExecutionError::Conversion {path,identity:None,..} if path=="columns.ID")
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
    let error = plan.execute(&source, limits()).unwrap_err();
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
        (records[1].condition, records[1].failed_count),
        ("duplicate_key", 1)
    );
}
/// Key storage is admitted before reads; collapsed output and released duplicate text have independent bounds.
#[test]
fn key_grain_budgets_cover_probes_and_release_duplicates() {
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
    let bounded = Limits {
        output_rows: 1,
        output_cells: 2,
        key_cells: 3,
        output_text_bytes: 12,
        ..limits()
    };
    assert_eq!(
        plan.execute(&source, bounded).unwrap().dataset.rows().len(),
        1
    );
    source.reads.borrow_mut().clear();
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    key_cells: 2,
                    ..bounded
                }
            )
            .unwrap_err(),
        ExecutionError::Grouping(GroupingError::KeyCellLimit { limit: 2 })
    ));
    assert!(source.reads.borrow().is_empty());
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    output_text_bytes: 11,
                    ..bounded
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_engine::dataset::Resource::OutputTextBytes,
            ..
        }
    ));
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 35,
                    ..bounded
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_engine::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    assert!(plan.execute(&source, bounded).is_ok());
    let empty = table(
        &[("id", ColumnType::Str), ("value", ColumnType::Str)],
        vec![],
    );
    assert!(plan
        .execute(&empty, bounded)
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
                *plan.execute(&source, limits()).unwrap_err(),
                ExecutionError::MultipleValues { value_count: 2, .. }
            )),
            Some(expected) => {
                let result = plan.execute(&source, limits()).unwrap();
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
/// Typed admission cannot expose first-record reads or incomplete keys in key-grain execution.
#[test]
fn key_grain_rejects_incomplete_or_wrong_phase_assignments() {
    let source = schema(&[("id", ColumnType::Int)]);
    let output = schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]);
    for (assignments, columns) in [
        (
            vec![],
            vec![assign(0, Expression::Source(0)), assign(1, collect(0))],
        ),
        (vec![assign(0, collect(0))], vec![assign(1, collect(0))]),
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

/// Admit three unfiltered numbering operations over the same completed output relation.
fn numbering_plan(source: &Table, descending: bool, nulls_first: bool) -> DatasetPlan {
    numbering_with_filter(source, descending, nulls_first, None, ColumnType::Int)
}
/// Admit optional output-column eligibility and ordinary completed-result conversion.
fn numbering_with_filter(
    source: &Table,
    descending: bool,
    nulls_first: bool,
    predicate: Option<Filter>,
    output_kind: ColumnType,
) -> DatasetPlan {
    use yamaa_engine::dataset::{OrderTerm, Window, WindowKind};
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
fn numbering_admits_complete_dependencies_only_in_key_grain_column_phase() {
    use yamaa_engine::dataset::{OrderTerm, Window, WindowKind};
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
    assert_eq!(
        build(window.clone(), RowMode::Records, false),
        Err(PlanError::InvalidWindow)
    );
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
            resource: yamaa_engine::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    bounded = limits();
    bounded.work_cells = 20;
    assert!(matches!(
        *plan.execute(&source, bounded).unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_engine::dataset::Resource::WorkCells,
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
fn missing_or_bad_window_predicate(and_false: bool) -> Filter {
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
fn value_window_plan(
    source: &Table,
    kind: yamaa_engine::dataset::WindowKind,
    filter: Option<Filter>,
) -> Result<DatasetPlan, PlanError> {
    use yamaa_engine::dataset::{OrderTerm, Window};
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
    use yamaa_engine::dataset::WindowKind;
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
    use yamaa_engine::dataset::WindowKind;
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
    use yamaa_engine::dataset::WindowKind;
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
    use yamaa_engine::dataset::WindowKind;
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
            resource: yamaa_engine::dataset::Resource::ScalarTextBytes,
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
    use yamaa_engine::dataset::WindowKind;
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
fn baseline_plan(source: &Table, groups: Vec<usize>, filter: Option<Filter>) -> DatasetPlan {
    use yamaa_engine::dataset::{Window, WindowKind};
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

/// Bind a source-only root predicate before standalone key construction.
fn root_filtered_plan(source: &Table, predicate: Filter) -> Result<DatasetPlan, PlanError> {
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
fn selected_source_plan(source: &Table, predicate: Filter) -> Result<DatasetPlan, PlanError> {
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
fn ordered_source_plan(
    source: &Table,
    keep: yamaa_engine::dataset::Keep,
    descending: bool,
    nulls_first: bool,
    later_failure: bool,
) -> DatasetPlan {
    use yamaa_engine::dataset::{OrderTerm, SourceSelection};
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
    use yamaa_engine::{dataset::Keep, numeric_lifecycle::HandlerKind};
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
    use yamaa_engine::dataset::Keep;
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
    use yamaa_engine::dataset::Keep;
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
    use yamaa_engine::dataset::{Keep, OrderTerm, SourceSelection};
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
fn secondary_lookup_plan(source: &Table, secondary: &Table) -> Result<DatasetPlan, PlanError> {
    use yamaa_engine::dataset::{Lookup, MatchKey, SecondarySource};
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
fn named_plan(
    source: &Table,
    secondary: &Table,
    item: yamaa_engine::dataset::Intermediate,
) -> Result<DatasetPlan, PlanError> {
    use yamaa_engine::dataset::{SecondarySource, SourceSchemas};
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
fn named_item() -> yamaa_engine::dataset::Intermediate {
    use yamaa_engine::dataset::{Intermediate, Keep, MatchKey, OrderTerm, SourceSelection};
    Intermediate {
        identifier: "SELECTED".into(),
        path: "intermediates[0]".into(),
        source: 0,
        keys: vec![MatchKey {
            source_column: 0,
            output_column: 0,
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
    item.filter = Some(positive(Read::Source(0)));
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
    item.filter = Some(positive(Read::Source(1)));
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
    item.keys[0].output_column = 1;
    assert_eq!(
        named_plan(&source, &secondary, item),
        Err(PlanError::InvalidLookup)
    );
    let mut item = named_item();
    item.filter = Some(positive(Read::Column(0)));
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
        ExecutionError::Capacity
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

/// Bind a replacement independently of assignment execution order.
fn conversion_handler(column: usize, value: Value) -> yamaa_engine::dataset::ConversionHandler {
    yamaa_engine::dataset::ConversionHandler {
        assignment_path: format!("columns.C{column}.derivation"),
        handler: yamaa_engine::numeric_lifecycle::LiteralHandler {
            spec_path: format!("columns.C{column}.derivation.unconvertible"),
            value,
        },
    }
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
        let attempt = bound.execute_observed(&source, limits());
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
    let attempt = bound.execute_observed(&empty, limits());
    assert!(attempt.result.unwrap().dataset.rows().is_empty());
    assert!(attempt.handler_counts.iter().all(|entry| entry.count == 0));
    let missing = plan
        .clone()
        .with_conversion_handlers(vec![conversion_handler(1, Value::Missing)])
        .unwrap()
        .execute_observed(&source, limits());
    assert_eq!(
        missing.result.unwrap().dataset.rows()[0],
        vec![Value::Int(1), Value::Missing, Value::Missing]
    );
    let bad = plan
        .clone()
        .with_conversion_handlers(vec![conversion_handler(1, Value::Str("still bad".into()))])
        .unwrap()
        .execute_observed(&source, limits());
    assert!(
        matches!(*bad.result.unwrap_err(),ExecutionError::Conversion { path, identity:Some(_), .. } if path=="columns.C1.derivation.unconvertible")
    );
    assert_eq!(bad.handler_counts[0].count, 1);
    let mut broken = source;
    broken.fail = Some((0, 1));
    let attempt = bound.execute_observed(&broken, limits());
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Cell { .. }
    ));
    assert!(attempt.handler_counts.iter().all(|entry| entry.count == 0));
}

/// Unused literals consume no scalar budget; invalid handler bindings read no cells.
#[test]
fn conversion_handler_admission_and_reached_resource_accounting() {
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
    let mut tiny = limits();
    tiny.scalar_text_bytes = 0;
    let bound = plan
        .with_conversion_handlers(vec![conversion_handler(
            0,
            Value::Str("large unused replacement".into()),
        )])
        .unwrap();
    assert!(bound.execute(&source, tiny).is_ok());
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
    .execute_observed(&source, tiny);
    assert!(matches!(
        *bad.result.unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_engine::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(bad.handler_counts[0].count, 0);
}

/// Raw record/group match values remain independent of already converted output keys.
#[test]
fn row_lookups_match_driver_fields_and_count_records_before_donor_reads() {
    use yamaa_engine::dataset::{RowLookup, RowMatchKey, SecondarySource, SourceSchemas};
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
                plan.execute_observed_sources(&driver, &[&right], limits())
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
    let failed = plan.execute_observed_sources(&driver, &[&right], limits());
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
        plan.execute_observed_sources(&missing, &[&right], limits())
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
            .execute_observed_sources(&failing, &[&right], limits())
            .result
            .unwrap_err(),
        ExecutionError::Cell { source_row: 0, .. }
    ));
    let mut tiny = limits();
    tiny.work_cells = 0;
    failing.reads.borrow_mut().clear();
    assert!(matches!(
        *plan
            .execute_observed_sources(&failing, &[&right], tiny)
            .result
            .unwrap_err(),
        ExecutionError::Limit { .. }
    ));
    assert!(failing.reads.borrow().is_empty());
}

/// Placement preserves filtering order without changing raw-key matching or duplicate priority.
#[test]
fn row_lookup_assignment_phase_controls_reads_for_discarded_candidates() {
    use yamaa_engine::dataset::{RowLookup, RowMatchKey, SecondarySource};
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
                let attempt = plan.execute_observed_sources(&driver, &[&right], limits());
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
                let attempt = plan.execute_observed_sources(&driver, &[&right], limits());
                assert!(matches!(
                    *attempt.result.unwrap_err(),
                    ExecutionError::MultipleMatches { match_count: 2, .. }
                ));
                assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
            }
        }
    }
}

mod dataset_functions {
    use super::*;
    use yamaa_core::value::ValueType;
    use yamaa_engine::{
        dataset::{BoundFunction, FunctionArgument, FunctionBindings, FunctionInput},
        function_invocation::{
            Argument, FailureKind, FunctionIdentity, HostError, InvocationPlan, Parameter, Presence,
        },
    };

    /// Fixed activated identity and independently declared host-argument ordering.
    fn signature(returns: ColumnType) -> InvocationPlan {
        InvocationPlan::new(
            FunctionIdentity {
                name: "sum".into(),
                contract_version: "1".into(),
                implementation_version: "2".into(),
                call: "project.sum".into(),
            },
            vec![
                Parameter {
                    name: "first".into(),
                    host_name: "lhs".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "second".into(),
                    host_name: "rhs".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "factor".into(),
                    host_name: "scale".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Optional(Value::Int(5)),
                },
            ],
            returns,
            false,
        )
        .unwrap()
    }

    struct Callbacks {
        signature: InvocationPlan,
        calls: Vec<Vec<(String, String)>>,
        result: Result<Value, &'static str>,
    }
    impl FunctionBindings for Callbacks {
        type Error = &'static str;
        /// Slot lookup borrows metadata and never invokes the callable.
        fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
            (slot == 0).then_some(&self.signature)
        }
        /// Capture the complete host call while preserving an opaque failure payload.
        fn call(
            &mut self,
            slot: usize,
            arguments: &[Argument<'_>],
        ) -> Result<Value, HostError<Self::Error>> {
            assert_eq!(slot, 0);
            self.calls.push(
                arguments
                    .iter()
                    .map(|a| (a.name.into(), format!("{:?}", a.value)))
                    .collect(),
            );
            self.result.clone().map_err(HostError::Raised)
        }
    }
    /// Construct an explicitly bound callback without activation or discovery effects.
    fn callbacks() -> Callbacks {
        Callbacks {
            signature: signature(ColumnType::Int),
            calls: vec![],
            result: Ok(Value::Int(11)),
        }
    }
    /// The authored order is deliberately the reverse of parameter declaration order.
    fn arguments() -> Vec<FunctionArgument> {
        vec![
            FunctionArgument {
                name: "second".into(),
                input: FunctionInput::Read(Read::Source(2)),
            },
            FunctionArgument {
                name: "first".into(),
                input: FunctionInput::Read(Read::Source(1)),
            },
        ]
    }
    /// Bind a call at one of the two assignment phases without changing its signature.
    fn plan(
        source: &Table,
        mode: RowMode,
        row_call: bool,
        predicate: Option<Filter>,
        signature: InvocationPlan,
        args: Vec<FunctionArgument>,
    ) -> Result<DatasetPlan, PlanError> {
        let call = assign(
            1,
            Expression::Function(BoundFunction::new(0, signature, args)?),
        );
        let mut row = vec![assign(0, Expression::Source(0))];
        let mut columns = vec![];
        if row_call {
            row.push(call);
        } else {
            columns.push(call);
        }
        DatasetPlan::new(
            source.schema.clone(),
            schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
            vec![RowTemplate {
                mode,
                assignments: row,
                filter: predicate,
            }],
            columns,
            vec![0],
            vec![],
        )
    }
    /// Three independent integer source fields with deterministic row membership.
    fn source(rows: Vec<Vec<Value>>) -> Table {
        table(
            &[
                ("ID", ColumnType::Int),
                ("A", ColumnType::Int),
                ("B", ColumnType::Int),
            ],
            rows,
        )
    }

    /// Phase observation must not repeat source reads or callbacks or expose partial output.
    #[test]
    fn phase_observation_preserves_execution_and_effects() {
        use yamaa_engine::dataset::ExecutionPhase::*;
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
        let measured =
            plan.execute_with_phase_observer(&source, &[], &mut calls, limits(), &mut |phase| {
                phases.push((phase, source.reads.borrow().len()))
            });
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
        let plain = plan.execute_observed_functions(&source, &[], &mut plain_calls, limits());
        assert_eq!(measured, plain);
        assert_eq!(calls.calls, plain_calls.calls);
        assert_eq!(*source.reads.borrow(), reads);
    }

    /// Every returned failure closes the reached phase without inventing later work.
    #[test]
    fn phase_observation_finishes_at_the_actual_failure_boundary() {
        use yamaa_engine::dataset::ExecutionPhase::*;
        for failure in [Admission, Derivation, OutputKeys, Verification] {
            let mut source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
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
            let measured = plan.execute_with_phase_observer(
                &source,
                &[],
                &mut callbacks(),
                limits(),
                &mut |phase| phases.push(phase),
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
            expected.truncate(expected.iter().position(|phase| *phase == failure).unwrap() + 1);
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
                .execute_observed_functions(&source, &[], &mut callbacks, limits())
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
            .execute_observed_functions(&source, &[], &mut callbacks, limits())
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
                .execute_observed_functions(&source, &[], &mut callbacks, limits())
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
                .execute_observed_functions(&empty, &[], &mut callbacks, limits())
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
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
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
                .execute_observed(&Unreadable, limits())
                .result
                .unwrap_err(),
            ExecutionError::FunctionBinding { slot: 0 }
        ));
        let mut callbacks = callbacks();
        callbacks.signature = signature(ColumnType::Str);
        assert!(matches!(
            *plan
                .execute_observed_functions(&Unreadable, &[], &mut callbacks, limits())
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
            let attempt = plan.execute_observed_functions(&source, &[], &mut callbacks, limits());
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
        let attempt = converted.execute_observed_functions(&source, &[], &mut callbacks, limits());
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

    /// Work exhaustion precedes argument reads and calls; text is charged before copying.
    #[test]
    fn function_limits_precede_callback_effects_and_reset_between_attempts() {
        let source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
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
        let mut tiny = limits();
        tiny.work_cells = 3;
        assert!(matches!(
            *plan
                .execute_observed_functions(&source, &[], &mut callbacks, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit { .. }
        ));
        assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
        assert!(callbacks.calls.is_empty());
        assert!(plan
            .execute_observed_functions(&source, &[], &mut callbacks, limits())
            .result
            .is_ok());
        assert_eq!(callbacks.calls.len(), 1);
        let mut args = arguments();
        args[0].input = FunctionInput::Literal(Value::Str("large".into()));
        let text_plan = super::dataset_functions::plan(
            &source,
            RowMode::Records,
            true,
            None,
            signature(ColumnType::Int),
            args,
        )
        .unwrap();
        tiny = limits();
        tiny.scalar_text_bytes = 0;
        assert!(matches!(
            *text_plan
                .execute_observed_functions(&source, &[], &mut callbacks, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit {
                resource: yamaa_engine::dataset::Resource::ScalarTextBytes,
                ..
            }
        ));
        assert_eq!(callbacks.calls.len(), 1);
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
                .execute_observed_functions(&source, &[], &mut calls, limits())
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
                .execute_observed_functions(&source, &[], &mut calls, limits())
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
                        BoundFunction::new(0, signature(ColumnType::Str), args(0, 0)).unwrap(),
                    ),
                ),
                assign(
                    2,
                    Expression::Function(
                        BoundFunction::new(0, signature(ColumnType::Str), args(1, 0)).unwrap(),
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
            plan.execute_observed_functions(&source, &[], &mut calls, limits())
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
    /// A key-grain call explicitly collects each source leaf in authored order.
    fn collected_arguments() -> Vec<FunctionArgument> {
        [("second", 2, "SOURCE.B"), ("first", 1, "SOURCE.A")]
            .into_iter()
            .map(|(name, column, identifier)| FunctionArgument {
                name: name.into(),
                input: FunctionInput::Collect {
                    column,
                    identifier: identifier.into(),
                },
            })
            .collect()
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
            let result = plan
                .execute_observed_functions(&source, &[], &mut calls, limits())
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
            .execute_observed_functions(&source, &[], &mut calls, limits())
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
                .execute_observed_functions(&source, &[], &mut calls, limits())
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

    /// Work limits stop before a collected scan; fresh attempts and text limits remain independent.
    #[test]
    fn collected_argument_budgets_precede_reads_and_callbacks() {
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
        let mut tiny = limits();
        tiny.work_cells = 14;
        assert!(matches!(
            *plan
                .execute_observed_functions(&source, &[], &mut calls, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit {
                resource: yamaa_engine::dataset::Resource::WorkCells,
                ..
            }
        ));
        assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
        assert!(calls.calls.is_empty());
        assert!(plan
            .execute_observed_functions(&source, &[], &mut calls, limits())
            .result
            .is_ok());
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
        tiny = limits();
        tiny.scalar_text_bytes = 4;
        calls.calls.clear();
        assert!(matches!(
            *text_plan
                .execute_observed_functions(&text_source, &[], &mut calls, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit {
                resource: yamaa_engine::dataset::Resource::ScalarTextBytes,
                ..
            }
        ));
        assert_eq!(*text_source.reads.borrow(), vec![(0, 0), (1, 0), (0, 2)]);
        assert!(calls.calls.is_empty());
        text_source.rows.clear();
        assert!(text_plan
            .execute_observed_functions(&text_source, &[], &mut calls, limits())
            .result
            .is_ok());
        assert!(calls.calls.is_empty());
    }
}

/// Regex work survives row/template boundaries; rejected attempts never refund counters.
#[test]
fn contains_limits_span_rows_templates_and_fresh_dataset_runs() {
    use yamaa_core::regex::{CompileLimits, MatchBudget, MatchLimits, Pattern};
    use yamaa_engine::dataset::Resource;
    let subject = "a".repeat(100);
    let pattern = Pattern::compile("z", CompileLimits::default()).unwrap();
    let mut measured = MatchBudget::new(MatchLimits::default());
    assert!(pattern
        .search_with_budget(&subject, MatchLimits::default(), &mut measured)
        .unwrap()
        .is_none());
    let usage = measured.used();
    let ceiling = 2 * usage.work.max(usage.state_cells) - 1;
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
        let error = plan
            .execute(
                &source,
                Limits {
                    work_cells: ceiling,
                    ..limits()
                },
            )
            .unwrap_err();
        assert!(
            matches!(*error, ExecutionError::Limit { resource: Resource::PredicateRegexWork | Resource::PredicateRegexStateCells, limit, .. } if limit == ceiling)
        );
        assert_eq!(source.reads.borrow().len(), 2);
        assert!(plan
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
}

/// Empty-output declaration checks share matching quotas across predicate plans.
#[test]
fn contains_declaration_checks_do_not_reset_match_budgets() {
    use yamaa_engine::dataset::Resource;
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
                Limits {
                    work_cells: 1000,
                    ..limits()
                }
            )
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
    // Each literal search fits alone, while both exhaust this dataset-wide state quota.
    let error = plan
        .execute(
            &source,
            Limits {
                work_cells: 1000,
                ..limits()
            },
        )
        .unwrap_err();
    assert!(matches!(
        *error,
        ExecutionError::Limit {
            resource: Resource::PredicateRegexWork | Resource::PredicateRegexStateCells,
            limit: 1000,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().verifications.len(),
        2
    );
}

#[test]
fn numeric_reductions_check_present_values_after_collecting_source_arguments() {
    use yamaa_core::value::ValueType;
    use yamaa_engine::table_reduction::TableReductionError;
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
            plan.execute(&source, limits()).unwrap().dataset.rows(),
            &[vec![Value::Str("A".into()), Value::Missing]]
        );
        source.rows[1][1] = Value::Str("2".into());
        // Numeric-looking text is not coerced. The authored expression/operand
        // remain attached to the runtime type finding, with no output identity.
        for (plan, name) in [(&plan, "T.x"), (&fallback, "x")] {
            assert_eq!(
                *plan.execute(&source, limits()).unwrap_err(),
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
            *plan.execute(&source, limits()).unwrap_err(),
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
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
}
