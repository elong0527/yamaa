//! Actual cumulative charges, closed refusals and original failure owners.
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
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits, Resource},
    execution_work::WorkBudget,
    function_invocation::NoFunctions,
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
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 10,
        key_cells: 10,
        work_cells: 1,
        scalar_text_bytes: 10,
        output_text_bytes: 10,
        identity_cells: 10,
        identity_text_bytes: 10,
    }
}
#[test]
fn two_actual_executions_share_equality_and_one_under_without_node_grants() {
    // One literal assignment costs one unit; two complete output-key scans cost two.
    let (table, plan) = fixture(Expression::Literal(Value::Int(1)));
    for (ceiling, success, used) in [(6, true, 6), (5, false, 5)] {
        let work = WorkBudget::new(ceiling);
        let first =
            plan.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work);
        assert_eq!(first.result.unwrap().dataset.rows(), [vec![Value::Int(1)]]);
        assert_eq!(work.used(), 3);
        let second =
            plan.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work);
        if success {
            assert_eq!(second.result.unwrap().dataset.rows(), [vec![Value::Int(1)]]);
        } else {
            assert!(matches!(
                *second.result.unwrap_err(),
                ExecutionError::Limit {
                    resource: Resource::WorkCells,
                    limit: 5,
                    required: Some(6)
                }
            ));
        }
        assert_eq!(work.limit(), ceiling);
        assert_eq!(work.used(), used);
        assert_eq!(table.cells.get(), 0);
    }
}
#[test]
fn static_shape_rejections_preserve_the_completed_work_prefix_and_do_not_enter_data() {
    let (mut table, plan) = fixture(Expression::Literal(Value::Int(1)));
    let work = WorkBudget::new(6);
    plan.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work)
        .result
        .unwrap();
    table.schema = schema(ColumnType::Str);
    assert!(matches!(
        *plan
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work)
            .result
            .unwrap_err(),
        ExecutionError::SchemaMismatch
    ));
    assert_eq!(work.used(), 3);
    assert_eq!(table.cells.get(), 0);
    table.schema = schema(ColumnType::Int);
    table.rows = 11;
    assert!(matches!(
        *plan
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work)
            .result
            .unwrap_err(),
        ExecutionError::Capacity
    ));
    assert_eq!(work.used(), 3);
    assert_eq!(table.cells.get(), 0);
}
#[test]
fn a_returned_port_failure_keeps_its_original_owner_and_actual_admitted_charges() {
    let (table, plan) = fixture(Expression::Source(0));
    let owner = Rc::new(());
    let value = Box::new(27);
    let original = &*value as *const u8;
    *table.error.borrow_mut() = Some(Original {
        owner: Rc::clone(&owner),
        value,
    });
    let work = WorkBudget::new(10);
    let attempted =
        plan.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work);
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
    assert_eq!(work.used(), 2);
    assert_eq!(table.cells.get(), 1);
    assert!(attempted.handler_counts.is_empty());
    assert!(attempted.retained_verifications.is_empty());
}
#[test]
fn an_unwind_preserves_its_owner_and_prevents_a_second_grant_of_remaining_work() {
    let (mut table, plan) = fixture(Expression::Source(0));
    let original = Arc::new(());
    table.panic = Some(Arc::clone(&original));
    let work = WorkBudget::new(10);
    let raised = catch_unwind(AssertUnwindSafe(|| {
        plan.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work)
    }))
    .unwrap_err();
    assert!(Arc::ptr_eq(
        raised.downcast_ref::<Arc<()>>().unwrap(),
        &original
    ));
    assert_eq!(work.used(), 10);
    assert_eq!(table.cells.get(), 1);
    table.panic = None;
    assert!(matches!(
        *plan
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits(), &work)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: 10,
            required: Some(11)
        }
    ));
    assert_eq!(work.used(), 10);
    assert_eq!(table.cells.get(), 1);
}
