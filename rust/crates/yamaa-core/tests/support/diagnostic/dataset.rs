#![allow(unused_imports)]
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    diagnostic::{ConditionCode, DatasetFinding as F, ProjectionError},
    reduction::NumericReducer,
    temporal::{Date, DateTime, Precision},
    value::{Value, ValueType},
};
pub(crate) fn text(value: &str) -> V {
    V::String(value.into())
}

/// Independent normative assertions; engine tests reach the same causes from
/// real attempted runs and pin their first-failure table effects.
pub fn reached() -> BTreeSet<ConditionCode> {
    let date = Value::Date(Date::new(2024, 2, 29, Precision::Month).unwrap());
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
            vec![("identifier", text("SRC.A")), ("value_count", V::from(3))],
        ),
        (
            F::UnmatchedKey {
                path: "columns.A.source",
                dataset: "DM",
                intermediate: "I",
                implicit_join: false,
            },
            "join",
            "unmatched_key",
            "REQ-0124",
            vec![("dataset", text("DM")), ("intermediate", text("I"))],
        ),
        (
            F::UnmatchedKey {
                path: "columns.A.derivation.source",
                dataset: "DM",
                intermediate: "intermediate(DM)",
                implicit_join: true,
            },
            "join",
            "unmatched_key",
            "LOOKUP-24",
            vec![
                ("dataset", text("DM")),
                ("intermediate", text("intermediate(DM)")),
            ],
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
                ("match_count", V::from(4)),
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
                ("match_count", V::from(2)),
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
