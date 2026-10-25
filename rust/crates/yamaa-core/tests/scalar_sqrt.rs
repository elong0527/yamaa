//! Scalar contracts: sqrt, temporal, temporal arithmetic, text case, text
//! extract, text pad, text positions, text template and values. These are
//! the seams the diagnostic replay cannot reach, each named in its test's
//! first comment: the values a kernel returns, generated sizes and internal
//! refusals. What the operations refuse is `temporal.json`'s and
//! `text.json`'s.
mod scalar {
    mod sqrt {
        use yamaa_core::numeric::{sqrt, ArithmeticErrorKind, Number};

        /// Seam: the square root kernel's values and its arithmetic error, which is not
        /// an issue record.
        /// Missing normalizes before domain checking; negative present values retain diagnostics.
        #[test]
        fn square_root_domain_conditions_and_normalization() {
            for missing in [
                Number::Missing,
                Number::float(f64::NAN),
                Number::float(f64::NEG_INFINITY),
            ] {
                assert_eq!(sqrt(missing, "SQRT(A)"), Ok(Number::Missing));
            }
            for negative in [Number::Int(i64::MIN), Number::float(-f64::from_bits(1))] {
                let error = sqrt(negative, "SQRT(A)").unwrap_err();
                assert_eq!(error.kind, ArithmeticErrorKind::SqrtOfNegative);
                assert_eq!(error.expression, "SQRT(A)");
                assert_eq!(error.phase(), "derivation");
                assert_eq!(error.condition(), "sqrt_of_negative");
                assert_eq!(error.rule(), "FORMULA-15");
            }
        }

        /// Seam: square roots compared bit for bit with the standard library's.
        /// Compare square roots bit-for-bit, including sign and integer promotion boundaries.
        #[test]
        fn square_root_preserves_zero_sign_and_promotes_integers() {
            for value in [-0.0, 0.0, f64::from_bits(1), f64::MIN_POSITIVE, f64::MAX] {
                let Number::Float(actual) = sqrt(Number::float(value), "SQRT(A)").unwrap() else {
                    panic!("expected float");
                };
                assert_eq!(actual.get().to_bits(), value.sqrt().to_bits());
            }
            for value in [0, 1, (1_i64 << 53) - 1, (1_i64 << 53) + 1, i64::MAX] {
                let Number::Float(actual) = sqrt(Number::Int(value), "SQRT(A)").unwrap() else {
                    panic!("expected float");
                };
                assert_eq!(actual.get().to_bits(), (value as f64).sqrt().to_bits());
            }
        }
    }
    mod temporal {
        use yamaa_core::temporal::{Date, DateTime, Precision as DP, Precision as TP};

        /// Seam: the date parser's accepted and refused texts, values no reader
        /// returns.
        #[test]
        /// Check range endpoints and the Gregorian leap-century exception.
        fn calendar_boundaries_and_gregorian_century_rules() {
            for text in [
                "0001-01-01",
                "9999-12-31",
                "1600-02-29",
                "2000-02-29",
                "2024-02-29",
            ] {
                let date: Date = text.parse().unwrap();
                assert_eq!(date.to_string(), text);
                assert_eq!(date.collected_precision(), DP::Day);
            }
            for text in [
                "0000-01-01",
                "10000-01-01",
                "1900-02-29",
                "2100-02-29",
                "2025-02-29",
                "2025-04-31",
                "2025-00-01",
                "2025-13-01",
                "2025-01-00",
                "2025-01-32",
            ] {
                assert!(text.parse::<Date>().is_err(), "{text}");
            }
        }

        /// Seam: the date and datetime parsers' refusals, which report no issue.
        #[test]
        /// Reject noncanonical forms and non-ASCII field layouts without panicking.
        fn lexical_forms_reject_partial_dates_zones_offsets_and_unicode_digits() {
            for text in [
                "2025-01",
                "20250112",
                "2025-1-2",
                " 2025-01-12",
                "2025-01-12\n",
                "２０２５-01-12",
                "2025-01-12T14:00:00",
                "éé-01-12",
            ] {
                assert!(text.parse::<Date>().is_err(), "{text}");
            }
            for text in [
                "2025-01-12",
                "14:00:00",
                "2025-01-12 14:00:00",
                "2025-01-12t14:00:00",
                "2025-01-12T14:00:00Z",
                "2025-01-12T14:00:00+02:00",
                "2025-01-12T14:00:00.5",
                "2025-01-12T24:00",
                "2025-01-12T23:59:60",
                "2025-01-12T23:60",
                "2025-01-12T4:00",
                "2025-01-12T14:00\n",
                "2025-01-12Té:00",
                "éééééééé",
            ] {
                assert!(text.parse::<DateTime>().is_err(), "{text}");
            }
        }

        /// Seam: parsed datetimes and their canonical text.
        #[test]
        /// Confirm omitted seconds become zero and civil times have no DST gaps.
        fn minute_form_is_complete_to_second_and_dst_has_no_effect() {
            let minute: DateTime = "2025-01-12T14:00".parse().unwrap();
            assert_eq!(minute.to_string(), "2025-01-12T14:00:00");
            assert_eq!(minute.collected_precision(), TP::Second);
            for text in [
                "2025-03-09T02:30:00",
                "2025-11-02T01:30:00",
                "0001-01-01T00:00:00",
                "9999-12-31T23:59:59",
            ] {
                assert_eq!(text.parse::<DateTime>().unwrap().to_string(), text);
            }
        }

        /// Seam: collected precision, equality and ordering of temporal values.
        #[test]
        /// Preserve collected precision internally and drop it at the canonical-text boundary.
        fn precision_survives_selection_but_not_equality_or_canonical_text() {
            let parsed: Date = "2025-01-12".parse().unwrap();
            for precision in [DP::Year, DP::Month, DP::Day] {
                let date = Date::new(2025, 1, 12, precision).unwrap();
                assert_eq!(date, parsed);
                assert_eq!(date.cmp(&parsed), std::cmp::Ordering::Equal);
                assert_eq!(date.collected_precision(), precision);
                assert_eq!(
                    date.to_string()
                        .parse::<Date>()
                        .unwrap()
                        .collected_precision(),
                    DP::Day
                );
            }
            let a = DateTime::new(parsed, 14, 0, 0, TP::Day).unwrap();
            let b = DateTime::new(parsed, 14, 0, 0, TP::Second).unwrap();
            assert_eq!(a, b);
            assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
            assert_eq!(a.collected_precision(), TP::Day);
            assert_eq!(
                a.to_string()
                    .parse::<DateTime>()
                    .unwrap()
                    .collected_precision(),
                TP::Second
            );
            assert!(b < "2025-01-12T14:00:01".parse().unwrap());
        }

        /// Seam: a generated walk of one full 400-year Gregorian cycle.
        #[test]
        /// Check one complete Gregorian cycle for valid, distinct, ordered round-trip dates.
        fn a_full_gregorian_cycle_has_146097_distinct_ordered_days() {
            let mut count = 0;
            let mut previous = None;
            for year in 1600..2000 {
                for month in 1..=12 {
                    for day in 1..=31 {
                        if let Ok(date) = Date::new(year, month, day, DP::Day) {
                            assert_eq!(date.to_string().parse::<Date>(), Ok(date));
                            if let Some(previous) = previous {
                                assert!(previous < date);
                            }
                            previous = Some(date);
                            count += 1;
                        }
                    }
                }
            }
            assert_eq!(count, 146097);
        }
    }
    mod temporal_arithmetic {
        use yamaa_core::{
            dataset::{ElapsedUnit as U, Temporal, TemporalKind as K, TemporalStep},
            temporal::{Date, DateTime, Precision},
            value::Value,
        };

