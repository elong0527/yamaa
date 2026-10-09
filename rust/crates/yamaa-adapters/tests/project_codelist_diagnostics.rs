#[path = "../../yamaa-core/tests/support/project_codelist_compiler.rs"]
mod support;
use std::{cell::Cell, convert::Infallible};
use support::{environment, schema, spec};
use yamaa_adapters::project_codelist_diagnostics::{
    codelist_issues, codelist_issues_with_limit, Error,
};
use yamaa_core::table::{CellError, TableAccess, TableSchema, ValueRef};
use yamaa_core::{specification::PreparedSpecification, value::Value};
use yamaa_engine::dataset::{
    CheckRecord, CodelistObservation, DatasetExecution, ExecutionError, RowIdentity,
};
struct Source {
    schema: TableSchema,
    reads: Cell<usize>,
}
impl TableAccess for Source {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        assert_eq!(row, 0);
        self.reads.set(self.reads.get() + 1);
        Ok(match column {
            0 => ValueRef::Int(i64::MAX),
            1 => ValueRef::Str("é\0🙂"),
            _ => panic!("later source read"),
        })
    }
}
fn compiled() -> PreparedSpecification {
    PreparedSpecification::prepare_with_environment(
        &spec("SEX", "str", None, false, None),
        &environment(false),
    )
    .unwrap()
}
fn record(value: Value) -> CheckRecord {
    CheckRecord {
        path: "columns.CODE.submission.codelist".into(),
        condition: "allowed_values_failed",
        requirement: "REQ-0957",
        evaluated_count: 2,
        failed_count: 1,
        output_rows: 2,
        offending_rows: vec![RowIdentity {
            position: 1,
            values: vec![Value::Int(i64::MAX)],
        }],
        codelist: Some(CodelistObservation {
            id: "SEX".into(),
            values: vec![value],
        }),
    }
}
#[test]
fn complete_retained_values_and_full_width_keys_are_projected_without_output_or_sources() {
    let compiled = compiled();
    let source = Source {
        schema: schema(),
        reads: Cell::new(0),
    };
    let plan = compiled.bind(&source.schema).unwrap();
    let limits = yamaa_engine::dataset::Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 1000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    };
    let ExecutionError::VerificationFailures(records) = *plan.execute(&source, limits).unwrap_err()
    else {
        panic!("actual fixed-list failure")
    };
    let reads = source.reads.get();
    let before = records[0].codelist.as_ref().unwrap().values.as_ptr();
    let issues = codelist_issues(&compiled, &records).unwrap();
    assert_eq!(issues.len(), 1);
    let issue = &issues[0];
    assert_eq!(
        (
            issue.phase.as_str(),
            issue.condition.as_str(),
            issue.requirement.as_deref()
        ),
        ("verification", "allowed_values_failed", Some("REQ-0957"))
    );
    assert_eq!(issue.spec_paths, ["columns.CODE.submission.codelist"]);
    let context: serde_json::Value = serde_json::from_str(&issue.context).unwrap();
    assert_eq!(context["column"], "CODE");
    assert_eq!(context["codelist"], "SEX");
    assert_eq!(context["keys"][0]["ID"].as_i64(), Some(i64::MAX));
    assert_eq!(context["values"], serde_json::json!(["é\0🙂"]));
    assert_eq!(codelist_issues(&compiled, &records).unwrap(), issues);
    assert_eq!(
        before,
        records[0].codelist.as_ref().unwrap().values.as_ptr()
    );
    assert_eq!(source.reads.get(), reads);
}
#[test]
fn contradictory_observation_geometry_never_returns_partial_or_invented_issues() {
    let compiled = compiled();
    let original = record(Value::Str("bad".into()));
    for case in 0..6 {
        let mut wrong = original.clone();
        match case {
            0 => wrong.codelist = None,
            1 => wrong.failed_count = 2,
            2 => wrong.codelist.as_mut().unwrap().id = "OTHER".into(),
            3 => wrong.offending_rows[0].values.clear(),
            4 => wrong.codelist.as_mut().unwrap().values[0] = Value::Missing,
            _ => wrong.requirement = "REQ-0376",
        }
        assert_eq!(
            codelist_issues(&compiled, &[wrong]),
            Err(Error::InvalidObservation)
        );
    }
    let mut passing = original;
    passing.failed_count = 0;
    passing.offending_rows.clear();
    passing.codelist.as_mut().unwrap().values.clear();
    assert!(codelist_issues(&compiled, &[passing]).unwrap().is_empty());
}
#[test]
fn escaped_context_and_record_counts_obey_whole_projection_quotas() {
    let compiled = compiled();
    let records = vec![record(Value::Str("\0".repeat(1_500_000)))];
    assert_eq!(codelist_issues(&compiled, &records), Err(Error::Limit));
    let small = record(Value::Str("bad".into()));
    assert_eq!(
        codelist_issues_with_limit(&compiled, std::slice::from_ref(&small), 1),
        Err(Error::Limit)
    );
    let mut too_many = small;
    too_many.failed_count = 65_537;
    too_many.evaluated_count = 65_537;
    too_many.output_rows = 65_537;
    too_many.codelist.as_mut().unwrap().values = vec![Value::Int(1); 65_537];
    assert_eq!(codelist_issues(&compiled, &[too_many]), Err(Error::Limit));
}
