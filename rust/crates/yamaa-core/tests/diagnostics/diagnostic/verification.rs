use super::project_support as support;
use std::collections::BTreeSet;
use support::Tree as T;
use yamaa_core::{
    diagnostic::ConditionCode,
    project_codelist_diagnostics::{CheckObservation, RowObservation},
    project_diagnostics::Fact,
    schema::DocumentNode,
    specification::PreparedSpecification,
    value::Value,
    verification_diagnostics::{project, Error, Phase},
};
struct Row(Vec<Value>);
impl RowObservation for Row {
    fn position(&self) -> usize {
        0
    }
    fn values(&self) -> &[Value] {
        &self.0
    }
}
fn compiled() -> PreparedSpecification {
    let list = || T::List(vec![T::text("VAL"), T::text("ID")]);
    let spec = support::specification_with_verifications(
        vec![(
            "VAL",
            "int",
            T::map(vec![(
                "source",
                T::map(vec![("variable", T::text("SRC.VAL"))]),
            )]),
        )],
        vec![],
        vec![
            T::map(vec![(
                "unique",
                T::map(vec![
                    ("columns", list()),
                    ("id", T::text("u")),
                    ("severity", T::text("warning")),
                ]),
            )]),
            T::map(vec![(
                "all_or_none",
                T::map(vec![("columns", list()), ("id", T::text("a"))]),
            )]),
            T::map(vec![(
                "assert",
                T::map(vec![("require", T::text("VAL > 0")), ("id", T::text("b"))]),
            )]),
            T::map(vec![(
                "row_count",
                T::map(vec![
                    ("min", T::Scalar(DocumentNode::Integer("1".into()))),
                    ("id", T::text("r")),
                ]),
            )]),
        ],
        Some("warnings.csv"),
    );
    PreparedSpecification::prepare(&spec).unwrap()
}
fn record<'a>(code: ConditionCode, path: &'a str, rows: &'a [Row]) -> CheckObservation<'a, Row> {
    CheckObservation {
        path,
        code,
        evaluated_count: 9,
        failed_count: 1,
        output_rows: 9,
        offending_rows: rows,
        codelist: None,
        inventory: None,
        row_count: None,
    }
}
fn object<'a>(fact: &'a Fact<'a>) -> &'a yamaa_core::project_diagnostics::Context<'a> {
    match fact {
        Fact::Object(fields) => fields,
        _ => panic!("complete context"),
    }
}
/// Reach the six registered causes through real compiled declarations and retained facts.
pub fn reached() -> BTreeSet<ConditionCode> {
    use ConditionCode::*;
    let compiled = compiled();
    let rows = [Row(vec![Value::Int(i64::MAX)])];
    let mut reached = BTreeSet::new();
    for (code, path, phase, condition, requirement) in [
        (
            OutputMissingKeyFailed,
            "keys[0]",
            Phase::Output,
            "missing_key",
            "REQ-0240",
        ),
        (
            OutputDuplicateKeyFailed,
            "keys",
            Phase::Output,
            "duplicate_key",
            "REQ-0240",
        ),
        (
            VerificationUniqueFailed,
            "verifications[0].unique",
            Phase::Verification,
            "unique_failed",
            "REQ-0381",
        ),
        (
            VerificationAllOrNoneFailed,
            "verifications[1].all_or_none",
            Phase::Verification,
            "all_or_none_failed",
            "REQ-0382",
        ),
        (
            VerificationAssertFailed,
            "verifications[2].assert",
            Phase::Verification,
            "assert_failed",
            "REQ-0384",
        ),
        (
            VerificationRowCountFailed,
            "verifications[3].row_count",
            Phase::Verification,
            "row_count_failed",
            "REQ-0385",
        ),
    ] {
        let projected =
            project(&compiled, &[record(code, path, &rows)], phase, "test.yaml").unwrap();
        if code == VerificationUniqueFailed {
            assert!(projected.diagnostics.is_empty());
        } else {
            assert_eq!(projected.diagnostics.len(), 1);
            assert_eq!(projected.diagnostics[0].code, code);
            assert_eq!(
                projected.diagnostics[0].code.definition().condition,
                condition
            );
            assert_eq!(
                projected.diagnostics[0].code.definition().requirement,
                Some(requirement)
            );
        }
        if phase == Phase::Verification {
            let observation = object(&projected.observations[0]);
            let failure = object(&observation["failure"]);
            assert_eq!(failure["condition"], Fact::Text(condition));
            assert_eq!(failure["requirement"], Fact::Text(requirement));
            assert_eq!(observation["evaluated_count"], Fact::Unsigned(9));
            assert_eq!(observation["specification"], Fact::Text("test.yaml"));
        } else {
            assert!(projected.observations.is_empty());
        }
        reached.insert(code);
    }
    reached
}
#[test]
fn warning_display_is_bounded_but_all_keys_and_exact_scalars_remain_in_order() {
    let compiled = compiled();
    let rows: Vec<_> = (0..7)
        .map(|n| {
            Row(vec![if n == 0 {
                Value::Int(i64::MAX)
            } else {
                Value::Int(n)
            }])
        })
        .collect();
    let mut record = record(
        ConditionCode::VerificationUniqueFailed,
        "verifications[0].unique",
        &rows,
    );
    record.evaluated_count = usize::MAX;
    record.output_rows = usize::MAX;
    // Unique failure count is repeated groups, rather than all group members.
    record.failed_count = 2;
    let projected = project(&compiled, &[record], Phase::Verification, "study.yaml").unwrap();
    assert!(projected.diagnostics.is_empty());
    let observation = object(&projected.observations[0]);
    assert_eq!(observation["severity"], Fact::Text("warning"));
    assert_eq!(observation["verification_id"], Fact::Text("u"));
    assert_eq!(observation["evaluated_count"], Fact::Unsigned(usize::MAX));
    let failure = object(&observation["failure"]);
    let context = object(&failure["context"]);
    assert_eq!(
        context["columns"],
        Fact::Sequence(vec![Fact::Text("VAL"), Fact::Text("ID")])
    );
    assert_eq!(context["failure_count"], Fact::Unsigned(2));
    let Fact::Sequence(display) = &context["keys"] else {
        panic!("display keys")
    };
    assert_eq!(display.len(), 5);
    let Fact::Sequence(retained) = &failure["offending_keys"] else {
        panic!("complete keys")
    };
    assert_eq!(retained.len(), 7);
    let Fact::Scalar(value) = object(&retained[0])["ID"] else {
        panic!("borrowed key")
    };
    assert!(std::ptr::eq(value, &rows[0].0[0]));
    let Fact::Sequence(logged) = &object(&failure["log_context"])["keys"] else {
        panic!("log keys")
    };
    assert_eq!(logged.len(), 7);
}
#[test]
fn output_missing_keys_are_not_rejected_or_lost_and_row_count_keeps_full_width() {
    let compiled = compiled();
    let rows = [Row(vec![Value::Missing])];
    let result = project(
        &compiled,
        &[record(
            ConditionCode::OutputMissingKeyFailed,
            "keys[0]",
            &rows,
        )],
        Phase::Output,
        "s.yaml",
    )
    .unwrap();
    let context = &result.diagnostics[0].context;
    assert_eq!(context["column"], Fact::Text("ID"));
    let Fact::Sequence(keys) = &context["keys"] else {
        panic!("missing keys")
    };
    assert_eq!(object(&keys[0])["ID"], Fact::Scalar(&Value::Missing));
    let mut count = record(
        ConditionCode::VerificationRowCountFailed,
        "verifications[3].row_count",
        &[],
    );
    count.output_rows = usize::MAX;
    count.evaluated_count = 1;
    let result = project(&compiled, &[count], Phase::Verification, "s.yaml").unwrap();
    assert_eq!(
        result.diagnostics[0].context["count"],
        Fact::Unsigned(usize::MAX)
    );
    assert_eq!(
        result.diagnostics[0].context["keys"],
        Fact::Sequence(vec![Fact::Object(Default::default())])
    );
}
#[test]
fn later_invalid_geometry_refuses_the_entire_projection() {
    let compiled = compiled();
    let good = [Row(vec![Value::Int(1)])];
    let bad = [Row(vec![Value::Int(1), Value::Int(2)])];
    let records = [
        record(
            ConditionCode::VerificationAssertFailed,
            "verifications[2].assert",
            &good,
        ),
        record(
            ConditionCode::VerificationAllOrNoneFailed,
            "verifications[1].all_or_none",
            &bad,
        ),
    ];
    assert!(matches!(
        project(&compiled, &records, Phase::Verification, "s.yaml"),
        Err(Error::InvalidObservation)
    ));
}

#[test]
fn typed_wrong_family_or_phase_refuses_the_entire_projection() {
    let compiled = compiled();
    let rows = [Row(vec![Value::Int(1)])];
    for (code, path) in [
        (ConditionCode::ConversionInput, "verifications[2].assert"),
        (ConditionCode::OutputMissingKeyFailed, "keys[0]"),
    ] {
        let records = [
            record(
                ConditionCode::VerificationAssertFailed,
                "verifications[2].assert",
                &rows,
            ),
            record(code, path, &rows),
        ];
        assert!(matches!(
            project(&compiled, &records, Phase::Verification, "s.yaml"),
            Err(Error::UnsupportedOutcome)
        ));
    }
}