        fn operation(kind: K) -> Temporal {
            Temporal::new(
                kind,
                kind.fields().iter().map(|name| (*name).into()).collect(),
                "columns.RESULT.derivation.operation".into(),
            )
            .unwrap()
        }
        fn result(kind: K, values: &[Value]) -> Value {
            match operation(kind).apply_values(values).unwrap().unwrap() {
                TemporalStep::Value(value) => value,
                TemporalStep::Invalid => panic!("unexpected recovery"),
            }
        }
        fn date(text: &str) -> Value {
            Value::Date(text.parse().unwrap())
        }
        fn datetime(text: &str) -> Value {
            Value::DateTime(text.parse().unwrap())
        }
        fn text(text: &str) -> Value {
            Value::Str(text.into())
        }

        /// Seam: shifted dates and their collected precision (core outcome).
        #[test]
        fn date_shift_crosses_calendar_boundaries_and_preserves_imputation_precision() {
            for (source, days, expected) in [
                ("2030-01-31", 28, "2030-02-28"),
                ("2024-02-28", 1, "2024-02-29"),
                ("2024-03-01", -1, "2024-02-29"),
                ("2024-12-31", 1, "2025-01-01"),
                ("0001-01-01", 3_652_058, "9999-12-31"),
                ("9999-12-31", -3_652_058, "0001-01-01"),
            ] {
                assert_eq!(
                    result(K::DateShift, &[date(source), Value::Int(days)]),
                    date(expected)
                );
            }
            let source = Value::Date(Date::new(2024, 2, 1, Precision::Month).unwrap());
            let Value::Date(shifted) = result(K::DateShift, &[source, Value::Int(28)]) else {
                panic!()
            };
            assert_eq!(shifted.to_string(), "2024-02-29");
            assert_eq!(shifted.collected_precision(), Precision::Month);
        }

        /// Seam: datetime differences, values no reader returns (core outcome).
        #[test]
        fn datetime_diff_matches_pk_relative_times_and_civil_dst_boundaries() {
            for (sample, expected) in [
                ("2030-01-01T07:45:00", -0.25),
                ("2030-01-01T10:00:00", 2.0),
                ("2030-01-02T09:30:00", 25.5),
            ] {
                assert_eq!(
                    result(
                        K::DateTimeDifference { unit: U::Hour },
                        &[datetime("2030-01-01T08:00:00"), datetime(sample)]
                    ),
                    Value::float(expected)
                );
            }
            for (start, end, expected) in [
                ("2024-02-28T23:59:59", "2024-03-01T00:00:00", 86401),
                ("2024-12-31T23:59:59", "2025-01-01T00:00:00", 1),
                ("2025-03-09T01:30:00", "2025-03-09T03:30:00", 7200),
                ("0001-01-01T00:00:00", "9999-12-31T23:59:59", 315537897599),
            ] {
                assert_eq!(
                    result(
                        K::DateTimeDifference { unit: U::Second },
                        &[datetime(start), datetime(end)]
                    ),
                    Value::Int(expected)
                );
                assert_eq!(
                    result(
                        K::DateTimeDifference { unit: U::Second },
                        &[datetime(end), datetime(start)]
                    ),
                    Value::Int(-expected)
                );
            }
            for (unit, divisor) in [(U::Minute, 60.0), (U::Hour, 3600.0), (U::Day, 86400.0)] {
                assert_eq!(
                    result(
                        K::DateTimeDifference { unit },
                        &[
                            datetime("2030-01-01T00:00:00"),
                            datetime("2030-01-01T00:00:01")
                        ]
                    ),
                    Value::float(1.0 / divisor)
                );
            }
        }

        /// Seam: shifted datetimes, values no reader returns (core outcome).
        #[test]
        fn datetime_shift_scales_exact_integers_and_rounds_amount_ties_away_from_zero() {
            // 0.025 minute is exactly 1.5 seconds after binary64 scaling, a tie
            // that rounds to 2 seconds away from zero whatever the source second.
            for (source, amount, unit, expected) in [
                (
                    "2024-02-28T23:59:59",
                    Value::Int(1),
                    U::Second,
                    "2024-02-29T00:00:00",
                ),
                (
                    "2024-12-31T23:30:00",
                    Value::float(0.5),
                    U::Hour,
                    "2025-01-01T00:00:00",
                ),
                (
                    "2030-01-01T08:00:00",
                    Value::Int(24),
                    U::Hour,
                    "2030-01-02T08:00:00",
                ),
                (
                    "2030-01-01T08:00:00",
                    Value::Int(4),
                    U::Hour,
                    "2030-01-01T12:00:00",
                ),
                (
                    "2030-01-01T00:00:00",
                    Value::float(0.025),
                    U::Minute,
                    "2030-01-01T00:00:02",
                ),
                (
                    "2030-01-01T00:00:01",
                    Value::float(0.025),
                    U::Minute,
                    "2030-01-01T00:00:03",
                ),
                (
                    "2030-01-01T00:00:00",
                    Value::float(-0.025),
                    U::Minute,
                    "2029-12-31T23:59:58",
                ),
                (
                    "2030-01-01T00:00:01",
                    Value::float(-0.025),
                    U::Minute,
                    "2029-12-31T23:59:59",
                ),
                // 0.0249999999 minute is 1.499999994 seconds, 6.0e-9 below the
                // tie and inside FORMULA-16's 2^-26 interval; 0.024999999 minute is
                // 1.49999994 seconds, 6.0e-8 below and outside it.
                (
                    "2030-01-01T00:00:00",
                    Value::float(0.0249999999),
                    U::Minute,
                    "2030-01-01T00:00:02",
                ),
                (
                    "2030-01-01T00:00:00",
                    Value::float(-0.0249999999),
                    U::Minute,
                    "2029-12-31T23:59:58",
                ),
                (
                    "2030-01-01T00:00:00",
                    Value::float(0.024999999),
                    U::Minute,
                    "2030-01-01T00:00:01",
                ),
                (
                    "9999-12-31T00:00:01",
                    Value::float(0.025),
                    U::Minute,
                    "9999-12-31T00:00:03",
                ),
                (
                    "0001-01-01T00:00:00",
                    Value::Int(3652058),
                    U::Day,
                    "9999-12-31T00:00:00",
                ),
            ] {
                assert_eq!(
                    result(K::DateTimeShift { unit }, &[datetime(source), amount]),
                    datetime(expected)
                );
            }
            let date: Date = "2024-02-28".parse().unwrap();
            let source = DateTime::new(date, 0, 0, 0, Precision::Day).unwrap();
            let Value::DateTime(shifted) = result(
                K::DateTimeShift { unit: U::Day },
                &[Value::DateTime(source), Value::Int(1)],
            ) else {
                panic!()
            };
            assert_eq!(shifted.collected_precision(), Precision::Day);
        }

