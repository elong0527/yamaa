use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V, DatasetFinding as F, ProjectionError},
    reduction::NumericReducer,
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::{Value, ValueType},
};
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}

/// Independent normative assertions; engine tests reach the same causes from
/// real attempted runs and pin their first-failure table effects.
pub fn reached() -> BTreeSet<ConditionCode> {
    let date = Value::Date(Date::new(2024, 2, 29, DatePrecision::Month).unwrap());
    let cases = [
        (
            F::MultipleValues {
                path: "columns.A.source",
                identifier: "SRC.A",
                count: 3,
            },
            "derivation",
            "multiple_values_per_key",
            "REQ-0075",
            vec![
                ("identifier", text("SRC.A")),
                ("value_count", V::Scalar(Value::Int(3))),
            ],
        ),
        (
            F::UnmatchedKey {
                path: "columns.A.source",
                dataset: "DM",
                intermediate: "I",
            },
            "join",
            "unmatched_key",
            "REQ-0124",
            vec![("dataset", text("DM")), ("intermediate", text("I"))],
        ),
        (
            F::MultipleMatches {
                path: "columns.A.source",
                dataset: "DM",
                intermediate: "I",
                count: 4,
            },
            "join",
            "multiple_matches",
            "REQ-0127",
            vec![
                ("dataset", text("DM")),
                ("intermediate", text("I")),
                ("match_count", V::Scalar(Value::Int(4))),
            ],
        ),
        (
            F::BaselineAmbiguity {
                path: "columns.A.baseline",
                column: "A",
                date: &date,
                count: 2,
            },
            "derivation",
            "ambiguous_baseline",
            "REQ-0322",
            vec![
                ("column", text("A")),
                ("operation", text("baseline_flag")),
                ("date", text("2024-02-29")),
                ("match_count", V::Scalar(Value::Int(2))),
            ],
        ),
        (
            F::ReductionInputType {
                path: "columns.A.aggregate",
                expression: "MEAN(SRC.A)",
                reducer: NumericReducer::Mean,
                source: "SRC.A",
                actual: ValueType::Bool,
            },
            "validation",
            "incompatible_input_type",
            "REQ-0510",
            vec![
                ("expr", text("MEAN(SRC.A)")),
                ("reducer", text("MEAN")),
                ("source", text("SRC.A")),
                ("expected", text("numeric")),
                ("actual", text("bool")),
            ],
        ),
    ];
    cases
        .into_iter()
        .map(|(cause, phase, condition, requirement, context)| {
            let diagnostic = cause.diagnostic().unwrap();
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().requirement
                ),
                (phase, condition, Some(requirement))
            );
            let path = match cause {
                F::MultipleValues { path, .. }
                | F::UnmatchedKey { path, .. }
                | F::MultipleMatches { path, .. }
                | F::BaselineAmbiguity { path, .. }
                | F::ReductionInputType { path, .. } => path,
            };
            assert_eq!(diagnostic.spec_paths, [path]);
            assert_eq!(
                diagnostic.context,
                context
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect()
            );
            assert!(diagnostic.source_span.is_none());
            assert!(diagnostic.operand_route.is_none());
            diagnostic.code
        })
        .collect()
}

#[test]
fn temporal_spelling_and_nonnumeric_reduction_types_remain_complete() {
    let date = Date::new(1, 1, 1, DatePrecision::Year).unwrap();
    let datetime = Value::DateTime(DateTime::new(date, 1, 2, 3, DateTimePrecision::Day).unwrap());
    let diagnostic = F::BaselineAmbiguity {
        path: "baseline",
        column: "B",
        date: &datetime,
        count: 2,
    }
    .diagnostic()
    .unwrap();
    assert_eq!(diagnostic.context["date"], text("0001-01-01T01:02:03"));
    for (actual, spelling) in [
        (ValueType::Str, "str"),
        (ValueType::Bool, "bool"),
        (ValueType::Date, "date"),
        (ValueType::DateTime, "datetime"),
    ] {
        let diagnostic = F::ReductionInputType {
            path: "aggregate",
            expression: "SUM(A)",
            reducer: NumericReducer::Sum,
            source: "A",
            actual,
        }
        .diagnostic()
        .unwrap();
        assert_eq!(diagnostic.context["actual"], text(spelling));
    }
}

#[test]
fn invalid_causes_and_whole_context_quota_refuse_without_changing_borrowed_facts() {
    let huge = "x".repeat(3_000_000);
    let address = huge.as_ptr();
    let cause = F::MultipleMatches {
        path: "source",
        dataset: "DM",
        intermediate: &huge,
        count: 2,
    };
    assert_eq!(
        cause.diagnostic_with_limit(1024),
        Err(ProjectionError::Limit)
    );
    assert_eq!(huge.as_ptr(), address);
    assert_eq!(huge.len(), 3_000_000);
    for count in [0, 1] {
        assert_eq!(
            F::MultipleValues {
                path: "p",
                identifier: "A",
                count
            }
            .diagnostic_with_limit(2048),
            Err(ProjectionError::InvalidContext)
        );
        assert_eq!(
            F::MultipleMatches {
                path: "p",
                dataset: "DM",
                intermediate: "I",
                count
            }
            .diagnostic_with_limit(2048),
            Err(ProjectionError::InvalidContext)
        );
    }
    let wrong = Value::Str("2024-01-01".into());
    assert_eq!(
        F::BaselineAmbiguity {
            path: "p",
            column: "A",
            date: &wrong,
            count: 2
        }
        .diagnostic_with_limit(2048),
        Err(ProjectionError::InvalidContext)
    );
    assert_eq!(
        F::ReductionInputType {
            path: "p",
            expression: "SUM(A)",
            reducer: NumericReducer::Sum,
            source: "A",
            actual: ValueType::Int
        }
        .diagnostic_with_limit(2048),
        Err(ProjectionError::InvalidContext)
    );
}

#[test]
#[cfg(target_pointer_width = "64")]
fn counts_above_i64_remain_mathematical_diagnostic_integers() {
    let diagnostic = F::MultipleValues {
        path: "p",
        identifier: "A",
        count: usize::MAX,
    }
    .diagnostic()
    .unwrap();
    assert_eq!(
        diagnostic.context["value_count"],
        V::Integer(usize::MAX.to_string())
    );
    let diagnostic = F::MultipleMatches {
        path: "p",
        dataset: "DM",
        intermediate: "I",
        count: usize::MAX,
    }
    .diagnostic()
    .unwrap();
    assert_eq!(
        diagnostic.context["match_count"],
        V::Integer(usize::MAX.to_string())
    );
}
