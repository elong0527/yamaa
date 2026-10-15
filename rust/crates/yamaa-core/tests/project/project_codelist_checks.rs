use yamaa_core::{
    dataset::{Check, ColumnVerifications, DatasetPlan, Verification},
    dataset_checks::{column_diagnostic, column_offenders},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};

#[test]
fn numeric_codes_compare_exactly_across_integer_binary64_and_zero_representations() {
    let check = Check::Codelist {
        id: "NUM".into(),
        values: vec![
            Value::Int(9_007_199_254_740_993),
            Value::float(1.0),
            Value::float(-0.0),
            Value::Int(i64::MAX),
        ],
    };
    let values = [
        Value::float(9_007_199_254_740_992.0),
        Value::Int(9_007_199_254_740_993),
        Value::Int(1),
        Value::float(0.0),
        Value::Missing,
        Value::float(9_223_372_036_854_775_808.0),
        Value::Int(i64::MAX),
    ];
    assert_eq!(column_offenders(&check, &values), [0, 5]);
    let finding = column_diagnostic(
        &check,
        "columns.NUM.submission.codelist".into(),
        "NUM".into(),
        2,
    )
    .unwrap();
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().requirement
        ),
        ("verification", "allowed_values_failed", Some("REQ-0957"))
    );
}
#[test]
fn text_codes_use_scalar_equality_and_empty_fixed_lists_only_admit_missing() {
    let check = Check::Codelist {
        id: "TEXT".into(),
        values: vec![Value::Str("e\u{301}\0🙂".into())],
    };
    assert_eq!(
        column_offenders(
            &check,
            &[
                Value::Str("e\u{301}\0🙂".into()),
                Value::Str("é\0🙂".into()),
                Value::Missing
            ]
        ),
        [1]
    );
    let empty = Check::Codelist {
        id: "EMPTY".into(),
        values: vec![],
    };
    assert_eq!(
        column_offenders(
            &empty,
            &[Value::Missing, Value::Str("".into()), Value::Int(0)]
        ),
        [1, 2]
    );
    assert!(column_diagnostic(
        &empty,
        "columns.CODE.submission.codelist".into(),
        "CODE".into(),
        0
    )
    .is_none());
}
fn plan(kind: ColumnType, check: Check) -> Result<DatasetPlan, yamaa_core::dataset::PlanError> {
    let schema = TableSchema::new(vec![Column {
        name: "CODE".into(),
        kind,
    }])
    .unwrap();
    DatasetPlan::new(
        schema.clone(),
        schema,
        vec![yamaa_core::dataset::RowTemplate {
            mode: yamaa_core::dataset::RowMode::Records,
            assignments: vec![yamaa_core::dataset::Assignment {
                column: 0,
                expression: yamaa_core::dataset::Expression::Source(0),
                path: "columns.CODE.derivation".into(),
            }],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
    .with_column_verifications(vec![ColumnVerifications {
        column: 0,
        checks: vec![Verification {
            path: "columns.CODE.submission.codelist".into(),
            check,
        }],
    }])
}
#[test]
fn fixed_check_admission_rejects_wrong_kinds_without_changing_existing_allowed_values() {
    for (kind, values) in [
        (ColumnType::Str, vec![Value::Int(1)]),
        (ColumnType::Int, vec![Value::float(1.5)]),
        (ColumnType::Float, vec![Value::Bool(true)]),
        (ColumnType::Float, vec![Value::Missing]),
    ] {
        assert!(plan(
            kind,
            Check::Codelist {
                id: "CODE".into(),
                values
            }
        )
        .is_err());
    }
    assert!(plan(
        ColumnType::Str,
        Check::Codelist {
            id: "".into(),
            values: vec![]
        }
    )
    .is_err());
    assert!(plan(
        ColumnType::Str,
        Check::Codelist {
            id: "EMPTY".into(),
            values: vec![]
        }
    )
    .is_ok());
    assert!(plan(
        ColumnType::Float,
        Check::Codelist {
            id: "NUM".into(),
            values: vec![Value::Int(1), Value::float(2.0)]
        }
    )
    .is_ok());
    assert!(plan(ColumnType::Float, Check::AllowedValues(vec![Value::Int(1)])).is_err());
}