        /// Seam: duration values read from text (core outcome).
        #[test]
        fn duration_value_reads_signed_fractional_and_week_forms_exactly() {
            // A float in every unit: the exact seconds are rounded once to
            // binary64, so whole seconds beyond 2^53 take the nearest float.
            for (source, unit, expected) in [
                ("-PT15M", U::Hour, Value::float(-0.25)),
                ("P1DT2H3M", U::Second, Value::float(93780.0)),
                ("P2W", U::Day, Value::float(14.0)),
                ("PT0.5H", U::Second, Value::float(1800.0)),
                ("PT0,5H", U::Minute, Value::float(30.0)),
                ("P0.5D", U::Hour, Value::float(12.0)),
                ("PT0.1S", U::Second, Value::float(0.1)),
                ("-PT0S", U::Second, Value::float(-0.0)),
                (
                    "PT9007199254740993S",
                    U::Second,
                    Value::float(9007199254740992.0),
                ),
                (
                    "PT9223372036854775807S",
                    U::Second,
                    Value::float(9223372036854775808.0),
                ),
                (
                    "-PT9223372036854775808S",
                    U::Second,
                    Value::float(-9223372036854775808.0),
                ),
            ] {
                assert_eq!(
                    result(K::DurationValue { unit }, &[text(source)]),
                    expected,
                    "{source}"
                );
            }
        }

        /// Seam: duration text written for each amount (core outcome).
        #[test]
        fn duration_text_writes_one_canonical_text_for_each_amount() {
            // TIME-21: days, then hours, minutes and seconds, each zero
            // component left out and a fraction only on the seconds. An int
            // scales exactly; a float scales once as binary64 and splits the
            // shortest digits of its seconds (VALUE-8).
            let tiny = format!("PT0.{}5S", "0".repeat(323));
            for (amount, unit, expected) in [
                (Value::Int(120), U::Minute, "PT2H"),
                (Value::Int(90), U::Minute, "PT1H30M"),
                (Value::Int(45), U::Minute, "PT45M"),
                (Value::Int(0), U::Minute, "PT0S"),
                (Value::Int(-90), U::Minute, "-PT1H30M"),
                (Value::Int(26), U::Hour, "P1DT2H"),
                (Value::Int(14), U::Day, "P14D"),
                (Value::Int(86400), U::Second, "P1D"),
                (Value::Int(3601), U::Second, "PT1H1S"),
                (Value::Int(90061), U::Second, "P1DT1H1M1S"),
                (Value::Int(93780), U::Second, "P1DT2H3M"),
                (Value::Int(i64::MAX), U::Day, "P9223372036854775807D"),
                (Value::Int(i64::MIN), U::Day, "-P9223372036854775808D"),
                (
                    Value::Int(i64::MIN),
                    U::Second,
                    "-P106751991167300DT15H30M8S",
                ),
                (Value::float(-0.25), U::Hour, "-PT15M"),
                (Value::float(1.75), U::Hour, "PT1H45M"),
                (Value::float(1.5), U::Minute, "PT1M30S"),
                (Value::float(90.5), U::Second, "PT1M30.5S"),
                (Value::float(0.5), U::Second, "PT0.5S"),
                (Value::float(0.1), U::Second, "PT0.1S"),
                // 0.1 minute is 6.0000000000000003 seconds exactly, within
                // half a unit in the last place of 6.
                (Value::float(0.1), U::Minute, "PT6S"),
                // One third of an hour as a float is 1199.99999999999993
                // seconds exactly, within half a unit in the last place of
                // 1200.
                (Value::float(1.0 / 3.0), U::Hour, "PT20M"),
                (Value::float(-0.0), U::Second, "PT0S"),
                (Value::float(0.0), U::Day, "PT0S"),
                (Value::float(1e20), U::Second, "P1157407407407407DT9H46M40S"),
                (Value::float(5e-324), U::Second, tiny.as_str()),
            ] {
                assert_eq!(
                    result(K::DurationText { unit }, std::slice::from_ref(&amount)),
                    text(expected),
                    "{amount:?} {unit:?}"
                );
            }
        }

        /// Seam: a round trip of written duration text, compared with its own amount.
        #[test]
        fn duration_text_read_back_returns_every_whole_second_amount() {
            // TIME-21: an amount that is a whole number of seconds, at most
            // 2^53 of them, reads back in its own unit as itself.
            for (amount, unit) in [
                (Value::Int(120), U::Minute),
                (Value::Int(-90), U::Minute),
                (Value::Int(0), U::Second),
                (Value::Int(1 << 53), U::Second),
                (Value::Int(-(1 << 53)), U::Second),
                (Value::Int(104249991374), U::Day),
                (Value::Int(26), U::Hour),
                (Value::float(1.5), U::Minute),
                (Value::float(-0.25), U::Hour),
                (Value::float(2.5), U::Day),
                (Value::float(9007199254740992.0), U::Second),
            ] {
                let written = result(K::DurationText { unit }, std::slice::from_ref(&amount));
                let read = result(K::DurationValue { unit }, &[written]);
                let expected = match &amount {
                    Value::Int(whole) => Value::float(*whole as f64),
                    other => other.clone(),
                };
                assert_eq!(read, expected, "{amount:?} {unit:?}");
            }
        }

        /// Seam: the recovery step a declared `invalid` gives, which is no
        /// issue; without it, each text fails as a `temporal.json` case
        /// `duration_value_fails_on_...`.
        #[test]
        fn a_declared_invalid_answers_every_invalid_duration_text() {
            // A duration whose float is not finite is outside the numeric range.
            let infinite = format!("PT1{}S", "0".repeat(309));
            for source in [
                "",
                "P",
                "PT",
                "P1DT",
                "P1M",
                "P1Y",
                "P1W2D",
                "P1DT2W",
                "PT1M1H",
                "PT1H1H",
                "PT1.0H2M",
                "P0.5DT1H",
                "+PT1S",
                " PT1S",
                "PT1S ",
                "PT-1S",
                "PT.5S",
                "PT1.S",
                "PT1e3S",
                "pt1s",
                infinite.as_str(),
            ] {
                assert_eq!(
                    operation(K::DurationValue { unit: U::Second })
                        .with_invalid(Value::Missing)
                        .unwrap()
                        .apply_values(&[text(source)]),
                    Some(Ok(TemporalStep::Invalid)),
                    "{source}"
                );
            }
        }

        /// Seam: the missing result of all-missing operands and the arity refusal
        /// (`None`), neither an issue; the overflow and type refusals are
        /// `temporal.json` cases.
        #[test]
        fn all_missing_operands_give_missing_and_no_operands_give_no_step() {
            for kind in [
                K::DateShift,
                K::DateTimeDifference { unit: U::Hour },
                K::DateTimeShift { unit: U::Second },
                K::DurationValue { unit: U::Second },
                K::DurationText { unit: U::Minute },
            ] {
                assert_eq!(
                    result(kind, &vec![Value::Missing; kind.fields().len()]),
                    Value::Missing
                );
                assert_eq!(operation(kind).apply_values(&[]), None);
            }
        }

        /// Seam: a generated 200,000-character handler, an operand index past the
        /// operands and the handler shape refusal.
        #[test]
        fn duration_handlers_remain_complete_and_operand_indexes_are_validated() {
            use yamaa_core::dataset::InvalidTemporal;
            let duration = operation(K::DurationValue { unit: U::Hour });
            assert_eq!(duration.validate_operand(1, &Value::Missing), None);
            let handler = Value::Str("x".repeat(200_000));
            assert_eq!(
                duration
                    .with_invalid(handler.clone())
                    .unwrap()
                    .invalid_handler(),
                Some(&handler)
            );
            assert_eq!(
                operation(K::DateShift).with_invalid(Value::Missing),
                Err(InvalidTemporal::Shape)
            );
        }
    }
    mod text_case {
        use yamaa_core::text_case::{transform, Case, Delimiters};

