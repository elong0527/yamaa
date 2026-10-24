//! Scalar contracts: aggregate reduction, arithmetic, conversion, evaluation,
//! integral, predicate, predicate contains, reduction and scalar language.
mod scalar {
    mod aggregate_reduction {
        use yamaa_core::{
            aggregate_reduction::{reduce_values, AggregateReductionError, ReductionFields},
            expression::Reducer,
            table::ValueRef,
            temporal::{Date, DateTime, Precision},
            value::{FiniteFloat, Value, ValueType},
        };

        const PLAIN: ReductionFields<'static> = ReductionFields {
            separator: "",
            distinct: false,
            quantile: 0.0,
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
                reduce_values(&borrowed, Reducer::Max, PLAIN, "MAX(D.V)"),
                Ok(Value::Int(9_007_199_254_740_993))
            );
            assert_eq!(
                reduce_values(&borrowed, Reducer::Min, PLAIN, "MIN(D.V)"),
                Ok(values[2].clone())
            );
            let text = [
                ValueRef::Str("é"),
                ValueRef::Str("😀"),
                ValueRef::Missing,
                ValueRef::Str(""),
            ];
            assert_eq!(
                reduce_values(&text, Reducer::Min, PLAIN, "MIN(D.V)"),
                Ok(Value::Str("".into()))
            );
            assert_eq!(
                reduce_values(&text, Reducer::Max, PLAIN, "MAX(D.V)"),
                Ok(Value::Str("😀".into()))
            );
            let equal = [Value::Int(2), Value::float(2.0)];
            for reducer in [Reducer::Min, Reducer::Max] {
                assert_eq!(
                    reduce_values(
                        &equal.iter().map(ValueRef::from).collect::<Vec<_>>(),
                        reducer,
                        PLAIN,
                        "D.V"
                    ),
                    Ok(Value::Int(2))
                );
                assert_eq!(
                    reduce_values(&[], reducer, PLAIN, "D.V"),
                    Ok(Value::Missing)
                );
                assert_eq!(
                    reduce_values(&[ValueRef::Missing; 2], reducer, PLAIN, "D.V"),
                    Ok(Value::Missing)
                );
            }
            assert_eq!(
                reduce_values(
                    &[ValueRef::Int(1), ValueRef::Str("1")],
                    Reducer::Min,
                    PLAIN,
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
                reduce_values(&[], Reducer::Only, PLAIN, "ONLY(D.V)"),
                Ok(Value::Missing)
            );
            assert_eq!(
                reduce_values(&[ValueRef::Missing], Reducer::Only, PLAIN, "ONLY(D.V)"),
                Ok(Value::Missing)
            );
            assert_eq!(
                reduce_values(&[ValueRef::Bool(false)], Reducer::Only, PLAIN, "ONLY(D.V)"),
                Ok(Value::Bool(false))
            );
            assert_eq!(
                reduce_values(&[ValueRef::Missing; 2], Reducer::Only, PLAIN, "ONLY(D.V)"),
                Err(AggregateReductionError::MultipleRecords { count: 2 })
            );
            assert_eq!(
                reduce_values(&[], Reducer::Count, PLAIN, "COUNT(D.V)"),
                Ok(Value::Missing)
            );
            assert_eq!(
                reduce_values(&[ValueRef::Missing; 2], Reducer::Count, PLAIN, "COUNT(D.V)"),
                Ok(Value::Int(0))
            );
        }

