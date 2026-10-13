#[path = "support/project_codelist_compiler.rs"]
#[allow(dead_code)]
mod support;
use yamaa_core::{
    diagnostic::ConditionCode,
    project_codelist_diagnostics::{
        diagnostics, CheckObservation, CodelistObservation, Error, RowObservation,
    },
    project_diagnostics::{Fact, MAX_DIAGNOSTIC_BYTES},
    specification::PreparedSpecification,
    value::Value,
};
struct Row {
    position: usize,
    values: Vec<Value>,
}
impl RowObservation for Row {
    fn position(&self) -> usize {
        self.position
    }
    fn values(&self) -> &[Value] {
        &self.values
    }
}
fn compiled() -> PreparedSpecification {
    PreparedSpecification::prepare_with_environment(
        &support::spec("SEX", "str", None, false, None),
        &support::environment(false),
    )
    .unwrap()
}
fn record<'a>(rows: &'a [Row], values: &'a [Value]) -> CheckObservation<'a, Row> {
    CheckObservation {
        path: "columns.CODE.submission.codelist",
        code: ConditionCode::VerificationCodelistFailed,
        evaluated_count: 2,
        failed_count: 1,
        output_rows: 2,
        offending_rows: rows,
        codelist: Some(CodelistObservation { id: "SEX", values }),
        inventory: None,
    }
}
#[test]
fn complete_context_borrows_exact_keys_values_and_selected_metadata() {
    let compiled = compiled();
    let rows = [Row {
        position: 1,
        values: vec![Value::Int(i64::MAX)],
    }];
    let values = [Value::Str("\u{e9}\0\u{1f642}".into())];
    let result = diagnostics(&compiled, &[record(&rows, &values)], MAX_DIAGNOSTIC_BYTES).unwrap();
    assert_eq!(result.len(), 1);
    let diagnostic = &result[0];
    assert_eq!(diagnostic.code, ConditionCode::VerificationCodelistFailed);
    assert_eq!(diagnostic.spec_path, "columns.CODE.submission.codelist");
    assert_eq!(diagnostic.context.len(), 5);
    assert_eq!(diagnostic.context["column"], Fact::Text("CODE"));
    assert_eq!(diagnostic.context["codelist"], Fact::Text("SEX"));
    assert_eq!(diagnostic.context["failure_count"], Fact::Integer(1));
    let Fact::Sequence(keys) = &diagnostic.context["keys"] else {
        panic!("full keys")
    };
    let [Fact::Object(keys)] = keys.as_slice() else {
        panic!("one full row")
    };
    let Fact::Scalar(key) = keys["ID"] else {
        panic!("exact key")
    };
    assert!(std::ptr::eq(key, &rows[0].values[0]));
    let Fact::Sequence(offenders) = &diagnostic.context["values"] else {
        panic!("all values")
    };
    let [Fact::Scalar(value)] = offenders.as_slice() else {
        panic!("one full value")
    };
    assert!(std::ptr::eq(*value, &values[0]));
    assert_eq!(
        diagnostics(&compiled, &[record(&rows, &values)], MAX_DIAGNOSTIC_BYTES).unwrap(),
        result
    );
}
#[test]
fn late_invalid_observations_and_quotas_refuse_the_complete_report() {
    let compiled = compiled();
    let rows = [Row {
        position: 1,
        values: vec![Value::Int(i64::MAX)],
    }];
    let values = [Value::Str("bad".into())];
    let valid = record(&rows, &values);
    let mut invalid = record(&rows, &values);
    invalid.path = "columns.LATE.verifications[0]";
    assert!(matches!(
        diagnostics(&compiled, &[valid, invalid], MAX_DIAGNOSTIC_BYTES),
        Err(Error::InvalidObservation)
    ));
    assert!(matches!(
        diagnostics(&compiled, &[record(&rows, &values)], 1),
        Err(Error::Limit)
    ));
    let mut invalid = record(&rows, &values);
    invalid.failed_count = 2;
    assert!(matches!(
        diagnostics(&compiled, &[invalid], MAX_DIAGNOSTIC_BYTES),
        Err(Error::InvalidObservation)
    ));
    let missing = [Value::Missing];
    assert!(matches!(
        diagnostics(&compiled, &[record(&rows, &missing)], MAX_DIAGNOSTIC_BYTES),
        Err(Error::InvalidObservation)
    ));
}

#[test]
fn same_condition_with_a_different_typed_owner_refuses_codelist_projection() {
    let compiled = compiled();
    let rows = [Row {
        position: 1,
        values: vec![Value::Int(i64::MAX)],
    }];
    let values = [Value::Str("bad".into())];
    let mut wrong = record(&rows, &values);
    wrong.code = ConditionCode::VerificationAllowedValuesFailed;
    assert_eq!(wrong.code.definition().condition, "allowed_values_failed");
    assert_eq!(wrong.code.definition().requirement, Some("REQ-0376"));
    assert!(matches!(
        diagnostics(
            &compiled,
            &[record(&rows, &values), wrong],
            MAX_DIAGNOSTIC_BYTES
        ),
        Err(Error::InvalidObservation)
    ));
}