        /// Seam: already data, the application's shared `text_case_reference.json` rows,
        /// whose results are text values.
        /// The proper-case rows of the application's shared truth (TEXT-5), which
        /// both crates read, so each proves the same authored rows.
        #[test]
        fn proper_raises_the_first_scalar_and_each_scalar_after_a_delimiter() {
            let reference = crate::support::diagnostic::json::read(
                include_str!("../../yamaa-application/tests/fixtures/text_case_reference.json"),
                "text_case_reference.json",
            );
            let rows = reference.field("proper").items();
            assert_eq!(rows.len(), 23, "the authored proper-case rows");
            for row in rows {
                let row = row.closed(&["source", "delimiters", "expected"]);
                let source = row.field("source").text();
                let list = row.get("delimiters").map(|list| list.text());
                let delimiters = list
                    .map_or(Some(Delimiters::PROPCASE), Delimiters::new)
                    .expect("a row declares an admitted list");
                let actual = transform(source, Case::Proper(delimiters));
                assert_eq!(
                    (actual.as_str(), actual.len()),
                    (row.field("expected").text(), source.len()),
                    "{source:?} with {list:?}"
                );
            }
        }

        /// Seam: the delimiter set a declaration admits, a value no reader returns.
        /// Without a list, `proper` takes SAS PROPCASE's six delimiters; a
        /// list names a set only when it holds ASCII scalars, each once, from
        /// `U+0000` to `U+007F` (TEXT-5).
        #[test]
        fn proper_takes_sas_six_delimiters_or_a_list_of_distinct_ascii_scalars() {
            assert_eq!(
                Delimiters::new(" /-(.\t"),
                Some(Delimiters::PROPCASE),
                "the blank, slash, hyphen, open parenthesis, period and tab"
            );
            for (list, admitted, label) in [
                ("", false, "an empty list"),
                ("-/-", false, "a repeated scalar"),
                ("-\u{80}", false, "the first scalar outside ASCII"),
                ("\0", true, "the first ASCII scalar"),
                ("\u{7f}", true, "the last ASCII scalar"),
            ] {
                assert_eq!(Delimiters::new(list).is_some(), admitted, "{label}");
            }
        }

        /// Seam: cased text values (core outcome).
        #[test]
        fn normative_ascii_modes_preserve_foreign_scalars_and_first_scalar_boundaries() {
            for (source, mode, expected) in [
                ("FOLLOW-UP", Case::Sentence, "Follow-up"),
                ("FOLLOW-UP", Case::Title, "Follow-Up"),
                (" END OF TREATMENT", Case::Sentence, " end of treatment"),
                (" END OF TREATMENT", Case::Title, " End Of Treatment"),
                ("abc_123DEF", Case::Title, "Abc_123Def"),
                (
                    "\u{00df}\u{0130}\u{00e9}\u{1f642}AbC",
                    Case::Upper,
                    "\u{00df}\u{0130}\u{00e9}\u{1f642}ABC",
                ),
                (
                    "\u{00df}\u{0130}\u{00e9}\u{1f642}AbC",
                    Case::Lower,
                    "\u{00df}\u{0130}\u{00e9}\u{1f642}abc",
                ),
                ("\u{00e9}ABC", Case::Sentence, "\u{00e9}abc"),
                ("abc\u{00e9}DEF", Case::Title, "Abc\u{00e9}Def"),
                ("a\0BC", Case::Title, "A\0Bc"),
                ("", Case::Upper, ""),
            ] {
                let actual = transform(source, mode);
                assert_eq!(actual, expected);
                assert_eq!(actual.chars().count(), source.chars().count());
                assert_eq!(actual.len(), source.len());
            }
        }

        /// Seam: cased text values (core outcome).
        #[test]
        fn original_ascii_casing_truth_preserves_range_endpoints_and_unicode_word_boundaries() {
            // Retain the original Python expression tests' inclusive-range mutation
            // controls and Unicode cases without consulting a host casing routine.
            for (source, mode, expected) in [
                ("abcZ", Case::Upper, "ABCZ"),
                ("ABCz", Case::Lower, "abcz"),
                ("a1!_", Case::Upper, "A1!_"),
                ("A1!_", Case::Lower, "a1!_"),
                ("z", Case::Upper, "Z"),
                (
                    "abcdefghijklmnopqrstuvwxyz",
                    Case::Upper,
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                ),
                ("Z", Case::Lower, "z"),
                (
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                    Case::Lower,
                    "abcdefghijklmnopqrstuvwxyz",
                ),
                ("WEEK 8", Case::Sentence, "Week 8"),
                ("WeEk 8", Case::Sentence, "Week 8"),
                ("8-WEEK", Case::Sentence, "8-week"),
                ("a", Case::Sentence, "A"),
                (
                    "\u{00df}TRA\u{00df}e",
                    Case::Sentence,
                    "\u{00df}tra\u{00df}e",
                ),
                ("\u{0131}ABC", Case::Sentence, "\u{0131}abc"),
                ("\u{0130}ABC", Case::Sentence, "\u{0130}abc"),
                ("\u{212a}ELVIN", Case::Sentence, "\u{212a}elvin"),
                ("END OF TREATMENT", Case::Title, "End Of Treatment"),
                ("WEEK 8", Case::Title, "Week 8"),
                ("follow-up", Case::Title, "Follow-Up"),
                ("8-WEEK", Case::Title, "8-Week"),
                ("\u{00e9}COLE", Case::Title, "\u{00e9}Cole"),
                (
                    "\u{00e9}cole \u{00c9}COLE",
                    Case::Title,
                    "\u{00e9}Cole \u{00c9}Cole",
                ),
                ("Stra\u{00df}e", Case::Upper, "STRA\u{00df}E"),
                ("Stra\u{00df}e", Case::Lower, "stra\u{00df}e"),
                ("apple", Case::Title, "Apple"),
                ("pizza", Case::Title, "Pizza"),
                ("ABC", Case::Title, "Abc"),
                ("aZ", Case::Title, "Az"),
            ] {
                let actual = transform(source, mode);
                assert_eq!(actual, expected, "{source:?}, {mode:?}");
                assert_eq!(actual.chars().count(), source.chars().count());
            }
            for source in [
                "\u{00df}", "\u{0130}", "\u{0131}", "\u{212a}", "\u{00e9}", "\u{00c9}",
            ] {
                for mode in [
                    Case::Upper,
                    Case::Lower,
                    Case::Sentence,
                    Case::Title,
                    Case::Proper(Delimiters::PROPCASE),
                ] {
                    assert_eq!(transform(source, mode), source);
                }
            }
            for mode in [
                Case::Sentence,
                Case::Title,
                Case::Proper(Delimiters::PROPCASE),
            ] {
                assert_eq!(transform("", mode), "");
            }
        }

