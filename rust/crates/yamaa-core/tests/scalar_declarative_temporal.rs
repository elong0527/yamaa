//! Scalar contracts: declarative rounding, declarative temporal and
//! declarative temporal imputation. These are the seams the diagnostic
//! replay cannot reach, each named in its test's first comment: the values
//! an operation returns, generated sizes and constructor shapes. What the
//! operations refuse is `registry.json`'s and `temporal.json`'s.
mod scalar {
    mod declarative_rounding {
        use yamaa_core::{dataset::Rounding, table::ValueRef, value::Value};

        /// Seam: rounded values, which no reader returns (core outcome).
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

        /// Seam: already data, the authored `numeric_rounding.tsv` vectors, whose results are
        /// rounded values.
        /// Replay the independent exact rounding controls through the one rounding operation.
        #[test]
        fn shared_rounding_vectors() {
            for row in include_str!("fixtures/numeric_rounding.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 4);
                let source = match fields[1].split_once(':') {
                    None => Value::Missing,
                    Some(("int", text)) => Value::Int(text.parse().unwrap()),
                    Some(("float", text)) => Value::float(text.parse().unwrap()),
                    _ => panic!("bad fixture source {}", fields[1]),
                };
                let rounding = Rounding::new(
                    "VAL",
                    "columns.RND.derivation.round_half_away_from_zero",
                    fields[2],
                )
                .unwrap();
                let actual = match rounding.apply(ValueRef::from(&source)).unwrap() {
                    Value::Missing => "missing".into(),
                    Value::Float(value) => format!("float:{:016x}", value.get().to_bits()),
                    other => panic!("rounding returned {other:?}"),
                };
                assert_eq!(actual, fields[3], "{}", fields[0]);
            }
        }

        /// Seam: bit-exact rounded values at extreme digits.
        /// Extreme digits keep a float result; a rounded overflow normalizes to missing.
        #[test]
        fn extreme_digits_and_values_keep_the_rounding_range_contract() {
            for input in [f64::MAX, -f64::MAX] {
                let rounding = Rounding::new("VAL", "round", "-308").unwrap();
                assert_eq!(
                    rounding.apply(ValueRef::from(&Value::float(input))),
                    Ok(Value::Missing)
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
                    let rounding = Rounding::new("VAL", "round", &digits.to_string()).unwrap();
                    let Ok(Value::Float(result)) =
                        rounding.apply(ValueRef::from(&Value::float(input)))
                    else {
                        panic!("float required")
                    };
                    assert_eq!(result.get().to_bits(), expected.to_bits());
                }
            }
        }

        /// Seam: the `Rounding::new` shape refusal, which reports no issue, and rounded values.
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
        /// Seam: date differences, values no reader returns (core outcome).
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
        /// Seam: study and epoch day values, and the arity refusal (`None`).
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
        /// Seam: the constructor's arity and bounds refusals and a missing operand's `Ok(None)`,
        /// none of them an issue; a text operand's refusal is
        /// `temporal.json::study_day_refuses_a_date_operand_that_is_text`.
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
        }

