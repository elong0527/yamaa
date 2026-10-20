//! Collected date text with unknown components, intervals and durations
//! (TIME-2, TIME-7, TIME-13), date parts written as collected date text
//! (TIME-19), times of day (TIME-20) and the ISO 8601 forms of the date and
//! time submission data types (SUB-26), against hand-written vectors.
mod temporal_date_text {
    use yamaa_core::{
        dataset::{
            completion_policy, DayDeclaration, Imputation, ImputationKind as K,
            ImputationStep as S, Temporal, TemporalKind, TemporalStep, TemporalType as T, TimeRule,
        },
        diagnostic::Diagnostic,
        iso8601::Form,
        table::ValueRef,
        temporal::Precision as P,
        value::Value,
    };

    const PATH: &str = "columns.X.derivation.op";

    fn operation(kind: K) -> Imputation {
        Imputation::new(kind, "SRC.TEXT".into(), None, None, None, PATH.into(), None).unwrap()
    }

    fn condition(diagnostic: &Diagnostic) -> String {
        diagnostic.code.definition().condition.into()
    }

    /// One reading as the fixture spells it: a precision word, a date, a
    /// completed datetime with its precision, `partial`, `missing`, or the
    /// condition of the failure.
    fn spell(step: Result<S, Diagnostic>) -> String {
        match step {
            Ok(S::Value(Value::Str(word))) => word,
            Ok(S::Value(Value::Date(date))) => date.to_string(),
            Ok(S::Value(Value::DateTime(moment))) => {
                format!("{moment}@{}", moment.collected_precision().word())
            }
            Ok(S::Value(Value::Missing)) => "missing".into(),
            Ok(S::Partial) => "partial".into(),
            Ok(other) => panic!("unexpected step {other:?}"),
            Err(diagnostic) => condition(&diagnostic),
        }
    }

    #[test]
    fn collected_text_reads_its_prefix_and_names_intervals_and_durations() {
        let date = operation(K::Precision { target: T::Date });
        let datetime = operation(K::Precision {
            target: T::DateTime,
        });
        let to_date = operation(K::ToDate);
        let impute = operation(K::Impute(
            completion_policy(
                T::DateTime,
                Some("1"),
                Some(DayDeclaration::Position("first")),
                P::Year,
                Some(TimeRule::First),
                PATH,
            )
            .unwrap(),
        ));
        let mut rows = 0;
        for row in include_str!("fixtures/collected_date_text.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 6, "{row}");
            let text = ValueRef::Str(fields[1]);
            let actual = [
                spell(date.source_value(text)),
                spell(datetime.source_value(text)),
                spell(to_date.source_value(text)),
                spell(impute.source_value(text)),
            ];
            assert_eq!(actual, fields[2..], "{}", fields[0]);
            rows += 1;
        }
        assert!(rows > 20);
    }

    #[test]
    fn a_declared_invalid_answers_an_interval_as_it_answers_invalid_text() {
        let answered = Imputation::new(
            K::Precision { target: T::Date },
            "SRC.TEXT".into(),
            None,
            None,
            None,
            PATH.into(),
            Some(Value::Missing),
        )
        .unwrap();
        for text in ["2025-01-01/2025-01-10", "P3D", "UNK"] {
            assert_eq!(
                answered.source_value(ValueRef::Str(text)),
                Ok(S::Recovered { handler: "invalid" }),
                "{text}"
            );
        }
        let Err(failure) = operation(K::Precision { target: T::Date })
            .source_value(ValueRef::Str("2025-01-01/2025-01-10"))
        else {
            panic!("an interval fails without a handler")
        };
        assert_eq!(failure.spec_paths, [PATH]);
        let definition = failure.code.definition();
        assert_eq!(
            (definition.phase, definition.rule),
            ("impute", Some("TIME-13"))
        );
    }

    fn dtc(values: &[Value], invalid: Option<Value>) -> Result<TemporalStep, Diagnostic> {
        let sources = ["YR", "MO", "DY", "HR", "MI", "SC"][..values.len()]
            .iter()
            .map(|name| String::from(*name))
            .collect();
        let mut operation = Temporal::new(
            TemporalKind::DateText {
                depth: values.len() as u8,
            },
            sources,
            "columns.DTC.derivation.dtc".into(),
        )
        .unwrap();
        if let Some(value) = invalid {
            operation = operation.with_invalid(value).unwrap();
        }
        operation.apply_values(values).unwrap()
    }

    fn text(value: &str) -> Value {
        Value::Str(value.into())
    }