        /// Seam: a declaration's steps and its missing handler, values no reader
        /// returns.
        #[test]
        fn owned_declarations_preserve_missing_literal_presence_and_semantic_truth() {
            use yamaa_core::{
                table::ValueRef,
                text_case::{Step, StringCase},
                value::Value,
            };
            // An omitted handler answers a missing source with missing.
            let omitted =
                StringCase::new("SRC.V".into(), "case".into(), Case::Lower, None).unwrap();
            assert_eq!(omitted.apply(ValueRef::Missing), Ok(Step::Recovered));
            assert_eq!(omitted.missing_substitute(), Value::Missing);
            for handler in [Value::Missing, Value::Str("MiXeD".into()), Value::Int(7)] {
                let operation = StringCase::new(
                    "SRC.V".into(),
                    "case".into(),
                    Case::Lower,
                    Some(handler.clone()),
                )
                .unwrap();
                assert_eq!(operation.apply(ValueRef::Missing), Ok(Step::Recovered));
                assert_eq!(operation.missing(), Some(&handler));
                assert_eq!(
                    operation.apply(ValueRef::Str("ABC")),
                    Ok(Step::Value(Value::Str("abc".into())))
                );
            }
        }
        /// Seam: the constructor's shape refusals and generated 100,000-character
        /// metadata.
        #[test]
        fn owned_metadata_retains_complete_values_and_rejects_only_invalid_shapes() {
            use yamaa_core::{text_case::StringCase, value::Value};
            assert!(StringCase::new(String::new(), "path".into(), Case::Upper, None).is_none());
            assert!(StringCase::new("SRC.V".into(), String::new(), Case::Upper, None).is_none());
            let oversized = "x".repeat(100_000);
            assert!(StringCase::new(oversized.clone(), "path".into(), Case::Upper, None).is_some());
            assert!(
                StringCase::new("SRC.V".into(), oversized.clone(), Case::Upper, None).is_some()
            );
            assert!(StringCase::new(
                "SRC.V".into(),
                "path".into(),
                Case::Upper,
                Some(Value::Str(oversized.clone()))
            )
            .is_some());
            assert!(StringCase::new(
                oversized[..oversized.len() / 2].into(),
                oversized[..oversized.len() / 2].into(),
                Case::Upper,
                None
            )
            .is_some());
        }
    }
    mod text_extract {
        use yamaa_core::{
            table::ValueRef,
            text_extract::{AdmissionError, Step, StringExtract},
            value::Value,
        };
        fn operation(
            pattern: &str,
            group: &str,
            missing: Option<Value>,
            no_match: Option<Value>,
        ) -> StringExtract {
            StringExtract::new(
                "SRC.TEXT".into(),
                "columns.TEXT.derivation.str_extract".into(),
                pattern.into(),
                group.into(),
                missing,
                no_match,
            )
            .unwrap()
        }
        /// Seam: the captures a search returns (core outcome).
        #[test]
        fn shared_search_capture_truth_preserves_absence_empty_unicode_and_nul() {
            for (pattern, group, subject, expected) in [
                ("(a)?b", "1", "b", Step::Capture(None)),
                ("()b", "1", "b", Step::Capture(Some(""))),
                ("()", "0", "abc", Step::Capture(Some(""))),
                ("a", "-0", "abc", Step::Capture(Some("a"))),
                ("(a|ab)", "1", "zab", Step::Capture(Some("a"))),
                ("(a|b)+", "1", "ab", Step::Capture(Some("b"))),
                ("x(.)", "1", "x\u{1f642}", Step::Capture(Some("\u{1f642}"))),
                ("x(.)", "1", "x\0", Step::Capture(Some("\0"))),
                ("(a)", "1", "xyz", Step::NoMatch("xyz")),
            ] {
                assert_eq!(
                    operation(pattern, group, None, None).apply(ValueRef::Str(subject)),
                    Ok(expected)
                );
            }
        }
        /// Seam: the recovery steps and handler values of declared handlers.
        #[test]
        fn omitted_handlers_null_literals_and_nonconversion_are_distinct() {
            for handler in [Value::Missing, Value::Str("12".into()), Value::Int(12)] {
                let declared = operation("(a)", "1", Some(handler.clone()), Some(handler.clone()));
                assert_eq!(
                    declared.apply(ValueRef::Missing),
                    Ok(Step::Recovered("missing"))
                );
                assert_eq!(
                    declared.apply(ValueRef::Str("x")),
                    Ok(Step::Recovered("no_match"))
                );
                assert_eq!(declared.handler("missing"), Some(&handler));
                assert_eq!(
                    declared.apply(ValueRef::Str("a")),
                    Ok(Step::Capture(Some("a")))
                );
            }
            // An omitted `missing` still answers, with missing (COLUMN-10).
            let omitted = operation("()", "0", None, None);
            assert_eq!(
                omitted.apply(ValueRef::Missing),
                Ok(Step::Recovered("missing"))
            );
            assert_eq!(omitted.substitute("missing"), Value::Missing);
            assert_eq!(omitted.failure_diagnostic(Step::Recovered("missing")), None);
        }
        /// Seam: a generated 500,000-scalar subject.
        #[test]
        fn long_subjects_are_searched_to_completion_before_no_match() {
            // Half a million scalars, which the retired step budget refused.
            let op = operation("z", "0", None, Some(Value::Missing));
            assert_eq!(
                op.apply(ValueRef::Str(&"a".repeat(500_000))),
                Ok(Step::Recovered("no_match"))
            );
            assert_eq!(op.apply(ValueRef::Str("z")), Ok(Step::Capture(Some("z"))));
            assert_eq!(
                op.apply(ValueRef::Str("x")),
                Ok(Step::Recovered("no_match"))
            );
        }
        /// Seam: the constructor's shape refusals and generated 100,000-character
        /// patterns, handlers and subjects.
        #[test]
        fn constructor_rejects_shapes_and_keeps_complete_semantic_context() {
            assert_eq!(
                StringExtract::new("".into(), "p".into(), "[".into(), "0".into(), None, None),
                Err(AdmissionError::Shape)
            );
            assert_eq!(
                StringExtract::new("s".into(), "p".into(), "[".into(), "+1".into(), None, None),
                Err(AdmissionError::Shape)
            );
            let invalid = StringExtract::new(
                "s".into(),
                "p".into(),
                "[".repeat(100_000),
                "0".into(),
                None,
                None,
            )
            .unwrap();
            assert!(invalid.declaration_diagnostic().is_some());
            let handler = Value::Str("x".repeat(100_000));
            let op = StringExtract::new(
                "s".into(),
                "p".into(),
                "a".into(),
                "0".into(),
                Some(handler.clone()),
                None,
            )
            .unwrap();
            assert_eq!(op.handler("missing"), Some(&handler));
            let subject = "x".repeat(100_000);
            let finding = op.failure_diagnostic(Step::NoMatch(&subject)).unwrap();
            assert_eq!(
                finding.context["value"],
                yamaa_core::json::Value::from(subject)
            );
        }
    }
    mod text_pad {
        use yamaa_core::{
            temporal::{Date, DateTime, Precision},
            text_pad::{DeclarationError, OutputError, StringPad},
            value::{FiniteFloat, Value},
        };
        fn pad(value: &Value, width: &str) -> Result<Value, OutputError> {
            StringPad::new("SRC.X", "columns.Y.derivation.str_pad", width)
                .unwrap()
                .prepare_text(value)
                .unwrap()
                .render()
        }
        /// Seam: padded text values (core outcome).
        #[test]
        fn unicode_characters_nul_empty_and_no_truncation() {
            for (input, width, expected) in [
                ("\u{e9}\u{1f980}", "4", "  \u{e9}\u{1f980}"),
                ("e\u{301}", "4", "  e\u{301}"),
                ("a\0b", "5", "  a\0b"),
                ("", "3", "   "),
                ("longer", "2", "longer"),
            ] {
                assert_eq!(
                    pad(&Value::Str(input.into()), width).unwrap(),
                    Value::Str(expected.into())
                );
            }
        }
        /// Seam: padded text values (core outcome).
        #[test]
        fn canonical_primitives_preserve_exact_integer_and_zero() {
            for (input, width, expected) in [
                (Value::Int(i64::MIN), "22", "  -9223372036854775808"),
                (
                    Value::Int(9_007_199_254_740_993),
                    "18",
                    "  9007199254740993",
                ),
                (Value::Float(FiniteFloat::new(-0.0).unwrap()), "4", "  -0"),
                (Value::float(1.25), "6", "  1.25"),
            ] {
                assert_eq!(pad(&input, width).unwrap(), Value::Str(expected.into()));
            }
        }
        /// Seam: padded text values (core outcome).
        #[test]
        fn temporal_text_reuses_canonical_conversion_without_precision_suffixes() {
            let date = Date::new(2025, 7, 19, Precision::Month).unwrap();
            let time = DateTime::new(date, 12, 34, 56, Precision::Day).unwrap();
            assert_eq!(
                pad(&Value::Date(date), "12").unwrap(),
                Value::Str("  2025-07-19".into())
            );
            assert_eq!(
                pad(&Value::DateTime(time), "21").unwrap(),
                Value::Str("  2025-07-19T12:34:56".into())
            );
        }
        /// Seam: a padded missing value and the output size refusal, which is not an
        /// issue record.
        #[test]
        fn missing_bypasses_positive_mathematical_width_without_output_or_handler() {
            assert_eq!(
                pad(&Value::Missing, &"9".repeat(81)).unwrap(),
                Value::Missing
            );
            assert_eq!(
                pad(&Value::Str("".into()), &"9".repeat(81)),
                Err(OutputError { required: None })
            );
        }
        /// Seam: a generated 100,000-character value padded unchanged.
        #[test]
        fn complete_authored_text_and_unicode_padding_have_no_copy_quota() {
            assert_eq!(
                pad(&Value::Str("🦀".into()), "4").unwrap(),
                Value::Str("   🦀".into())
            );
            let value = Value::Str("x".repeat(100_000));
            assert_eq!(pad(&value, "1").unwrap(), value);
        }
        /// Seam: the width shape refusal, which reports no issue.
        #[test]
        fn width_spelling_shape_is_independent_of_metadata_size() {
            assert_eq!(
                StringPad::new("X", "p", &"!".repeat(100_000)),
                Err(DeclarationError::Shape)
            );
            for width in ["", "+1", "01", "1.0", "\u{661}"] {
                assert_eq!(
                    StringPad::new("X", "p", width),
                    Err(DeclarationError::Shape)
                );
            }
        }
        /// Seam: the conversion error's cause and target, which is not an issue record.
        #[test]
        fn bool_preserves_core_conversion_failure_instead_of_coercion() {
            let operation = StringPad::new("X", "p", "4").unwrap();
            let error = operation.prepare_text(&Value::Bool(true)).unwrap_err();
            assert_eq!(error.condition(), "conversion_failed");
            assert_eq!(error.rule(), "VALUE-3");
            assert_eq!(error.target, yamaa_core::value::ColumnType::Str);
        }

