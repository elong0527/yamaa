#![allow(unused_imports)]
pub(crate) use crate::support::project_call_compiler as support;
pub(crate) use std::collections::BTreeSet;
pub(crate) use support::Tree as T;
pub(crate) use yamaa_core::{
    dataset::{CheckRecord, RowIdentity},
    diagnostic::ConditionCode,
    project_diagnostics::Fact,
    schema::DocumentNode,
    specification::PreparedSpecification,
    value::Value,
    verification_diagnostics::{project, Error, Phase},
};
pub(crate) struct Row(pub(crate) Vec<Value>);
pub(crate) fn compiled() -> PreparedSpecification {
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
pub(crate) fn record(code: ConditionCode, path: &str, rows: &[Row]) -> CheckRecord {
    CheckRecord {
        path: path.into(),
        code,
        evaluated_count: 9,
        failed_count: 1,
        output_rows: 9,
        offending_rows: rows
            .iter()
            .map(|row| RowIdentity {
                position: 0,
                values: row.0.clone(),
            })
            .collect(),
        codelist: None,
        inventory: None,
        row_count: None,
    }
}
pub(crate) fn object<'a>(fact: &'a Fact<'a>) -> &'a yamaa_core::project_diagnostics::Context<'a> {
    match fact {
        Fact::Object(fields) => fields,
        _ => panic!("complete context"),
    }
}
/// Reach the six registered causes through real compiled declarations and retained facts.
pub fn reached() -> BTreeSet<ConditionCode> {
    pub(crate) use ConditionCode::*;
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
        let records = [record(code, path, &rows)];
        let projected = project(&compiled, &records, phase, "test.yaml").unwrap();
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
