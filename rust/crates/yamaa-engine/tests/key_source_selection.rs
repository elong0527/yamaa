//! Key choices share one physical attempt, preserve opaque failures and finish before columns.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{
        Assignment, BoundFunction, DatasetPlan, Expression, FirstAvailable, Intermediate,
        IntermediateKey, IntermediateSource, MatchRead, MatchValue, RowMode, RowTemplate,
        SecondarySource, SelectionRead, SelectionSource, SourceSchemas,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits, Resource},
    function_invocation::{Argument, FunctionIdentity, FunctionPort, HostError, InvocationPlan},
};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
struct Table {
    name: &'static str,
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    failure: RefCell<Option<(usize, usize, Failure)>>,
    trace: Trace,
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
        self.trace.borrow_mut().push((self.name, row, column));
        if self
            .failure
            .borrow()
            .as_ref()
            .is_some_and(|(r, c, _)| *r == row && *c == column)
        {
            return Err(CellError::Access(
                self.failure.borrow_mut().take().unwrap().2,
            ));
        }
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn schema(columns: &[(&str, T)]) -> TableSchema {
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
fn tables() -> (Table, Table, Trace) {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let text = |s: &str| Value::Str(s.into());
    let table = |name, columns, rows| Table {
        name,
        schema: schema(columns),
        rows,
        failure: RefCell::default(),
        trace: trace.clone(),
    };
    (
        table(
            "SRC",
            &[("ID", T::Str), ("K", T::Str), ("UNUSED", T::Str)],
            vec![
                vec![text("1"), text("x"), text("hidden")],
                vec![text("2"), text("y"), text("hidden")],
                vec![text("3"), text("z"), text("hidden")],
            ],
        ),
        table(
            "DONOR",
            &[("K", T::Str), ("V", T::Str)],
            vec![
                vec![text("x"), text("X")],
                vec![text("y"), text("Y")],
                vec![text("z"), text("Z")],
            ],
        ),
        trace,
    )
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation.source"),
    }
}
fn plan(src: &Table, donor: &Table, id_type: T, column: Expression) -> DatasetPlan {
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: src.schema.clone(),
            secondary: vec![SecondarySource {
                name: "DONOR".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![Intermediate {
            identifier: "P".into(),
            path: "intermediates[0]".into(),
            source: IntermediateSource::Input(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key".into(),
                value: MatchValue::Variable(MatchRead::Record {
                    column: 1,
                    identifier: "SRC.K".into(),
                }),
            }],
            filter: None,
            selection: None,
            no_match: None,
            unique: vec![],
            derivations: None,
            pending_schema: None,
            range: None,
            columns: None,
        }],
        schema(&[
            ("ID", id_type),
            ("FIRST", T::Str),
            ("SECOND", T::Str),
            ("LATER", T::Str),
        ]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            filter: None,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::FirstAvailable(Box::new(FirstAvailable::new(
                        vec![
                            SelectionSource {
                                path: "columns.C1.first_available.sources[0]".into(),
                                read: SelectionRead::Expression(Box::new(assign(
                                    1,
                                    Expression::Intermediate {
                                        index: 0,
                                        column: 1,
                                    },
                                ))),
                            },
                            SelectionSource {
                                path: "columns.C1.first_available.sources[1]".into(),
                                read: SelectionRead::Collect {
                                    column: 2,
                                    identifier: "SRC.UNUSED".into(),
                                    filter: None,
                                },
                            },
                        ],
                        Value::Missing,
                    ))),
                ),
                assign(
                    2,
                    Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                ),
            ],
        }],
        vec![assign(3, column)],
        vec![0, 1, 2],
        vec![],
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 100,
        key_cells: 100,
        work_cells: 10_000,
        scalar_text_bytes: 10_000,
        output_text_bytes: 10_000,
        identity_cells: 100,
        identity_text_bytes: 10_000,
    }
}

