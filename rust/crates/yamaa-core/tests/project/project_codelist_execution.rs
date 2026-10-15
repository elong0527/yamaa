use std::cell::RefCell;
use yamaa_core::dataset::{
    Assignment, Check, ColumnVerifications, DatasetPlan, ExecutionError, Expression, RowMode,
    RowTemplate, Verification,
};
use yamaa_core::{
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
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
    let attempt = plan.execute(
        &source,
        &[],
        &mut yamaa_core::function_invocation::NoFunctions::new(),
        None,
    );
    let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err() else {
        panic!("wrong failure")
    };
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(
        (
            record.definition().condition,
            record.definition().requirement.unwrap(),
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
    let ExecutionError::VerificationFailures(records) = *plan
        .execute(
            &source,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        )
        .result
        .unwrap_err()
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
    let result = plan
        .execute(
            &source,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        )
        .result
        .unwrap();
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
fn empty_codelist_accepts_missing_values_without_skipping_the_checkpoint() {
    let (source, plan) = setup(
        vec![Value::Missing, Value::Missing],
        ColumnType::Str,
        vec![],
    );
    let execution = plan
        .execute(
            &source,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        )
        .result
        .unwrap();
    assert_eq!(execution.dataset.rows().len(), 2);
    assert_eq!(execution.verifications.len(), 1);
    assert_eq!(execution.verifications[0].evaluated_count, 2);
    assert_eq!(execution.verifications[0].failed_count, 0);
    assert!(execution.verifications[0]
        .codelist
        .as_ref()
        .unwrap()
        .values
        .is_empty());
}

#[test]
fn value_scopes_prove_a_string_discriminator_and_wait_for_its_completed_value() {
    use yamaa_core::dataset::ValueCodelist;
    let (table, base) = setup(
        vec![Value::Str("bad".into())],
        ColumnType::Str,
        vec![Value::Str("good".into())],
    );
    let check = |discriminator| Check::ValueCodelists {
        discriminator,
        overrides: vec!["later".into()],
        bindings: vec![ValueCodelist {
            id: "SELECTED".into(),
            values: vec![Value::Str("good".into())],
            codes: vec!["later".into()],
            default: false,
        }],
    };
    let groups = |check| {
        vec![ColumnVerifications {
            column: 1,
            checks: vec![Verification {
                path: "columns.CODE.submission.codelist".into(),
                check,
            }],
        }]
    };
    for discriminator in [0, 3] {
        assert_eq!(
            base.clone()
                .with_column_verifications(groups(check(discriminator)))
                .unwrap_err(),
            yamaa_core::dataset::PlanError::InvalidColumns
        );
    }
    let plan = base.with_column_verifications(groups(check(2))).unwrap();
    let ExecutionError::VerificationFailures(records) = *plan
        .execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        )
        .result
        .unwrap_err()
    else {
        panic!("selected terminology failure")
    };
    assert_eq!(
        (records[0].evaluated_count, records[0].failed_count),
        (1, 1)
    );
    assert_eq!(records[0].codelist.as_ref().unwrap().id, "SELECTED");
    assert_eq!(table.reads.borrow().as_slice(), &[(0, 0), (0, 1), (0, 2)]);
}

#[test]
fn effective_length_scopes_reject_foreign_owners_and_check_a_later_completed_discriminator() {
    let (table, base) = setup(
        vec![Value::Str("abcde".into())],
        ColumnType::Str,
        vec![Value::Str("abcde".into())],
    );
    let check = |discriminator, max: &str| Check::ValueMaxLength {
        max: yamaa_core::dataset::RowCountBound::new(max).unwrap(),
        discriminator,
        codes: vec!["later".into()],
        include: true,
    };
    let group = |check| {
        vec![ColumnVerifications {
            column: 1,
            checks: vec![Verification {
                path: "rows.R.submission.CODE.length".into(),
                check,
            }],
        }]
    };
    for discriminator in [0, 3] {
        assert_eq!(
            base.clone()
                .with_column_verifications(group(check(discriminator, "4")))
                .unwrap_err(),
            yamaa_core::dataset::PlanError::InvalidColumns
        );
    }
    for max in ["0", "-1"] {
        assert_eq!(
            base.clone()
                .with_column_verifications(group(check(2, max)))
                .unwrap_err(),
            yamaa_core::dataset::PlanError::InvalidBounds
        );
    }
    let plan = base
        .with_column_verifications(group(check(2, "4")))
        .unwrap();
    let ExecutionError::VerificationFailures(records) = *plan
        .execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        )
        .result
        .unwrap_err()
    else {
        panic!("scoped length failure")
    };
    assert_eq!(
        (records[0].evaluated_count, records[0].failed_count),
        (1, 1)
    );
    assert_eq!(
        records[0].code,
        yamaa_core::diagnostic::ConditionCode::VerificationMaxLengthFailed
    );
    assert_eq!(table.reads.borrow().as_slice(), &[(0, 0), (0, 1), (0, 2)]);
}
