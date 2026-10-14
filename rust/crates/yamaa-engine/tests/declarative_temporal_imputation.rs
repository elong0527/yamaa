use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{
        date_policy, Assignment, DatasetPlan, DayDeclaration as D, Expression, Imputation,
        ImputationKind as K, RowMode, RowTemplate,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::DatePrecision as P,
    value::{ColumnType as T, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits, Resource},
    numeric_lifecycle::HandlerKind,
};
#[derive(Debug, PartialEq)]
struct Failure(Box<u8>, Rc<()>);
struct Table {
    schema: TableSchema,
    source: Value,
    bound: Value,
    fail: RefCell<Option<Failure>>,
    bad_column: usize,
    reads: RefCell<Vec<usize>>,
}
impl TableAccess for Table {
    type Error = Failure;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, _: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
        self.reads.borrow_mut().push(column);
        if column == self.bad_column {
            if let Some(failure) = self.fail.borrow_mut().take() {
                return Err(CellError::Access(failure));
            }
        }
        Ok(match column {
            0 => ValueRef::Int(1),
            1 => ValueRef::from(&self.source),
            2 => ValueRef::from(&self.bound),
            _ => panic!("column"),
        })
    }
}
fn schema(fields: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        fields
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
fn make_table(source: Value) -> Table {
    Table {
        schema: schema(&[("ID", T::Int), ("TEXT", T::Str), ("REF", T::Date)]),
        source,
        bound: Value::Date("2024-02-20".parse().unwrap()),
        fail: RefCell::default(),
        bad_column: usize::MAX,
        reads: RefCell::default(),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 1000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
fn make_plan(table: &Table, missing: Option<Value>, invalid: Option<Value>) -> DatasetPlan {
    let kind = date_policy(Some("6"), D::Position("first"), P::Year, "impute").unwrap();
    let operation = Imputation::new(
        kind,
        "SRC.TEXT".into(),
        Some("SRC.REF".into()),
        "columns.DATE.derivation.date_impute".into(),
        missing,
        invalid,
    )
    .unwrap();
    DatasetPlan::new(
        table.schema.clone(),
        schema(&[("ID", T::Int), ("DATE", T::Date)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: "columns.DATE.derivation".into(),
                    expression: Expression::Imputation {
                        operation,
                        operands: vec![
                            Assignment {
                                column: 1,
                                path: "columns.DATE.derivation.date_impute.source".into(),
                                expression: Expression::Source(1),
                            },
                            Assignment {
                                column: 1,
                                path: "columns.DATE.derivation.date_impute.not_before".into(),
                                expression: Expression::Source(2),
                            },
                        ],
                    },
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
#[test]
fn complete_sources_and_recovered_values_never_read_the_bound() {
    for source in [
        Value::Str("2024-02-01".into()),
        Value::Missing,
        Value::Str("invalid".into()),
    ] {
        let mut table = make_table(source);
        table.bad_column = 2;
        *table.fail.borrow_mut() = Some(Failure(Box::new(7), Rc::new(())));
        let result = make_plan(&table, Some(Value::Missing), Some(Value::Missing))
            .execute_observed(&table, limits());
        assert!(result.result.is_ok());
        assert_eq!(*table.reads.borrow(), [0, 1]);
        assert!(table.fail.borrow().is_some());
        let expected_missing = u64::from(table.source == Value::Missing);
        let expected_invalid = u64::from(table.source == Value::Str("invalid".into()));
        assert_eq!(
            result
                .handler_counts
                .iter()
                .map(|count| (count.handler, count.count))
                .collect::<Vec<_>>(),
            [
                (HandlerKind::Missing, expected_missing),
                (HandlerKind::Invalid, expected_invalid)
            ]
        );
    }
}
#[test]
fn opaque_bound_failure_preserves_nonclone_payload_identity_and_is_never_retried() {
    let mut table = make_table(Value::Str("2024-02".into()));
    table.bad_column = 2;
    let marker = Rc::new(());
    let failure = Failure(Box::new(9), marker.clone());
    let address = &*failure.0 as *const u8;
    *table.fail.borrow_mut() = Some(failure);
    let result = make_plan(&table, Some(Value::Missing), Some(Value::Missing))
        .execute_observed(&table, limits());
    let ExecutionError::Cell {
        path,
        error: CellError::Access(Failure(payload, retained_marker)),
        ..
    } = *result.result.unwrap_err()
    else {
        panic!("original source failure")
    };
    assert_eq!(path, "columns.DATE.derivation.date_impute.not_before");
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained_marker));
    assert_eq!(*table.reads.borrow(), [0, 1, 2]);
    assert!(result.handler_counts.iter().all(|count| count.count == 0));
}
#[test]
fn runtime_impute_failure_retains_current_key_and_is_not_answered_by_invalid_handler() {
    let mut table = make_table(Value::Str("2024-02".into()));
    let mut plan = make_plan(&table, None, Some(Value::Missing));
    // A real invalid completion cannot be answered by the text-invalid handler.
    let assignment = &plan.templates()[0].assignments[1];
    let Expression::Imputation { operands, .. } = &assignment.expression else {
        panic!("imputation")
    };
    let operation = Imputation::new(
        date_policy(Some("6"), D::Number("31"), P::Year, "impute").unwrap(),
        "SRC.TEXT".into(),
        Some("SRC.REF".into()),
        "columns.DATE.derivation.date_impute".into(),
        None,
        Some(Value::Missing),
    )
    .unwrap();
    plan = DatasetPlan::new(
        table.schema.clone(),
        plan.output().clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                plan.templates()[0].assignments[0].clone(),
                Assignment {
                    column: 1,
                    path: assignment.path.clone(),
                    expression: Expression::Imputation {
                        operation,
                        operands: operands.clone(),
                    },
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&table, limits());
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("calendar cause")
    };
    assert_eq!(diagnostic.definition().condition, "invalid_calendar_date");
    assert_eq!(identity.unwrap().values, [Value::Int(1)]);
    assert_eq!(*table.reads.borrow(), [0, 1, 2]);
    assert_eq!(attempt.handler_counts[0].count, 0);
    // The original evaluator resolves the bound before rejecting a bad completion.
    table.reads.borrow_mut().clear();
    table.bad_column = 2;
    *table.fail.borrow_mut() = Some(Failure(Box::new(6), Rc::new(())));
    assert!(matches!(
        *plan.execute_observed(&table, limits()).result.unwrap_err(),
        ExecutionError::Cell {
            error: CellError::Access(_),
            ..
        }
    ));
    assert_eq!(*table.reads.borrow(), [0, 1, 2]);
}
#[test]
fn cumulative_work_quota_precedes_parsing_and_current_key_copy_is_separately_admitted() {
    let table = make_table(Value::Str("2024-02".into()));
    let plan = make_plan(&table, None, None);
    let mut bounded = limits();
    bounded.work_cells = 0;
    assert!(matches!(
        *plan.execute_observed(&table, bounded).result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(table.reads.borrow().is_empty());
    let source = make_table(Value::Str("2024-02".into()));
    let operation = make_plan(&source, Some(Value::Missing), Some(Value::Missing));
    let mut bounded = limits();
    bounded.scalar_text_bytes = 7;
    assert!(matches!(
        *operation
            .execute_observed(&source, bounded)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(*source.reads.borrow(), [0, 1]);
    let table = make_table(Value::Str("invalid".into()));
    let plan = make_plan(&table, None, None);
    let mut bounded = limits();
    bounded.identity_cells = 0;
    assert!(matches!(
        *plan.execute_observed(&table, bounded).result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::IdentityCells,
            ..
        }
    ));
    assert_eq!(*table.reads.borrow(), [0, 1]);
}
#[test]
fn recovered_datetime_literal_is_converted_once_and_its_handler_is_counted() {
    let table = make_table(Value::Missing);
    let operation = Imputation::new(
        K::DateTime {
            time: yamaa_core::dataset::TimeRule::First,
        },
        "SRC.TEXT".into(),
        None,
        "impute".into(),
        Some(Value::Str("2024-02-29T12:13".into())),
        None,
    )
    .unwrap();
    let plan = DatasetPlan::new(
        table.schema.clone(),
        schema(&[("ID", T::Int), ("DATE", T::DateTime)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "ID".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: "impute".into(),
                    expression: Expression::Imputation {
                        operation,
                        operands: vec![Assignment {
                            column: 1,
                            path: "impute.source".into(),
                            expression: Expression::Source(1),
                        }],
                    },
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&table, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [vec![
            Value::Int(1),
            Value::DateTime("2024-02-29T12:13:00".parse().unwrap())
        ]]
    );
    assert_eq!(attempt.handler_counts[0].handler, HandlerKind::Missing);
    assert_eq!(attempt.handler_counts[0].count, 1);
    assert_eq!(*table.reads.borrow(), [0, 1]);
}