        /// Seam: a shape refusal reports no issue, and a width refusal keeps a
        /// generated 100,000-character path; each width below one is a `text.json` case
        /// `str_pad_refuses_...`.
        #[test]
        fn a_shape_refusal_reports_no_issue_and_a_width_refusal_keeps_a_long_path() {
            assert!(DeclarationError::Shape.diagnostic("p").is_none());
            assert!(DeclarationError::Width
                .diagnostic(&"p".repeat(100_000))
                .is_some());
        }

        /// Seam: a generated document of 32 columns with 100,000-character source
        /// names.
        #[test]
        fn static_check_retains_first_bad_width_with_complete_long_source_names() {
            use yamaa_core::{
                domain,
                front_end::v1::spec as front_end,
                schema::{Document, DocumentNode as N},
            };
            fn text(nodes: &mut Vec<N>, value: &str) -> usize {
                let id = nodes.len();
                nodes.push(N::Text(value.into()));
                id
            }
            fn map(nodes: &mut Vec<N>, fields: Vec<(&str, usize)>) -> usize {
                let fields = fields
                    .into_iter()
                    .map(|(name, value)| (text(nodes, name), value))
                    .collect();
                let id = nodes.len();
                nodes.push(N::Mapping(fields));
                id
            }
            fn list(nodes: &mut Vec<N>, items: Vec<usize>) -> usize {
                let id = nodes.len();
                nodes.push(N::Sequence(items));
                id
            }
            let mut nodes = Vec::new();
            let mut columns = Vec::new();
            let mut names = Vec::new();
            for index in 0..32 {
                let written_name = format!("C{index}");
                let name = text(&mut nodes, &written_name);
                names.push(written_name);
                let source = text(&mut nodes, &format!("SRC.{}", "A".repeat(100_000)));
                let width = nodes.len();
                nodes.push(N::Integer(if index == 0 { "0" } else { "4" }.into()));
                let kind = text(&mut nodes, "str");
                let payload = map(&mut nodes, vec![("source", source), ("width", width)]);
                let value = map(&mut nodes, vec![("str_pad", payload)]);
                let derivation = map(&mut nodes, vec![("value", value)]);
                columns.push(map(
                    &mut nodes,
                    vec![("name", name), ("type", kind), ("derivation", derivation)],
                ));
            }
            let key = text(&mut nodes, "C0");
            let keys = list(&mut nodes, vec![key]);
            let columns = list(&mut nodes, columns);
            let names = names.iter().map(|name| text(&mut nodes, name)).collect();
            let output_names = list(&mut nodes, names);
            let output_path = text(&mut nodes, "output.csv");
            let output = map(
                &mut nodes,
                vec![("path", output_path), ("columns", output_names)],
            );
            let input_path = text(&mut nodes, "absent.csv");
            let input_item = map(&mut nodes, vec![("path", input_path)]);
            let input = map(&mut nodes, vec![("SRC", input_item)]);
            let version = text(&mut nodes, "1.0");
            let file_type = text(&mut nodes, "spec");
            let domain = text(&mut nodes, "TEST");
            let root = map(
                &mut nodes,
                vec![
                    ("yamaa", version),
                    ("type", file_type),
                    ("domain", domain),
                    ("keys", keys),
                    ("columns", columns),
                    ("output", output),
                    ("input", input),
                ],
            );
            let raw = Document::new(nodes, root).unwrap();
            let tree = front_end::lower(&raw, None).into_spec();
            let checked = domain::check_tree_linked(tree, &raw, None, Default::default()).unwrap();
            let findings = checked.check_diagnostics();
            assert_eq!(findings.len(), 1);
            let finding = &findings[0];
            assert_eq!(finding.definition().condition, "invalid_field_type");
            assert_eq!(finding.definition().rule, Some("TEXT-2"));
            assert_eq!(finding.spec_paths, ["columns.C0.derivation.str_pad.width"]);
        }
    }
    mod text_position {
        //! TEXT-13 and TEXT-14: pieces and lengths by Unicode scalar position.
        use yamaa_core::{
            table::ValueRef,
            text_position::{DeclarationError, Step, StringLength, StringSubstr},
            value::Value,
        };

        const BEYOND: &str = "100000000000000000000000";

