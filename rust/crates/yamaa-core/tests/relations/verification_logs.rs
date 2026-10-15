use serde_json::Value as Json;
use std::collections::BTreeMap;
use yamaa_core::{
    dataset::VerificationSeverity,
    table::{TableAccess, ValueRef},
    value::Value,
    verification_logs::{self, Error, Failure, Record},
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
                .map(|n| Json::Object([("ID".into(), Json::from(n))].into_iter().collect()))
                .collect(),
            details: BTreeMap::from([
                ("z".into(), Json::String("é🙂\n\t\0\"\\".into())),
                ("minimum".into(), Json::from(i64::MIN)),
            ]),
        }),
    }
}
#[test]
fn complete_ordered_logs_preserve_missing_metadata_ascii_details_and_all_keys() {
    let logs = verification_logs::build(&[held(), warning()], "out.csv", true, true, true).unwrap();
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
    let logs = verification_logs::build(&[warning()], "out.csv", true, true, false).unwrap();
    assert_eq!(logs.verification.unwrap().row_count(), 1);
    assert!(logs.warning.is_none());
    let logs = verification_logs::build(&[], "out.csv", true, true, true).unwrap();
    assert_eq!(logs.verification.unwrap().schema().columns().len(), 13);
    assert_eq!(logs.warning.unwrap().schema().columns().len(), 10);
}
#[test]
fn malformed_records_refuse_before_rows_are_returned() {
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
                .insert("keys".into(), Json::Null);
            record
        },
        {
            let mut record = warning();
            record.failure.as_mut().unwrap().keys.push(Json::from(1));
            record
        },
        {
            let mut record = warning();
            record
                .failure
                .as_mut()
                .unwrap()
                .details
                // Number owns JSON grammar; a numeric log context still requires
                // a finite binary64 value for fractional/exponent spellings.
                .insert("bad".into(), Json::Number("1e999".parse().unwrap()));
            record
        },
    ] {
        assert!(matches!(
            verification_logs::build(&[malformed], "out.csv", true, true, true),
            Err(Error::InvalidRecord)
        ));
    }
    assert!(matches!(
        verification_logs::build(&[held(), held()], "out.csv", true, true, true),
        Err(Error::InvalidRecord)
    ));
}

#[test]
fn canonical_json_numbers_and_temporal_values_preserve_governed_log_bytes() {
    let mut record = warning();
    record.failure.as_mut().unwrap().details = BTreeMap::from([
        (
            "date".into(),
            Json::from(Value::Date("2025-01-02".parse().unwrap())),
        ),
        (
            "datetime".into(),
            Json::from(Value::DateTime("2025-01-02T03:04".parse().unwrap())),
        ),
        ("integer_float".into(), Json::from(Value::float(1.0))),
        ("large".into(), Json::from(Value::float(1.0e20))),
        ("negzero".into(), Json::from(Value::float(-0.0))),
        ("tiny".into(), Json::from(Value::float(1.0e-7))),
        (
            "wide".into(),
            Json::Number(
                "12345678901234567890123456789012345678901234567890"
                    .parse()
                    .unwrap(),
            ),
        ),
    ]);
    let logs = verification_logs::build(&[record], "out.csv", true, true, true).unwrap();
    let expected = r#"{"date":"2025-01-02","datetime":"2025-01-02T03:04:00","integer_float":1,"large":100000000000000000000,"negzero":-0,"tiny":0.0000001,"wide":12345678901234567890123456789012345678901234567890}"#;
    assert_eq!(
        logs.verification.unwrap().cell(0, 12).unwrap(),
        ValueRef::Str(expected)
    );
    assert_eq!(
        logs.warning.unwrap().cell(0, 9).unwrap(),
        ValueRef::Str(expected)
    );
}

#[test]
fn complete_record_sets_keep_all_rows_and_metadata_without_copy_allowances() {
    let records: Vec<_> = (0..6_000)
        .map(|index| {
            let mut record = held();
            record.path = format!("columns.AGE.verifications[{index}].not_missing");
            record.id = Some(format!("check {index}"));
            record
        })
        .collect();
    let logs = verification_logs::build(&records, "out.csv", true, true, true).unwrap();
    let verification = logs.verification.unwrap();
    assert_eq!(verification.row_count(), records.len());
    for (index, record) in records.iter().enumerate() {
        assert_eq!(
            verification.cell(index, 2).unwrap(),
            ValueRef::Str(&record.path)
        );
        assert_eq!(
            verification.cell(index, 3).unwrap(),
            ValueRef::Str(record.id.as_deref().unwrap())
        );
        assert_eq!(verification.cell(index, 10).unwrap(), ValueRef::Int(8));
    }
    assert_eq!(logs.warning.unwrap().row_count(), 0);
}

#[test]
fn recursive_json_details_use_the_shared_semantic_depth_boundary() {
    use yamaa_core::limits::Limits;
    for depth in [
        Limits::SEMANTIC_DEPTH - 1,
        Limits::SEMANTIC_DEPTH,
        Limits::SEMANTIC_DEPTH + 1,
    ] {
        let mut detail = Json::Null;
        for _ in 0..depth {
            detail = Json::Array(vec![detail]);
        }
        let mut record = held();
        record.failure = Some(Failure {
            condition: "missing_value".into(),
            count: 1,
            keys: vec![],
            details: BTreeMap::from([("nested".into(), detail)]),
        });
        let result = verification_logs::build(&[record], "out.csv", true, false, false);
        if depth <= Limits::SEMANTIC_DEPTH {
            assert!(result.is_ok());
        } else {
            let Err(Error::Limit(error)) = result else {
                panic!("deep details must refuse before encoding")
            };
            assert_eq!(error.resource, "semantic_depth");
            assert_eq!(error.limit, Limits::SEMANTIC_DEPTH);
            assert_eq!(error.actual, Some(depth));
        }
    }
}
