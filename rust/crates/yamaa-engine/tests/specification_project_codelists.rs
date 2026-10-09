#[path = "../../yamaa-core/tests/support/project_codelist_compiler.rs"]
mod support;
use std::{cell::RefCell, convert::Infallible};
use support::{environment, schema, spec};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::Value,
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits},
    project_domain,
};
struct Table {
    schema: TableSchema,
    reads: RefCell<Vec<(usize, usize)>>,
    bad: bool,
}
impl TableAccess for Table {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        2
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.reads.borrow_mut().push((row, column));
        Ok(match column {
            0 => ValueRef::Int(i64::MAX - row as i64),
            1 if row == 0 => ValueRef::Str("F"),
            1 if self.bad => ValueRef::Str("é\0🙂"),
            1 => ValueRef::Missing,
            2 => {
                assert!(!self.bad, "later cell read after a failed CT checkpoint");
                ValueRef::Int(9)
            }
            _ => panic!("bad coordinate"),
        })
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
#[test]
fn owned_checked_environment_executes_fixed_codelists_before_later_cells_in_both_plan_forms() {
    for rows in [false, true] {
        let checked =
            project_domain::check_owned(&spec("SEX", "str", None, rows, None), environment(false))
                .unwrap();
        assert!(checked.compiled().called_functions().is_empty());
        let table = Table {
            schema: schema(),
            reads: RefCell::new(vec![]),
            bad: true,
        };
        let plan = checked.compiled().bind(&table.schema).unwrap();
        let attempt = plan.execute_observed(&table, limits());
        let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err() else {
            panic!("fixed-list failure")
        };
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(
            (
                record.requirement,
                record.path.as_str(),
                record.evaluated_count,
                record.failed_count
            ),
            ("REQ-0957", "columns.CODE.submission.codelist", 2, 1)
        );
        assert_eq!(record.offending_rows[0].values, [Value::Int(i64::MAX - 1)]);
        assert_eq!(
            record.codelist.as_ref().unwrap().values,
            [Value::Str("é\0🙂".into())]
        );
        assert!(table.reads.borrow().iter().all(|&(_, column)| column < 2));
    }
}
#[test]
fn missing_cells_pass_repeated_compiled_builds_and_keep_complete_check_observations() {
    let checked =
        project_domain::check_owned(&spec("SEX", "str", None, false, None), environment(false))
            .unwrap();
    let table = Table {
        schema: schema(),
        reads: RefCell::new(vec![]),
        bad: false,
    };
    for _ in 0..2 {
        let result = checked
            .compiled()
            .bind(&table.schema)
            .unwrap()
            .execute(&table, limits())
            .unwrap();
        assert_eq!(
            result.dataset.rows(),
            &[
                vec![Value::Int(i64::MAX), Value::Str("F".into()), Value::Int(9)],
                vec![Value::Int(i64::MAX - 1), Value::Missing, Value::Int(9)]
            ]
        );
        assert_eq!(result.verifications.len(), 1);
        let record = &result.verifications[0];
        assert_eq!((record.evaluated_count, record.failed_count), (2, 0));
        assert_eq!(record.codelist.as_ref().unwrap().id, "SEX");
        assert!(record.codelist.as_ref().unwrap().values.is_empty());
    }
}
