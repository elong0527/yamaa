use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    diagnostic::{ConditionCode, ContextValue},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    text_case::{Case, StringCase},
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
    fail: RefCell<Option<Failure>>,
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
        if column == 1 {
            if let Some(error) = self.fail.borrow_mut().take() {
                return Err(CellError::Access(error));
            }
        }
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn schema() -> TableSchema {
    TableSchema::new(
        ["ID", "TEXT"]
            .into_iter()
            .map(|name| Column {
                name: name.into(),
                kind: T::Str,
            })
            .collect(),
    )
    .unwrap()
}
fn make_table(text: Value) -> Table {
    Table {
        schema: schema(),
        rows: vec![vec![Value::Str("row1".into()), text]],
        fail: RefCell::default(),
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
fn assignment(column: usize, mode: Case, missing: Option<Value>) -> Assignment {
    let path = "columns.TEXT.derivation.str_case";
    Assignment {
        column,
        path: path.into(),
        expression: Expression::StrCase {
            source: Box::new(Assignment {
                column,
                path: format!("{path}.source"),
                expression: Expression::Source(1),
            }),
            operation: StringCase::new("SRC.TEXT".into(), path.into(), mode, missing).unwrap(),
        },
    }
}
fn plan(mode: Case, missing: Option<Value>) -> DatasetPlan {
    DatasetPlan::new(
        schema(),
        schema(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                assignment(1, mode, missing),
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
fn all_ascii_modes_preserve_non_ascii_and_perform_one_source_read() {
    for (mode, expected) in [
        (Case::Upper, "AB\u{00e9}C_D"),
        (Case::Lower, "ab\u{00e9}c_d"),
        (Case::Sentence, "Ab\u{00e9}c_d"),
        (Case::Title, "Ab\u{00e9}C_D"),
    ] {
        let table = make_table(Value::Str("aB\u{00e9}c_D".into()));
        let attempt = plan(mode, Some(Value::Missing)).execute_observed(&table, limits());
        assert_eq!(
            attempt.result.unwrap().dataset.rows(),
            [vec![Value::Str("row1".into()), Value::Str(expected.into())]]
        );
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        assert_eq!(attempt.handler_counts.len(), 1);
        assert_eq!(attempt.handler_counts[0].handler, HandlerKind::Missing);
        assert_eq!(attempt.handler_counts[0].count, 0);
    }
}
#[test]
fn literal_recovery_is_not_case_transformed_and_is_converted_once() {
    for (handler, expected) in [
        (Value::Missing, Value::Missing),
        (Value::Str("MiXeD".into()), Value::Str("MiXeD".into())),
        (Value::Int(7), Value::Str("7".into())),
    ] {
        let table = make_table(Value::Missing);
        let attempt = plan(Case::Lower, Some(handler)).execute_observed(&table, limits());
        assert_eq!(
            attempt.result.unwrap().dataset.rows(),
            [vec![Value::Str("row1".into()), expected]]
        );
        assert_eq!(
            attempt.handler_counts[0].spec_path,
            "columns.TEXT.derivation.str_case.missing"
        );
        assert_eq!(attempt.handler_counts[0].count, 1);
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    }
}
#[test]
fn semantic_missing_preserves_complete_core_facts_and_admitted_current_keys() {
    let table = make_table(Value::Missing);
    let error = plan(Case::Upper, None)
        .execute(&table, limits())
        .unwrap_err();
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *error
    else {
        panic!("semantic missing");
    };
    assert_eq!(diagnostic.code, ConditionCode::StrCaseMissing);
    assert_eq!(diagnostic.spec_paths, ["columns.TEXT.derivation.str_case"]);
    assert_eq!(
        diagnostic.context,
        [(
            "variable".into(),
            ContextValue::Scalar(Value::Str("SRC.TEXT".into()))
        )]
        .into_iter()
        .collect()
    );
    let identity = identity.unwrap();
    assert_eq!(identity.position, 0);
    assert_eq!(identity.values, [Value::Str("row1".into())]);
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    let plan = DatasetPlan::new(
        schema(),
        schema(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assignment(0, Case::Upper, None),
                Assignment {
                    column: 1,
                    path: "columns.TEXT.derivation".into(),
                    expression: Expression::Literal(Value::Str("safe".into())),
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let ExecutionError::SemanticDiagnostic { identity, .. } =
        *plan.execute(&table, limits()).unwrap_err()
    else {
        panic!("key missing");
    };
    assert!(
        identity.is_none(),
        "unfinished keys have no fabricated identity"
    );
}
#[test]
fn nested_owned_case_operands_register_and_count_each_handler_once() {
    let mut declarations = plan(Case::Lower, Some(Value::Missing)).templates().to_vec();
    let Expression::StrCase { source, .. } = &mut declarations[0].assignments[1].expression else {
        panic!("outer case");
    };
    **source = assignment(1, Case::Upper, Some(Value::Str("MiXeD".into())));
    // Distinct authored occurrences retain distinct handler identities.
    let Expression::StrCase { operation, .. } = &mut source.expression else {
        panic!("inner case");
    };
    *operation = StringCase::new(
        "SRC.TEXT".into(),
        "columns.TEXT.derivation.inner.str_case".into(),
        Case::Upper,
        Some(Value::Str("MiXeD".into())),
    )
    .unwrap();
    let declaration =
        DatasetPlan::new(schema(), schema(), declarations, vec![], vec![0], vec![]).unwrap();
    let table = make_table(Value::Missing);
    let attempt = declaration.execute_observed(&table, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [vec![Value::Str("row1".into()), Value::Str("mixed".into())]]
    );
    assert_eq!(
        attempt
            .handler_counts
            .iter()
            .map(|count| (count.spec_path.as_str(), count.count))
            .collect::<Vec<_>>(),
        [
            ("columns.TEXT.derivation.str_case.missing", 0),
            ("columns.TEXT.derivation.inner.str_case.missing", 1)
        ]
    );
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn original_nonclone_error_allocation_and_rc_identity_return_without_retry_or_handler() {
    let table = make_table(Value::Missing);
    let marker = Rc::new(());
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    *table.fail.borrow_mut() = Some(Failure(payload, marker.clone()));
    let attempt =
        plan(Case::Upper, Some(Value::Str("fallback".into()))).execute_observed(&table, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original access error");
    };
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&retained, &marker));
    assert_eq!(path, "columns.TEXT.derivation.str_case.source");
    assert_eq!(source_row, 0);
    assert_eq!(attempt.handler_counts[0].count, 0);
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn cumulative_work_text_output_and_identity_limits_refuse_before_next_effect() {
    let table = make_table(Value::Str("abc".into()));
    let mut quota = limits();
    quota.work_cells = 0;
    assert!(matches!(
        *plan(Case::Upper, None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(table.reads.borrow().is_empty());
    let mut quota = limits();
    quota.scalar_text_bytes = 7;
    assert!(matches!(
        *plan(Case::Upper, None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    let table = Table {
        schema: schema(),
        rows: vec![
            vec![Value::Str("row1".into()), Value::Str("a".into())],
            vec![Value::Str("row2".into()), Value::Str("b".into())],
        ],
        fail: RefCell::default(),
        reads: RefCell::default(),
    };
    let mut quota = limits();
    quota.scalar_text_bytes = 11;
    assert!(matches!(
        *plan(Case::Upper, None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (1, 0), (1, 1)]);
    let table = make_table(Value::Missing);
    let mut quota = limits();
    quota.identity_cells = 0;
    assert!(matches!(
        *plan(Case::Upper, None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::IdentityCells,
            ..
        }
    ));
    let table = make_table(Value::Missing);
    let mut quota = limits();
    quota.scalar_text_bytes = 4;
    let attempt =
        plan(Case::Upper, Some(Value::Str("MiXeD".into()))).execute_observed(&table, quota);
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(
        attempt.handler_counts[0].count, 0,
        "recovery copies are admitted before counting"
    );
    let table = make_table(Value::Str("abc".into()));
    let mut quota = limits();
    quota.output_text_bytes = 4;
    assert!(matches!(
        *plan(Case::Upper, None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            ..
        }
    ));
}
