//! Scalar contracts.
#[allow(dead_code)]
mod support;
mod scalar {
    mod aggregate_reduction {
        use yamaa_core::{
            aggregate_parser::Reducer,
            aggregate_reduction::{reduce_values, AggregateReductionError},
            table::ValueRef,
            value::{Value, ValueType},
        };

        #[test]
        fn extrema_keep_exact_values_types_missingness_and_text_order() {
            let values = [
                Value::Missing,
                Value::Int(9_007_199_254_740_993),
                Value::float(9_007_199_254_740_992.0),
            ];
            let borrowed = values.iter().map(ValueRef::from).collect::<Vec<_>>();
            assert_eq!(
                reduce_values(&borrowed, Reducer::Max, "MAX(D.V)"),
                Ok(ValueRef::Int(9_007_199_254_740_993))
            );
            assert_eq!(
                reduce_values(&borrowed, Reducer::Min, "MIN(D.V)"),
                Ok(ValueRef::from(&values[2]))
            );
            let text = [
                ValueRef::Str("é"),
                ValueRef::Str("😀"),
                ValueRef::Missing,
                ValueRef::Str(""),
            ];
            assert_eq!(
                reduce_values(&text, Reducer::Min, "MIN(D.V)"),
                Ok(ValueRef::Str(""))
            );
            assert_eq!(
                reduce_values(&text, Reducer::Max, "MAX(D.V)"),
                Ok(ValueRef::Str("😀"))
            );
            let equal = [Value::Int(2), Value::float(2.0)];
            for reducer in [Reducer::Min, Reducer::Max] {
                assert_eq!(
                    reduce_values(
                        &equal.iter().map(ValueRef::from).collect::<Vec<_>>(),
                        reducer,
                        "D.V"
                    ),
                    Ok(ValueRef::Int(2))
                );
                assert_eq!(reduce_values(&[], reducer, "D.V"), Ok(ValueRef::Missing));
                assert_eq!(
                    reduce_values(&[ValueRef::Missing; 2], reducer, "D.V"),
                    Ok(ValueRef::Missing)
                );
            }
            assert_eq!(
                reduce_values(
                    &[ValueRef::Int(1), ValueRef::Str("1")],
                    Reducer::Min,
                    "MIN(D.V)"
                ),
                Err(AggregateReductionError::IncompatibleInputs {
                    argument: 1,
                    left: ValueType::Int,
                    right: ValueType::Str
                })
            );
        }

        #[test]
        fn only_counts_records_before_missing_handling_and_count_retains_empty_distinction() {
            assert_eq!(
                reduce_values(&[], Reducer::Only, "ONLY(D.V)"),
                Ok(ValueRef::Missing)
            );
            assert_eq!(
                reduce_values(&[ValueRef::Missing], Reducer::Only, "ONLY(D.V)"),
                Ok(ValueRef::Missing)
            );
            assert_eq!(
                reduce_values(&[ValueRef::Bool(false)], Reducer::Only, "ONLY(D.V)"),
                Ok(ValueRef::Bool(false))
            );
            assert_eq!(
                reduce_values(&[ValueRef::Missing; 2], Reducer::Only, "ONLY(D.V)"),
                Err(AggregateReductionError::MultipleRecords { count: 2 })
            );
            assert_eq!(
                reduce_values(&[], Reducer::Count, "COUNT(D.V)"),
                Ok(ValueRef::Missing)
            );
            assert_eq!(
                reduce_values(&[ValueRef::Missing; 2], Reducer::Count, "COUNT(D.V)"),
                Ok(ValueRef::Int(0))
            );
        }
    }
    mod arithmetic {
        use yamaa_core::numeric::{
            binary, unary, ArithmeticErrorKind, BinaryOperator as B, Number, UnaryOperator as U,
        };

        /// Decode one test operand while preserving explicit integer and float types.
        fn number(token: &str) -> Number {
            if token == "missing" {
                return Number::Missing;
            }
            let (kind, value) = token.split_once(':').unwrap();
            match kind {
                "int" => Number::Int(value.parse().unwrap()),
                "float" => Number::float(value.parse().unwrap()),
                _ => panic!("unknown fixture type {kind}"),
            }
        }

        #[test]
        /// Check shared hand-written values, float bits and complete arithmetic conditions.
        fn independently_specified_arithmetic_vectors() {
            for row in include_str!("fixtures/arithmetic.tsv").lines().skip(1) {
                let columns: Vec<_> = row.split('\t').collect();
                assert_eq!(columns.len(), 6, "malformed fixture {row}");
                let (id, expression, operator) = (columns[0], columns[1], columns[2]);
                let (left, right) = (number(columns[3]), number(columns[4]));
                let result = match operator {
                    "plus" => unary(U::Plus, left, expression),
                    "negate" => unary(U::Negate, left, expression),
                    "abs" => unary(U::Abs, left, expression),
                    _ => binary(
                        match operator {
                            "add" => B::Add,
                            "subtract" => B::Subtract,
                            "multiply" => B::Multiply,
                            "divide" => B::Divide,
                            "modulo" => B::Modulo,
                            _ => panic!("unknown fixture operator {operator}"),
                        },
                        left,
                        right,
                        expression,
                    ),
                };
                let actual = match result {
                    Ok(Number::Missing) => "missing".to_owned(),
                    Ok(Number::Int(value)) => format!("int:{value}"),
                    Ok(Number::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
                    Err(error) => {
                        assert_eq!(error.phase(), "derivation", "{id}");
                        assert_eq!(error.expression, expression, "{id}");
                        let mut text =
                            format!("error:{}:{}", error.condition(), error.requirement());
                        if let ArithmeticErrorKind::IntegerOverflow { value } = error.kind {
                            text.push_str(&format!(":{value}"));
                        }
                        text
                    }
                };
                assert_eq!(actual, columns[5], "{id}: {expression}");
            }
        }

        #[test]
        /// Ensure a non-finite intermediate becomes missing before a later zero divisor.
        fn every_operator_normalizes_before_the_next_operator() {
            let overflow =
                binary(B::Multiply, Number::float(1e308), Number::Int(2), "L * 2").unwrap();
            assert_eq!(overflow, Number::Missing);
            assert_eq!(
                binary(B::Divide, overflow, Number::Int(0), "(L * 2) / 0"),
                Ok(Number::Missing)
            );
        }

        #[test]
        /// Protect the required left-to-right binary64 addition result from reassociation.
        fn ordered_float_addition_does_not_reassociate() {
            let first =
                binary(B::Add, Number::float(0.1), Number::float(0.2), "0.1 + 0.2").unwrap();
            let sum = binary(B::Add, first, Number::float(0.3), "(0.1 + 0.2) + 0.3").unwrap();
            let Number::Float(sum) = sum else {
                panic!("float result required");
            };
            assert_eq!(sum.get().to_bits(), 0x3fe3333333333334);
        }

        #[test]
        /// Verify each primitive applies missing propagation before exceptional arithmetic.
        fn missing_propagates_for_every_operator_before_arithmetic() {
            for operator in [B::Add, B::Subtract, B::Multiply, B::Divide, B::Modulo] {
                for present in [
                    Number::Int(i64::MIN),
                    Number::Int(0),
                    Number::Int(i64::MAX),
                    Number::float(-0.0),
                ] {
                    assert_eq!(
                        binary(operator, Number::Missing, present, "missing left"),
                        Ok(Number::Missing)
                    );
                    assert_eq!(
                        binary(operator, present, Number::Missing, "missing right"),
                        Ok(Number::Missing)
                    );
                }
            }
            for operator in [U::Plus, U::Negate, U::Abs] {
                assert_eq!(
                    unary(operator, Number::Missing, "missing unary"),
                    Ok(Number::Missing)
                );
            }
        }
    }
    mod conversion {
        use yamaa_core::conversion::{convert, float_text};
        use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
        use yamaa_core::value::{ColumnType, FiniteFloat, Value, ValueType};

        /// Apply the numeric grammar without inheriting Python's configurable digit cap.
        #[test]
        fn long_integer_text_preserves_value_or_exact_range_diagnostic() {
            let leading_zeros = Value::Str("0".repeat(5000) + "1");
            assert_eq!(convert(&leading_zeros, ColumnType::Int), Ok(Value::Int(1)));
            let integer = "9".repeat(5000);
            let error = convert(&Value::Str(integer.clone()), ColumnType::Int).unwrap_err();
            assert_eq!(error.requirement(), "REQ-0021");
            assert_eq!(error.value.as_number().unwrap().as_str(), integer);
            assert_eq!(error.source_type(), Some(ValueType::Int));
        }

        /// Decode shared typed inputs, allowing explicit bits for decimal halfway cases.
        fn decode(token: &str) -> Value {
            if token == "missing" {
                return Value::Missing;
            }
            let (kind, text) = token.split_once(':').unwrap();
            match kind {
                "str" => Value::Str(text.into()),
                "int" => Value::Int(text.parse().unwrap()),
                "float" => Value::float(text.parse().unwrap()),
                "bits" => Value::float(f64::from_bits(u64::from_str_radix(text, 16).unwrap())),
                "bool" => Value::Bool(text.parse().unwrap()),
                "date" => Value::Date(text.parse().unwrap()),
                "datetime" => Value::DateTime(text.parse().unwrap()),
                _ => panic!("unknown fixture type {kind}"),
            }
        }

        /// Encode observed values without rounding floats or discarding runtime types.
        fn encode(value: &Value) -> String {
            match value {
                Value::Missing => "missing".into(),
                Value::Str(text) => format!("str:{text}"),
                Value::Int(number) => format!("int:{number}"),
                Value::Float(number) => format!("float:{:016x}", number.get().to_bits()),
                Value::Bool(boolean) => format!("bool:{boolean}"),
                Value::Date(date) => format!("date:{date}"),
                Value::DateTime(date) => format!("datetime:{date}"),
            }
        }

        /// Compare all successful matrix cells and portable failure data to hand-written truth.
        #[test]
        fn shared_conversion_vectors() {
            for row in include_str!("fixtures/conversion.tsv").lines().skip(1) {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 4, "{row}");
                let target = match fields[2] {
                    "str" => ColumnType::Str,
                    "int" => ColumnType::Int,
                    "float" => ColumnType::Float,
                    "date" => ColumnType::Date,
                    "datetime" => ColumnType::DateTime,
                    _ => panic!("bad target"),
                };
                let result = convert(&decode(fields[1]), target);
                let actual = match result {
                    Ok(value) => encode(&value),
                    Err(error) => {
                        assert_eq!(error.target, target);
                        assert_eq!(error.phase(), "convert");
                        assert_eq!(error.condition(), "conversion_failed");
                        assert_eq!(error.applicable_handler(), "unconvertible");
                        let context = match error.source_type() {
                            None => {
                                assert!(error.value.is_null());
                                "missing".into()
                            }
                            Some(ValueType::Float) => {
                                format!("float:{:016x}", error.value.as_f64().unwrap().to_bits())
                            }
                            Some(ValueType::Int) => {
                                format!("int:{}", error.value.as_number().unwrap())
                            }
                            Some(ValueType::Bool) => {
                                format!("bool:{}", error.value.as_bool().unwrap())
                            }
                            Some(ValueType::Str) => {
                                format!("str:{}", error.value.as_str().unwrap())
                            }
                            Some(ValueType::Date) => {
                                format!("date:{}", error.value.as_str().unwrap())
                            }
                            Some(ValueType::DateTime) => {
                                format!("datetime:{}", error.value.as_str().unwrap())
                            }
                        };
                        format!("error:{}:{context}", error.requirement())
                    }
                };
                assert_eq!(actual, fields[3], "{}", fields[0]);
            }
        }