        /// Seam: a generated 200,000-character source name and an operand index past the operands.
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
                completion_policy, Completion, DayDeclaration as D, FlagPart, Imputation,
                ImputationKind as K, ImputationStep as S, TemporalType as T, TimeRule,
            },
            table::ValueRef,
            temporal::{Date, DateTime, Precision as P},
            value::Value,
        };
        fn operation(kind: K, bound: bool, invalid: Option<Value>) -> Imputation {
            Imputation::new(
                kind,
                "SRC.TEXT".into(),
                bound.then(|| "REF".into()),
                None,
                None,
                "columns.DATE.derivation.impute".into(),
                invalid,
            )
            .unwrap()
        }
        fn completion(
            target: T,
            minimum: P,
            day: Option<D<'_>>,
            time: Option<TimeRule>,
        ) -> Completion {
            // A month, written only where the rules complete a year, implies
            // that precision (TIME-8).
            completion_policy(
                target,
                (minimum == P::Year).then_some("6"),
                day,
                time,
                "date",
            )
            .unwrap()
        }
        fn date_policy_kind(minimum: P, day: D<'_>) -> K {
            K::Impute(completion(T::Date, minimum, Some(day), None))
        }
        fn datetime_kind(time: TimeRule) -> K {
            K::Impute(completion(T::DateTime, P::Day, None, Some(time)))
        }
        fn precision(target: T) -> Imputation {
            operation(K::Precision { target }, false, None)
        }
        fn flag(part: FlagPart) -> Imputation {
            operation(
                K::Flag {
                    part,
                    seconds_collected: true,
                },
                false,
                None,
            )
        }
        fn word(text: &str) -> S {
            S::Value(Value::Str(text.into()))
        }
        /// Seam: completed dates, their collected precision and flags, values no reader returns.
        #[test]
        fn independent_partial_dates_preserve_precision_and_calendar_bound_intervals() {
            let first = operation(date_policy_kind(P::Year, D::Position("first")), false, None);
            let last = operation(date_policy_kind(P::Year, D::Position("last")), false, None);
            for (input, early, late, collected, name, code) in [
                (
                    "1975",
                    "1975-06-01",
                    "1975-06-30",
                    P::Year,
                    "year",
                    Some("M"),
                ),
                (
                    "1980-02",
                    "1980-02-01",
                    "1980-02-29",
                    P::Month,
                    "month",
                    Some("D"),
                ),
                (
                    "1900-02",
                    "1900-02-01",
                    "1900-02-28",
                    P::Month,
                    "month",
                    Some("D"),
                ),
                (
                    "1990-06-15",
                    "1990-06-15",
                    "1990-06-15",
                    P::Day,
                    "day",
                    None,
                ),
                // Text carrying a complete date yields that date, time or not.
                (
                    "1990-06-15T08",
                    "1990-06-15",
                    "1990-06-15",
                    P::Day,
                    "day",
                    None,
                ),
                (
                    "1990-06-15T08:30:15",
                    "1990-06-15",
                    "1990-06-15",
                    P::Day,
                    "day",
                    None,
                ),
            ] {
                for (op, expected) in [(&first, early), (&last, late)] {
                    let S::Value(Value::Date(date)) =
                        op.source_value(ValueRef::Str(input)).unwrap()
                    else {
                        panic!("date")
                    };
                    assert_eq!(date.to_string(), expected);
                    assert_eq!(date.collected_precision(), collected);
                    assert_eq!(
                        precision(T::Date).source_value(ValueRef::Date(date)),
                        Ok(word(name))
                    );
                    assert_eq!(
                        precision(T::Date).source_value(ValueRef::Str(input)),
                        Ok(word(name))
                    );
                    assert_eq!(
                        flag(FlagPart::Date).source_value(ValueRef::Date(date)),
                        Ok(S::Value(
                            code.map_or(Value::Missing, |code| Value::Str(code.into()))
                        ))
                    );
                }
            }
            let bounded = operation(date_policy_kind(P::Year, D::Position("first")), true, None);
            for (text, lower, expected) in [
                ("2024", "2024-07-20", "2024-07-20"),
                ("2024-02", "2024-02-29", "2024-02-29"),
                // A bound outside the interval the collected components admit
                // is ignored, so the completion stands.
                ("2024-02", "2024-03-01", "2024-02-01"),
                ("2024", "2025-01-01", "2024-06-01"),
                ("2024-02", "2024-01-31", "2024-02-01"),
            ] {
                let S::Bound(completed) = bounded.source_value(ValueRef::Str(text)).unwrap() else {
                    panic!("bound")
                };
                let (result, crossed) = bounded
                    .apply_bounds(
                        completed,
                        ValueRef::Date(lower.parse().unwrap()),
                        ValueRef::Missing,
                    )
                    .unwrap();
                assert_eq!(result, Value::Date(expected.parse().unwrap()), "{text}");
                assert!(!crossed);
                let Value::Date(date) = result else {
                    panic!("date")
                };
                assert_eq!(date.collected_precision(), completed.collected_precision());
            }
            assert!(matches!(
                bounded.source_value(ValueRef::Str("2024-02-29")),
                Ok(S::Value(_))
            ));
            let month = operation(
                date_policy_kind(P::Month, D::Position("first")),
                true,
                Some(Value::Int(2)),
            );
            assert_eq!(
                month.source_value(ValueRef::Str("2024")),
                Ok(S::Value(Value::Missing))
            );
            assert_eq!(month.source_value(ValueRef::Missing), Ok(S::Missing));
            assert_eq!(
                month.source_value(ValueRef::Str("2024-00")),
                Ok(S::Recovered { handler: "invalid" })
            );
            let complete = operation(
                K::Impute(completion(T::Date, P::Day, None, None)),
                false,
                None,
            );
            assert_eq!(
                complete.source_value(ValueRef::Str("2024-02")),
                Ok(S::Value(Value::Missing))
            );
            assert_eq!(
                complete.source_value(ValueRef::Str("2024-02-29T12")),
                Ok(S::Value(Value::Date("2024-02-29".parse().unwrap())))
            );
        }

        fn date(text: &str) -> ValueRef<'static> {
            ValueRef::Date(text.parse().unwrap())
        }
        fn bounded(not_before: bool, not_after: bool, day: D<'_>) -> Imputation {
            Imputation::new(
                date_policy_kind(P::Year, day),
                "SRC.TEXT".into(),
                not_before.then(|| "LOW".into()),
                not_after.then(|| "HIGH".into()),
                None,
                "columns.DATE.derivation.impute".into(),
                None,
            )
            .unwrap()
        }
        fn bounded_result(
            op: &Imputation,
            text: &str,
            not_before: ValueRef<'_>,
            not_after: ValueRef<'_>,
        ) -> (Value, bool) {
            let S::Bound(completed) = op.source_value(ValueRef::Str(text)).unwrap() else {
                panic!("bound")
            };
            let (result, crossed) = op.apply_bounds(completed, not_before, not_after).unwrap();
            let Value::Date(date) = result else {
                panic!("bounds never give missing")
            };
            assert_eq!(date.collected_precision(), completed.collected_precision());
            (result, crossed)
        }
        /// Seam: bounded dates, values no reader returns.
        #[test]
        fn upper_bound_moves_only_supplied_components_within_the_collected_interval() {
            let last = bounded(false, true, D::Position("last"));
            for (text, upper, expected) in [
                ("2025-03", "2025-03-12", "2025-03-12"),
                ("2025-03", "2025-03-31", "2025-03-31"),
                ("2025-03", "2025-04-20", "2025-03-31"),
                ("2025-03", "2025-03-01", "2025-03-01"),
                ("2025-03", "2025-02-28", "2025-03-31"),
                ("2024-02", "2024-02-29", "2024-02-29"),
                ("2024-02", "2024-02-28", "2024-02-28"),
                ("2025", "2025-02-10", "2025-02-10"),
                ("2025", "2025-01-01", "2025-01-01"),
                ("2025", "2024-12-31", "2025-06-30"),
                ("2025", "2026-01-15", "2025-06-30"),
            ] {
                assert_eq!(
                    bounded_result(&last, text, ValueRef::Missing, date(upper)),
                    (Value::Date(expected.parse().unwrap()), false),
                    "{text} not after {upper}"
                );
            }
            assert_eq!(
                bounded_result(&last, "2025-03", ValueRef::Missing, ValueRef::Missing),
                (Value::Date("2025-03-31".parse().unwrap()), false)
            );
            // A complete source never reaches a bound, whatever the bound says.
            assert_eq!(
                last.source_value(ValueRef::Str("2025-03-28")),
                Ok(S::Value(Value::Date("2025-03-28".parse().unwrap())))
            );
            // A completion already on or before the bound stands.
            let first = bounded(false, true, D::Position("first"));
            assert_eq!(
                bounded_result(&first, "2025-03", ValueRef::Missing, date("2025-03-12")),
                (Value::Date("2025-03-01".parse().unwrap()), false)
            );
        }
        /// Seam: bounded dates and the crossed-bounds flag.
        #[test]
        fn paired_bounds_keep_a_completion_between_them_and_crossed_bounds_give_not_after() {
            let both = bounded(true, true, D::Number("15"));
            for (text, lower, upper, expected, crossed) in [
                ("2025-03", "2025-03-05", "2025-03-25", "2025-03-15", false),
                ("2025-03", "2025-03-20", "2025-03-25", "2025-03-20", false),
                ("2025-03", "2025-03-01", "2025-03-10", "2025-03-10", false),
                ("2025-03", "2025-03-12", "2025-03-12", "2025-03-12", false),
                ("2025-03", "2025-02-01", "2025-04-30", "2025-03-15", false),
                ("2025-03", "2025-03-20", "2025-03-10", "2025-03-10", true),
                ("2025-03", "2025-04-01", "2025-05-01", "2025-03-15", false),
                ("2025-03", "2025-01-01", "2025-02-28", "2025-03-15", false),
                ("2025-03", "2025-03-20", "2025-04-30", "2025-03-20", false),
                ("2025-03", "2025-02-01", "2025-03-10", "2025-03-10", false),
                ("2025", "2025-07-01", "2025-08-31", "2025-07-01", false),
                ("2025", "2025-02-01", "2025-05-31", "2025-05-31", false),
                ("2025", "2025-09-01", "2025-03-01", "2025-03-01", true),
            ] {
                assert_eq!(
                    bounded_result(&both, text, date(lower), date(upper)),
                    (Value::Date(expected.parse().unwrap()), crossed),
                    "{text} between {lower} and {upper}"
                );
            }
            for (lower, upper, expected) in [
                (ValueRef::Missing, date("2025-03-10"), "2025-03-10"),
                (date("2025-03-20"), ValueRef::Missing, "2025-03-20"),
                (ValueRef::Missing, ValueRef::Missing, "2025-03-15"),
            ] {
                assert_eq!(
                    bounded_result(&both, "2025-03", lower, upper),
                    (Value::Date(expected.parse().unwrap()), false)
                );
            }
            // An undeclared bound is no bound, whatever value reaches it.
            let lower_only = bounded(true, false, D::Number("15"));
            assert_eq!(
                bounded_result(&lower_only, "2025-03", date("2025-03-01"), ValueRef::Int(1)),
                (Value::Date("2025-03-15".parse().unwrap()), false)
            );
            assert!(both.counts_crossed_bounds());
            assert!(!lower_only.counts_crossed_bounds());
        }
        /// Seam: bounded datetimes and the crossed-bounds flag.
        #[test]
        fn datetime_bounds_move_only_supplied_time_fields() {
            let op = Imputation::new(
                K::Impute(completion(
                    T::Date,
                    P::Year,
                    Some(D::Position("first")),
                    None,
                )),
                "SRC.TEXT".into(),
                None,
                None,
                None,
                "columns.DATE.derivation.impute".into(),
                None,
            )
            .unwrap();
            assert!(!op.counts_crossed_bounds());
            let both = Imputation::new(
                K::Impute(completion(T::DateTime, P::Day, None, Some(TimeRule::First))),
                "SRC.TEXT".into(),
                Some("LOW".into()),
                Some("HIGH".into()),
                None,
                "columns.DTM.derivation.impute".into(),
                None,
            )
            .unwrap();
            let moment = |text: &str| ValueRef::DateTime(text.parse().unwrap());
            for (text, lower, upper, expected, crossed) in [
                // The hour 08 admits 08:00:00 to 08:59:59.
                (
                    "2025-03-04T08",
                    "2025-03-04T08:30:00",
                    "2025-03-04T09:00",
                    "2025-03-04T08:30:00",
                    false,
                ),
                (
                    "2025-03-04T08",
                    "2025-03-04T07:30:00",
                    "2025-03-04T08:20:00",
                    "2025-03-04T08:00:00",
                    false,
                ),
                (
                    "2025-03-04T08",
                    "2025-03-04T08:40:00",
                    "2025-03-04T08:20:00",
                    "2025-03-04T08:20:00",
                    true,
                ),
                // A date alone admits its whole day.
                (
                    "2025-03-04",
                    "2025-03-04T09:15:00",
                    "2025-03-05T00:00:00",
                    "2025-03-04T09:15:00",
                    false,
                ),
            ] {
                let S::Bound(completed) = both.source_value(ValueRef::Str(text)).unwrap() else {
                    panic!("bound")
                };
                let (result, observed) = both
                    .apply_bounds(completed, moment(lower), moment(upper))
                    .unwrap();
                let Value::DateTime(value) = result else {
                    panic!("datetime")
                };
                assert_eq!(value.to_string(), expected, "{text}");
                assert_eq!(value.collected_precision(), completed.collected_precision());
                assert_eq!(observed, crossed, "{text}");
            }
            assert_eq!(
                both.source_value(ValueRef::Str("2025-03-04T08:30:15")),
                Ok(S::Value(Value::DateTime(
                    "2025-03-04T08:30:15".parse().unwrap()
                )))
            );
        }
        /// Seam: the fallback's value and its collected precision `none`; a fallback of another
        /// type is `temporal.json::a_datetime_fallback_that_is_a_date_names_its_own_field`.
        #[test]
        fn a_fallback_answers_a_missing_source_with_collected_precision_none() {
            let op = Imputation::new(
                date_policy_kind(P::Year, D::Position("first")),
                "SRC.TEXT".into(),
                None,
                None,
                Some("TRTSDT".into()),
                "columns.DATE.derivation.impute".into(),
                None,
            )
            .unwrap();
            assert_eq!(op.source_value(ValueRef::Missing), Ok(S::Missing));
            assert_eq!(
                op.operands().collect::<Vec<_>>(),
                [("source", "SRC.TEXT"), ("fallback", "TRTSDT")]
            );
            assert!(op.has_conditional_operands());
            // A missing fallback gives missing (TIME-13).
            assert_eq!(op.fallback_value(ValueRef::Missing), Ok(Value::Missing));
            let Ok(Value::Date(value)) = op.fallback_value(date("2025-01-10")) else {
                panic!("fallback date")
            };
            assert_eq!(value.to_string(), "2025-01-10");
            assert_eq!(value.collected_precision(), P::None);
            assert_eq!(
                precision(T::Date).source_value(ValueRef::Date(value)),
                Ok(word("none"))
            );
            assert_eq!(
                flag(FlagPart::Date).source_value(ValueRef::Date(value)),
                Ok(word("Y"))
            );
            let moment: DateTime = "2025-01-10T09:30:00".parse().unwrap();
            let none = moment.with_precision(P::None);
            assert_eq!(
                flag(FlagPart::Time).source_value(ValueRef::DateTime(none)),
                Ok(word("H"))
            );
            assert_eq!(
                flag(FlagPart::Date).source_value(ValueRef::DateTime(none)),
                Ok(word("Y"))
            );
        }

        /// Seam: completed datetimes, precision words and converted dates; its type refusals are
        /// asserted only as refusals.
        #[test]
        fn datetime_edges_precision_and_explicit_date_conversion_are_field_copies() {
            for (rule, expected) in [
                (TimeRule::First, "2024-02-29T00:00:00"),
                (TimeRule::Last, "2024-02-29T23:59:59"),
            ] {
                let op = operation(datetime_kind(rule), false, None);
                let S::Value(Value::DateTime(value)) =
                    op.source_value(ValueRef::Str("2024-02-29")).unwrap()
                else {
                    panic!("datetime")
                };
                assert_eq!(value.to_string(), expected);
                assert_eq!(value.collected_precision(), P::Day);
                assert_eq!(
                    precision(T::DateTime).source_value(ValueRef::DateTime(value)),
                    Ok(word("day"))
                );
                let S::Value(Value::Date(date)) = operation(K::ToDate, false, None)
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
            let op = operation(datetime_kind(TimeRule::First), false, None);
            assert!(op.source_value(ValueRef::Date(date)).is_err());
            assert_eq!(
                op.source_value(ValueRef::DateTime(datetime)),
                Ok(S::Value(Value::DateTime(datetime)))
            );
            assert!(precision(T::Date)
                .source_value(ValueRef::DateTime(datetime))
                .is_err());
            assert!(precision(T::DateTime)
                .source_value(ValueRef::Date(date))
                .is_err());
            assert!(flag(FlagPart::Time)
                .source_value(ValueRef::Date(date))
                .is_err());
            assert!(flag(FlagPart::Date)
                .source_value(ValueRef::Str("2024-02-29"))
                .is_err());
            for (text, date_word, datetime_word) in [
                ("2024", "year", "year"),
                ("2024-02", "month", "month"),
                ("2024-02-29", "day", "day"),
                ("2024-02-29T12", "day", "hour"),
                ("2024-02-29T12:13", "day", "minute"),
                ("2024-02-29T12:13:14", "day", "second"),
            ] {
                assert_eq!(
                    precision(T::Date).source_value(ValueRef::Str(text)),
                    Ok(word(date_word)),
                    "{text}"
                );
                assert_eq!(
                    precision(T::DateTime).source_value(ValueRef::Str(text)),
                    Ok(word(datetime_word)),
                    "{text}"
                );
            }
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
                    precision(T::Date)
                        .source_value(ValueRef::Str(invalid))
                        .is_err(),
                    "{invalid}"
                );
            }
            // to_date reads the date part of any text carrying a complete date;
            // year or year-month text is partial.
            let to_date = operation(K::ToDate, false, None);
            for text in [
                "2024-02-29",
                "2024-02-29T12",
                "2024-02-29T12:13",
                "2024-02-29T12:13:14",
            ] {
                assert_eq!(
                    to_date.source_value(ValueRef::Str(text)),
                    Ok(S::Value(Value::Date("2024-02-29".parse().unwrap()))),
                    "{text}"
                );
            }
            for text in ["2024", "2024-02"] {
                assert_eq!(to_date.source_value(ValueRef::Str(text)), Ok(S::Partial));
            }
            assert!(to_date.source_value(ValueRef::Str("2024-02-30")).is_err());
            // A datetime completed from a year and month keeps that precision,
            // which to_date keeps and caps at day.
            let monthly = operation(
                K::Impute(completion(
                    T::DateTime,
                    P::Month,
                    Some(D::Position("last")),
                    Some(TimeRule::Last),
                )),
                false,
                None,
            );
            let S::Value(Value::DateTime(moment)) =
                monthly.source_value(ValueRef::Str("2024-02")).unwrap()
            else {
                panic!("datetime")
            };
            assert_eq!(moment.to_string(), "2024-02-29T23:59:59");
            assert_eq!(moment.collected_precision(), P::Month);
            let S::Value(Value::Date(day)) =
                to_date.source_value(ValueRef::DateTime(moment)).unwrap()
            else {
                panic!("date")
            };
            assert_eq!(day.collected_precision(), P::Month);
            assert_eq!(
                flag(FlagPart::Date).source_value(ValueRef::DateTime(moment)),
                Ok(word("D"))
            );
            assert_eq!(
                flag(FlagPart::Time).source_value(ValueRef::DateTime(moment)),
                Ok(word("H"))
            );
        }
        /// Seam: completed datetimes and their collected precision.
        #[test]
        fn partial_dates_complete_to_datetimes_by_the_date_and_time_rules() {
            let op = operation(
                K::Impute(completion(
                    T::DateTime,
                    P::Year,
                    Some(D::Position("first")),
                    Some(TimeRule::First),
                )),
                false,
                None,
            );
            for (text, expected, collected) in [
                ("2025", "2025-06-01T00:00:00", P::Year),
                ("2025-03", "2025-03-01T00:00:00", P::Month),
                ("2025-03-04", "2025-03-04T00:00:00", P::Day),
                ("2025-03-04T08", "2025-03-04T08:00:00", P::Hour),
            ] {
                let S::Value(Value::DateTime(value)) =
                    op.source_value(ValueRef::Str(text)).unwrap()
                else {
                    panic!("datetime")
                };
                assert_eq!(value.to_string(), expected, "{text}");
                assert_eq!(value.collected_precision(), collected, "{text}");
            }
            // Without date rules, a datetime source coarser than its day gives missing.
            for text in ["2025", "2025-03"] {
                assert_eq!(
                    operation(datetime_kind(TimeRule::First), false, None)
                        .source_value(ValueRef::Str(text)),
                    Ok(S::Value(Value::Missing))
                );
            }
        }
        /// Seam: completed datetimes, precision words, flags and the `invalid` handler's recovery
        /// step.
        #[test]
        fn reduced_precision_times_receive_only_their_missing_fields() {
            for (input, early, late, collected, name, code) in [
                (
                    "2025-03-04",
                    "2025-03-04T00:00:00",
                    "2025-03-04T23:59:59",
                    P::Day,
                    "day",
                    Some("H"),
                ),
                (
                    "2025-03-04T08",
                    "2025-03-04T08:00:00",
                    "2025-03-04T08:59:59",
                    P::Hour,
                    "hour",
                    Some("M"),
                ),
                (
                    "2025-03-04T08:30",
                    "2025-03-04T08:30:00",
                    "2025-03-04T08:30:59",
                    P::Minute,
                    "minute",
                    Some("S"),
                ),
                (
                    "2025-03-04T08:30:15",
                    "2025-03-04T08:30:15",
                    "2025-03-04T08:30:15",
                    P::Second,
                    "second",
                    None,
                ),
                (
                    "2024-12-31T23",
                    "2024-12-31T23:00:00",
                    "2024-12-31T23:59:59",
                    P::Hour,
                    "hour",
                    Some("M"),
                ),
                (
                    "2025-03-04T00",
                    "2025-03-04T00:00:00",
                    "2025-03-04T00:59:59",
                    P::Hour,
                    "hour",
                    Some("M"),
                ),
            ] {
                for (rule, expected) in [(TimeRule::First, early), (TimeRule::Last, late)] {
                    let S::Value(Value::DateTime(value)) =
                        operation(datetime_kind(rule), false, None)
                            .source_value(ValueRef::Str(input))
                            .unwrap()
                    else {
                        panic!("datetime")
                    };
                    assert_eq!(value.to_string(), expected, "{input}");
                    assert_eq!(value.collected_precision(), collected, "{input}");
                    for source in [ValueRef::DateTime(value), ValueRef::Str(input)] {
                        assert_eq!(
                            precision(T::DateTime).source_value(source),
                            Ok(word(name)),
                            "{input}"
                        );
                    }
                    assert_eq!(
                        flag(FlagPart::Time).source_value(ValueRef::DateTime(value)),
                        Ok(S::Value(
                            code.map_or(Value::Missing, |code| Value::Str(code.into()))
                        )),
                        "{input}"
                    );
                    let minutes = operation(
                        K::Flag {
                            part: FlagPart::Time,
                            seconds_collected: false,
                        },
                        false,
                        None,
                    );
                    assert_eq!(
                        minutes.source_value(ValueRef::DateTime(value)),
                        Ok(S::Value(match code {
                            Some("S") | None => Value::Missing,
                            Some(code) => Value::Str(code.into()),
                        })),
                        "{input}"
                    );
                }
            }
            for invalid in [
                "2025-03-04T",
                "2025-03-04T8",
                "2025-03-04T08:",
                "2025-03-04T08:3",
                "2025-03-04T08:30:",
                "2025-03-04T24",
                "2025-03-04T08:60",
                "2025-03-04T08:30:60",
                "2025-03-04 08",
                "2025-03-04t08",
                "2025-03-04T08Z",
                "2025-03-04T08.5",
                "2025-03-04T0830",
                "2025-02-30T08",
                "2025-03-04T\u{0660}",
            ] {
                for kind in [
                    datetime_kind(TimeRule::First),
                    K::Precision {
                        target: T::DateTime,
                    },
                ] {
                    let op = operation(kind, false, Some(Value::Missing));
                    assert_eq!(
                        op.source_value(ValueRef::Str(invalid)),
                        Ok(S::Recovered { handler: "invalid" }),
                        "{invalid}"
                    );
                }
            }
        }
        /// Seam: the missing step each reading gives a missing source.
        /// Each reading of collected date text answers a missing source with
        /// missing, never a failure (TIME-13); its refusals are cases of
        /// `tests/fixtures/diagnostics/temporal.json`.
        #[test]
        fn a_missing_source_completes_to_missing_under_every_reading() {
            let date =
                completion_policy(T::Date, Some("12"), Some(D::Number("31")), None, "impute");
            for (kind, label) in [
                (K::Impute(date.unwrap()), "a date impute"),
                (datetime_kind(TimeRule::First), "a datetime impute"),
                (K::Precision { target: T::Date }, "a date precision"),
                (
                    K::Precision {
                        target: T::DateTime,
                    },
                    "a datetime precision",
                ),
            ] {
                assert_eq!(
                    operation(kind, false, None).source_value(ValueRef::Missing),
                    Ok(S::Missing),
                    "{label}"
                );
            }
        }

        /// Seam: generated 200,000-digit policy fields and the policy's shape refusal, which
        /// reports no issue.
        #[test]
        fn public_policy_admits_whole_context_before_semantic_copies_or_integer_scans() {
            use yamaa_core::{dataset::DatePolicyError as E, diagnostic::ConditionCode as C};
            let policy = |month: Option<&str>, day: D<'_>, path: &str| {
                completion_policy(T::Date, month, Some(day), None, path)
            };
            let oversized = "9".repeat(200_000);
            let E::Diagnostic(finding) =
                policy(Some(&oversized), D::Number("1"), "date").unwrap_err()
            else {
                panic!("month policy");
            };
            assert_eq!(finding.code, C::ImputationMonthRange);
            let E::Diagnostic(finding) =
                policy(Some("6"), D::Position(&oversized), "date").unwrap_err()
            else {
                panic!("day policy");
            };
            assert_eq!(finding.code, C::ImputationDayRule);
            assert!(policy(Some("6"), D::Number("1"), &oversized).is_ok());
            assert!(policy(Some("6"), D::Number("1"), &oversized[..100_000]).is_ok());
            for number in ["", "01", "+1", "-0", "1.0", "bad"] {
                assert_eq!(policy(Some(number), D::Number("1"), "date"), Err(E::Shape));
                assert_eq!(policy(Some("6"), D::Number(number), "date"), Err(E::Shape));
            }
            assert_eq!(policy(Some("6"), D::Number("1"), ""), Err(E::Shape));
            // The time rule belongs to the datetime type alone.
            assert_eq!(
                completion_policy(T::DateTime, None, None, None, "date"),
                Err(E::Shape)
            );
            assert_eq!(
                completion_policy(T::Date, None, None, Some(TimeRule::Last), "date"),
                Err(E::Shape)
            );
        }

        /// Seam: `Imputation::new`'s shape refusals and generated 100,000-character metadata.
        #[test]
        fn constructor_retains_complete_metadata_and_rejects_invalid_shapes() {
            let oversized = "x".repeat(100_000);
            let operand = |field: &str, name: &str, default: &str| {
                Some(if field == name {
                    oversized.clone()
                } else {
                    default.into()
                })
            };
            for field in [
                "source",
                "not_before",
                "not_after",
                "fallback",
                "path",
                "invalid",
            ] {
                assert!(
                    Imputation::new(
                        date_policy_kind(P::Year, D::Position("first")),
                        operand(field, "source", "SRC.D").unwrap(),
                        operand(field, "not_before", "SRC.B"),
                        operand(field, "not_after", "SRC.C"),
                        operand(field, "fallback", "SRC.E"),
                        operand(field, "path", "columns.D").unwrap(),
                        (field == "invalid").then(|| Value::Str(oversized.clone())),
                    )
                    .is_some(),
                    "{field} metadata must be retained"
                );
            }
            let precision = || K::Precision { target: T::Date };
            assert!(Imputation::new(
                precision(),
                oversized[..oversized.len() / 2].to_owned(),
                None,
                None,
                None,
                oversized[..oversized.len() / 2].to_owned(),
                None,
            )
            .is_some());
            assert!(Imputation::new(
                precision(),
                String::new(),
                None,
                None,
                None,
                "path".into(),
                None
            )
            .is_none());
            assert!(Imputation::new(
                precision(),
                "SRC.D".into(),
                None,
                None,
                None,
                String::new(),
                None
            )
            .is_none());
            for (not_before, not_after, fallback) in [
                (Some("SRC.B"), None, None),
                (None, Some("SRC.B"), None),
                (None, None, Some("SRC.B")),
            ] {
                assert!(Imputation::new(
                    precision(),
                    "SRC.D".into(),
                    not_before.map(String::from),
                    not_after.map(String::from),
                    fallback.map(String::from),
                    "path".into(),
                    None
                )
                .is_none());
                assert!(Imputation::new(
                    date_policy_kind(P::Year, D::Position("first")),
                    "SRC.D".into(),
                    not_before.map(|_| String::new()),
                    not_after.map(|_| String::new()),
                    fallback.map(|_| String::new()),
                    "path".into(),
                    None
                )
                .is_none());
            }
            for kind in [
                K::ToDate,
                K::Flag {
                    part: FlagPart::Date,
                    seconds_collected: true,
                },
            ] {
                assert!(Imputation::new(
                    kind,
                    "SRC.D".into(),
                    None,
                    None,
                    None,
                    "path".into(),
                    Some(Value::Missing)
                )
                .is_none());
            }
            // A completion that bypassed its policy is not an operation.
            for inconsistent in [
                Completion {
                    target: T::Date,
                    month: None,
                    day: Some(yamaa_core::dataset::DayRule::First),
                    minimum: P::Year,
                    time: None,
                },
                Completion {
                    target: T::Date,
                    month: None,
                    day: None,
                    minimum: P::Month,
                    time: None,
                },
                Completion {
                    target: T::DateTime,
                    month: None,
                    day: None,
                    minimum: P::Day,
                    time: None,
                },
            ] {
                assert!(Imputation::new(
                    K::Impute(inconsistent),
                    "SRC.D".into(),
                    None,
                    None,
                    None,
                    "path".into(),
                    None
                )
                .is_none());
            }
        }
    }
}