        fn substr(start: &str, length: Option<&str>) -> Result<StringSubstr, DeclarationError> {
            StringSubstr::new(
                "SRC.T".into(),
                "columns.T.derivation.str_substr".into(),
                start,
                length,
                None,
            )
        }

        /// Seam: the text pieces `str_substr` takes (core outcome).
        #[test]
        fn a_piece_takes_scalars_from_a_start_counted_from_one() {
            for (label, text, start, length, expected) in [
                ("the first two", "ABCDEFG", "1", Some("2"), Some("AB")),
                ("every one to the end", "ABCDEFG", "3", None, Some("CDEFG")),
                (
                    "a short tail keeps those left",
                    "ABCDEFG",
                    "6",
                    Some("5"),
                    Some("FG"),
                ),
                ("the last scalar", "ABCDEFG", "7", Some("1"), Some("G")),
                ("a start past the end", "ABCDEFG", "8", Some("1"), None),
                ("any start on the empty string", "", "1", None, None),
                (
                    "scalars, not bytes",
                    "\u{e9}\u{1f980}xyz",
                    "2",
                    Some("2"),
                    Some("\u{1f980}x"),
                ),
                (
                    "a start past five scalars",
                    "\u{e9}\u{1f980}xyz",
                    "6",
                    None,
                    None,
                ),
                (
                    "a length beyond any integer",
                    "ABC",
                    "2",
                    Some(BEYOND),
                    Some("BC"),
                ),
                ("a start beyond any integer", "ABC", BEYOND, None, None),
            ] {
                let expected =
                    expected.map_or(Value::Missing, |piece: &str| Value::Str(piece.into()));
                assert_eq!(
                    substr(start, length).unwrap().apply(ValueRef::Str(text)),
                    Ok(Step::Value(expected)),
                    "{label}"
                );
            }
        }

        /// Seam: the lengths `str_length` counts (core outcome).
        #[test]
        fn a_length_counts_scalars_not_bytes() {
            let operation = StringLength::new(
                "SRC.T".into(),
                "columns.N.derivation.str_length".into(),
                None,
            )
            .unwrap();
            for (label, text, expected) in [
                ("ASCII", "ABC", 3),
                ("the empty string", "", 0),
                ("a two-byte and a four-byte scalar", "\u{e9}\u{1f980}", 2),
                ("a line break", "A\nB", 3),
            ] {
                assert_eq!(
                    operation.apply(ValueRef::Str(text)),
                    Ok(Step::Value(Value::Int(expected))),
                    "{label}"
                );
            }
        }

        /// Seam: the recovery step and the substitute a missing source takes.
        #[test]
        fn a_missing_source_is_answered_by_the_declared_missing_or_missing() {
            for handler in [None, Some(Value::Str("NONE".into())), Some(Value::Int(-1))] {
                let substitute = handler.clone().unwrap_or(Value::Missing);
                let piece =
                    StringSubstr::new("SRC.T".into(), "p".into(), "1", None, handler.clone())
                        .unwrap();
                let length =
                    StringLength::new("SRC.T".into(), "p".into(), handler.clone()).unwrap();
                assert_eq!(
                    (
                        piece.apply(ValueRef::Missing),
                        piece.missing_substitute(),
                        length.apply(ValueRef::Missing),
                        length.missing_substitute(),
                    ),
                    (
                        Ok(Step::Recovered),
                        substitute.clone(),
                        Ok(Step::Recovered),
                        substitute,
                    ),
                    "{handler:?}"
                );
            }
        }

        /// Seam: the constructors' shape refusals, which report no issue; a start or
        /// length below one is a `text.json` case `str_substr_refuses_...`.
        #[test]
        fn only_what_lowering_never_writes_is_a_shape_failure() {
            for (label, source, path, start, length) in [
                ("no source", "", "p", "1", None),
                ("no path", "SRC.T", "", "1", None),
                ("a start that is not an integer", "SRC.T", "p", "1.5", None),
                (
                    "a length that is not an integer",
                    "SRC.T",
                    "p",
                    "1",
                    Some("x"),
                ),
            ] {
                assert_eq!(
                    StringSubstr::new(source.into(), path.into(), start, length, None),
                    Err(DeclarationError::Shape),
                    "{label}"
                );
            }
            for (source, path) in [("", "p"), ("SRC.T", "")] {
                assert_eq!(
                    StringLength::new(source.into(), path.into(), None),
                    None,
                    "{source:?} {path:?}"
                );
            }
        }
    }
    mod text_template {
        use yamaa_core::{
            expression::SourceSpan,
            text_template::{AdmissionError, MaterializeError, Part, StringTemplate},
            value::Value,
        };
        /// Seam: the syntax failure span a template keeps, read through its accessor.
        #[test]
        fn deferred_syntax_refusals_retain_original_utf8_spans() {
            for (text, start, end) in [("é{", 2, 3), ("é}", 2, 3), ("é{A + B}", 2, 9)] {
                let parsed = StringTemplate::new(text.into(), "path".into(), None).unwrap();
                assert_eq!(
                    parsed.syntax_failure_span(),
                    Some(SourceSpan { start, end })
                );
                assert!(parsed.declaration_diagnostic().is_some());
                assert!(parsed.parts().is_empty());
                assert!(parsed.identifiers().is_empty());
            }
            let parsed = StringTemplate::new("é{{}}{A}".into(), "path".into(), None).unwrap();
            assert_eq!(parsed.syntax_failure_span(), None);
            assert!(parsed.declaration_diagnostic().is_none());
        }
        /// Seam: the identifiers and the text a template writes (core outcome).
        #[test]
        fn literal_escapes_unique_bindings_and_ordered_occurrences_are_exact() {
            let op = StringTemplate::new(
                "{{{A}}}:{A}/{B}".into(),
                "columns.X.str_template".into(),
                None,
            )
            .unwrap();
            assert_eq!(op.identifiers(), ["A", "B"]);
            assert_eq!(
                op.parts()
                    .iter()
                    .filter_map(|p| if let Part::Placeholder(i) = p {
                        Some(*i)
                    } else {
                        None
                    })
                    .collect::<Vec<_>>(),
                [0, 0, 1]
            );
            assert_eq!(
                op.materialize(&[
                    Value::Str("first".into()),
                    Value::Str("second".into()),
                    Value::Str("third".into())
                ])
                .unwrap(),
                "{first}:second/third"
            );
            assert_eq!(
                StringTemplate::new("".into(), "path".into(), None)
                    .unwrap()
                    .materialize(&[])
                    .unwrap(),
                ""
            );
        }
        /// Seam: the text a template writes and its size.
        #[test]
        fn unicode_scalars_empty_strings_and_nul_have_no_implicit_formatting() {
            let op = StringTemplate::new("\u{03a9}:{A}/{A}".into(), "path".into(), None).unwrap();
            let values = [
                Value::Str("e\u{0301}\u{1f642}\0".into()),
                Value::Str("".into()),
            ];
            let expected = "\u{03a9}:e\u{0301}\u{1f642}\0/";
            assert_eq!(op.output_bytes(&values).unwrap(), expected.len());
            assert_eq!(op.materialize(&values).unwrap(), expected);
            assert_eq!(
                op.materialize(&[Value::Int(7), Value::Str("".into())]),
                Err(MaterializeError::Shape)
            );
            assert_eq!(op.materialize(&[]), Err(MaterializeError::Shape));
            assert!(StringTemplate::new("{A:>10}".into(), "path".into(), None)
                .unwrap()
                .declaration_diagnostic()
                .is_some());
        }
        /// Seam: the constructor's shape refusal and generated 100,000-character
        /// metadata.
        #[test]
        fn public_constructor_preserves_complete_metadata_and_path_occurrences() {
            assert_eq!(
                StringTemplate::new("".into(), "".into(), None),
                Err(AdmissionError::Shape)
            );
            let literal = "x".repeat(100_000);
            assert_eq!(
                StringTemplate::new(literal.clone(), "path".into(), None)
                    .unwrap()
                    .materialize(&[])
                    .unwrap(),
                literal
            );
            let repeated = StringTemplate::new("{A}".repeat(1000), "p".repeat(1000), None).unwrap();
            assert_eq!(repeated.parts().len(), 1000);
            assert_eq!(repeated.path().len(), 1000);
            let handler = Value::Str("x".repeat(100_000));
            assert_eq!(
                StringTemplate::new("".into(), "path".into(), Some(handler.clone()))
                    .unwrap()
                    .missing(),
                Some(&handler)
            );
            for value in [Value::Missing, Value::Int(7), Value::Str("MiXeD".into())] {
                let op =
                    StringTemplate::new("{A}".into(), "path".into(), Some(value.clone())).unwrap();
                assert_eq!(op.missing(), Some(&value));
            }
        }
        /// Seam: a generated template of 7,000 names.
        #[test]
        fn many_unique_names_retain_first_occurrence_indexes_without_a_work_quota() {
            let text = (0..7000)
                .map(|index| format!("{{N{index}}}"))
                .collect::<String>();
            let op = StringTemplate::new(text, "path".into(), None).unwrap();
            assert_eq!(op.identifiers().len(), 7000);
            assert_eq!(op.identifiers().first().unwrap(), "N0");
            assert_eq!(op.identifiers().last().unwrap(), "N6999");
            for _ in 0..2 {
                assert!(
                    StringTemplate::new("{A}-{A}-{B}".into(), "path".into(), None)
                        .unwrap()
                        .declaration_diagnostic()
                        .is_none()
                );
            }
            let op = StringTemplate::new("{A}".into(), "path".into(), None).unwrap();
            assert!(op.input_diagnostic(0, "not a closed type").is_none());
            assert!(op.input_diagnostic(usize::MAX, "int").is_none());
            assert!(op.unknown_diagnostic(usize::MAX).is_none());
        }
    }
    mod values {
        use std::cmp::Ordering::{Equal, Greater, Less};
        use yamaa_core::numeric::Number;
        use yamaa_core::temporal::{Date, Precision};
        use yamaa_core::value::{
            compare_present, ComparisonError, FiniteFloat, Selection, Value, ValueType,
        };