        /// Every scalar type/destination pairing has a declared result or a typed failure.
        #[test]
        fn complete_conversion_matrix() {
            let sources = [
                decode("str:42"),
                decode("int:42"),
                decode("float:42.0"),
                decode("date:2025-01-02"),
                decode("datetime:2025-01-02T00:00"),
                decode("bool:true"),
                Value::Missing,
            ];
            let targets = [
                ColumnType::Str,
                ColumnType::Int,
                ColumnType::Float,
                ColumnType::Date,
                ColumnType::DateTime,
            ];
            let allowed = [
                [true, true, true, false, false],
                [true, true, true, false, false],
                [true, true, true, false, false],
                [true, false, false, true, false],
                [true, false, false, false, true],
                [false, false, false, false, false],
                [true, true, true, true, true],
            ];
            for (source, outcomes) in sources.iter().zip(allowed) {
                for (target, succeeds) in targets.into_iter().zip(outcomes) {
                    assert_eq!(
                        convert(source, target).is_ok(),
                        succeeds,
                        "{source:?} -> {target:?}"
                    );
                }
            }
        }

        /// Identity retains collected precision while text round trips lose that provenance.
        #[test]
        fn temporal_precision_at_conversion_boundaries() {
            let date = Date::new(2025, 1, 2, DatePrecision::Year).unwrap();
            let Value::Date(identity) = convert(&Value::Date(date), ColumnType::Date).unwrap()
            else {
                panic!("date");
            };
            assert_eq!(identity.collected_precision(), DatePrecision::Year);
            let text = convert(&Value::Date(date), ColumnType::Str).unwrap();
            let Value::Date(parsed) = convert(&text, ColumnType::Date).unwrap() else {
                panic!("date");
            };
            assert_eq!(parsed.collected_precision(), DatePrecision::Day);
            let datetime = DateTime::new(date, 0, 0, 0, DateTimePrecision::Day).unwrap();
            let Value::DateTime(identity) =
                convert(&Value::DateTime(datetime), ColumnType::DateTime).unwrap()
            else {
                panic!("datetime");
            };
            assert_eq!(identity.collected_precision(), DateTimePrecision::Day);
            let text = convert(&Value::DateTime(datetime), ColumnType::Str).unwrap();
            let Value::DateTime(parsed) = convert(&text, ColumnType::DateTime).unwrap() else {
                panic!("datetime");
            };
            assert_eq!(parsed.collected_precision(), DateTimePrecision::Second);
        }

        /// Probe the extreme binary64 exponent range without using either engine as truth.
        #[test]
        fn extreme_float_text_is_positional_and_shortest() {
            assert_eq!(
                float_text(FiniteFloat::new(f64::MAX).unwrap()),
                "17976931348623157".to_owned() + &"0".repeat(292)
            );
            assert_eq!(
                float_text(FiniteFloat::new(f64::from_bits(1)).unwrap()),
                "0.".to_owned() + &"0".repeat(323) + "5"
            );
            assert_eq!(float_text(FiniteFloat::new(-0.0).unwrap()), "-0");
        }

        /// Check bit-preserving text round trips across a deterministic exponent/sign sample.
        #[test]
        fn sampled_floats_round_trip_without_exponent_notation() {
            let mut bits = 0x123456789abcdef0_u64;
            for _ in 0..20000 {
                bits ^= bits << 13;
                bits ^= bits >> 7;
                bits ^= bits << 17;
                if let Some(value) = FiniteFloat::new(f64::from_bits(bits)) {
                    let text = float_text(value);
                    assert!(!text.contains(['e', 'E']));
                    assert_eq!(text.parse::<f64>().unwrap().to_bits(), bits, "{text}");
                }
            }
        }

