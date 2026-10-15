use yamaa_core::{
    dataset::Rounding,
    diagnostic::{ConditionCode, ContextValue},
    table::ValueRef,
    value::Value,
};

#[test]
fn declarative_rounding_reuses_independent_numeric_truth_and_missing() {
    for (digits, input, expected) in [
        ("1", 2.45, 2.5),
        ("1", -2.45, -2.5),
        ("1", 2.41, 2.4),
        ("1", 2.449999999, 2.5),
        ("1", 2.44999999, 2.4),
        ("0", 2.5, 3.0),
        ("0", -2.5, -3.0),
        ("0", 2.4999999999, 3.0),
        ("0", 2.4999999, 2.0),
    ] {
        let rounding = Rounding::new(
            "VAL",
            "columns.RND.derivation.round_half_away_from_zero",
            digits,
        )
        .unwrap();
        assert_eq!(
            rounding.apply(ValueRef::from(&Value::float(input))),
            Ok(Value::float(expected))
        );
        assert_eq!(rounding.apply(ValueRef::Missing), Ok(Value::Missing));
    }
}

#[test]
fn digits_are_admitted_before_values_and_arbitrary_width_is_not_narrowed() {
    for invalid in ["", "-", "+", "1.0", "false", " 1", "١", "--1"] {
        assert!(Rounding::new("VAL", "round", invalid).is_err());
    }
    for (digits, expected) in [
        ("99999999999999999999999999999999999999999999", 2.45),
        ("-99999999999999999999999999999999999999999999", 0.0),
    ] {
        let rounding = Rounding::new("VAL", "round", digits).unwrap();
        assert_eq!(
            rounding.apply(ValueRef::from(&Value::float(2.45))),
            Ok(Value::float(expected))
        );
    }
}

#[test]
fn nonnumeric_cause_retains_original_source_without_formula_geometry() {
    let rounding = Rounding::new(
        "SEX",
        "columns.RNDVAL.derivation.round_half_away_from_zero",
        "0",
    )
    .unwrap();
    let error = rounding.apply(ValueRef::Str("F")).unwrap_err();
    assert_eq!(error.code, ConditionCode::DeclarativeRoundingInput);
    assert_eq!(error.definition().phase, "validation");
    assert_eq!(error.definition().condition, "incompatible_input_type");
    assert_eq!(error.definition().requirement, Some("REQ-0418"));
    assert_eq!(
        error.spec_paths,
        ["columns.RNDVAL.derivation.round_half_away_from_zero.source"]
    );
    assert_eq!(
        error.context,
        [
            (
                "source".into(),
                ContextValue::Scalar(Value::Str("SEX".into()))
            ),
            (
                "expected".into(),
                ContextValue::Scalar(Value::Str("numeric".into()))
            ),
            (
                "actual".into(),
                ContextValue::Scalar(Value::Str("str".into()))
            ),
        ]
        .into()
    );
    assert_eq!(error.source_span, None);
    assert_eq!(error.operand_route, None);
    assert_eq!(rounding.apply(ValueRef::Missing), Ok(Value::Missing));
}

#[test]
fn rounding_operand_admission_checks_target_source_and_nested_depth_before_execution() {
    use yamaa_core::{
        dataset::{Assignment, DatasetPlan, Expression, PlanError, RowMode, RowTemplate},
        table::{Column, TableSchema},
        value::ColumnType,
    };
    let schema = TableSchema::new(vec![Column {
        name: "V".into(),
        kind: ColumnType::Float,
    }])
    .unwrap();
    let wrap = |column, expression| Expression::Round {
        source: Box::new(Assignment {
            column,
            expression,
            path: "round.source".into(),
        }),
        rounding: Rounding::new("SRC.V", "round", "1").unwrap(),
    };
    let make = |expression| {
        DatasetPlan::new(
            schema.clone(),
            schema.clone(),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![Assignment {
                    column: 0,
                    expression,
                    path: "round".into(),
                }],
                filter: None,
            }],
            vec![],
            vec![0],
            vec![],
        )
    };
    assert_eq!(
        make(wrap(1, Expression::Source(0))),
        Err(PlanError::InvalidColumns)
    );
    assert_eq!(
        make(wrap(0, Expression::Source(1))),
        Err(PlanError::InvalidSource)
    );
    let mut nested = Expression::Source(0);
    for _ in 0..64 {
        nested = wrap(0, nested);
    }
    assert!(make(nested.clone()).is_ok());
    assert_eq!(make(wrap(0, nested)), Err(PlanError::InvalidColumns));
}
