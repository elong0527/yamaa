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
        ExecutionError::Limit {
            resource: Resource::SourceRows,
            limit: 10,
            required: Some(11)
        }
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
fn pooled_output_geometry_spends_actual_counts_and_refuses_the_next_growth() {
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let limits = Limits {
        work_cells: 1000,
        ..limits()
    };
    let owner = WorkBudget::pooled(limits);
    let seven = literal_rows(&table, 7, ColumnType::Int, false);
    let one = literal_rows(&table, 1, ColumnType::Int, false);
    assert_eq!(
        seven
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
            .result
            .unwrap()
            .dataset
            .row_count(),
        7
    );
    assert_eq!(
        one.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
            .result
            .unwrap()
            .dataset
            .row_count(),
        1
    );
    let before = owner.usage().unwrap();
    assert_eq!((before.output_rows, before.output_cells), (8, 8));
    assert!(seven
        .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
        .result
        .is_err());
    let after = owner.usage().unwrap();
    assert_eq!((after.output_rows, after.output_cells), (8, 8));
    assert!(after.runtime.work >= before.runtime.work);
}
#[test]
fn pooled_text_failure_commits_the_known_prefix_and_unwind_reserves_unknown_usage() {
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let limits = Limits {
        work_cells: 1000,
        scalar_text_bytes: 100,
        ..limits()
    };
    let owner = WorkBudget::pooled(limits);
    let text = literal_rows(&table, 1, ColumnType::Str, true);
    text.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
        .result
        .unwrap();
    let before = owner.usage().unwrap();
    assert_eq!(before.output_text_bytes, 6);
    assert!(matches!(
        *text
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            limit: 10,
            required: Some(12)
        }
    ));
    let failed = owner.usage().unwrap();
    assert_eq!(failed.output_text_bytes, 6);
    assert_eq!(failed.output_rows, 1);
    assert!(failed.runtime.work >= before.runtime.work);
    let (mut failing, source) = fixture(Expression::Source(0));
    let payload = Arc::new(());
    failing.panic = Some(Arc::clone(&payload));
    let held = catch_unwind(AssertUnwindSafe(|| {
        source.execute_with_work_budget(&failing, &[], &mut NoFunctions::new(), limits, &owner)
    }))
    .unwrap_err();
    assert!(Arc::ptr_eq(
        held.downcast_ref::<Arc<()>>().unwrap(),
        &payload
    ));
    assert_eq!(owner.usage(), None);
    assert_eq!(owner.used(), owner.limit());
    assert!(matches!(
        *text
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
            .result
            .unwrap_err(),
        ExecutionError::Capacity
    ));
    assert_eq!(owner.usage(), None);
}

#[test]
fn pooled_identity_failures_keep_actual_cell_and_text_prefixes_across_nodes() {
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let duplicate = literal_rows(&table, 2, ColumnType::Str, true);
    for (cells, text, resource, required) in [
        (1, 100, Resource::IdentityCells, 2),
        (100, 6, Resource::IdentityTextBytes, 12),
    ] {
        let limits = Limits {
            work_cells: 1000,
            scalar_text_bytes: 100,
            output_text_bytes: 100,
            identity_cells: cells,
            identity_text_bytes: text,
            ..limits()
        };
        let owner = WorkBudget::pooled(limits);
        assert!(matches!(
            *duplicate
                .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
                .result
                .unwrap_err(),
            ExecutionError::KeyFailures(_)
        ));
        let before = owner.usage().unwrap();
        assert_eq!((before.identity_cells, before.identity_text_bytes), (1, 6));
        assert!(
            matches!(*duplicate.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner).result.unwrap_err(), ExecutionError::Limit { resource: actual, limit, required: Some(count) } if actual == resource && count == required && limit == if cells == 1 { cells } else { text })
        );
        let after = owner.usage().unwrap();
        if cells == 1 {
            assert_eq!((after.identity_cells, after.identity_text_bytes), (1, 6));
        } else {
            assert_eq!((after.identity_cells, after.identity_text_bytes), (2, 6));
        }
        assert_eq!(after.output_rows, 0);
    }
}
#[test]
fn pooled_scalar_and_matching_counters_do_not_restart_at_a_node_boundary() {
    use yamaa_engine::dataset::{Check, ColumnVerifications, Verification};
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let text = literal_rows(&table, 1, ColumnType::Str, true);
    let scalar_limits = Limits {
        work_cells: 1000,
        output_text_bytes: 100,
        ..limits()
    };
    let scalar = WorkBudget::pooled(scalar_limits);
    text.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), scalar_limits, &scalar)
        .result
        .unwrap();
    assert_eq!(scalar.usage().unwrap().runtime.text_bytes, 6);
    assert!(matches!(
        *text
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), scalar_limits, &scalar)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: 10,
            required: Some(12)
        }
    ));
    assert_eq!(scalar.usage().unwrap().runtime.text_bytes, 6);
    let pattern = yamaa_core::regex::Pattern::compile("abcdef", Default::default()).unwrap();
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
    let limits = Limits {
        work_cells: 100_000,
        scalar_text_bytes: 100,
        output_text_bytes: 100,
        ..limits()
    };
    let owner = WorkBudget::pooled(limits);
    checked
        .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
        .result
        .unwrap();
    let before = owner.usage().unwrap();
    assert_eq!(before.runtime.regex.subject_bytes, 72);
    assert!(checked
        .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
        .result
        .is_err());
    let after = owner.usage().unwrap();
    assert_eq!(after.runtime.regex.subject_bytes, 96);
    assert_eq!(after.runtime.text_bytes, 12);
    assert!(after.runtime.regex.work > before.runtime.regex.work);
    assert!(after.runtime.regex.state_cells > before.runtime.regex.state_cells);
}
#[test]
fn pooled_predicate_resolutions_follow_actual_entered_rows() {
    use yamaa_core::predicate::{self, Comparison, Node, Scalar};
    use yamaa_engine::dataset_predicate::{Binding, BoundPredicate, Read};
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
            Default::default(),
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
    let limits = Limits {
        work_cells: 1000,
        ..limits()
    };
    let owner = WorkBudget::pooled(limits);
    for count in 1..=2 {
        plan.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), limits, &owner)
            .result
            .unwrap();
        assert_eq!(owner.usage().unwrap().runtime.resolutions, count);
    }
}

#[test]
fn pooled_owner_bounds_survive_local_grant_changes_without_losing_known_usage() {
    let (table, _) = fixture(Expression::Literal(Value::Int(1)));
    let text = literal_rows(&table, 1, ColumnType::Str, true);
    let pool = Limits {
        work_cells: 1000,
        output_text_bytes: 100,
        ..limits()
    };
    let owner = WorkBudget::pooled(pool);
    let large = Limits {
        scalar_text_bytes: 100,
        ..pool
    };
    text.execute_with_work_budget(&table, &[], &mut NoFunctions::new(), large, &owner)
        .result
        .unwrap();
    let before = owner.usage().unwrap();
    assert_eq!(before.runtime.text_bytes, 6);
    let small = Limits {
        scalar_text_bytes: 5,
        ..pool
    };
    assert!(matches!(
        *text
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), small, &owner)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: 5,
            required: Some(6)
        }
    ));
    assert_eq!(owner.usage(), Some(before));
    assert!(matches!(
        *text
            .execute_with_work_budget(&table, &[], &mut NoFunctions::new(), large, &owner)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: 10,
            required: Some(12)
        }
    ));
    assert_eq!(owner.usage().unwrap().runtime.text_bytes, 6);
}
