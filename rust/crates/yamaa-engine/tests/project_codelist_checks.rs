use std::cell::RefCell;
use yamaa_core::{
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{
    Assignment, Check, ColumnVerifications, DatasetExecution, DatasetPlan, ExecutionError,
    Expression, Limits, Resource, RowMode, RowTemplate, Verification,
};
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        self.rows
            .get(row)
            .and_then(|v| v.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
fn setup(values: Vec<Value>, kind: ColumnType, accepted: Vec<Value>) -> (Table, DatasetPlan) {
    let schema = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "CODE".into(),
            kind,
        },
        Column {
            name: "LATER".into(),
            kind: ColumnType::Str,
        },
    ])
    .unwrap();
    let table = Table {
        schema: schema.clone(),
        rows: values
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                vec![
                    Value::Int(i64::MAX - i as i64),
                    v,
                    Value::Str("later".into()),
                ]
            })
            .collect(),
        reads: RefCell::default(),
    };
    let assign = |column| Assignment {
        column,
        expression: Expression::Source(column),
        path: format!("columns.C{column}.derivation"),
    };
    let plan = DatasetPlan::new(
        schema.clone(),
        schema,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0), assign(1)],
            filter: None,
        }],
        vec![assign(2)],
        vec![0],
        vec![],
    )
    .unwrap()
    .with_column_verifications(vec![ColumnVerifications {
        column: 1,
        checks: vec![Verification {
            path: "columns.CODE.submission.codelist".into(),
            check: Check::Codelist {
                id: "CODES".into(),
                values: accepted,
            },
        }],
    }])
    .unwrap();
    (table, plan)
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
#[test]
fn failing_checkpoint_keeps_full_original_values_and_keys_before_later_source_reads() {
    let (source, plan) = setup(
        vec![
            Value::Str("F".into()),
            Value::Str("é\0🙂".into()),
            Value::Missing,
        ],
        ColumnType::Str,
        vec![Value::Str("F".into())],
    );
    let attempt = plan.execute_observed(&source, limits());
    let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err() else {
        panic!("wrong failure")
    };
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(
        (
            record.condition,
            record.requirement,
            record.evaluated_count,
            record.failed_count
        ),
        ("allowed_values_failed", "REQ-0957", 3, 1)
    );
    assert_eq!(record.offending_rows[0].values, [Value::Int(i64::MAX - 1)]);
    assert_eq!(
        record.codelist.as_ref().unwrap().values,
        [Value::Str("é\0🙂".into())]
    );
    assert_eq!(record.codelist.as_ref().unwrap().id, "CODES");
    assert!(source.reads.borrow().iter().all(|&(_, column)| column < 2));
    // The failed checkpoint owns its records in VerificationFailures.
    assert!(attempt.retained_verifications.is_empty());
}
#[test]
fn exact_numeric_failure_and_missing_success_keep_the_column_boundary() {
    let (source, plan) = setup(
        vec![Value::float(9_007_199_254_740_992.0), Value::Missing],
        ColumnType::Float,
        vec![Value::Int(9_007_199_254_740_993)],
    );
    let ExecutionError::VerificationFailures(records) =
        *plan.execute_observed(&source, limits()).result.unwrap_err()
    else {
        panic!("wide code rounded")
    };
    assert_eq!(
        records[0].codelist.as_ref().unwrap().values,
        [Value::float(9_007_199_254_740_992.0)]
    );
    let (source, plan) = setup(
        vec![Value::float(0.0), Value::float(1.0), Value::Missing],
        ColumnType::Float,
        vec![Value::float(-0.0), Value::Int(1)],
    );
    let result = plan.execute_observed(&source, limits()).result.unwrap();
    assert_eq!(result.dataset.rows().len(), 3);
    assert!(result.verifications[0]
        .codelist
        .as_ref()
        .unwrap()
        .values
        .is_empty());
    assert!(source.reads.borrow().iter().any(|&(_, column)| column == 2));
}
#[test]
fn retained_value_text_and_empty_list_scans_consume_the_existing_cumulative_quotas() {
    let (source, plan) = setup(
        vec![Value::Str("oversized".into())],
        ColumnType::Str,
        vec![Value::Str("F".into())],
    );
    let mut quota = limits();
    quota.identity_text_bytes = 8;
    assert!(matches!(
        *plan.execute_observed(&source, quota).result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::IdentityTextBytes,
            ..
        }
    ));
    let (source, plan) = setup(
        vec![Value::Missing, Value::Missing],
        ColumnType::Str,
        vec![],
    );
    assert!(plan.execute_observed(&source, limits()).result.is_ok());
    let control = plan.clone().with_column_verifications(vec![]).unwrap();
    let smallest = (1..100)
        .find(|&work| {
            let mut quota = limits();
            quota.work_cells = work;
            control.execute_observed(&source, quota).result.is_ok()
        })
        .expect("bounded control did not execute");
    let mut quota = limits();
    quota.work_cells = smallest;
    assert!(matches!(
        *plan.execute_observed(&source, quota).result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
}