#[test]
fn two_key_reads_share_the_choice_but_columns_and_fresh_attempts_select_again() {
    let (src, donor, trace) = tables();
    *src.failure.borrow_mut() = Some((0, 2, Failure(Box::new(1), Rc::new(()))));
    let p = plan(
        &src,
        &donor,
        T::Str,
        Expression::Intermediate {
            index: 0,
            column: 1,
        },
    );
    for _ in 0..2 {
        trace.borrow_mut().clear();
        let result = p
            .execute_observed_sources(&src, &[&donor], limits())
            .result
            .unwrap();
        assert_eq!(
            result
                .dataset
                .rows()
                .iter()
                .map(|r| r[1..].to_vec())
                .collect::<Vec<_>>(),
            [
                vec![Value::Str("X".into()); 3],
                vec![Value::Str("Y".into()); 3],
                vec![Value::Str("Z".into()); 3]
            ]
        );
        assert_eq!(
            trace
                .borrow()
                .iter()
                .filter(|&&(name, _, col)| name == "DONOR" && col == 0)
                .count(),
            18,
            "one scan per key candidate and another per column candidate"
        );
        assert_eq!(
            trace
                .borrow()
                .iter()
                .filter(|&&(name, _, col)| name == "DONOR" && col == 1)
                .count(),
            9,
            "sharing the choice still reads each requested value"
        );
        assert!(
            !trace
                .borrow()
                .iter()
                .any(|&(name, _, col)| name == "SRC" && col == 2),
            "first_available never reaches its later operand"
        );
    }
    assert!(
        src.failure.borrow().is_some(),
        "unreached source failure remains owned by the source"
    );
}

#[test]
fn selected_key_cell_failure_moves_once_and_prevents_the_next_key_or_column() {
    let (src, donor, trace) = tables();
    let payload = Box::new(7);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *donor.failure.borrow_mut() = Some((0, 1, Failure(payload, marker.clone())));
    let p = plan(
        &src,
        &donor,
        T::Str,
        Expression::Intermediate {
            index: 0,
            column: 1,
        },
    );
    let ExecutionError::SecondaryCell {
        error: CellError::Access(Failure(held, retained)),
        source_row,
        ..
    } = *p
        .execute_observed_sources(&src, &[&donor], limits())
        .result
        .unwrap_err()
    else {
        panic!("original donor failure")
    };
    assert_eq!(source_row, 0);
    assert_eq!(&*held as *const u8, address);
    assert!(Rc::ptr_eq(&retained, &marker));
    assert_eq!(
        trace
            .borrow()
            .iter()
            .filter(|&&(name, _, col)| name == "DONOR" && col == 1)
            .count(),
        1
    );
    assert!(!trace
        .borrow()
        .iter()
        .any(|&(name, row, _)| name == "SRC" && row > 0));
}

struct Calls {
    signature: InvocationPlan,
    count: usize,
}
impl FunctionPort for Calls {
    type Error = Failure;
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        (slot == 0).then_some(&self.signature)
    }
    fn call(&mut self, _: usize, _: &[Argument<'_>]) -> Result<Value, HostError<Failure>> {
        self.count += 1;
        Ok(Value::Str("called".into()))
    }
}
#[test]
fn all_key_conversions_precede_callbacks_and_work_refusals_keep_the_existing_limit() {
    let (mut src, donor, trace) = tables();
    src.rows[1][0] = Value::Str("bad".into());
    let signature = InvocationPlan::new(
        FunctionIdentity {
            name: "later".into(),
            contract_version: "1".into(),
            implementation_version: "1".into(),
            call: "project.later".into(),
        },
        vec![],
        T::Str,
        false,
    )
    .unwrap();
    let p = plan(
        &src,
        &donor,
        T::Int,
        Expression::Function(BoundFunction::new(0, signature.clone(), vec![]).unwrap()),
    );
    let mut calls = Calls {
        signature,
        count: 0,
    };
    assert!(matches!(
        *p.execute_observed_functions(&src, &[&donor], &mut calls, limits())
            .result
            .unwrap_err(),
        ExecutionError::Conversion { .. }
    ));
    assert_eq!(calls.count, 0);
    assert!(!trace
        .borrow()
        .iter()
        .any(|&(name, row, _)| name == "SRC" && row == 2));
    trace.borrow_mut().clear();
    let limited = Limits {
        work_cells: 0,
        ..limits()
    };
    assert!(matches!(
        *p.execute_observed_functions(&src, &[&donor], &mut calls, limited)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: 0,
            ..
        }
    ));
    assert!(trace.borrow().is_empty());
    assert_eq!(calls.count, 0);
}