        /// Reject host-friendly numeric spellings that are outside the specification grammar.
        #[test]
        fn numeric_text_rejects_whitespace_unicode_and_incomplete_forms() {
            for text in [
                "1 ", "\t1", "1\n", "1\r", "１", "١", "+", "-", "1e", "1.e2", "1e.2", "+-1",
                ".Nan", "-.nan",
            ] {
                for target in [ColumnType::Int, ColumnType::Float] {
                    let error = convert(&Value::Str(text.into()), target).unwrap_err();
                    assert_eq!(error.requirement(), "REQ-0013", "{text}");
                }
            }
        }
    }
    mod declarative_rounding {
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            dataset::Rounding, diagnostic::ConditionCode, table::ValueRef, value::Value,
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
                    ("source".into(), JsonValue::String("SEX".into())),
                    ("expected".into(), JsonValue::String("numeric".into())),
                    ("actual".into(), JsonValue::String("str".into())),
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
            for _ in 0..128 {
                nested = wrap(0, nested);
            }
            assert!(make(nested.clone()).is_ok());
            assert_eq!(make(wrap(0, nested)), Err(PlanError::InvalidColumns));
        }
    }
    mod declarative_temporal {
        use yamaa_core::{
            dataset::{CalendarUnit as U, DateBounds as B, Temporal, TemporalKind as K},
            table::ValueRef,
            temporal::Date,
            value::Value,
        };
        fn date(text: &str) -> Date {
            text.parse().unwrap()
        }
        fn difference(start: &str, end: &str, unit: U, bounds: B) -> i64 {
            let operation = Temporal::new(
                K::Difference { unit, bounds },
                vec!["START".into(), "END".into()],
                "diff".into(),
            )
            .unwrap();
            match operation
                .apply(&[Some(date(start)), Some(date(end))])
                .unwrap()
            {
                Value::Int(value) => value,
                other => panic!("{other:?}"),
            }
        }
        #[test]
        fn whole_calendar_units_preserve_clamped_anniversaries_sign_and_endpoint_rules() {
            for (start, end, unit, bounds, expected) in [
                ("2020-02-29", "2021-02-28", U::Year, B::Exclusive, 1),
                ("2020-02-29", "2021-02-27", U::Year, B::Exclusive, 0),
                ("2023-01-31", "2023-02-28", U::Month, B::Exclusive, 1),
                ("2024-01-31", "2024-02-28", U::Month, B::Exclusive, 0),
                ("2024-01-31", "2024-02-29", U::Month, B::Exclusive, 1),
                ("2023-01-31", "2023-03-30", U::Month, B::Exclusive, 1),
                ("2023-01-31", "2023-03-31", U::Month, B::Exclusive, 2),
                ("2024-02-28", "2024-03-07", U::Week, B::Exclusive, 1),
                ("2023-02-28", "2023-03-06", U::Week, B::Exclusive, 0),
                ("2024-02-28", "2024-03-01", U::Day, B::Exclusive, 2),
                ("2024-02-28", "2024-03-01", U::Day, B::Inclusive, 3),
                ("2024-02-28", "2024-03-01", U::Day, B::Between, 1),
                ("0001-01-01", "9999-12-31", U::Day, B::Exclusive, 3_652_058),
            ] {
                assert_eq!(difference(start, end, unit, bounds), expected);
                assert_eq!(difference(end, start, unit, bounds), -expected);
            }
            assert_eq!(
                difference("2024-03-01", "2024-03-01", U::Day, B::Between),
                0
            );
            assert_eq!(
                difference("2024-03-01", "2024-03-01", U::Day, B::Inclusive),
                1
            );
        }
        #[test]
        fn study_day_has_no_zero_and_epoch_counts_use_complete_civil_dates() {
            let study = Temporal::new(
                K::StudyDay,
                vec!["DATE".into(), "REF".into()],
                "study".into(),
            )
            .unwrap();
            for (day, expected) in [("2023-12-31", -1), ("2024-01-01", 1), ("2024-01-02", 2)] {
                assert_eq!(
                    study.apply(&[Some(date(day)), Some(date("2024-01-01"))]),
                    Some(Value::Int(expected))
                );
            }
            let epoch = Temporal::new(K::EpochDay, vec!["DATE".into()], "epoch".into()).unwrap();
            for (day, expected) in [
                ("1970-01-01", 0),
                ("1969-12-31", -1),
                ("2000-01-01", 10_957),
            ] {
                assert_eq!(epoch.apply(&[Some(date(day))]), Some(Value::Int(expected)));
            }
            assert_eq!(epoch.apply(&[None]), Some(Value::Missing));
            assert_eq!(epoch.apply(&[]), None);
            assert_eq!(
                study.apply(&[None, Some(date("2024-01-01"))]),
                Some(Value::Missing)
            );
        }
        #[test]
        fn immutable_declarations_reject_invalid_arity_and_bounds_and_retain_typed_operand_facts() {
            assert!(Temporal::new(K::StudyDay, vec!["DATE".into()], "study".into()).is_err());
            assert!(Temporal::new(
                K::Difference {
                    unit: U::Year,
                    bounds: B::Inclusive
                },
                vec!["S".into(), "E".into()],
                "diff".into()
            )
            .is_err());
            let operation = Temporal::new(
                K::StudyDay,
                vec!["DTHDTM".into(), "REF".into()],
                "columns.DTHDY.derivation.study_day".into(),
            )
            .unwrap();
            assert_eq!(operation.operand(0, ValueRef::Missing), Some(Ok(None)));
            let diagnostic = operation
                .operand(0, ValueRef::Str("2024-01-01"))
                .unwrap()
                .unwrap_err();
            assert_eq!(diagnostic.definition().phase, "validation");
            assert_eq!(diagnostic.definition().condition, "incompatible_input_type");
            assert_eq!(diagnostic.definition().requirement, Some("REQ-0606"));
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.DTHDY.derivation.study_day.date"]
            );
            assert_eq!(diagnostic.context.len(), 3);
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
        }

        #[test]
        fn temporal_metadata_is_complete_and_operand_indexes_stay_validated() {
            let huge = "x".repeat(200_000);
            let operation = Temporal::new(K::EpochDay, vec![huge.clone()], "epoch".into()).unwrap();
            assert_eq!(operation.sources(), [huge]);
            assert_eq!(operation.operand(1, ValueRef::Missing), None);
        }
    }
    mod declarative_temporal_imputation {
        use yamaa_core::{
            dataset::{
                date_policy, DayDeclaration as D, Imputation, ImputationKind as K,
                ImputationStep as S, TimeRule,
            },
            table::ValueRef,
            temporal::{Date, DatePrecision as P, DateTime, DateTimePrecision as TP},
            value::Value,
        };
        fn operation(
            kind: K,
            bound: bool,
            missing: Option<Value>,
            invalid: Option<Value>,
        ) -> Imputation {
            Imputation::new(
                kind,
                "SRC.TEXT".into(),
                bound.then(|| "REF".into()),
                "columns.DATE.derivation.impute".into(),
                missing,
                invalid,
            )
            .unwrap()
        }
        fn date_policy_kind(minimum: P, day: D<'_>) -> K {
            date_policy((minimum == P::Year).then_some("6"), day, minimum, "date").unwrap()
        }
        #[test]
        fn independent_partial_dates_preserve_precision_and_calendar_bound_intervals() {
            let first = operation(
                date_policy_kind(P::Year, D::Position("first")),
                false,
                None,
                None,
            );
            let last = operation(
                date_policy_kind(P::Year, D::Position("last")),
                false,
                None,
                None,
            );
            for (input, early, late, precision) in [
                ("1975", "1975-06-01", "1975-06-30", P::Year),
                ("1980-02", "1980-02-01", "1980-02-29", P::Month),
                ("1900-02", "1900-02-01", "1900-02-28", P::Month),
                ("1990-06-15", "1990-06-15", "1990-06-15", P::Day),
            ] {
                for (op, expected) in [(&first, early), (&last, late)] {
                    let S::Value(Value::Date(date)) =
                        op.source_value(ValueRef::Str(input)).unwrap()
                    else {
                        panic!("date")
                    };
                    assert_eq!(date.to_string(), expected);
                    assert_eq!(date.collected_precision(), precision);
                    let flag = operation(K::DatePrecision, false, None, None)
                        .source_value(ValueRef::Date(date))
                        .unwrap();
                    assert_eq!(
                        flag,
                        S::Value(Value::Str(
                            match precision {
                                P::Year => "Y",
                                P::Month => "M",
                                P::Day => "D",
                            }
                            .into()
                        ))
                    );
                }
            }
            let bounded = operation(
                date_policy_kind(P::Year, D::Position("first")),
                true,
                None,
                None,
            );
            for (text, lower, expected) in [
                ("2024", "2024-07-20", Some("2024-07-20")),
                ("2024-02", "2024-02-29", Some("2024-02-29")),
                ("2024-02", "2024-03-01", None),
                ("2024", "2025-01-01", None),
            ] {
                let S::Bound(completed) = bounded.source_value(ValueRef::Str(text)).unwrap() else {
                    panic!("bound")
                };
                let result = bounded
                    .apply_bound(completed, ValueRef::Date(lower.parse().unwrap()))
                    .unwrap();
                assert_eq!(
                    result,
                    expected.map_or(Value::Missing, |text| Value::Date(text.parse().unwrap()))
                );
                if let Value::Date(date) = result {
                    assert_eq!(date.collected_precision(), completed.collected_precision());
                }
            }
            assert!(matches!(
                bounded.source_value(ValueRef::Str("2024-02-29")),
                Ok(S::Value(_))
            ));
            let month = operation(
                date_policy_kind(P::Month, D::Position("first")),
                true,
                Some(Value::Int(1)),
                Some(Value::Int(2)),
            );
            assert_eq!(
                month.source_value(ValueRef::Str("2024")),
                Ok(S::Value(Value::Missing))
            );
            assert_eq!(
                month.source_value(ValueRef::Missing),
                Ok(S::Recovered { handler: "missing" })
            );
            assert_eq!(
                month.source_value(ValueRef::Str("2024-00")),
                Ok(S::Recovered { handler: "invalid" })
            );
        }

        #[test]
        fn original_zero_year_month_prefix_retains_both_operation_diagnostics() {
            use yamaa_core::json::Value as JsonValue;
            for (kind, name) in [
                (
                    date_policy_kind(P::Year, D::Position("first")),
                    "date_impute",
                ),
                (K::DatePrecision, "date_precision"),
            ] {
                let path = format!("columns.DATE.derivation.{name}");
                let op = Imputation::new(kind, "S".into(), None, path.clone(), None, None).unwrap();
                let error = op.source_value(ValueRef::Str("0000-01")).unwrap_err();
                assert_eq!(error.definition().condition, "invalid_date_text");
                assert_eq!(error.definition().requirement, Some("REQ-0588"));
                assert_eq!(error.spec_paths, [path]);
                assert_eq!(
                    error.context,
                    [
                        ("operation".into(), JsonValue::String(name.into())),
                        ("source".into(), JsonValue::String("S".into())),
                        ("value".into(), JsonValue::String("0000-01".into())),
                    ]
                    .into()
                );
            }
        }

        #[test]
        fn datetime_edges_precision_and_explicit_date_conversion_are_field_copies() {
            for (rule, expected) in [
                (TimeRule::First, "2024-02-29T00:00:00"),
                (TimeRule::Last, "2024-02-29T23:59:59"),
            ] {
                let op = operation(K::DateTime { time: rule }, false, None, None);
                let S::Value(Value::DateTime(value)) =
                    op.source_value(ValueRef::Str("2024-02-29")).unwrap()
                else {
                    panic!("datetime")
                };
                assert_eq!(value.to_string(), expected);
                assert_eq!(value.collected_precision(), TP::Day);
                assert_eq!(
                    operation(K::DateTimePrecision, false, None, None)
                        .source_value(ValueRef::DateTime(value)),
                    Ok(S::Value(Value::Str("D".into())))
                );
                let S::Value(Value::Date(date)) = operation(K::ToDate, false, None, None)
                    .source_value(ValueRef::DateTime(value))
                    .unwrap()
                else {
                    panic!("date")
                };
                assert_eq!(date.to_string(), "2024-02-29");
                assert_eq!(date.collected_precision(), P::Day);
            }
            let date: Date = "2024-02-29".parse().unwrap();
            let datetime: DateTime = "2024-02-29T12:13".parse().unwrap();
            let op = operation(
                K::DateTime {
                    time: TimeRule::First,
                },
                false,
                None,
                None,
            );
            assert!(op.source_value(ValueRef::Date(date)).is_err());
            assert_eq!(
                op.source_value(ValueRef::DateTime(datetime)),
                Ok(S::Value(Value::DateTime(datetime)))
            );
            assert_eq!(
                operation(K::DateTimePrecision, false, None, None)
                    .source_value(ValueRef::Str("2024-02-29T12:13")),
                Ok(S::Value(Value::Str("S".into())))
            );
            for invalid in [
                "0000",
                "2024-13",
                "2023-02-29",
                "2024-02-",
                "2024-02-29Z",
                "2024-02-29T00:00:00+00:00",
                "\u{0662}024",
                "2024\u{2010}02",
            ] {
                assert!(
                    operation(K::DatePrecision, false, None, None)
                        .source_value(ValueRef::Str(invalid))
                        .is_err(),
                    "{invalid}"
                );
            }
        }
        use crate::support::diagnostic::temporal_imputation as diagnostic;
        #[test]
        fn independent_diagnostic_truth_reaches_every_imputation_cause() {
            assert_eq!(diagnostic::reached().len(), 16);
        }

        #[test]
        fn calendar_policy_preserves_wide_integers_and_unreachable_month_precedence() {
            use yamaa_core::dataset::DatePolicyError;
            use yamaa_core::diagnostic::ConditionCode as C;
            use yamaa_core::json::Value as JsonValue;
            let wide = "9".repeat(5000);
            let DatePolicyError::Diagnostic(error) =
                date_policy(Some(&wide), D::Number("0"), P::Month, "date").unwrap_err()
            else {
                panic!("admitted semantic failure")
            };
            assert_eq!(error.code, C::ImputationMonthNotPermitted);
            assert_eq!(
                error.context["month"],
                JsonValue::Number((wide.clone()).parse().expect("canonical integer"))
            );
            let DatePolicyError::Diagnostic(error) =
                date_policy(Some(&wide), D::Number("1"), P::Year, "date").unwrap_err()
            else {
                panic!("admitted semantic failure")
            };
            assert_eq!(error.code, C::ImputationMonthRange);
            assert_eq!(
                error.context["month"],
                JsonValue::Number((wide.clone()).parse().expect("canonical integer"))
            );
            let DatePolicyError::Diagnostic(error) =
                date_policy(Some("6"), D::Number(&wide), P::Year, "date").unwrap_err()
            else {
                panic!("admitted semantic failure")
            };
            assert_eq!(error.code, C::ImputationDayRange);
            assert_eq!(
                error.context["value"],
                JsonValue::Number((wide).parse().expect("canonical integer"))
            );
        }

        #[test]
        fn public_policy_admits_whole_context_before_semantic_copies_or_integer_scans() {
            use yamaa_core::{dataset::DatePolicyError as E, diagnostic::ConditionCode as C};
            let oversized = "9".repeat(200_000);
            let E::Diagnostic(finding) =
                date_policy(Some(&oversized), D::Number("0"), P::Month, "date").unwrap_err()
            else {
                panic!("month policy");
            };
            assert_eq!(finding.code, C::ImputationMonthNotPermitted);
            let E::Diagnostic(finding) =
                date_policy(Some("6"), D::Position(&oversized), P::Year, "date").unwrap_err()
            else {
                panic!("day policy");
            };
            assert_eq!(finding.code, C::ImputationDayRule);
            assert!(date_policy(Some("6"), D::Number("1"), P::Year, &oversized).is_ok());
            assert!(date_policy(Some("6"), D::Number("1"), P::Year, &oversized[..100_000]).is_ok());
            for number in ["", "01", "+1", "-0", "1.0", "bad"] {
                assert_eq!(
                    date_policy(Some(number), D::Number("1"), P::Year, "date"),
                    Err(E::Shape)
                );
                assert_eq!(
                    date_policy(Some("6"), D::Number(number), P::Year, "date"),
                    Err(E::Shape)
                );
            }
            assert_eq!(
                date_policy(None, D::Number("1"), P::Day, "date"),
                Err(E::Shape)
            );
            assert_eq!(
                date_policy(Some("6"), D::Number("1"), P::Year, ""),
                Err(E::Shape)
            );
        }

        #[test]
        fn constructor_retains_complete_metadata_and_rejects_invalid_shapes() {
            let oversized = "x".repeat(100_000);
            for field in ["source", "bound", "path", "missing", "invalid"] {
                assert!(
                    Imputation::new(
                        date_policy_kind(P::Year, D::Position("first")),
                        if field == "source" {
                            oversized.clone()
                        } else {
                            "SRC.D".into()
                        },
                        Some(if field == "bound" {
                            oversized.clone()
                        } else {
                            "SRC.B".into()
                        }),
                        if field == "path" {
                            oversized.clone()
                        } else {
                            "columns.D".into()
                        },
                        (field == "missing").then(|| Value::Str(oversized.clone())),
                        (field == "invalid").then(|| Value::Str(oversized.clone())),
                    )
                    .is_some(),
                    "{field} metadata must be retained"
                );
            }
            assert!(Imputation::new(
                K::DatePrecision,
                oversized[..oversized.len() / 2].to_owned(),
                None,
                oversized[..oversized.len() / 2].to_owned(),
                None,
                None,
            )
            .is_some());
            assert!(Imputation::new(
                K::DatePrecision,
                String::new(),
                None,
                "path".into(),
                None,
                None
            )
            .is_none());
            assert!(Imputation::new(
                K::DatePrecision,
                "SRC.D".into(),
                None,
                String::new(),
                None,
                None
            )
            .is_none());
            assert!(Imputation::new(
                K::DatePrecision,
                "SRC.D".into(),
                Some("SRC.B".into()),
                "path".into(),
                None,
                None
            )
            .is_none());
            assert!(Imputation::new(
                K::ToDate,
                "SRC.D".into(),
                None,
                "path".into(),
                Some(Value::Missing),
                None
            )
            .is_none());
        }
    }
    mod evaluation {
        use std::collections::BTreeMap;
        use std::convert::Infallible;

        use yamaa_core::evaluation::{
            EvaluationErrorKind, NumericCondition, NumericNode, NumericPlan, NumericResolver,
            Operand,
        };
        use yamaa_core::numeric::{
            ArithmeticErrorKind, BinaryOperator as B, Number, UnaryOperator as U,
        };
        use yamaa_core::numeric_compiler::compile_numeric;
        use yamaa_core::value::{Selection, Value, ValueType};

        const SPEC_PATH: &str = "columns.A.derivation.compute";

        /// Decode fixture values without coercion or inference from the destination type.
        fn value(token: &str) -> Value {
            if token == "missing" {
                return Value::Missing;
            }
            let (kind, text) = token.split_once(':').unwrap();
            match kind {
                "int" => Value::Int(text.parse().unwrap()),
                "float" => Value::float(text.parse().unwrap()),
                "str" => Value::Str(text.into()),
                "bool" => Value::Bool(text.parse().unwrap()),
                _ => panic!("bad fixture value {token}"),
            }
        }

        /// Convert test-only postfix notation into typed nodes, not the language grammar.
        fn tree(postfix: &str) -> NumericNode {
            let mut stack = Vec::new();
            for token in postfix.split_whitespace() {
                let node = if let Some(name) = token.strip_prefix('$') {
                    NumericNode::Identifier(name.into())
                } else if let Some(operator) = match token {
                    "plus" => Some(U::Plus),
                    "negate" => Some(U::Negate),
                    "abs" => Some(U::Abs),
                    _ => None,
                } {
                    NumericNode::Unary {
                        operator,
                        operand: Box::new(stack.pop().unwrap()),
                    }
                } else if let Some(operator) = match token {
                    "add" => Some(B::Add),
                    "subtract" => Some(B::Subtract),
                    "multiply" => Some(B::Multiply),
                    "divide" => Some(B::Divide),
                    "modulo" => Some(B::Modulo),
                    _ => None,
                } {
                    let right = Box::new(stack.pop().unwrap());
                    let left = Box::new(stack.pop().unwrap());
                    NumericNode::Binary {
                        operator,
                        left,
                        right,
                    }
                } else {
                    NumericNode::Literal(Number::try_from(&value(token)).unwrap())
                };
                stack.push(node);
            }
            assert_eq!(stack.len(), 1, "malformed test tree {postfix}");
            stack.pop().unwrap()
        }

        /// Associate a caller-supplied tree with the original expression's provenance.
        fn plan(expression: &str, postfix: &str) -> NumericPlan {
            NumericPlan {
                spec_path: SPEC_PATH.into(),
                expression: expression.into(),
                root: tree(postfix),
            }
        }

        /// A fake port that records every occurrence, including absent identifiers.
        struct RecordingResolver {
            values: BTreeMap<String, Value>,
            trace: Vec<String>,
        }

        impl RecordingResolver {
            /// Decode fixture bindings without coupling their order to evaluation order.
            fn new(bindings: &str) -> Self {
                Self {
                    values: if bindings == "-" {
                        BTreeMap::new()
                    } else {
                        bindings
                            .split(';')
                            .map(|entry| {
                                let (name, token) = entry.split_once('=').unwrap();
                                (name.into(), value(token))
                            })
                            .collect()
                    },
                    trace: Vec::new(),
                }
            }
        }

        impl NumericResolver for RecordingResolver {
            type Error = Infallible;

            /// Distinguish lookup absence from an explicitly present missing value.
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.trace.push(name.into());
                Ok(self
                    .values
                    .get(name)
                    .cloned()
                    .map_or(Selection::Absent, Selection::Present))
            }
        }

        /// Encode exact numeric types and bits, including signed zero and missingness.
        fn encoded(number: Number) -> String {
            match number {
                Number::Missing => "missing".into(),
                Number::Int(value) => format!("int:{value}"),
                Number::Float(value) => format!("float:{:016x}", value.get().to_bits()),
            }
        }

        /// Check portable condition metadata and encode exact failure-specific context.
        fn encoded_condition(condition: &NumericCondition) -> String {
            let mut result = format!(
                "error:{}:{}",
                condition.condition(),
                condition.requirement()
            );
            match condition {
                NumericCondition::UnknownField { identifier } => {
                    assert_eq!(condition.phase(), "validation");
                    result.push_str(&format!(":{identifier}"));
                }
                NumericCondition::IncompatibleInput { identifier, actual } => {
                    assert_eq!(condition.phase(), "validation");
                    let actual = match actual {
                        ValueType::Str => "str",
                        ValueType::Bool => "bool",
                        _ => panic!("unexpected fixture type"),
                    };
                    result.push_str(&format!(":{identifier}:{actual}"));
                }
                NumericCondition::LiteralOverflow { value } => {
                    assert_eq!(condition.phase(), "derivation");
                    result.push_str(&format!(":{value}"));
                }
                NumericCondition::Arithmetic(ArithmeticErrorKind::InvalidRoundingDigits) => {
                    assert_eq!(condition.phase(), "validation");
                    result.push_str(":float");
                }
                NumericCondition::Arithmetic(kind) => {
                    assert_eq!(condition.phase(), "derivation");
                    if let ArithmeticErrorKind::IntegerOverflow { value } = kind {
                        result.push_str(&format!(":{value}"));
                    }
                }
            }
            result
        }

        /// Replay the same fixtures through either caller-supplied IR or actual source compilation.
        fn fixture_evaluate(
            expression: &str,
            postfix: &str,
            compiled: bool,
            resolver: &mut RecordingResolver,
        ) -> Result<Number, Box<yamaa_core::evaluation::EvaluationError<Infallible>>> {
            if compiled {
                compile_numeric(expression, SPEC_PATH)
                    .unwrap()
                    .evaluate(resolver)
                    .map_err(|failure| {
                        assert!(failure.source_span.start < failure.source_span.end);
                        assert!(expression
                            .get(failure.source_span.start..failure.source_span.end)
                            .is_some());
                        failure.evaluation
                    })
            } else {
                plan(expression, postfix)
                    .evaluate(resolver)
                    .map_err(Box::new)
            }
        }

        /// Replay independent Python/Rust results, call traces and structural failure paths.
        #[test]
        fn shared_evaluation_vectors() {
            for compiled in [false, true] {
                for row in include_str!("fixtures/evaluation.tsv").lines().skip(1) {
                    let fields: Vec<_> = row.split('\t').collect();
                    assert_eq!(fields.len(), 7);
                    let mut resolver = RecordingResolver::new(fields[3]);
                    let mut route = Vec::new();
                    let actual =
                        match fixture_evaluate(fields[1], fields[2], compiled, &mut resolver) {
                            Ok(number) => encoded(number),
                            Err(error) => {
                                let error = *error;
                                assert_eq!(error.location.spec_path, SPEC_PATH);
                                assert_eq!(error.location.expression, fields[1]);
                                route = error.location.operands;
                                let EvaluationErrorKind::Numeric(condition) = error.kind;
                                encoded_condition(&condition)
                            }
                        };
                    assert_eq!(actual, fields[4], "{}", fields[0]);
                    assert_eq!(
                        resolver.trace.join(","),
                        fields[5].trim_matches('-'),
                        "{}",
                        fields[0]
                    );
                    let route: Vec<_> = route
                        .iter()
                        .map(|position| match position {
                            Operand::Unary => "unary".into(),
                            Operand::Left => "left".into(),
                            Operand::Right => "right".into(),
                            Operand::Argument(index) => format!("argument:{index}"),
                        })
                        .collect();
                    assert_eq!(
                        route.join(","),
                        fields[6].trim_matches('-'),
                        "{}",
                        fields[0]
                    );
                }
            }
        }

        /// Exercise the evaluator against every existing independently specified primitive case.
        #[test]
        fn shared_arithmetic_vectors_through_typed_evaluation() {
            for compiled in [false, true] {
                for row in include_str!("fixtures/arithmetic.tsv").lines().skip(1) {
                    let fields: Vec<_> = row.split('\t').collect();
                    let mut resolver =
                        RecordingResolver::new(&format!("L={};R={}", fields[3], fields[4]));
                    let unary = matches!(fields[2], "plus" | "negate" | "abs");
                    let postfix = format!("{} {}", if unary { "$L" } else { "$L $R" }, fields[2]);
                    let actual =
                        match fixture_evaluate(fields[1], &postfix, compiled, &mut resolver) {
                            Ok(number) => encoded(number),
                            Err(error) => {
                                let error = *error;
                                assert_eq!(error.location.expression, fields[1]);
                                assert_eq!(error.location.spec_path, SPEC_PATH);
                                assert!(error.location.operands.is_empty());
                                let EvaluationErrorKind::Numeric(condition) = error.kind;
                                encoded_condition(&condition)
                            }
                        };
                    assert_eq!(actual, fields[5], "{}", fields[0]);
                    assert_eq!(resolver.trace.join(","), if unary { "L" } else { "L,R" });
                }
            }
        }

        /// Cover every nonnumeric runtime type without silently converting a numeric string.
        #[test]
        fn identifiers_reject_every_nonnumeric_type() {
            for value in [
                Value::Str("42".into()),
                Value::Bool(true),
                Value::Date("2025-01-02".parse().unwrap()),
                Value::DateTime("2025-01-02T03:04".parse().unwrap()),
            ] {
                let actual = value.value_type().unwrap();
                let mut resolver = RecordingResolver {
                    values: [("A".into(), value)].into(),
                    trace: Vec::new(),
                };
                let failure = plan("+A", "$A plus").evaluate(&mut resolver).unwrap_err();
                assert_eq!(failure.location.operands, [Operand::Unary]);
                assert_eq!(
                    failure.kind,
                    EvaluationErrorKind::Numeric(NumericCondition::IncompatibleInput {
                        identifier: "A".into(),
                        actual,
                    })
                );
            }
        }

        /// Preserve opaque, non-Clone resolution errors with their own diagnostic payload.
        #[test]
        fn resolver_failure_is_retained_and_stops_following_operands() {
            #[derive(Debug, PartialEq)]
            struct Failure {
                condition: &'static str,
                context: Vec<&'static str>,
            }
            struct Resolver {
                error: Option<Failure>,
                trace: Vec<String>,
            }
            impl NumericResolver for Resolver {
                type Error = Failure;

                /// Return missing first, then move the original boundary failure exactly once.
                fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                    self.trace.push(name.into());
                    match name {
                        "L" => Ok(Selection::Present(Value::Missing)),
                        "A" => Err(self.error.take().unwrap()),
                        _ => panic!("later operand must not be resolved"),
                    }
                }
            }
            let mut resolver = Resolver {
                error: Some(Failure {
                    condition: "multiple_matches",
                    context: vec!["donor-1", "donor-2"],
                }),
                trace: Vec::new(),
            };
            let failure = plan("L + (A * B)", "$L $A $B multiply add")
                .evaluate(&mut resolver)
                .unwrap_err();
            assert_eq!(resolver.trace, ["L", "A"]);
            assert_eq!(failure.location.spec_path, SPEC_PATH);
            assert_eq!(failure.location.expression, "L + (A * B)");
            assert_eq!(failure.location.operands, [Operand::Right, Operand::Left]);
            assert_eq!(
                failure.kind,
                EvaluationErrorKind::Resolution {
                    identifier: "A".into(),
                    error: Failure {
                        condition: "multiple_matches",
                        context: vec!["donor-1", "donor-2"],
                    },
                }
            );
        }

        /// Each occurrence and each plan execution reaches its resolver, without memoization.
        #[test]
        fn repeated_resolution_and_plan_reuse_do_not_cache_values() {
            struct CountingResolver(i64);
            impl NumericResolver for CountingResolver {
                type Error = Infallible;

                /// Return a distinct number for each observed occurrence.
                fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                    assert_eq!(name, "A");
                    self.0 += 1;
                    Ok(Selection::Present(Value::Int(self.0)))
                }
            }
            let plan = plan("A + A", "$A $A add");
            let mut resolver = CountingResolver(0);
            assert_eq!(plan.evaluate(&mut resolver), Ok(Number::Int(3)));
            assert_eq!(plan.evaluate(&mut resolver), Ok(Number::Int(7)));
            assert_eq!(resolver.0, 4);
        }

        /// Replay function values, eager traces and argument-level failures through compilation.
        #[test]
        fn shared_function_vectors() {
            for row in [
                include_str!("fixtures/numeric_selection.tsv"),
                include_str!("fixtures/numeric_integral.tsv"),
                include_str!("fixtures/numeric_sqrt.tsv"),
                include_str!("fixtures/numeric_rounding.tsv"),
                include_str!("fixtures/numeric_math.tsv"),
            ]
            .into_iter()
            .flat_map(|fixture| fixture.lines().skip(1))
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 6);
                let mut resolver = RecordingResolver::new(fields[2]);
                let plan =
                    yamaa_core::numeric_compiler::compile_numeric(fields[1], SPEC_PATH).unwrap();
                let mut route = Vec::new();
                let actual = match plan.evaluate(&mut resolver) {
                    Ok(number) => encoded(number),
                    Err(error) => {
                        assert!(fields[1]
                            .get(error.source_span.start..error.source_span.end)
                            .is_some());
                        let error = *error.evaluation;
                        assert_eq!(error.location.expression, fields[1]);
                        assert_eq!(error.location.spec_path, SPEC_PATH);
                        route = error.location.operands;
                        let EvaluationErrorKind::Numeric(condition) = error.kind;
                        encoded_condition(&condition)
                    }
                };
                assert_eq!(actual, fields[3], "{}", fields[0]);
                assert_eq!(
                    resolver.trace.join(","),
                    fields[4].trim_matches('-'),
                    "{}",
                    fields[0]
                );
                let route = route
                    .iter()
                    .map(|operand| match operand {
                        Operand::Unary => "unary".into(),
                        Operand::Left => "left".into(),
                        Operand::Right => "right".into(),
                        Operand::Argument(index) => format!("argument:{index}"),
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                assert_eq!(route, fields[5].trim_matches('-'), "{}", fields[0]);
            }
        }
    }
    mod integral {
        use yamaa_core::numeric::{integral, IntegralFunction, Number};

        /// Compare exact result bits against independent standard-library operations,
        /// canonicalizing zero to match the language's integer-intermediate semantics.
        fn check(value: f64) {
            for (function, expected) in [
                (IntegralFunction::Ceil, value.ceil()),
                (IntegralFunction::Floor, value.floor()),
                (IntegralFunction::Trunc, value.trunc()),
            ] {
                let expected = if expected == 0.0 { 0.0 } else { expected };
                let Number::Float(actual) = integral(function, Number::float(value)) else {
                    panic!("finite input must return float");
                };
                assert_eq!(
                    actual.get().to_bits(),
                    expected.to_bits(),
                    "{function:?}({value:?})"
                );
            }
        }

        /// Cover every finite exponent and adjacent significand boundaries under both signs,
        /// then sample deterministic binary64 patterns without a random-library dependency.
        #[test]
        fn integral_functions_match_standard_library_across_binary64() {
            for exponent in 0..2047_u64 {
                for fraction in [0, 1, (1_u64 << 51) - 1, 1_u64 << 51, (1_u64 << 52) - 1] {
                    for sign in [0, 1_u64 << 63] {
                        check(f64::from_bits(sign | (exponent << 52) | fraction));
                    }
                }
            }
            let mut bits = 0x1234_5678_9abc_def0_u64;
            for _ in 0..100_000 {
                bits ^= bits << 13;
                bits ^= bits >> 7;
                bits ^= bits << 17;
                let value = f64::from_bits(bits);
                if value.is_finite() {
                    check(value);
                }
            }
        }
    }
    mod predicate {
        use std::collections::{BTreeMap, VecDeque};
        use std::convert::Infallible;

        use yamaa_core::predicate::{
            Comparison as C, Condition, ErrorKind, Node, Plan, PlanError, Resolver, Route, Scalar,
            Truth,
        };
        use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
        use yamaa_core::value::{Selection, Value, ValueType};

        /// The fixture port records every occurrence and can vary repeated reads.
        #[derive(Default)]
        struct Port {
            values: BTreeMap<String, VecDeque<Value>>,
            trace: Vec<String>,
        }

        impl Port {
            /// Bind a stable value unless a test supplies an explicit sequence later.
            fn with(values: &[(&str, Value)]) -> Self {
                Self {
                    values: values
                        .iter()
                        .map(|(name, value)| ((*name).into(), [value.clone()].into()))
                        .collect(),
                    trace: Vec::new(),
                }
            }
        }

        impl Resolver for Port {
            type Error = Infallible;
            /// Retain the last supplied value while consuming preceding sequence entries.
            fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
                self.trace.push(identifier.into());
                Ok(match self.values.get_mut(identifier) {
                    None => Selection::Absent,
                    Some(values) => Selection::Present(if values.len() > 1 {
                        values.pop_front().unwrap()
                    } else {
                        values[0].clone()
                    }),
                })
            }
        }

        /// Keep authored source context observable in failures.
        fn plan(nodes: Vec<Node>) -> Plan {
            let root = nodes.len() - 1;
            Plan::new(
                nodes,
                root,
                "rows[0].filter".into(),
                "original predicate".into(),
            )
            .unwrap()
        }

        /// Use identifiers to exercise deferred resolution rather than constant folding.
        fn id(name: &str) -> Scalar {
            Scalar::Identifier(name.into())
        }

        /// Embed a normalized operand without host coercion.
        fn lit(value: Value) -> Scalar {
            Scalar::Literal(value)
        }

        /// Produce a one-node comparison over the fixture's two names.
        fn comparison(operator: C) -> Node {
            Node::Compare {
                operator,
                left: id("a"),
                right: id("b"),
            }
        }

        #[test]
        fn complete_three_valued_tables_and_null_tests() {
            use Truth::{False as F, True as T, Unknown as U};
            let values = [T, F, U];
            let and = [[T, F, U], [F, F, F], [U, F, U]];
            let or = [[T, T, T], [T, F, U], [T, U, U]];
            for (i, left) in values.iter().enumerate() {
                assert_eq!(left.negate(), [F, T, U][i]);
                for (j, right) in values.iter().enumerate() {
                    assert_eq!(left.and(*right), and[i][j]);
                    assert_eq!(left.or(*right), or[i][j]);
                }
            }
            for (value, expected) in [
                (Value::Missing, T),
                (Value::Bool(false), F),
                (Value::Int(0), F),
            ] {
                for negated in [false, true] {
                    let p = plan(vec![Node::IsNull {
                        value: lit(value.clone()),
                        negated,
                    }]);
                    assert_eq!(
                        p.evaluate(&mut Port::default()).unwrap(),
                        if negated { expected.negate() } else { expected }
                    );
                }
            }
        }

        #[test]
        fn comparison_promotion_order_missing_and_temporal_precision() {
            use Truth::{False as F, True as T};
            let date = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
            let day = Date::new(2025, 1, 1, DatePrecision::Day).unwrap();
            let datetime = DateTime::new(day, 0, 0, 0, DateTimePrecision::Day).unwrap();
            let second = DateTime::new(day, 0, 0, 0, DateTimePrecision::Second).unwrap();
            let pairs = [
                (
                    Value::Int(9_007_199_254_740_993),
                    Value::float(9_007_199_254_740_992.0),
                    [T, F, F, T, F, T],
                ),
                (
                    Value::Int(9_007_199_254_740_993),
                    Value::Int(9_007_199_254_740_992),
                    [F, T, F, F, T, T],
                ),
                (
                    Value::Int(i64::MAX),
                    Value::float(9_223_372_036_854_775_808.0),
                    [T, F, F, T, F, T],
                ),
                (
                    Value::Int(i64::MIN),
                    Value::Int(i64::MAX),
                    [F, T, T, T, F, F],
                ),
                (Value::float(-0.0), Value::float(0.0), [T, F, F, T, F, T]),
                (
                    Value::Str("A".into()),
                    Value::Str("a".into()),
                    [F, T, T, T, F, F],
                ),
                (
                    Value::Str("\u{e9}".into()),
                    Value::Str("e\u{301}".into()),
                    [F, T, F, F, T, T],
                ),
                (Value::Date(date), Value::Date(day), [T, F, F, T, F, T]),
                (
                    Value::DateTime(datetime),
                    Value::DateTime(second),
                    [T, F, F, T, F, T],
                ),
            ];
            for (a, b, truth) in pairs {
                for (operator, expected) in [
                    C::Equal,
                    C::NotEqual,
                    C::Less,
                    C::LessEqual,
                    C::Greater,
                    C::GreaterEqual,
                ]
                .into_iter()
                .zip(truth)
                {
                    let p = plan(vec![comparison(operator)]);
                    let mut port = Port::with(&[("a", a.clone()), ("b", b.clone())]);
                    assert_eq!(
                        p.evaluate(&mut port).unwrap(),
                        expected,
                        "{operator:?} {a:?} {b:?}"
                    );
                    assert_eq!(port.trace, ["a", "b"]);
                }
            }
            for value in [
                Value::Bool(true),
                Value::Int(1),
                Value::Str("1".into()),
                Value::Date(day),
            ] {
                for (a, b) in [(Value::Missing, value.clone()), (value, Value::Missing)] {
                    assert_eq!(
                        plan(vec![comparison(C::Equal)])
                            .evaluate(&mut Port::with(&[("a", a), ("b", b)]))
                            .unwrap(),
                        Truth::Unknown
                    );
                }
            }
            for (a, b, left, right) in [
                (
                    Value::Bool(true),
                    Value::Bool(true),
                    ValueType::Bool,
                    ValueType::Bool,
                ),
                (
                    Value::Int(1),
                    Value::Str("1".into()),
                    ValueType::Int,
                    ValueType::Str,
                ),
                (
                    Value::Date(day),
                    Value::DateTime(second),
                    ValueType::Date,
                    ValueType::DateTime,
                ),
            ] {
                let error = plan(vec![comparison(C::Equal)])
                    .evaluate(&mut Port::with(&[("a", a), ("b", b)]))
                    .unwrap_err();
                assert_eq!(
                    error.kind,
                    ErrorKind::Condition(Condition::IncompatiblePair { left, right })
                );
                assert!(error.route.is_empty());
            }
        }

        #[test]
        fn eager_boolean_and_membership_failure_order() {
            for (truth, and) in [(false, true), (true, false)] {
                let p = plan(vec![
                    Node::Boolean(truth),
                    comparison(C::Equal),
                    if and { Node::And(0, 1) } else { Node::Or(0, 1) },
                ]);
                let mut port = Port::default();
                let error = p.evaluate(&mut port).unwrap_err();
                assert_eq!(port.trace, ["a"]);
                assert_eq!(error.route, [Route::Right, Route::Left]);
                assert_eq!(error.spec_path, "rows[0].filter");
                assert_eq!(error.expression, "original predicate");
                assert_eq!(
                    error.kind,
                    ErrorKind::Condition(Condition::UnknownField {
                        identifier: "a".into()
                    })
                );
            }
            let p = plan(vec![Node::In {
                value: id("a"),
                items: vec![id("b"), id("c")],
                negated: false,
            }]);
            let mut port = Port::with(&[("a", Value::Int(1)), ("b", Value::Int(1))]);
            assert_eq!(p.evaluate(&mut port).unwrap_err().route, [Route::Item(1)]);
            assert_eq!(port.trace, ["a", "b", "c"]);
            port = Port::with(&[("a", Value::Int(1)), ("b", Value::Str("1".into()))]);
            assert!(matches!(
                p.evaluate(&mut port).unwrap_err().kind,
                ErrorKind::Condition(Condition::IncompatiblePair { .. })
            ));
            assert_eq!(port.trace, ["a", "b"]);
            for negated in [false, true] {
                for (items, expected) in [
                    (vec![lit(Value::Int(1)), lit(Value::Missing)], Truth::True),
                    (
                        vec![lit(Value::Int(2)), lit(Value::Missing)],
                        Truth::Unknown,
                    ),
                    (vec![lit(Value::Int(2)), lit(Value::Int(3))], Truth::False),
                ] {
                    assert_eq!(
                        plan(vec![Node::In {
                            value: lit(Value::Int(1)),
                            items,
                            negated
                        }])
                        .evaluate(&mut Port::default())
                        .unwrap(),
                        if negated { expected.negate() } else { expected }
                    );
                }
            }
        }

        #[test]
        fn between_repeats_subject_before_comparisons() {
            let p = plan(vec![Node::Between {
                value: id("x"),
                lower: id("lo"),
                upper: id("hi"),
                negated: false,
            }]);
            let mut port = Port::with(&[("x", Value::Int(2)), ("lo", Value::Str("bad".into()))]);
            let error = p.evaluate(&mut port).unwrap_err();
            assert_eq!(port.trace, ["x", "lo", "x", "hi"]);
            assert_eq!(error.route, [Route::Upper]);
            assert_eq!(
                error.kind,
                ErrorKind::Condition(Condition::UnknownField {
                    identifier: "hi".into()
                })
            );
            port = Port::with(&[
                ("x", Value::Int(2)),
                ("lo", Value::Int(1)),
                ("hi", Value::Int(3)),
            ]);
            port.values
                .insert("x".into(), [Value::Int(2), Value::Int(4)].into());
            assert_eq!(p.evaluate(&mut port).unwrap(), Truth::False);
            assert_eq!(port.trace, ["x", "lo", "x", "hi"]);
            port = Port::with(&[
                ("x", Value::Missing),
                ("lo", Value::Int(1)),
                ("hi", Value::Int(3)),
            ]);
            assert_eq!(p.evaluate(&mut port).unwrap(), Truth::Unknown);
        }

        /// This deliberately lacks Clone, proving an opaque port failure is moved intact.
        #[derive(Debug, PartialEq, Eq)]
        struct Failure(Box<u8>);
        struct FailingPort;
        impl Resolver for FailingPort {
            type Error = Failure;
            /// Model an owning scope's condition without relabeling it as an absent field.
            fn resolve(&mut self, _: &str) -> Result<Selection, Self::Error> {
                Err(Failure(Box::new(37)))
            }
        }

        #[test]
        fn opaque_resolution_and_condition_metadata() {
            let error = plan(vec![comparison(C::Equal)])
                .evaluate(&mut FailingPort)
                .unwrap_err();
            assert_eq!(
                error.kind,
                ErrorKind::Resolution {
                    identifier: "a".into(),
                    error: Failure(Box::new(37))
                }
            );
            for (condition, name, requirement) in [
                (
                    Condition::UnknownField {
                        identifier: "a".into(),
                    },
                    "unknown_field",
                    "REQ-0189",
                ),
                (
                    Condition::IncompatiblePair {
                        left: ValueType::Bool,
                        right: ValueType::Int,
                    },
                    "incompatible_input_type",
                    "REQ-0190",
                ),
                (
                    Condition::ExpectedText {
                        actual: ValueType::Float,
                    },
                    "incompatible_input_type",
                    "REQ-0190",
                ),
                (Condition::DanglingEscape, "invalid_predicate", "REQ-0191"),
            ] {
                assert_eq!(
                    (
                        condition.phase(),
                        condition.condition(),
                        condition.requirement()
                    ),
                    ("validation", name, requirement)
                );
            }
        }

        /// Construct LIKE over literals without involving a regex library.
        fn like(value: &str, pattern: &str, escape: Option<char>) -> Plan {
            plan(vec![Node::Like {
                value: lit(Value::Str(value.into())),
                pattern: lit(Value::Str(pattern.into())),
                escape,
                negated: false,
            }])
        }

        #[test]
        fn unicode_like_escape_and_missing_precedence() {
            for (value, pattern, escape, expected) in [
                ("", "", None, Truth::True),
                ("", "%", None, Truth::True),
                ("", "_", None, Truth::False),
                ("abc", "a%c", None, Truth::True),
                ("abc", "b", None, Truth::False),
                ("a\nb", "a_b", None, Truth::True),
                ("\u{1f600}", "_", None, Truth::True),
                ("e\u{301}", "_", None, Truth::False),
                ("e\u{301}", "__", None, Truth::True),
                ("100%", "100!%", Some('!'), Truth::True),
                ("a_b", "a\u{1f600}_b", Some('\u{1f600}'), Truth::True),
                ("a\\b", "a\\_", None, Truth::True),
                ("%", "%%", Some('%'), Truth::True),
                ("_", "__", Some('_'), Truth::True),
                ("a\0b", "a_b", None, Truth::True),
            ] {
                assert_eq!(
                    like(value, pattern, escape)
                        .evaluate(&mut Port::default())
                        .unwrap(),
                    expected,
                    "{value:?} {pattern:?}"
                );
            }
            let error = like("no match", "abc!", Some('!'))
                .evaluate(&mut Port::default())
                .unwrap_err();
            assert_eq!(error.kind, ErrorKind::Condition(Condition::DanglingEscape));
            let p = plan(vec![Node::Like {
                value: lit(Value::Missing),
                pattern: lit(Value::Int(1)),
                escape: None,
                negated: true,
            }]);
            assert_eq!(p.evaluate(&mut Port::default()).unwrap(), Truth::Unknown);
            let p = plan(vec![Node::Like {
                value: lit(Value::Str("a".into())),
                pattern: lit(Value::Bool(false)),
                escape: None,
                negated: false,
            }]);
            assert_eq!(
                p.evaluate(&mut Port::default()).unwrap_err().kind,
                ErrorKind::Condition(Condition::ExpectedText {
                    actual: ValueType::Bool
                })
            );
        }

        /// Exhaustive short matching oracle expands percent lengths recursively, not DP.
        fn brute(value: &[char], pattern: &[char]) -> bool {
            match pattern.split_first() {
                None => value.is_empty(),
                Some(('%', rest)) => (0..=value.len()).any(|n| brute(&value[n..], rest)),
                Some(('_', rest)) => !value.is_empty() && brute(&value[1..], rest),
                Some((literal, rest)) => value.first() == Some(literal) && brute(&value[1..], rest),
            }
        }

        /// Enumerate bounded authored alphabets, including empty strings.
        fn words(alphabet: &[char], max: usize) -> Vec<String> {
            let mut result = vec![String::new()];
            let mut layer = vec![String::new()];
            for _ in 0..max {
                layer = layer
                    .iter()
                    .flat_map(|prefix| {
                        alphabet
                            .iter()
                            .map(move |character| format!("{prefix}{character}"))
                    })
                    .collect();
                result.extend(layer.clone());
            }
            result
        }

        #[test]
        fn like_matches_independent_exhaustive_short_oracle() {
            for value in words(&['a', 'b', '\n', '\u{1f600}'], 3) {
                for pattern in words(&['a', 'b', '%', '_'], 3) {
                    let expected = if brute(
                        &value.chars().collect::<Vec<_>>(),
                        &pattern.chars().collect::<Vec<_>>(),
                    ) {
                        Truth::True
                    } else {
                        Truth::False
                    };
                    assert_eq!(
                        like(&value, &pattern, None)
                            .evaluate(&mut Port::default())
                            .unwrap(),
                        expected,
                        "{value:?} {pattern:?}"
                    );
                }
            }
        }

        #[test]
        fn structural_admission_preserves_coordinates_and_fixed_semantic_depth() {
            let admit = |nodes: Vec<Node>| {
                Plan::new(nodes.clone(), nodes.len() - 1, String::new(), String::new())
            };
            assert_eq!(
                admit(vec![Node::Not(0)]).unwrap_err(),
                PlanError::InvalidChild
            );
            assert_eq!(
                admit(vec![Node::In {
                    value: lit(Value::Missing),
                    items: vec![],
                    negated: false
                }])
                .unwrap_err(),
                PlanError::EmptyMembership
            );
            // Shared arena references do not create an artificial expanded-work quota.
            let mut shared = vec![Node::Boolean(true)];
            for i in 0..20 {
                shared.push(Node::And(i, i));
            }
            assert!(admit(shared).is_ok());
            let mut nodes = vec![Node::Boolean(true)];
            for i in 0..127 {
                nodes.push(Node::Not(i));
            }
            let edge = admit(nodes.clone()).unwrap();
            assert_eq!(edge.evaluate(&mut Port::default()).unwrap(), Truth::False);
            nodes.push(Node::Not(127));
            assert_eq!(
                admit(nodes).unwrap_err(),
                PlanError::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "semantic_depth",
                    limit: 128,
                    actual: Some(129),
                })
            );
        }

        #[test]
        fn repeated_like_and_text_comparisons_keep_the_original_truth_and_reads() {
            let p = like("abc", "%", None);
            for _ in 0..2 {
                assert_eq!(p.evaluate(&mut Port::default()).unwrap(), Truth::True);
            }
            let node = Node::Like {
                value: lit(Value::Str("abc".into())),
                pattern: lit(Value::Str("%".into())),
                escape: None,
                negated: false,
            };
            let p = plan(vec![node, Node::Or(0, 0)]);
            assert_eq!(p.evaluate(&mut Port::default()).unwrap(), Truth::True);
            let p = plan(vec![comparison(C::Equal)]);
            let mut port = Port::with(&[
                ("a", Value::Str("a".repeat(40))),
                ("b", Value::Str("a".repeat(40))),
            ]);
            assert_eq!(p.evaluate(&mut port).unwrap(), Truth::True);
            assert_eq!(port.trace, ["a", "b"]);
            assert_eq!(
                p.evaluate(&mut Port::with(&[
                    ("a", Value::Str("a".into())),
                    ("b", Value::Str("a".into()))
                ]))
                .unwrap(),
                Truth::True
            );
        }

        /// Decode exact fixture types independently from the Python parser/evaluator.
        fn fixture_value(token: &str) -> Value {
            if token == "missing" {
                return Value::Missing;
            }
            let (kind, value) = token.split_once(':').unwrap();
            match kind {
                "i" => Value::Int(value.parse().unwrap()),
                "f" => Value::float(value.parse().unwrap()),
                "b" => Value::Bool(value.parse().unwrap()),
                "s" => Value::Str(
                    String::from_utf8(
                        (0..value.len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
                            .collect(),
                    )
                    .unwrap(),
                ),
                _ => panic!("invalid fixture value"),
            }
        }

        /// The fixture's small postfix notation constructs typed IR; it is not a parser
        /// for the language. The Python test separately parses the authored expression.
        fn fixture_tree(text: &str) -> Vec<Node> {
            let mut nodes = Vec::new();
            let mut predicates = Vec::new();
            let mut operands = Vec::new();
            for token in text.split_whitespace() {
                let node = match token {
                    "true" | "false" => Node::Boolean(token == "true"),
                    "not" => Node::Not(predicates.pop().unwrap()),
                    "and" | "or" => {
                        let right = predicates.pop().unwrap();
                        let left = predicates.pop().unwrap();
                        if token == "and" {
                            Node::And(left, right)
                        } else {
                            Node::Or(left, right)
                        }
                    }
                    "eq" => {
                        let right = operands.pop().unwrap();
                        let left = operands.pop().unwrap();
                        Node::Compare {
                            operator: C::Equal,
                            left,
                            right,
                        }
                    }
                    "between" => {
                        let upper = operands.pop().unwrap();
                        let lower = operands.pop().unwrap();
                        let value = operands.pop().unwrap();
                        Node::Between {
                            value,
                            lower,
                            upper,
                            negated: false,
                        }
                    }
                    _ if token.starts_with("in:") => {
                        let count = token[3..].parse::<usize>().unwrap();
                        let items = operands.split_off(operands.len() - count);
                        Node::In {
                            value: operands.pop().unwrap(),
                            items,
                            negated: false,
                        }
                    }
                    _ if token == "like" || token.starts_with("like:") => {
                        let escape = token.strip_prefix("like:").map(|hex| {
                            char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap()
                        });
                        let pattern = operands.pop().unwrap();
                        let value = operands.pop().unwrap();
                        Node::Like {
                            value,
                            pattern,
                            escape,
                            negated: false,
                        }
                    }
                    _ => {
                        operands.push(if let Some(name) = token.strip_prefix('$') {
                            id(name)
                        } else {
                            lit(fixture_value(token))
                        });
                        continue;
                    }
                };
                predicates.push(nodes.len());
                nodes.push(node);
            }
            assert!(operands.is_empty());
            assert_eq!(predicates, [nodes.len() - 1]);
            nodes
        }

        #[test]
        fn shared_reference_truth_and_occurrence_traces() {
            for line in include_str!("fixtures/predicate_evaluation.tsv")
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
            {
                let fields: Vec<_> = line.split('\t').collect();
                assert_eq!(fields.len(), 6);
                let nodes = fixture_tree(fields[2]);
                let p = Plan::new(
                    nodes.clone(),
                    nodes.len() - 1,
                    "rows[0].filter".into(),
                    fields[1].into(),
                )
                .unwrap();
                let compiled =
                    yamaa_core::predicate_compiler::compile(fields[1], "rows[0].filter").unwrap();
                // Both independently authored typed IR and original syntax must satisfy
                // the same truth and read trace; neither supplies the other's oracle.
                for p in [p, compiled] {
                    let mut port = Port::default();
                    if fields[3] != "-" {
                        for binding in fields[3].split(';') {
                            let (name, value) = binding.split_once('=').unwrap();
                            port.values
                                .insert(name.into(), [fixture_value(value)].into());
                        }
                    }
                    let result = p.evaluate(&mut port);
                    let actual = match &result {
                        Ok(Truth::True) => "TRUE",
                        Ok(Truth::False) => "FALSE",
                        Ok(Truth::Unknown) => "UNKNOWN",
                        Err(error) => match &error.kind {
                            ErrorKind::Condition(condition) => condition.condition(),
                            _ => panic!("unexpected failure {error:?}"),
                        },
                    };
                    assert_eq!(actual, fields[4], "{}", fields[0]);
                    assert_eq!(port.trace.join(","), fields[5], "{}", fields[0]);
                }
            }
        }

        #[test]
        fn repeated_plans_resolve_each_written_occurrence() {
            let p = plan(vec![comparison(C::Equal)]);
            let mut port =
                Port::with(&[("a", Value::Str("x".into())), ("b", Value::Str("x".into()))]);
            for _ in 0..2 {
                assert_eq!(p.evaluate(&mut port).unwrap(), Truth::True);
            }
            assert_eq!(port.trace, ["a", "b", "a", "b"]);
        }

        struct BorrowingPort {
            text: String,
            calls: usize,
        }
        impl Resolver for BorrowingPort {
            type Error = Infallible;
            /// Any owning call would bypass the table boundary that this regression proves.
            fn resolve(&mut self, _: &str) -> Result<Selection, Self::Error> {
                panic!("owning fallback was called")
            }
            /// Lend each written operand without invoking the owning fallback.
            fn resolve_value(
                &mut self,
                _: &str,
            ) -> Result<yamaa_core::predicate::Resolved<'_>, Self::Error> {
                self.calls += 1;
                Ok(yamaa_core::predicate::Resolved::Borrowed(
                    yamaa_core::table::ValueRef::Str(&self.text),
                ))
            }
        }

        #[test]
        fn borrowed_text_resolves_each_operand_without_owning_fallback() {
            let p = plan(vec![comparison(C::Equal)]);
            let mut port = BorrowingPort {
                text: "large".repeat(100),
                calls: 0,
            };
            assert_eq!(p.evaluate(&mut port).unwrap(), Truth::True);
            assert_eq!(port.calls, 2);
            assert_eq!(p.identifiers(), ["a", "b"]);
        }
    }
    mod predicate_contains {
        //! Independent REQ-1244 truth, error ordering and the fixed per-match boundary.
        use yamaa_core::{
            predicate::{
                Condition, ErrorKind, Node, Plan, PlanError, Resolver, Route, Scalar, Truth,
            },
            regex::CompileError,
            value::{Selection, Value, ValueType},
        };

        struct Port {
            value: Value,
            reads: Vec<String>,
            failure: bool,
        }
        impl Resolver for Port {
            type Error = &'static str;
            /// Retain occurrence order, including eager reads after a decisive truth value.
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.reads.push(name.into());
                if self.failure {
                    Err("opaque failure")
                } else {
                    Ok(Selection::Present(self.value.clone()))
                }
            }
        }
        /// A single observable source occurrence and an independently authored pattern.
        fn contains(pattern: &str) -> Node {
            Node::Contains {
                value: Scalar::Identifier("s".into()),
                pattern: pattern.into(),
            }
        }
        /// Admit the entire arena before constructing or invoking any resolver.
        fn plan(nodes: Vec<Node>) -> Plan {
            let root = nodes.len() - 1;
            Plan::new(
                nodes,
                root,
                "rows[0].filter".into(),
                "str_contains(s, 'a')".into(),
            )
            .unwrap()
        }
        /// Each execution receives its own observable source port.
        fn port(value: Value) -> Port {
            Port {
                value,
                reads: vec![],
                failure: false,
            }
        }

        #[test]
        fn contains_search_truth_missing_negation_and_type_requirement() {
            for (pattern, subject, expected) in [
                ("a", "cat", Truth::True),
                ("^a$", "cat", Truth::False),
                ("(?<letter>a)\\k<letter>", "baac", Truth::True),
                ("(?<=a)b", "ab", Truth::True),
                ("^.$", "😀", Truth::True),
                ("z", "cat", Truth::False),
                ("", "", Truth::True),
            ] {
                let p = plan(vec![contains(pattern)]);
                let mut source = port(Value::Str(subject.into()));
                assert_eq!(
                    p.evaluate(&mut source).unwrap(),
                    expected,
                    "{pattern}: {subject}"
                );
                assert_eq!(source.reads, ["s"]);
                assert_eq!(p.identifiers(), ["s"]);
            }
            for negated in [false, true] {
                let mut nodes = vec![contains("a")];
                if negated {
                    nodes.push(Node::Not(0));
                }
                let p = plan(nodes);
                assert_eq!(
                    p.evaluate(&mut port(Value::Missing)).unwrap(),
                    Truth::Unknown
                );
                for (value, actual) in [
                    (Value::Int(1), ValueType::Int),
                    (Value::Bool(false), ValueType::Bool),
                ] {
                    let error = p.evaluate(&mut port(value)).unwrap_err();
                    let condition = Condition::ContainsExpectedText { actual };
                    assert_eq!(error.kind, ErrorKind::Condition(condition.clone()));
                    assert_eq!(condition.condition(), "incompatible_input_type");
                    assert_eq!(condition.requirement(), "REQ-1244");
                    assert_eq!(error.spec_path, "rows[0].filter");
                    assert_eq!(
                        error.route,
                        if negated { vec![Route::Child] } else { vec![] }
                    );
                }
            }
        }

        #[test]
        fn contains_admits_every_literal_including_unreachable_nodes() {
            let error = Plan::new(
                vec![contains("["), Node::Boolean(false)],
                1,
                "p".into(),
                "text".into(),
            )
            .unwrap_err();
            assert!(matches!(
                error,
                PlanError::Regex {
                    node: 0,
                    error: CompileError::Invalid { .. }
                }
            ));
            let pattern = format!("(a){}(?<=\\1)", "(a|aa)".repeat(8));
            plan(vec![contains(&pattern)]);
            Plan::new(
                vec![contains(&pattern); 512],
                511,
                "p".into(),
                "text".into(),
            )
            .unwrap();
        }

        #[test]
        fn contains_eager_order_and_original_failure_precede_matching() {
            for (first, combine) in [(false, Node::And(0, 1)), (true, Node::Or(0, 1))] {
                let p = plan(vec![Node::Boolean(first), contains("a"), combine]);
                let mut source = port(Value::Missing);
                source.failure = true;
                let error = p.evaluate(&mut source).unwrap_err();
                assert_eq!(source.reads, ["s"]);
                assert_eq!(error.route, [Route::Right, Route::Value]);
                assert_eq!(
                    error.kind,
                    ErrorKind::Resolution {
                        identifier: "s".into(),
                        error: "opaque failure"
                    }
                );
            }
            let p = plan(vec![contains("(a+)+$")]);
            assert_eq!(
                p.evaluate(&mut port(Value::Missing)).unwrap(),
                Truth::Unknown
            );
            assert_eq!(
                p.evaluate(&mut port(Value::Int(0))).unwrap_err().kind,
                ErrorKind::Condition(Condition::ContainsExpectedText {
                    actual: ValueType::Int
                })
            );
        }

        #[test]
        fn contains_enforces_the_fixed_per_match_boundary_and_keeps_small_matches_independent() {
            let single = plan(vec![contains("a+")]);
            for _ in 0..2 {
                let mut source = port(Value::Str("aaaa".into()));
                assert_eq!(single.evaluate(&mut source).unwrap(), Truth::True);
                assert_eq!(source.reads, ["s"]);
            }
            let repeated = plan(vec![contains("a+"), contains("a+"), Node::And(0, 1)]);
            let mut source = port(Value::Str("aaaa".into()));
            assert_eq!(repeated.evaluate(&mut source).unwrap(), Truth::True);
            assert_eq!(source.reads, ["s", "s"]);
            let bounded = plan(vec![contains("a+"), contains("(a+)+$"), Node::And(0, 1)]);
            for _ in 0..2 {
                let mut source = port(Value::Str("a".repeat(30) + "!"));
                let error = bounded.evaluate(&mut source).unwrap_err();
                assert_eq!(error.route, [Route::Right]);
                assert_eq!(
                    error.kind,
                    ErrorKind::Regex(yamaa_core::regex::MatchError {
                        resource: yamaa_core::regex::Resource::Work,
                        limit: 1_000_000,
                    })
                );
                assert_eq!(source.reads, ["s", "s"]);
            }
        }
    }
    mod reduction {
        use yamaa_core::{
            numeric::{ArithmeticErrorKind, Number},
            reduction::{
                count_records, count_values, reduce_numeric, NumericReducer, ReductionError,
            },
            table::ValueRef,
            value::Value,
        };

        /// Missing relations, missing values, empty text and false remain distinct.
        #[test]
        fn counts_are_exact_cardinalities_without_type_coercion() {
            assert_eq!(count_records(0), Ok(Number::Missing));
            assert_eq!(count_values(&[]), Ok(Number::Missing));
            assert_eq!(count_records(3), Ok(Number::Int(3)));
            assert_eq!(count_values(&[ValueRef::Missing; 3]), Ok(Number::Int(0)));
            let values = [
                Value::Missing,
                Value::Str("".into()),
                Value::Bool(false),
                Value::Int(i64::MIN),
                Value::float(-0.0),
                Value::Missing,
            ];
            assert_eq!(
                count_values(&values.iter().map(ValueRef::from).collect::<Vec<_>>()),
                Ok(Number::Int(4))
            );
        }

        /// Cardinality conversion is checked independently of allocating or visiting rows.
        #[test]
        fn record_count_capacity_does_not_wrap_into_a_negative_value() {
            if let Ok(maximum) = usize::try_from(i64::MAX) {
                assert_eq!(count_records(maximum), Ok(Number::Int(i64::MAX)));
                assert_eq!(
                    count_records(maximum + 1),
                    Err(ReductionError::CountOverflow)
                );
            }
        }

        /// Decode hand-written fixture inputs without consulting either evaluator.
        fn value(token: &str) -> Value {
            if token == "missing" {
                return Value::Missing;
            }
            let (kind, value) = token.split_once(':').unwrap();
            match kind {
                "int" => Value::Int(value.parse().unwrap()),
                "float" => Value::float(value.parse().unwrap()),
                "str" => Value::Str(value.into()),
                "bool" => Value::Bool(value.parse().unwrap()),
                _ => panic!("unknown value {token}"),
            }
        }

        #[test]
        /// Pin all shared values, exact float bits and portable failure facts.
        fn independent_ordered_reduction_truth_shared_with_python() {
            let mut count = 0;
            for row in include_str!("fixtures/reduction.tsv").lines().skip(1) {
                let columns: Vec<_> = row.split('\t').collect();
                assert_eq!(columns.len(), 4);
                let (id, name, tokens, expected) = (columns[0], columns[1], columns[2], columns[3]);
                let reducer = match name {
                    "SUM" => NumericReducer::Sum,
                    "MEAN" => NumericReducer::Mean,
                    _ => panic!("bad reducer"),
                };
                assert_eq!(reducer.name(), name);
                let values: Vec<_> = if tokens == "-" {
                    vec![]
                } else {
                    tokens.split(',').map(value).collect()
                };
                let borrowed: Vec<_> = values.iter().map(ValueRef::from).collect();
                let expression = format!("{name}(A)");
                let actual = match reduce_numeric(&borrowed, reducer, &expression) {
                    Ok(Number::Missing) => "missing".into(),
                    Ok(Number::Int(value)) => format!("int:{value}"),
                    Ok(Number::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
                    Err(ReductionError::IncompatibleInputType { argument, actual }) => {
                        assert_eq!(borrowed[argument].value_type(), Some(actual));
                        format!(
                            "error:incompatible_input_type:REQ-0510:{}",
                            format!("{actual:?}").to_lowercase()
                        )
                    }
                    Err(ReductionError::Arithmetic { argument, error }) => {
                        assert_eq!(argument, 1, "{id}");
                        assert_eq!(error.expression, expression);
                        assert_eq!(error.phase(), "derivation");
                        let ArithmeticErrorKind::IntegerOverflow { value } = error.kind else {
                            panic!("unexpected failure")
                        };
                        format!(
                            "error:{}:{}:{value}",
                            error.condition(),
                            error.requirement()
                        )
                    }
                    other => panic!("unexpected result: {other:?}"),
                };
                assert_eq!(actual, expected, "{id}");
                count += 1;
            }
            assert_eq!(count, 32);
        }

        #[test]
        /// Reject each nonnumeric type at its original position including missing slots.
        fn nonnumeric_arguments_keep_original_index_and_type() {
            use yamaa_core::temporal::{Date, DateTime};
            let date: Date = "2025-01-01".parse().unwrap();
            let datetime: DateTime = "2025-01-01T00:00:00".parse().unwrap();
            for value in [
                ValueRef::Str(""),
                ValueRef::Bool(false),
                ValueRef::Date(date),
                ValueRef::DateTime(datetime),
            ] {
                for reducer in [NumericReducer::Sum, NumericReducer::Mean] {
                    assert_eq!(
                        reduce_numeric(
                            &[ValueRef::Missing, ValueRef::Int(1), value],
                            reducer,
                            "reduce(A)"
                        ),
                        Err(ReductionError::IncompatibleInputType {
                            argument: 2,
                            actual: value.value_type().unwrap()
                        })
                    );
                }
            }
        }
    }
    mod rounding {
        use yamaa_core::evaluation::{
            EvaluationErrorKind, NumericCondition, NumericResolver, Operand,
        };
        use yamaa_core::numeric::{
            round_half_away_from_zero as rounded, ArithmeticErrorKind, Number,
        };
        use yamaa_core::numeric_compiler::compile_numeric;
        use yamaa_core::value::{Selection, Value};

        /// Missing suppresses digits validation only after both operand values are available.
        #[test]
        fn rounding_missing_type_and_range_contracts() {
            assert_eq!(
                rounded(Number::Missing, Number::float(1.0), "expr"),
                Ok(Number::Missing)
            );
            assert_eq!(
                rounded(Number::Int(1), Number::Missing, "expr"),
                Ok(Number::Missing)
            );
            let error = rounded(Number::Int(1), Number::float(1.0), "expr").unwrap_err();
            assert_eq!(error.kind, ArithmeticErrorKind::InvalidRoundingDigits);
            assert_eq!(error.expression, "expr");
            assert_eq!(error.phase(), "validation");
            assert_eq!(error.condition(), "incompatible_input_type");
            assert_eq!(error.requirement(), "REQ-0418");
            for input in [f64::MAX, -f64::MAX] {
                assert_eq!(
                    rounded(Number::float(input), Number::Int(-308), "expr"),
                    Ok(Number::Missing)
                );
            }
            for input in [
                -0.0,
                0.0,
                f64::from_bits(1),
                -f64::from_bits(1),
                f64::MAX,
                -f64::MAX,
            ] {
                for (digits, expected) in [
                    (i64::MIN, 0.0),
                    (i64::MAX, if input == 0.0 { 0.0 } else { input }),
                ] {
                    let Number::Float(result) =
                        rounded(Number::float(input), Number::Int(digits), "expr").unwrap()
                    else {
                        panic!("float required")
                    };
                    assert_eq!(result.get().to_bits(), expected.to_bits());
                }
            }
        }

        /// A non-clone resolver failure must survive rounding without being replaced by missing.
        #[derive(Debug)]
        struct Opaque(String);

        struct Resolver(Vec<String>);
        impl NumericResolver for Resolver {
            type Error = Opaque;
            /// Resolve numeric/missing values and a deliberate callback failure in written order.
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.0.push(name.into());
                match name {
                    "BROKEN" => Err(Opaque("opaque failure".into())),
                    "A" => Ok(Selection::Present(Value::Int(1))),
                    "D" => Ok(Selection::Present(Value::float(1.0))),
                    _ => Ok(Selection::Absent),
                }
            }
        }

        /// Compilation retains call/operand source spans and eager resolver failures.
        #[test]
        fn rounding_compilation_preserves_provenance_and_occurrences() {
            for (text, span, path, expected_trace) in [
                (
                    "1 + ROUND_HALF_AWAY_FROM_ZERO(A, D)",
                    "ROUND_HALF_AWAY_FROM_ZERO(A, D)",
                    vec![Operand::Right],
                    vec!["A", "D"],
                ),
                (
                    "ROUND_HALF_AWAY_FROM_ZERO(NULL, BROKEN)",
                    "BROKEN",
                    vec![Operand::Right],
                    vec!["BROKEN"],
                ),
            ] {
                let plan = compile_numeric(text, "spec").unwrap();
                let mut resolver = Resolver(Vec::new());
                let error = plan.evaluate(&mut resolver).unwrap_err();
                assert_eq!(&text[error.source_span.start..error.source_span.end], span);
                assert_eq!(error.evaluation.location.spec_path, "spec");
                assert_eq!(error.evaluation.location.operands, path);
                assert_eq!(resolver.0, expected_trace);
                match error.evaluation.kind {
                    EvaluationErrorKind::Numeric(NumericCondition::Arithmetic(
                        ArithmeticErrorKind::InvalidRoundingDigits,
                    )) => assert!(text.starts_with("1 +")),
                    EvaluationErrorKind::Resolution { identifier, error } => {
                        assert_eq!(identifier, "BROKEN");
                        assert_eq!(error.0, "opaque failure");
                    }
                    _ => panic!("unexpected error"),
                }
            }
        }
    }
    mod scalar_language {
        use yamaa_core::{
            scalar_language::replace_extreme,
            temporal::{Date, DatePrecision},
            value::Value,
        };
        #[test]
        fn ties_preserve_first_type_and_precision_and_integer_comparison_is_exact() {
            for largest in [true, false] {
                assert!(!replace_extreme(&Value::Int(1), &Value::float(1.0), largest).unwrap());
                assert!(!replace_extreme(&Value::float(1.0), &Value::Int(1), largest).unwrap());
                let month = Value::Date(Date::new(2025, 1, 1, DatePrecision::Month).unwrap());
                let day = Value::Date(Date::new(2025, 1, 1, DatePrecision::Day).unwrap());
                assert!(!replace_extreme(&month, &day, largest).unwrap());
                assert!(!replace_extreme(&day, &month, largest).unwrap());
            }
            assert!(replace_extreme(
                &Value::float(9_007_199_254_740_992.0),
                &Value::Int(9_007_199_254_740_993),
                true
            )
            .unwrap());
            assert!(replace_extreme(
                &Value::Int(i64::MAX),
                &Value::float(9_223_372_036_854_775_808.0),
                true
            )
            .unwrap());
            assert!(!replace_extreme(
                &Value::Int(i64::MIN),
                &Value::float(-9_223_372_036_854_775_808.0),
                false
            )
            .unwrap());
            assert!(!replace_extreme(&Value::Missing, &Value::Missing, true).unwrap());
            assert!(replace_extreme(&Value::Missing, &Value::Int(2), true).unwrap());
            assert!(replace_extreme(&Value::Int(1), &Value::Str("2".into()), true).is_err());
        }
    }
}