    #[test]
    fn dtc_writes_its_parts_and_stops_before_the_first_missing_one() {
        let missing = Value::Missing;
        for (values, expected) in [
            (
                vec![text("2025"), text("03"), text("15")],
                text("2025-03-15"),
            ),
            (
                vec![text("2025"), text("03"), missing.clone()],
                text("2025-03"),
            ),
            (
                vec![text("2025"), missing.clone(), text("15")],
                text("2025"),
            ),
            // A part after the first missing one is never read, so neither
            // its form nor its type is checked.
            (
                vec![text("2025"), missing.clone(), text("99")],
                text("2025"),
            ),
            (
                vec![text("2025"), missing.clone(), Value::float(3.0)],
                text("2025"),
            ),
            (
                vec![missing.clone(), text("03"), text("15")],
                Value::Missing,
            ),
            (
                vec![Value::Int(2025), Value::Int(3), Value::Int(5)],
                text("2025-03-05"),
            ),
            (vec![Value::Int(7)], text("0007")),
            (
                vec![text("2024"), text("02"), text("29")],
                text("2024-02-29"),
            ),
            (
                vec![
                    text("2025"),
                    text("03"),
                    text("15"),
                    text("08"),
                    text("30"),
                    text("05"),
                ],
                text("2025-03-15T08:30:05"),
            ),
            (
                vec![
                    text("2025"),
                    text("03"),
                    text("15"),
                    text("08"),
                    missing.clone(),
                    text("05"),
                ],
                text("2025-03-15T08"),
            ),
        ] {
            assert_eq!(
                dtc(&values, None),
                Ok(TemporalStep::Value(expected)),
                "{values:?}"
            );
        }
    }

    #[test]
    fn dtc_refuses_a_part_outside_its_form_unless_invalid_answers_it() {
        for (values, part) in [
            (vec![text("2025"), text("3")], "month"),
            (vec![text("2025"), text("MAR")], "month"),
            (vec![text("2025"), text("13")], "month"),
            (vec![text("25")], "year"),
            (vec![text("0000")], "year"),
            (vec![Value::Int(0)], "year"),
            (vec![Value::Int(10000)], "year"),
            (vec![text("2025"), text("02"), text("29")], "day"),
            (vec![text("2025"), text("04"), Value::Int(31)], "day"),
            (
                vec![text("2025"), text("03"), text("15"), text("24")],
                "hour",
            ),
            (vec![text("2025"), Value::Int(-1)], "month"),
        ] {
            let Err(failure) = dtc(&values, None) else {
                panic!("{values:?} must fail")
            };
            let definition = failure.code.definition();
            assert_eq!(
                (definition.phase, definition.condition, definition.rule),
                ("impute", "invalid_date_text", Some("TIME-19")),
                "{values:?}"
            );
            assert_eq!(failure.spec_paths, ["columns.DTC.derivation.dtc"]);
            assert_eq!(
                failure.context["part"],
                yamaa_core::json::Value::String(part.into()),
                "{values:?}"
            );
            assert_eq!(
                dtc(&values, Some(Value::Missing)),
                Ok(TemporalStep::Invalid),
                "{values:?}"
            );
        }
        let Err(failure) = dtc(&[text("2025"), Value::float(3.0)], None) else {
            panic!("a float part is an incompatible input")
        };
        let definition = failure.code.definition();
        assert_eq!(
            (definition.condition, definition.rule),
            ("incompatible_input_type", Some("TIME-19"))
        );
    }

    #[test]
    fn time_of_day_counts_whole_seconds_since_midnight() {
        let operation = Temporal::new(
            TemporalKind::TimeOfDay,
            vec!["DTM".into()],
            "columns.TM.derivation.time_of_day".into(),
        )
        .unwrap();
        for (moment, seconds) in [
            ("2025-03-04T08:30:15", 30615),
            ("2025-03-04T00:00:00", 0),
            ("2025-03-04T23:59:59", 86399),
            ("2024-02-29T12:00", 43200),
        ] {
            assert_eq!(
                operation.apply_values(&[Value::DateTime(moment.parse().unwrap())]),
                Some(Ok(TemporalStep::Value(Value::Int(seconds)))),
                "{moment}"
            );
        }
        assert_eq!(
            operation.apply_values(&[Value::Missing]),
            Some(Ok(TemporalStep::Value(Value::Missing)))
        );
        let Some(Err(failure)) = operation.apply_values(&[text("2025-03-04T08:30:15")]) else {
            panic!("text has no time of day")
        };
        let definition = failure.code.definition();
        assert_eq!(
            (definition.condition, definition.rule),
            ("incompatible_input_type", Some("TIME-20"))
        );
        assert_eq!(
            failure.spec_paths,
            ["columns.TM.derivation.time_of_day.source"]
        );
    }

    #[test]
    fn date_and_time_data_types_admit_exactly_their_iso_8601_forms() {
        let mut seen = Vec::new();
        for row in include_str!("fixtures/submission_data_types.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 3, "{row}");
            let form = Form::of(fields[0]).unwrap_or_else(|| panic!("{row}"));
            assert_eq!(form.name(), fields[0]);
            let admitted = match fields[2] {
                "yes" => true,
                "no" => false,
                other => panic!("bad verdict {other}"),
            };
            assert_eq!(form.admits(fields[1]), admitted, "{row}");
            if !seen.contains(&form) {
                seen.push(form);
            }
        }
        // Every form has vectors, and no other data type names one.
        assert_eq!(seen, Form::ALL);
        for other in ["text", "integer", "float", "URI", "Date"] {
            assert_eq!(Form::of(other), None, "{other}");
        }
    }
}