        /// Seam: value normalization compared bit for bit.
        #[test]
        /// Check finite bit preservation and missing normalization across non-finite encodings.
        fn boundary_normalization_preserves_finite_bits_and_rejects_every_nonfinite_kind() {
            for bits in [
                0,
                1,
                0x8000000000000000,
                0x7fefffffffffffff,
                0xffefffffffffffff,
            ] {
                let Value::Float(value) = Value::float(f64::from_bits(bits)) else {
                    panic!("finite");
                };
                assert_eq!(value.get().to_bits(), bits);
            }
            for bits in [
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff0000000000001,
                0x7ff8000000000000,
                0xffffffffffffffff,
            ] {
                let float = f64::from_bits(bits);
                assert_eq!(FiniteFloat::new(float), None);
                assert_eq!(Value::float(float), Value::Missing);
                assert_eq!(Number::float(float), Number::Missing);
            }
        }

        /// Seam: the value model's distinctions, which no reader returns.
        #[test]
        /// Protect the closed value distinctions and full-range integer representation.
        fn missing_absence_empty_text_boolean_and_i64_are_distinct() {
            assert_ne!(Selection::Absent, Selection::Present(Value::Missing));
            assert_ne!(Value::Missing, Value::Str(String::new()));
            assert_eq!(Value::Str(".nan".into()).value_type(), Some(ValueType::Str));
            assert_eq!(Value::Bool(true).value_type(), Some(ValueType::Bool));
            assert_eq!(Number::try_from(&Value::Bool(true)), Err(ValueType::Bool));
            assert_eq!(
                Number::try_from(&Value::Str("42".into())),
                Err(ValueType::Str)
            );
            assert_eq!(Number::try_from(&Value::Missing), Ok(Number::Missing));
            for integer in [i64::MIN, i64::MAX, 9_007_199_254_740_993] {
                let number = Number::try_from(&Value::Int(integer)).unwrap();
                assert_eq!(Value::from(number), Value::Int(integer));
            }
        }

        /// Seam: the comparison kernel's orderings.
        #[test]
        /// Exercise mixed comparisons where rounding integers to binary64 changes ordering.
        fn mixed_numeric_comparison_keeps_full_integer_precision() {
            for (integer, float, expected) in [
                (9_007_199_254_740_993, 9_007_199_254_740_992.0, Greater),
                (-9_007_199_254_740_993, -9_007_199_254_740_992.0, Less),
                (i64::MAX, 9_223_372_036_854_775_808.0, Less),
                (i64::MIN, -9_223_372_036_854_775_808.0, Equal),
                (i64::MIN + 1, -9_223_372_036_854_775_808.0, Greater),
                (i64::MAX, f64::MAX, Less),
                (i64::MIN, -f64::MAX, Greater),
                (0, -0.0, Equal),
                (0, 0.5, Less),
                (0, -0.5, Greater),
                (1, 1.5, Less),
                (-1, -1.5, Greater),
                (0, f64::from_bits(1), Less),
                (0, -f64::from_bits(1), Greater),
            ] {
                let (left, right) = (Value::Int(integer), Value::float(float));
                assert_eq!(compare_present(&left, &right), Ok(expected));
                assert_eq!(compare_present(&right, &left), Ok(expected.reverse()));
            }
            assert_eq!(
                compare_present(&Value::float(-0.0), &Value::float(0.0)),
                Ok(Equal)
            );
        }

        /// Seam: the comparison kernel's orderings and its refusals, which are not
        /// issue records.
        #[test]
        /// Check text and temporal ordering and reject coercion or implicit null placement.
        fn ordered_comparison_does_not_coerce_or_choose_missing_placement() {
            assert_eq!(
                compare_present(&Value::Missing, &Value::Int(1)),
                Err(ComparisonError::MissingOperand)
            );
            assert_eq!(
                compare_present(&Value::Int(1), &Value::Str("1".into())),
                Err(ComparisonError::IncompatibleTypes)
            );
            assert_eq!(
                compare_present(&Value::Bool(false), &Value::Bool(true)),
                Err(ComparisonError::IncompatibleTypes)
            );
            assert_eq!(
                compare_present(
                    &Value::Str("\u{e000}".into()),
                    &Value::Str("\u{10000}".into())
                ),
                Ok(Less)
            );
            assert_eq!(
                compare_present(&Value::Str("é".into()), &Value::Str("e\u{301}".into())),
                Ok(Greater)
            );
            let imputed = Date::new(2024, 2, 29, Precision::Year).unwrap();
            let collected: Date = "2024-02-29".parse().unwrap();
            assert_eq!(
                compare_present(&Value::Date(imputed), &Value::Date(collected)),
                Ok(Equal)
            );
            assert_eq!(
                Number::try_from(&Value::Date(collected)),
                Err(ValueType::Date)
            );
        }
    }
}
