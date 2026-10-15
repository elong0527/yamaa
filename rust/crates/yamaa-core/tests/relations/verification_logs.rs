use std::collections::BTreeMap;
use yamaa_core::{
    dataset::VerificationSeverity,
    table::{TableAccess, ValueRef},
    value::Value,
    verification_logs::{self, Error, Failure, Json, Limits, Record},
};

fn held() -> Record {
    Record {
        path: "columns.AGE.verifications[0].not_missing".into(),
        id: None,
        check: "not_missing".into(),
        target: Some("AGE".into()),
        requirement: "REQ-0376".into(),
        severity: VerificationSeverity::Error,
        evaluated: 8,
        failure: None,
    }
}
fn warning() -> Record {
    Record {
        path: "columns.AGE.verifications[1].range".into(),
        id: Some("é🙂".into()),
        check: "range".into(),
        target: Some("AGE".into()),
        requirement: "REQ-0377".into(),
        severity: VerificationSeverity::Warning,
        evaluated: 8,
        failure: Some(Failure {
            condition: "range_failed".into(),
            count: 7,
            keys: (0..7)
                .map(|n| Json::Object(BTreeMap::from([("ID".into(), Json::Scalar(Value::Int(n)))])))
                .collect(),
            details: BTreeMap::from([
                ("z".into(), Json::Scalar(Value::Str("é🙂\n\t\0\"\\".into()))),
                (
                    "minimum".into(),
                    Json::Integer("-9223372036854775808".into()),
                ),
            ]),
        }),
    }
}
#[test]
fn complete_ordered_logs_preserve_missing_metadata_ascii_details_and_all_keys() {
    let logs = verification_logs::build(
        &[held(), warning()],
        "out.csv",
        true,
        true,
        true,
        Limits::default(),
    )
    .unwrap();
    let verification = logs.verification.unwrap();
    assert_eq!(verification.row_count(), 2);
    assert_eq!(
        verification
            .schema()
            .columns()
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        [
            "REPORT_VERSION",
            "ARTIFACT",
            "SPEC_PATH",
            "VERIFICATION_ID",
            "CHECK",
            "TARGET",
            "REQUIREMENT",
            "SEVERITY",
            "OUTCOME",
            "CONDITION",
            "EVALUATED_COUNT",
            "FAILURE_COUNT",
            "DETAILS"
        ]
    );
    assert_eq!(verification.cell(0, 3).unwrap(), ValueRef::Missing);
    assert_eq!(verification.cell(0, 8).unwrap(), ValueRef::Str("held"));
    assert_eq!(verification.cell(0, 9).unwrap(), ValueRef::Missing);
    assert_eq!(verification.cell(0, 11).unwrap(), ValueRef::Int(0));
    assert_eq!(verification.cell(0, 12).unwrap(), ValueRef::Str("{}"));
    let expected =
        "{\"minimum\":-9223372036854775808,\"z\":\"\\u00e9\\ud83d\\ude42\\n\\t\\u0000\\\"\\\\\"}";
    assert_eq!(verification.cell(1, 12).unwrap(), ValueRef::Str(expected));
    let warning = logs.warning.unwrap();
    assert_eq!(warning.row_count(), 1);
    assert_eq!(warning.schema().columns().len(), 10);
    assert_eq!(warning.cell(0, 7).unwrap(), ValueRef::Int(7));
    assert_eq!(
        warning.cell(0, 8).unwrap(),
        ValueRef::Str(
            "[{\"ID\":0},{\"ID\":1},{\"ID\":2},{\"ID\":3},{\"ID\":4},{\"ID\":5},{\"ID\":6}]"
        )
    );
    assert_eq!(warning.cell(0, 9).unwrap(), ValueRef::Str(expected));
}
#[test]
fn failures_expose_only_verification_prefix_and_empty_declarations_retain_schema() {
    let logs = verification_logs::build(
        &[warning()],
        "out.csv",
        true,
        true,
        false,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(logs.verification.unwrap().row_count(), 1);
    assert!(logs.warning.is_none());
    let logs =
        verification_logs::build(&[], "out.csv", true, true, true, Limits::default()).unwrap();
    assert_eq!(logs.verification.unwrap().schema().columns().len(), 13);
    assert_eq!(logs.warning.unwrap().schema().columns().len(), 10);
}
#[test]
fn malformed_records_and_exhausted_ownership_budgets_refuse_before_rows_are_returned() {
    for limits in [
        Limits {
            records: 0,
            ..Limits::default()
        },
        Limits {
            cells: 22,
            ..Limits::default()
        },
        Limits {
            text: 20,
            ..Limits::default()
        },
        Limits {
            json_nodes: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            verification_logs::build(&[warning()], "out.csv", true, true, true, limits),
            Err(Error::Limit)
        ));
    }
    for malformed in [
        {
            let mut record = warning();
            record.failure.as_mut().unwrap().count = 9;
            record
        },
        {
            let mut record = warning();
            record
                .failure
                .as_mut()
                .unwrap()
                .details
                .insert("keys".into(), Json::Scalar(Value::Missing));
            record
        },
        {
            let mut record = warning();
            record
                .failure
                .as_mut()
                .unwrap()
                .keys
                .push(Json::Scalar(Value::Int(1)));
            record
        },
        {
            let mut record = warning();
            record
                .failure
                .as_mut()
                .unwrap()
                .details
                .insert("bad".into(), Json::Integer("01".into()));
            record
        },
    ] {
        assert!(matches!(
            verification_logs::build(&[malformed], "out.csv", true, true, true, Limits::default()),
            Err(Error::InvalidRecord)
        ));
    }
    assert!(matches!(
        verification_logs::build(
            &[held(), held()],
            "out.csv",
            true,
            true,
            true,
            Limits::default()
        ),
        Err(Error::InvalidRecord)
    ));
}
