//! Independent attempts retain exact values, original failures and effect order.
use std::{
    cell::{Cell, RefCell},
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
    sync::Arc,
};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_core::{
    dataset::{ExecutionError, Resource},
    function_invocation::NoFunctions,
    Limits,
};

#[derive(Debug)]
struct Original {
    owner: Rc<()>,
    value: Box<u8>,
}
struct Table {
    schema: TableSchema,
    rows: usize,
    cells: Cell<usize>,
    error: RefCell<Option<Original>>,
    panic: Option<Arc<()>>,
}
impl TableAccess for Table {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        self.cells.set(self.cells.get() + 1);
        if let Some(payload) = &self.panic {
            std::panic::panic_any(Arc::clone(payload));
        }
        if let Some(error) = self.error.borrow_mut().take() {
            return Err(CellError::Access(error));
        }
        if row >= self.rows || column != 0 {
            return Err(CellError::OutOfBounds { row, column });
        }
        Ok(ValueRef::Int(row as i64 + 1))
    }
}
fn schema(kind: ColumnType) -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind,
    }])
    .unwrap()
}
fn fixture(expression: Expression) -> (Table, DatasetPlan) {
    let schema = schema(ColumnType::Int);
    let plan = DatasetPlan::new(
        schema.clone(),
        schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![Assignment {
                column: 0,
                path: "columns.ID".into(),
                expression,
            }],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    (
        Table {
            schema,
            rows: 1,
            cells: Cell::new(0),
            error: RefCell::new(None),
            panic: None,
        },
        plan,
    )
}
#[test]
fn repeated_literal_attempts_keep_exact_rows_without_source_reads() {
    let (table, plan) = fixture(Expression::Literal(Value::Int(1)));
    for _ in 0..2 {
        let attempted = plan.execute(&table, &[], &mut NoFunctions::new(), None);
        assert_eq!(
            attempted.result.unwrap().dataset.rows(),
            [vec![Value::Int(1)]]
        );
        assert_eq!(table.cells.get(), 0);
    }
}

#[test]
fn schema_and_fixed_shape_refusals_precede_source_reads() {
    let (mut table, plan) = fixture(Expression::Literal(Value::Int(1)));
    plan.execute(&table, &[], &mut NoFunctions::new(), None)
        .result
        .unwrap();
    table.schema = schema(ColumnType::Str);
    assert!(matches!(
        *plan
            .execute(&table, &[], &mut NoFunctions::new(), None)
            .result
            .unwrap_err(),
        ExecutionError::SchemaMismatch
    ));
    assert_eq!(table.cells.get(), 0);
    table.schema = schema(ColumnType::Int);
    table.rows = Limits::TABLE_ROWS + 1;
    *table.error.borrow_mut() = Some(Original {
        owner: Rc::new(()),
        value: Box::new(0),
    });
    assert!(matches!(
        *plan.execute(&table, &[], &mut NoFunctions::new(), None).result.unwrap_err(),
        ExecutionError::SourceLimit {
            resource: Resource::SourceRows,
            limit: Limits::TABLE_ROWS,
            required: Some(required),
            source_index: 0,
        } if required == Limits::TABLE_ROWS + 1
    ));
    assert_eq!(table.cells.get(), 0);
    assert!(table.error.borrow().is_some());
}

#[test]
fn returned_port_failure_keeps_its_original_owner_and_completed_observations() {
    let (table, plan) = fixture(Expression::Source(0));
    let owner = Rc::new(());
    let value = Box::new(27);
    let original = &*value as *const u8;
    *table.error.borrow_mut() = Some(Original {
        owner: Rc::clone(&owner),
        value,
    });
    let attempted = plan.execute(&table, &[], &mut NoFunctions::new(), None);
    let ExecutionError::Cell {
        error: CellError::Access(error),
        source_row: 0,
        ..
    } = *attempted.result.unwrap_err()
    else {
        panic!("original access owner was replaced")
    };
    assert!(Rc::ptr_eq(&error.owner, &owner));
    assert_eq!(&*error.value as *const u8, original);
    assert_eq!(table.cells.get(), 1);
    assert!(attempted.handler_counts.is_empty());
    assert!(attempted.retained_verifications.is_empty());
}

#[test]
fn unwind_keeps_its_owner_and_a_fresh_attempt_uses_the_same_immutable_plan() {
    let (mut table, plan) = fixture(Expression::Source(0));
    let original = Arc::new(());
    table.panic = Some(Arc::clone(&original));
    let raised = catch_unwind(AssertUnwindSafe(|| {
        plan.execute(&table, &[], &mut NoFunctions::new(), None)
    }))
    .unwrap_err();
    assert!(Arc::ptr_eq(
        raised.downcast_ref::<Arc<()>>().unwrap(),
        &original
    ));
    assert_eq!(table.cells.get(), 1);
    table.panic = None;
    assert_eq!(
        plan.execute(&table, &[], &mut NoFunctions::new(), None)
            .result
            .unwrap()
            .dataset
            .rows(),
        [vec![Value::Int(1)]]
    );
    assert_eq!(table.cells.get(), 2);
}

fn literal_rows(table: &Table, count: usize, kind: ColumnType, text: bool) -> DatasetPlan {
    DatasetPlan::new(
        table.schema.clone(),
        schema(kind),
        (0..count)
            .map(|row| RowTemplate {
                mode: RowMode::Records,
                assignments: vec![Assignment {
                    column: 0,
                    path: "columns.ID".into(),
                    expression: Expression::Literal(if text {
                        Value::Str("abcdef".into())
                    } else {
                        Value::Int(row as i64 + 1)
                    }),
                }],
                filter: None,
            })
            .collect(),
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
#[test]
fn separate_attempts_keep_their_original_output_geometry_and_text() {
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let seven = literal_rows(&table, 7, ColumnType::Int, false);
    let one = literal_rows(&table, 1, ColumnType::Int, false);
    assert_eq!(
        seven
            .execute(&table, &[], &mut NoFunctions::new(), None)
            .result
            .unwrap()
            .dataset
            .row_count(),
        7
    );
    assert_eq!(
        one.execute(&table, &[], &mut NoFunctions::new(), None)
            .result
            .unwrap()
            .dataset
            .row_count(),
        1
    );
    let text = literal_rows(&table, 1, ColumnType::Str, true);
    for _ in 0..2 {
        assert_eq!(
            text.execute(&table, &[], &mut NoFunctions::new(), None)
                .result
                .unwrap()
                .dataset
                .rows(),
            [vec![Value::Str("abcdef".into())]]
        );
    }
}

#[test]
fn duplicate_identity_failures_remain_fatal_on_each_attempt() {
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let duplicate = literal_rows(&table, 2, ColumnType::Str, true);
    for _ in 0..2 {
        assert!(matches!(
            *duplicate
                .execute(&table, &[], &mut NoFunctions::new(), None)
                .result
                .unwrap_err(),
            ExecutionError::KeyFailures(_)
        ));
    }
}

#[test]
fn repeated_column_patterns_keep_their_original_successful_findings() {
    use yamaa_core::dataset::{Check, ColumnVerifications, Verification};
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let text = literal_rows(&table, 1, ColumnType::Str, true);
    let pattern = yamaa_core::regex::Pattern::compile("abcdef").unwrap();
    let checked = text
        .with_column_verifications(vec![ColumnVerifications {
            column: 0,
            checks: (0..12)
                .map(|index| Verification {
                    path: format!("columns.ID.verifications[{index}].matches"),
                    check: Check::Matches(pattern.clone()),
                })
                .collect(),
        }])
        .unwrap();
    let result = checked
        .execute(&table, &[], &mut NoFunctions::new(), None)
        .result
        .unwrap();
    assert_eq!(result.dataset.rows(), [vec![Value::Str("abcdef".into())]]);
    assert_eq!(result.verifications.len(), 12);
    assert!(result
        .verifications
        .iter()
        .all(|check| check.evaluated_count == 1 && check.failed_count == 0));
}

#[test]
fn repeated_predicate_attempts_keep_the_original_source_values() {
    use yamaa_core::bound_expression::{Binding, BoundPredicate, Read};
    use yamaa_core::predicate::{self, Comparison, Node, Scalar};
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let filter = BoundPredicate::new(
        predicate::Plan::new(
            vec![Node::Compare {
                operator: Comparison::Greater,
                left: Scalar::Identifier("x".into()),
                right: Scalar::Literal(Value::Int(0)),
            }],
            0,
            "rows[0].filter".into(),
            "x > 0".into(),
        )
        .unwrap(),
        vec![Binding {
            name: "x".into(),
            read: Read::Source(0),
        }],
    )
    .unwrap();
    let plan = DatasetPlan::new(
        table.schema.clone(),
        table.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![Assignment {
                column: 0,
                path: "columns.ID".into(),
                expression: Expression::Source(0),
            }],
            filter: Some(filter),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    for _ in 0..2 {
        assert_eq!(
            plan.execute(&table, &[], &mut NoFunctions::new(), None)
                .result
                .unwrap()
                .dataset
                .rows(),
            [vec![Value::Int(1)]]
        );
    }
}
