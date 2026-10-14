use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    diagnostic::{ConditionCode, ContextValue},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    text_template::StringTemplate,
    value::{ColumnType as T, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits, Resource},
    numeric_lifecycle::HandlerKind,
};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    failure: RefCell<Option<(usize, Failure)>>,
    reads: RefCell<Vec<(usize, usize)>>,
}
impl TableAccess for Table {
    type Error = Failure;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
        self.reads.borrow_mut().push((row, column));
        if self
            .failure
            .borrow()
            .as_ref()
            .is_some_and(|(selected, _)| *selected == column)
        {
            let (_, error) = self.failure.borrow_mut().take().unwrap();
            return Err(CellError::Access(error));
        }
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: T::Str,
            })
            .collect(),
    )
    .unwrap()
}
fn table(a: Value, b: Value) -> Table {
    Table {
        schema: schema(&["ID", "A", "B"]),
        rows: vec![vec![Value::Str("row1".into()), a, b]],
        failure: RefCell::default(),
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
const PATH: &str = "columns.RESULT.derivation.str_template";
fn assignment(template: &str, missing: Option<Value>) -> Assignment {
    let operation = StringTemplate::new(template.into(), PATH.into(), missing).unwrap();
    let operands = operation
        .identifiers()
        .iter()
        .map(|name| Assignment {
            column: 1,
            path: format!("{PATH}.template"),
            expression: Expression::Source(if name == "A" { 1 } else { 2 }),
        })
        .collect();
    Assignment {
        column: 1,
        path: PATH.into(),
        expression: Expression::StrTemplate {
            operands,
            operation,
        },
    }
}
fn from_assignment(assignment: Assignment) -> DatasetPlan {
    DatasetPlan::new(
        schema(&["ID", "A", "B"]),
        schema(&["ID", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                assignment,
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
fn plan(template: &str, missing: Option<Value>) -> DatasetPlan {
    from_assignment(assignment(template, missing))
}
#[test]
fn repeated_occurrences_are_independent_reads_in_authored_order() {
    let table = table(Value::Str("alpha".into()), Value::Str("beta".into()));
    let result = plan("{A}-{A}-{B}", None).execute_observed(&table, limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [vec![
            Value::Str("row1".into()),
            Value::Str("alpha-alpha-beta".into())
        ]]
    );
    assert!(result.handler_counts.is_empty());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (0, 1), (0, 2)]);
}
#[test]
fn first_missing_recovery_stops_before_later_failure_and_retains_literal_presence() {
    for missing in [None, Some(Value::Missing), Some(Value::Str("MiXeD".into()))] {
        let table = table(Value::Missing, Value::Str("later".into()));
        *table.failure.borrow_mut() = Some((2, Failure(Box::new(9), Rc::new(()))));
        let result = plan("{A}:{B}", missing.clone()).execute_observed(&table, limits());
        if let Some(expected) = missing {
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [vec![Value::Str("row1".into()), expected]]
            );
            assert_eq!(result.handler_counts.len(), 1);
            assert_eq!(result.handler_counts[0].handler, HandlerKind::Missing);
            assert_eq!(result.handler_counts[0].count, 1);
        } else {
            let ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            } = *result.result.unwrap_err()
            else {
                panic!("missing cause")
            };
            assert_eq!(diagnostic.code, ConditionCode::StrCaseMissing);
            assert_eq!(diagnostic.spec_paths, [PATH]);
            assert_eq!(
                diagnostic.context,
                [(
                    "variable".into(),
                    ContextValue::Scalar(Value::Str("A".into()))
                )]
                .into()
            );
            assert_eq!(identity.unwrap().values, [Value::Str("row1".into())]);
            assert!(result.handler_counts.is_empty());
        }
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        assert!(table.failure.borrow().is_some());
    }
}
#[test]
fn opaque_nonclone_allocation_and_rc_move_without_retry_or_later_effects() {
    let table = table(Value::Missing, Value::Missing);
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *table.failure.borrow_mut() = Some((1, Failure(payload, marker.clone())));
    let result =
        plan("{A}:{B}", Some(Value::Str("replacement".into()))).execute_observed(&table, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *result.result.unwrap_err()
    else {
        panic!("original cell error")
    };
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained));
    assert_eq!(path, format!("{PATH}.template"));
    assert_eq!(source_row, 0);
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(result.handler_counts.iter().all(|c| c.count == 0));
}
#[test]
fn type_failure_precedes_later_error_without_formatting_or_recovery() {
    let table = table(Value::Int(7), Value::Missing);
    *table.failure.borrow_mut() = Some((2, Failure(Box::new(9), Rc::new(()))));
    let result = plan("{A}:{B}", Some(Value::Missing)).execute_observed(&table, limits());
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *result.result.unwrap_err()
    else {
        panic!("type")
    };
    assert_eq!(diagnostic.code, ConditionCode::StrCaseInput);
    assert_eq!(diagnostic.spec_paths, [format!("{PATH}.template")]);
    assert_eq!(
        diagnostic.context,
        [
            (
                "source".into(),
                ContextValue::Scalar(Value::Str("A".into()))
            ),
            (
                "expected".into(),
                ContextValue::Scalar(Value::Str("str".into()))
            ),
            (
                "actual".into(),
                ContextValue::Scalar(Value::Str("int".into()))
            )
        ]
        .into()
    );
    assert!(identity.is_none());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(result.handler_counts.iter().all(|c| c.count == 0));
}
#[test]
fn quotas_refuse_complete_output_or_identity_and_fresh_runs_reset_counters() {
    let table = table(Value::Str("a".repeat(40)), Value::Str("b".repeat(40)));
    let plan = plan("{A}-{A}-{B}", Some(Value::Missing));
    let mut quota = limits();
    quota.scalar_text_bytes = 200;
    assert!(matches!(
        *plan.execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    for _ in 0..2 {
        assert_eq!(
            plan.execute(&table, limits()).unwrap().dataset.rows()[0][1],
            Value::Str(format!(
                "{}-{}-{}",
                "a".repeat(40),
                "a".repeat(40),
                "b".repeat(40)
            ))
        );
    }
    let table = self::table(Value::Missing, Value::Missing);
    let mut quota = limits();
    quota.identity_cells = 0;
    assert!(matches!(
        *self::plan("{A}", None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::IdentityCells,
            ..
        }
    ));
    let mut quota = limits();
    quota.work_cells = 2;
    let result = self::plan("{A}:{B}", Some(Value::Missing)).execute_observed(&table, quota);
    assert!(matches!(
        *result.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(result.handler_counts.iter().all(|c| c.count == 0));
}
#[test]
fn nested_text_handlers_register_and_count_only_reached_sites() {
    use yamaa_core::text_case::{Case, StringCase};
    for template_outer in [false, true] {
        let table = table(Value::Missing, Value::Str("beta".into()));
        let mut template = assignment("{A}:{B}", Some(Value::Str("MiXeD".into())));
        let text = Assignment {
            column: 1,
            path: "columns.RESULT.derivation.str_case".into(),
            expression: Expression::StrCase {
                source: Box::new(Assignment {
                    column: 1,
                    path: "source".into(),
                    expression: Expression::Source(1),
                }),
                operation: StringCase::new(
                    "A".into(),
                    "columns.RESULT.derivation.str_case".into(),
                    Case::Upper,
                    Some(Value::Str("lower".into())),
                )
                .unwrap(),
            },
        };
        let outer = if template_outer {
            let Expression::StrTemplate { operands, .. } = &mut template.expression else {
                unreachable!()
            };
            operands[0] = text;
            template
        } else {
            let mut text = text;
            let Expression::StrCase { source, .. } = &mut text.expression else {
                unreachable!()
            };
            **source = template;
            text
        };
        let result = from_assignment(outer).execute_observed(&table, limits());
        assert_eq!(
            result.result.unwrap().dataset.rows()[0][1],
            Value::Str(
                if template_outer {
                    "lower:beta"
                } else {
                    "MIXED"
                }
                .into()
            )
        );
        assert_eq!(result.handler_counts.len(), 2);
        let reached = result
            .handler_counts
            .iter()
            .filter(|c| c.count > 0)
            .collect::<Vec<_>>();
        assert_eq!(reached.len(), 1);
        assert_eq!(reached[0].count, 1);
    }
}
#[test]
fn exact_retained_text_preflight_preserves_equality_and_stops_later_assignment() {
    let table = table(Value::Str("abc".into()), Value::Str("later".into()));
    let output = schema(&["ID", "RESULT", "LATER"]);
    let plan = DatasetPlan::new(
        table.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID".into(),
                    expression: Expression::Source(0),
                },
                assignment("{A}", None),
                Assignment {
                    column: 2,
                    path: "columns.LATER".into(),
                    expression: Expression::Source(2),
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let mut quota = limits();
    quota.output_text_bytes = 6;
    assert!(matches!(
        *plan.execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            limit: 6,
            required: Some(7)
        }
    ));
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    // Equality for the template itself succeeds, then the later assignment has
    // its ordinary final-output admission, proving the check did not spend twice.
    table.reads.borrow_mut().clear();
    quota.output_text_bytes = 12;
    assert!(plan.execute(&table, quota).is_ok());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (0, 2)]);
    let plan = self::plan("{A}", None);
    quota.output_text_bytes = 7;
    assert!(plan.execute(&table, quota).is_ok());
    quota.output_text_bytes = 6;
    assert!(matches!(
        *plan.execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            required: Some(7),
            ..
        }
    ));
}
#[test]
fn numeric_targets_and_nested_transforms_do_not_precharge_unretained_raw_text() {
    let table = table(Value::Str("7".into()), Value::Str("later".into()));
    let output = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: T::Str,
        },
        Column {
            name: "RESULT".into(),
            kind: T::Int,
        },
    ])
    .unwrap();
    let plan = DatasetPlan::new(
        table.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID".into(),
                    expression: Expression::Source(0),
                },
                assignment("{A}", Some(Value::Missing)),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let mut quota = limits();
    quota.output_text_bytes = 4;
    assert_eq!(
        plan.execute(&table, quota).unwrap().dataset.rows()[0],
        [Value::Str("row1".into()), Value::Int(7)]
    );
    let table = self::table(Value::Str("not-int".into()), Value::Missing);
    assert!(matches!(
        *plan.execute(&table, quota).unwrap_err(),
        ExecutionError::Conversion { .. }
    ));
    // A nested extraction can shrink the raw template, so it has no authority
    // to charge that raw text as the owning assignment's retained value.
    use yamaa_core::text_extract::StringExtract;
    let table = self::table(Value::Str("abcdef".into()), Value::Missing);
    let outer = Assignment {
        column: 1,
        path: "columns.RESULT.str_extract".into(),
        expression: Expression::StrExtract {
            source: Box::new(assignment("{A}", None)),
            operation: StringExtract::new(
                "A".into(),
                "columns.RESULT.str_extract".into(),
                "(.)".into(),
                "1".into(),
                None,
                None,
            )
            .unwrap(),
        },
    };
    quota.output_text_bytes = 5;
    assert_eq!(
        from_assignment(outer)
            .execute(&table, quota)
            .unwrap()
            .dataset
            .rows()[0][1],
        Value::Str("a".into())
    );
}
#[test]
fn selected_case_preserves_exact_retained_template_admission_and_unselected_effects() {
    use yamaa_core::dataset::CaseBranch;
    let table = table(Value::Str("abc".into()), Value::Missing);
    let when = yamaa_core::bound_expression::BoundPredicate::new(
        yamaa_core::predicate_compiler::compile(
            "ID = 'row1'",
            "columns.RESULT.case[0].when",
            Default::default(),
        )
        .unwrap(),
        vec![yamaa_core::bound_expression::Binding {
            name: "ID".into(),
            read: yamaa_core::bound_expression::Read::Column(0),
        }],
    )
    .unwrap();
    let case = Assignment {
        column: 1,
        path: "columns.RESULT.case".into(),
        expression: Expression::Case(vec![
            CaseBranch {
                when: Some(when),
                assignment: Box::new(assignment("{A}", None)),
            },
            CaseBranch {
                when: None,
                assignment: Box::new(assignment("{B}", Some(Value::Str("unselected".into())))),
            },
        ]),
    };
    let plan = from_assignment(case);
    *table.failure.borrow_mut() = Some((2, Failure(Box::new(9), Rc::new(()))));
    let mut quota = limits();
    quota.output_text_bytes = 7;
    assert!(plan.execute(&table, quota).is_ok());
    quota.output_text_bytes = 6;
    assert!(matches!(
        *plan.execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            limit: 6,
            required: Some(7)
        }
    ));
}
// Test-only allocation instrumentation uses the system allocator unchanged. The
// thread-local size watch observes the complete template result's sole reserve;
// unrelated parallel tests and already-owned source captures are excluded.
struct AllocationProbe;
std::thread_local! { static ALLOCATIONS: std::cell::Cell<(bool,usize,usize)> = const {std::cell::Cell::new((false,0,0))}; }
unsafe impl std::alloc::GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|state| {
            let (enabled, size, count) = state.get();
            if enabled && layout.size() == size {
                state.set((enabled, size, count + 1));
            }
        });
        unsafe { std::alloc::GlobalAlloc::alloc(&std::alloc::System, layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::GlobalAlloc::dealloc(&std::alloc::System, pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|state| {
            let (enabled, watched, count) = state.get();
            if enabled && size == watched {
                state.set((enabled, watched, count + 1));
            }
        });
        unsafe { std::alloc::GlobalAlloc::realloc(&std::alloc::System, pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;
#[test]
fn retained_output_refusal_has_no_complete_result_allocation() {
    let table = table(Value::Str("z".into()), Value::Missing);
    let plan = plan(&format!("{}{{A}}", "a".repeat(996)), None);
    let mut quota = limits();
    quota.work_cells = 10000;
    quota.scalar_text_bytes = 10000;
    quota.output_text_bytes = 1000;
    ALLOCATIONS.with(|state| state.set((true, 997, 0)));
    let result = plan.execute(&table, quota);
    let refused = ALLOCATIONS.with(|state| {
        let (_, _, count) = state.get();
        state.set((false, 0, 0));
        count
    });
    assert_eq!(refused, 0);
    assert!(matches!(
        *result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            limit: 1000,
            required: Some(1001)
        }
    ));
    quota.output_text_bytes = 1001;
    ALLOCATIONS.with(|state| state.set((true, 997, 0)));
    let result = plan.execute(&table, quota);
    let accepted = ALLOCATIONS.with(|state| {
        let (_, _, count) = state.get();
        state.set((false, 0, 0));
        count
    });
    assert!(result.is_ok());
    assert!(accepted >= 1);
}