        fn date(text: &str) -> ValueRef<'static> {
            ValueRef::Date(text.parse().unwrap())
        }

        fn float(value: f64) -> ValueRef<'static> {
            ValueRef::Float(FiniteFloat::new(value).unwrap())
        }

        /// FORMULA-18: each present value as the text a `str` column converts
        /// it to (VALUE-3, VALUE-8, VALUE-17), in the order given, the
        /// separator between two of them, each text once with `distinct`, and
        /// missing for a group with no present value (FORMULA-24).
        #[test]
        fn str_join_writes_present_values_as_column_text_in_the_given_order() {
            let datetime: DateTime = "2025-03-01T08:00:00".parse().unwrap();
            let cases: [(&str, Vec<ValueRef<'_>>, &str, bool, Value); 12] = [
                (
                    "ints keep the order given, not numeric order",
                    vec![ValueRef::Int(10), ValueRef::Int(2)],
                    ", ",
                    false,
                    Value::Str("10, 2".into()),
                ),
                (
                    "a float is its shortest text and an integral one drops .0",
                    vec![float(1.5), float(2.0), float(0.1)],
                    ", ",
                    false,
                    Value::Str("1.5, 2, 0.1".into()),
                ),
                (
                    "dates and datetimes are their canonical text",
                    vec![date("2025-01-08"), ValueRef::DateTime(datetime)],
                    "; ",
                    false,
                    Value::Str("2025-01-08; 2025-03-01T08:00:00".into()),
                ),
                (
                    "missing values are skipped",
                    vec![ValueRef::Str("a"), ValueRef::Missing, ValueRef::Str("b")],
                    "|",
                    false,
                    Value::Str("a|b".into()),
                ),
                (
                    "the separator may be empty",
                    vec![ValueRef::Str("A"), ValueRef::Str("B")],
                    "",
                    false,
                    Value::Str("AB".into()),
                ),
                (
                    "empty text is a present value",
                    vec![ValueRef::Str(""), ValueRef::Str("x")],
                    ", ",
                    false,
                    Value::Str(", x".into()),
                ),
                (
                    "one value is written alone",
                    vec![ValueRef::Missing, ValueRef::Int(3)],
                    ", ",
                    false,
                    Value::Str("3".into()),
                ),
                (
                    "every value missing joins to missing, never empty text",
                    vec![ValueRef::Missing, ValueRef::Missing],
                    ", ",
                    false,
                    Value::Missing,
                ),
                (
                    "no record joins to missing",
                    vec![],
                    ", ",
                    true,
                    Value::Missing,
                ),
                (
                    "distinct keeps each text where it first occurs",
                    vec![
                        ValueRef::Str("HEADACHE"),
                        ValueRef::Str("HEADACHE"),
                        ValueRef::Str("FEVER"),
                        ValueRef::Str("HEADACHE"),
                    ],
                    ", ",
                    true,
                    Value::Str("HEADACHE, FEVER".into()),
                ),
                (
                    "distinct compares the written text, so 2 and 2.0 are one",
                    vec![ValueRef::Int(2), float(2.0), ValueRef::Int(3)],
                    ", ",
                    true,
                    Value::Str("2, 3".into()),
                ),
                (
                    "distinct text is case-sensitive",
                    vec![ValueRef::Str("a"), ValueRef::Str("A"), ValueRef::Str("a")],
                    ", ",
                    true,
                    Value::Str("a, A".into()),
                ),
            ];
            for (label, values, separator, distinct, expected) in cases {
                assert_eq!(
                    reduce_values(
                        &values,
                        Reducer::StrJoin,
                        ReductionFields {
                            separator,
                            distinct,
                            ..PLAIN
                        },
                        "STR_JOIN(D.V)"
                    ),
                    Ok(expected),
                    "{label}"
                );
            }
        }

        /// FORMULA-18 and FORMULA-24: `COUNT(x)` with `distinct` counts the
        /// different present values, text scalar for scalar (VALUE-12),
        /// numbers by exact value (VALUE-19), dates on the day they name
        /// (VALUE-21); zero when every value is missing, missing for no record.
        #[test]
        fn count_with_distinct_counts_each_different_present_value_once() {
            let month = Date::new(2025, 1, 1, Precision::Month).unwrap();
            let day = Date::new(2025, 1, 1, Precision::Day).unwrap();
            let cases: [(&str, Vec<ValueRef<'_>>, Value); 7] = [
                (
                    "repeated text counts once and missing is skipped",
                    vec![
                        ValueRef::Str("a"),
                        ValueRef::Missing,
                        ValueRef::Str("b"),
                        ValueRef::Str("a"),
                    ],
                    Value::Int(2),
                ),
                (
                    "text is case-sensitive",
                    vec![ValueRef::Str("a"), ValueRef::Str("A")],
                    Value::Int(2),
                ),
                (
                    "an int and an equal float are one value",
                    vec![ValueRef::Int(2), float(2.0), ValueRef::Int(3)],
                    Value::Int(2),
                ),
                (
                    "an int and the nearest float of another value differ",
                    vec![
                        ValueRef::Int(9_007_199_254_740_993),
                        float(9_007_199_254_740_992.0),
                    ],
                    Value::Int(2),
                ),
                (
                    "two dates naming one day are one value whatever their precision",
                    vec![ValueRef::Date(month), ValueRef::Date(day)],
                    Value::Int(1),
                ),
                (
                    "every value missing counts zero",
                    vec![ValueRef::Missing, ValueRef::Missing],
                    Value::Int(0),
                ),
                ("no record is missing", vec![], Value::Missing),
            ];
            let fields = ReductionFields {
                distinct: true,
                ..PLAIN
            };
            for (label, values, expected) in cases {
                assert_eq!(
                    reduce_values(&values, Reducer::Count, fields, "COUNT(D.V)"),
                    Ok(expected),
                    "{label}"
                );
            }
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
                        let mut text = format!("error:{}:{}", error.condition(), error.rule());
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
        use yamaa_core::temporal::{Date, DateTime, Precision};
        use yamaa_core::value::{ColumnType, FiniteFloat, Value, ValueType};

        /// Apply the grammar's number production without inheriting Python's configurable digit cap.
        #[test]
        fn long_integer_text_preserves_value_or_exact_range_diagnostic() {
            let leading_zeros = Value::Str("0".repeat(5000) + "1");
            assert_eq!(convert(&leading_zeros, ColumnType::Int), Ok(Value::Int(1)));
            let integer = "9".repeat(5000);
            let error = convert(&Value::Str(integer.clone()), ColumnType::Int).unwrap_err();
            assert_eq!(error.rule(), "VALUE-7");
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
                        format!("error:{}:{context}", error.rule())
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
            let date = Date::new(2025, 1, 2, Precision::Year).unwrap();
            let Value::Date(identity) = convert(&Value::Date(date), ColumnType::Date).unwrap()
            else {
                panic!("date");
            };
            assert_eq!(identity.collected_precision(), Precision::Year);
            let text = convert(&Value::Date(date), ColumnType::Str).unwrap();
            let Value::Date(parsed) = convert(&text, ColumnType::Date).unwrap() else {
                panic!("date");
            };
            assert_eq!(parsed.collected_precision(), Precision::Day);
            let datetime = DateTime::new(date, 0, 0, 0, Precision::Day).unwrap();
            let Value::DateTime(identity) =
                convert(&Value::DateTime(datetime), ColumnType::DateTime).unwrap()
            else {
                panic!("datetime");
            };
            assert_eq!(identity.collected_precision(), Precision::Day);
            let text = convert(&Value::DateTime(datetime), ColumnType::Str).unwrap();
            let Value::DateTime(parsed) = convert(&text, ColumnType::DateTime).unwrap() else {
                panic!("datetime");
            };
            assert_eq!(parsed.collected_precision(), Precision::Second);
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
                    assert_eq!(error.rule(), "VALUE-3", "{text}");
                }
            }
        }
    }
    mod evaluation {
        use std::collections::BTreeMap;
        use std::convert::Infallible;

        use yamaa_core::expression::{
            compile, CompileError, Condition, Context, ErrorKind, EvaluationError, Failure,
            Resolver as NumericResolver, Step,
        };
        use yamaa_core::numeric::{ArithmeticErrorKind, Number};
        use yamaa_core::value::{Selection, Value, ValueType};

        const SPEC_PATH: &str = "columns.A.derivation";

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
        fn encoded_condition(condition: &Condition) -> String {
            let mut result = format!("error:{}:{}", condition.condition(), condition.rule());
            match condition {
                Condition::NumericUnknownField { identifier } => {
                    assert_eq!(condition.phase(), "validation");
                    result.push_str(&format!(":{identifier}"));
                }
                Condition::NumericInput { identifier, actual } => {
                    assert_eq!(condition.phase(), "validation");
                    let actual = match actual {
                        ValueType::Str => "str",
                        ValueType::Bool => "bool",
                        _ => panic!("unexpected fixture type"),
                    };
                    result.push_str(&format!(":{identifier}:{actual}"));
                }
                Condition::Arithmetic(kind) => {
                    assert_eq!(condition.phase(), "derivation");
                    if let ArithmeticErrorKind::IntegerOverflow { value } = kind {
                        result.push_str(&format!(":{value}"));
                    }
                }
                other => panic!("a formula raises no {other:?}"),
            }
            result
        }

        /// Encode a refusal found before any value is read.
        fn encoded_refusal(failure: &Failure) -> String {
            format!("error:{}:{}", failure.condition(), failure.rule())
        }

        fn route_text(route: &[Step]) -> String {
            route
                .iter()
                .map(|step| match step {
                    Step::Unary => "unary",
                    Step::LeftOperand => "left",
                    Step::RightOperand => "right",
                    other => panic!("a formula takes no {other:?} step"),
                })
                .collect::<Vec<_>>()
                .join(",")
        }

        /// Evaluate one written formula and check the failed node's exact extent.
        fn fixture_evaluate(
            expression: &str,
            resolver: &mut RecordingResolver,
        ) -> Result<Result<Number, Box<EvaluationError<Infallible>>>, Failure> {
            match compile(expression, Context::Derivation, SPEC_PATH) {
                Ok(plan) => Ok(plan.evaluate_number(resolver).inspect_err(|failure| {
                    let span = failure.source_span.expect("a formula failure has a span");
                    assert!(span.start < span.end);
                    assert!(expression.get(span.start..span.end).is_some());
                })),
                Err(CompileError::Invalid { failure, .. }) => Err(failure),
                Err(error) => panic!("fixture exceeds resource policy: {error:?}"),
            }
        }

        /// Encode one outcome and its structural route.
        fn outcome(
            expression: &str,
            result: Result<Result<Number, Box<EvaluationError<Infallible>>>, Failure>,
        ) -> (String, String) {
            match result {
                Ok(Ok(number)) => (encoded(number), String::new()),
                Ok(Err(error)) => {
                    let error = *error;
                    assert_eq!(error.spec_path, SPEC_PATH);
                    assert_eq!(error.expression, expression);
                    let ErrorKind::Condition(condition) = error.kind;
                    (encoded_condition(&condition), route_text(&error.route))
                }
                Err(failure) => (encoded_refusal(&failure), String::new()),
            }
        }

        /// Replay independent results, call traces and structural failure paths.
        #[test]
        fn shared_evaluation_vectors() {
            for row in include_str!("fixtures/evaluation.tsv").lines().skip(1) {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 7);
                let mut resolver = RecordingResolver::new(fields[3]);
                let (actual, route) =
                    outcome(fields[1], fixture_evaluate(fields[1], &mut resolver));
                assert_eq!(actual, fields[4], "{}", fields[0]);
                assert_eq!(
                    resolver.trace.join(","),
                    fields[5].trim_matches('-'),
                    "{}",
                    fields[0]
                );
                assert_eq!(route, fields[6].trim_matches('-'), "{}", fields[0]);
            }
        }

        /// Exercise the evaluator against every independently specified primitive case.
        #[test]
        fn shared_arithmetic_vectors_through_typed_evaluation() {
            for row in include_str!("fixtures/arithmetic.tsv").lines().skip(1) {
                let fields: Vec<_> = row.split('\t').collect();
                let mut resolver =
                    RecordingResolver::new(&format!("L={};R={}", fields[3], fields[4]));
                let unary = matches!(fields[2], "plus" | "negate" | "abs");
                let (actual, route) =
                    outcome(fields[1], fixture_evaluate(fields[1], &mut resolver));
                assert_eq!(actual, fields[5], "{}", fields[0]);
                assert_eq!(route, "", "{}", fields[0]);
                assert_eq!(resolver.trace.join(","), if unary { "L" } else { "L,R" });
            }
        }

        /// Cover every nonnumeric runtime type without silently converting a numeric string.
        #[test]
        fn identifiers_reject_every_nonnumeric_type() {
            let plan = compile("+A", Context::Derivation, SPEC_PATH).unwrap();
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
                let failure = plan.evaluate_number(&mut resolver).unwrap_err();
                assert_eq!(failure.route, [Step::Unary]);
                assert_eq!(
                    failure.kind,
                    ErrorKind::Condition(Condition::NumericInput {
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
            let failure = compile("L + (A * B)", Context::Derivation, SPEC_PATH)
                .unwrap()
                .evaluate_number(&mut resolver)
                .unwrap_err();
            assert_eq!(resolver.trace, ["L", "A"]);
            assert_eq!(failure.spec_path, SPEC_PATH);
            assert_eq!(failure.expression, "L + (A * B)");
            assert_eq!(failure.route, [Step::RightOperand, Step::LeftOperand]);
            assert_eq!(
                failure.kind,
                ErrorKind::Resolution {
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
            let plan = compile("A + A", Context::Derivation, SPEC_PATH).unwrap();
            let mut resolver = CountingResolver(0);
            assert_eq!(plan.evaluate_number(&mut resolver), Ok(Number::Int(3)));
            assert_eq!(plan.evaluate_number(&mut resolver), Ok(Number::Int(7)));
            assert_eq!(resolver.0, 4);
        }

        /// Replay function values, eager traces and argument-level failures through compilation.
        #[test]
        fn shared_function_vectors() {
            for row in [
                include_str!("fixtures/numeric_selection.tsv"),
                include_str!("fixtures/numeric_integral.tsv"),
                include_str!("fixtures/numeric_sqrt.tsv"),
                include_str!("fixtures/numeric_math.tsv"),
            ]
            .into_iter()
            .flat_map(|fixture| fixture.lines().skip(1))
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 6);
                let mut resolver = RecordingResolver::new(fields[2]);
                let (actual, route) =
                    outcome(fields[1], fixture_evaluate(fields[1], &mut resolver));
                assert_eq!(actual, fields[3], "{}", fields[0]);
                assert_eq!(
                    resolver.trace.join(","),
                    fields[4].trim_matches('-'),
                    "{}",
                    fields[0]
                );
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

        use crate::support::dataset::{predicate_text, Node, Scalar};
        use yamaa_core::expression::{
            compile, Comparison as C, CompileError, CompiledExpression, Condition, Context,
            ErrorKind, Resolver, Resource, Step as Route, Truth,
        };
        use yamaa_core::temporal::{Date, DateTime, Precision};
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

        /// Compile an authored arena's text; failures keep that text and path.
        fn plan(nodes: Vec<Node>) -> CompiledExpression {
            let root = nodes.len() - 1;
            compile(
                &predicate_text(&nodes, root),
                Context::Predicate,
                "rows[0].filter",
            )
            .unwrap()
        }

        /// Use identifiers to exercise deferred resolution rather than constant folding.
        fn id(name: &str) -> Scalar {
            Scalar::Identifier(name.into())
        }

        /// Embed a literal operand without host coercion.
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
            // A bool value reaches a predicate only through a name.
            for (value, expected) in [
                (Value::Missing, T),
                (Value::Bool(false), F),
                (Value::Int(0), F),
            ] {
                for negated in [false, true] {
                    let p = plan(vec![Node::IsNull {
                        value: id("v"),
                        negated,
                    }]);
                    assert_eq!(
                        p.evaluate_truth(&mut Port::with(&[("v", value.clone())]))
                            .unwrap(),
                        if negated { expected.negate() } else { expected }
                    );
                }
            }
            for negated in [false, true] {
                let p = plan(vec![Node::IsNull {
                    value: lit(Value::Missing),
                    negated,
                }]);
                assert_eq!(
                    p.evaluate_truth(&mut Port::default()).unwrap(),
                    if negated { F } else { T }
                );
            }
        }

        #[test]
        fn comparison_exact_mixed_order_missing_and_temporal_precision() {
            use Truth::{False as F, True as T};
            let date = Date::new(2025, 1, 1, Precision::Year).unwrap();
            let day = Date::new(2025, 1, 1, Precision::Day).unwrap();
            let datetime = DateTime::new(day, 0, 0, 0, Precision::Day).unwrap();
            let second = DateTime::new(day, 0, 0, 0, Precision::Second).unwrap();
            // Truth for =, <>, <, <=, >, >=. Mixed numbers compare by exact
            // value: 2^53 + 1 exceeds 2^53, and i64::MAX is below 2^63.
            let pairs = [
                (
                    Value::Int(9_007_199_254_740_993),
                    Value::float(9_007_199_254_740_992.0),
                    [F, T, F, F, T, T],
                ),
                (
                    Value::float(9_007_199_254_740_992.0),
                    Value::Int(9_007_199_254_740_993),
                    [F, T, T, T, F, F],
                ),
                (
                    Value::Int(9_007_199_254_740_992),
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
                    [F, T, T, T, F, F],
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
                        p.evaluate_truth(&mut port).unwrap(),
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
                            .evaluate_truth(&mut Port::with(&[("a", a), ("b", b)]))
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
                    .evaluate_truth(&mut Port::with(&[("a", a), ("b", b)]))
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
                let nodes = vec![
                    Node::Boolean(truth),
                    comparison(C::Equal),
                    if and { Node::And(0, 1) } else { Node::Or(0, 1) },
                ];
                let text = predicate_text(&nodes, 2);
                let p = plan(nodes);
                let mut port = Port::default();
                let error = p.evaluate_truth(&mut port).unwrap_err();
                assert_eq!(port.trace, ["a"]);
                assert_eq!(error.route, [Route::Right, Route::Left]);
                assert_eq!(error.spec_path, "rows[0].filter");
                assert_eq!(error.expression, text);
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
            assert_eq!(
                p.evaluate_truth(&mut port).unwrap_err().route,
                [Route::Item(1)]
            );
            assert_eq!(port.trace, ["a", "b", "c"]);
            port = Port::with(&[("a", Value::Int(1)), ("b", Value::Str("1".into()))]);
            assert!(matches!(
                p.evaluate_truth(&mut port).unwrap_err().kind,
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
                        .evaluate_truth(&mut Port::default())
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
            let error = p.evaluate_truth(&mut port).unwrap_err();
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
            assert_eq!(p.evaluate_truth(&mut port).unwrap(), Truth::False);
            assert_eq!(port.trace, ["x", "lo", "x", "hi"]);
            port = Port::with(&[
                ("x", Value::Missing),
                ("lo", Value::Int(1)),
                ("hi", Value::Int(3)),
            ]);
            assert_eq!(p.evaluate_truth(&mut port).unwrap(), Truth::Unknown);
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
                .evaluate_truth(&mut FailingPort)
                .unwrap_err();
            assert_eq!(
                error.kind,
                ErrorKind::Resolution {
                    identifier: "a".into(),
                    error: Failure(Box::new(37))
                }
            );
            for (condition, name, rule) in [
                (
                    Condition::UnknownField {
                        identifier: "a".into(),
                    },
                    "unknown_field",
                    "FORMULA-9",
                ),
                (
                    Condition::IncompatiblePair {
                        left: ValueType::Bool,
                        right: ValueType::Int,
                    },
                    "incompatible_input_type",
                    "FORMULA-5",
                ),
                (
                    Condition::ExpectedText {
                        actual: ValueType::Float,
                    },
                    "incompatible_input_type",
                    "FORMULA-5",
                ),
                (Condition::DanglingEscape, "invalid_expression", "FORMULA-7"),
            ] {
                assert_eq!(
                    (condition.phase(), condition.condition(), condition.rule()),
                    ("validation", name, rule)
                );
            }
        }

        /// Construct LIKE over literals without involving a regex library.
        fn like(value: &str, pattern: &str, escape: Option<char>) -> CompiledExpression {
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
                        .evaluate_truth(&mut Port::default())
                        .unwrap(),
                    expected,
                    "{value:?} {pattern:?}"
                );
            }
            // A literal pattern's dangling escape is refused before any value
            // is read; a resolved pattern's fails when it is matched.
            assert!(matches!(
                compile(
                    "'no match' LIKE 'abc!' ESCAPE '!'",
                    Context::Predicate,
                    "rows[0].filter"
                ),
                Err(CompileError::Invalid { .. })
            ));
            let p = plan(vec![Node::Like {
                value: lit(Value::Str("no match".into())),
                pattern: id("p"),
                escape: Some('!'),
                negated: false,
            }]);
            let error = p
                .evaluate_truth(&mut Port::with(&[("p", Value::Str("abc!".into()))]))
                .unwrap_err();
            assert_eq!(error.kind, ErrorKind::Condition(Condition::DanglingEscape));
            let p = plan(vec![Node::Like {
                value: lit(Value::Missing),
                pattern: lit(Value::Int(1)),
                escape: None,
                negated: true,
            }]);
            assert_eq!(
                p.evaluate_truth(&mut Port::default()).unwrap(),
                Truth::Unknown
            );
            let p = plan(vec![Node::Like {
                value: lit(Value::Str("a".into())),
                pattern: id("p"),
                escape: None,
                negated: false,
            }]);
            assert_eq!(
                p.evaluate_truth(&mut Port::with(&[("p", Value::Bool(false))]))
                    .unwrap_err()
                    .kind,
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
                            .evaluate_truth(&mut Port::default())
                            .unwrap(),
                        expected,
                        "{value:?} {pattern:?}"
                    );
                }
            }
        }

        #[test]
        fn empty_membership_and_fixed_semantic_depth_are_refused_before_values() {
            assert!(matches!(
                compile("NULL IN ()", Context::Predicate, "rows[0].filter"),
                Err(CompileError::Invalid { .. })
            ));
            let edge = format!("{}TRUE", "NOT ".repeat(127));
            let edge = compile(&edge, Context::Predicate, "rows[0].filter").unwrap();
            assert_eq!(
                edge.evaluate_truth(&mut Port::default()).unwrap(),
                Truth::False
            );
            let deep = format!("{}TRUE", "NOT ".repeat(128));
            assert!(matches!(
                compile(&deep, Context::Predicate, "rows[0].filter"),
                Err(CompileError::Limit {
                    resource: Resource::Depth,
                    limit: 128,
                    ..
                })
            ));
        }

        #[test]
        fn repeated_like_and_text_comparisons_keep_the_original_truth_and_reads() {
            let p = like("abc", "%", None);
            for _ in 0..2 {
                assert_eq!(p.evaluate_truth(&mut Port::default()).unwrap(), Truth::True);
            }
            let node = Node::Like {
                value: lit(Value::Str("abc".into())),
                pattern: lit(Value::Str("%".into())),
                escape: None,
                negated: false,
            };
            let p = plan(vec![node, Node::Or(0, 0)]);
            assert_eq!(p.evaluate_truth(&mut Port::default()).unwrap(), Truth::True);
            let p = plan(vec![comparison(C::Equal)]);
            let mut port = Port::with(&[
                ("a", Value::Str("a".repeat(40))),
                ("b", Value::Str("a".repeat(40))),
            ]);
            assert_eq!(p.evaluate_truth(&mut port).unwrap(), Truth::True);
            assert_eq!(port.trace, ["a", "b"]);
            assert_eq!(
                p.evaluate_truth(&mut Port::with(&[
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

        #[test]
        fn shared_reference_truth_and_occurrence_traces() {
            for line in include_str!("fixtures/predicate_evaluation.tsv")
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
            {
                let fields: Vec<_> = line.split('\t').collect();
                assert_eq!(fields.len(), 6);
                let p = compile(fields[1], Context::Predicate, "rows[0].filter").unwrap();
                let mut port = Port::default();
                if fields[3] != "-" {
                    for binding in fields[3].split(';') {
                        let (name, value) = binding.split_once('=').unwrap();
                        port.values
                            .insert(name.into(), [fixture_value(value)].into());
                    }
                }
                let result = p.evaluate_truth(&mut port);
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

        #[test]
        fn repeated_plans_resolve_each_written_occurrence() {
            let p = plan(vec![comparison(C::Equal)]);
            let mut port =
                Port::with(&[("a", Value::Str("x".into())), ("b", Value::Str("x".into()))]);
            for _ in 0..2 {
                assert_eq!(p.evaluate_truth(&mut port).unwrap(), Truth::True);
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
            ) -> Result<yamaa_core::expression::Resolved<'_>, Self::Error> {
                self.calls += 1;
                Ok(yamaa_core::expression::Resolved::Borrowed(
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
            assert_eq!(p.evaluate_truth(&mut port).unwrap(), Truth::True);
            assert_eq!(port.calls, 2);
            assert_eq!(p.identifiers().collect::<Vec<_>>(), ["a", "b"]);
        }
    }
    mod predicate_contains {
        //! Independent FORMULA-8 truth, error ordering and linear-time matching.
        use crate::support::dataset::{predicate_text, Node, Scalar};
        use yamaa_core::{
            expression::{
                compile, CompileError, CompiledExpression, Condition, Context, ErrorKind, Failure,
                Resolver, Step as Route, Truth,
            },
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
        /// Admit the entire text before constructing or invoking any resolver.
        fn plan(nodes: Vec<Node>) -> CompiledExpression {
            let root = nodes.len() - 1;
            compile(
                &predicate_text(&nodes, root),
                Context::Predicate,
                "rows[0].filter",
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
                ("(?<letter>a)ac", "baac", Truth::True),
                ("\\Bb", "ab", Truth::True),
                ("\\bb", "ab", Truth::False),
                ("^.$", "😀", Truth::True),
                ("z", "cat", Truth::False),
                ("", "", Truth::True),
            ] {
                let p = plan(vec![contains(pattern)]);
                let mut source = port(Value::Str(subject.into()));
                assert_eq!(
                    p.evaluate_truth(&mut source).unwrap(),
                    expected,
                    "{pattern}: {subject}"
                );
                assert_eq!(source.reads, ["s"]);
                assert_eq!(p.identifiers().collect::<Vec<_>>(), ["s"]);
            }
            for negated in [false, true] {
                let mut nodes = vec![contains("a")];
                if negated {
                    nodes.push(Node::Not(0));
                }
                let p = plan(nodes);
                assert_eq!(
                    p.evaluate_truth(&mut port(Value::Missing)).unwrap(),
                    Truth::Unknown
                );
                for (value, actual) in [
                    (Value::Int(1), ValueType::Int),
                    (Value::Bool(false), ValueType::Bool),
                ] {
                    let error = p.evaluate_truth(&mut port(value)).unwrap_err();
                    let condition = Condition::ContainsExpectedText { actual };
                    assert_eq!(error.kind, ErrorKind::Condition(condition.clone()));
                    assert_eq!(condition.condition(), "incompatible_input_type");
                    assert_eq!(condition.rule(), "FORMULA-8");
                    assert_eq!(error.spec_path, "rows[0].filter");
                    assert_eq!(
                        error.route,
                        if negated { vec![Route::Child] } else { vec![] }
                    );
                }
            }
        }

        /// Every pattern literal is compiled once with the text, before any value is read.
        #[test]
        fn contains_admits_every_literal_including_unreachable_nodes() {
            let text = predicate_text(&[contains("["), Node::Boolean(false), Node::And(1, 0)], 2);
            assert!(matches!(
                compile(&text, Context::Predicate, "p"),
                Err(CompileError::Invalid {
                    failure: Failure::InvalidRegex { .. },
                    ..
                })
            ));
            let pattern = format!("(a){}b", "(a|aa)".repeat(8));
            plan(vec![contains(&pattern)]);
            // 512 pattern literals in a balanced disjunction each compile once.
            let mut terms = vec![predicate_text(&[contains(&pattern)], 0); 512];
            while terms.len() > 1 {
                terms = terms
                    .chunks(2)
                    .map(|pair| format!("({} OR {})", pair[0], pair[1]))
                    .collect();
            }
            compile(&terms[0], Context::Predicate, "p").unwrap();
        }

        #[test]
        fn contains_eager_order_and_original_failure_precede_matching() {
            for (first, combine) in [(false, Node::And(0, 1)), (true, Node::Or(0, 1))] {
                let p = plan(vec![Node::Boolean(first), contains("a"), combine]);
                let mut source = port(Value::Missing);
                source.failure = true;
                let error = p.evaluate_truth(&mut source).unwrap_err();
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
                p.evaluate_truth(&mut port(Value::Missing)).unwrap(),
                Truth::Unknown
            );
            assert_eq!(
                p.evaluate_truth(&mut port(Value::Int(0))).unwrap_err().kind,
                ErrorKind::Condition(Condition::ContainsExpectedText {
                    actual: ValueType::Int
                })
            );
        }

        #[test]
        fn contains_answers_catastrophic_patterns_and_keeps_small_matches_independent() {
            let single = plan(vec![contains("a+")]);
            for _ in 0..2 {
                let mut source = port(Value::Str("aaaa".into()));
                assert_eq!(single.evaluate_truth(&mut source).unwrap(), Truth::True);
                assert_eq!(source.reads, ["s"]);
            }
            let repeated = plan(vec![contains("a+"), contains("a+"), Node::And(0, 1)]);
            let mut source = port(Value::Str("aaaa".into()));
            assert_eq!(repeated.evaluate_truth(&mut source).unwrap(), Truth::True);
            assert_eq!(source.reads, ["s", "s"]);
            // (a+)+$ cannot match before the final "!": a linear-time FALSE,
            // where the retired step budget refused the whole predicate.
            let catastrophic = plan(vec![contains("a+"), contains("(a+)+$"), Node::And(0, 1)]);
            for _ in 0..2 {
                let mut source = port(Value::Str("a".repeat(100_000) + "!"));
                assert_eq!(
                    catastrophic.evaluate_truth(&mut source).unwrap(),
                    Truth::False
                );
                assert_eq!(source.reads, ["s", "s"]);
            }
        }
    }
    mod reduction {
        use yamaa_core::{
            aggregate_reduction::{reduce_values, AggregateReductionError, ReductionFields},
            expression::Reducer,
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

        /// A fixture reducer: its name, and for `QUANTILE` the probability
        /// written after a colon, as the aggregate's `quantile` gives it.
        fn reducer(token: &str) -> (Reducer, f64) {
            let (name, quantile) = match token.split_once(':') {
                Some((name, quantile)) => (name, quantile.parse().unwrap()),
                None => (token, 0.0),
            };
            let reducer = Reducer::ALL
                .into_iter()
                .find(|reducer| reducer.name() == name)
                .unwrap_or_else(|| panic!("bad reducer {token}"));
            (reducer, quantile)
        }

        #[test]
        /// Pin all shared values, exact float bits and portable failure facts:
        /// the ordered `SUM` and `MEAN` of FORMULA-19 and the statistical
        /// reducers of FORMULA-18, each through the aggregate reduction.
        fn independent_ordered_reduction_truth_shared_with_python() {
            let mut count = 0;
            for row in include_str!("fixtures/reduction.tsv").lines().skip(1) {
                let columns: Vec<_> = row.split('\t').collect();
                assert_eq!(columns.len(), 4);
                let (id, name, tokens, expected) = (columns[0], columns[1], columns[2], columns[3]);
                let (reducer, quantile) = reducer(name);
                let values: Vec<_> = if tokens == "-" {
                    vec![]
                } else {
                    tokens.split(',').map(value).collect()
                };
                let borrowed: Vec<_> = values.iter().map(ValueRef::from).collect();
                let expression = format!("{}(A)", reducer.name());
                let fields = ReductionFields {
                    quantile,
                    ..ReductionFields::default()
                };
                let actual = match reduce_values(&borrowed, reducer, fields, &expression) {
                    Ok(Value::Missing) => "missing".into(),
                    Ok(Value::Int(value)) => format!("int:{value}"),
                    Ok(Value::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
                    Err(AggregateReductionError::Numeric(
                        ReductionError::IncompatibleInputType { argument, actual },
                    )) => {
                        assert_eq!(borrowed[argument].value_type(), Some(actual), "{id}");
                        format!(
                            "error:incompatible_input_type:FORMULA-23:{}",
                            format!("{actual:?}").to_lowercase()
                        )
                    }
                    // An arithmetic failure names the position of the value
                    // whose step failed, after `@`.
                    Err(AggregateReductionError::Numeric(ReductionError::Arithmetic {
                        argument,
                        error,
                    })) => {
                        assert_eq!(error.expression, expression);
                        assert_eq!(error.phase(), "derivation");
                        match error.kind {
                            ArithmeticErrorKind::IntegerOverflow { value } => format!(
                                "error:{}:{}:{value}@{argument}",
                                error.condition(),
                                error.rule()
                            ),
                            ArithmeticErrorKind::LnOfNonpositive { function } => format!(
                                "error:{}:{}:{function}@{argument}",
                                error.condition(),
                                error.rule()
                            ),
                            other => panic!("unexpected failure {other:?}"),
                        }
                    }
                    other => panic!("unexpected result: {other:?}"),
                };
                assert_eq!(actual, expected, "{id}");
                count += 1;
            }
            assert_eq!(count, 85);
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
    mod scalar_language {
        use yamaa_core::{
            scalar_language::replace_extreme,
            temporal::{Date, Precision},
            value::Value,
        };
        #[test]
        fn ties_preserve_first_type_and_precision_and_integer_comparison_is_exact() {
            for largest in [true, false] {
                assert!(!replace_extreme(&Value::Int(1), &Value::float(1.0), largest).unwrap());
                assert!(!replace_extreme(&Value::float(1.0), &Value::Int(1), largest).unwrap());
                let month = Value::Date(Date::new(2025, 1, 1, Precision::Month).unwrap());
                let day = Value::Date(Date::new(2025, 1, 1, Precision::Day).unwrap());
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
