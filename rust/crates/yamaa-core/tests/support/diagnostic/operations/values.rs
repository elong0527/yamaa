//! The value-level entry points: mapping and cut (EXPR-11 to EXPR-13), the
//! dataset findings of a run, and predicate evaluation (FORMULA-5 to
//! FORMULA-9).
use super::{bindings, text, value, value_type, File};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    diagnostic::{DatasetFinding, Diagnostic},
    expression::{compile, Context},
    mapping_cut::{AdmissionError, Cut, Mapping},
    reduction::NumericReducer,
    table::ValueRef,
};

pub(super) const FILES: &[File] = &[fixture!("values.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "cut" => cut(input),
        "dataset_finding" => dataset_finding(input),
        "mapping" => mapping(input),
        "predicate" => predicate(input),
        _ => return None,
    })
}

/// `mapping`'s declaration, `{source, path, entries, case_sensitive}`, with
/// `entries` as `[key, value]` pairs, then the selection of `select` when the
/// case writes one.
fn mapping(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["source", "path", "entries", "case_sensitive", "select"]);
    let declared = Mapping::new(
        text(input, "source").into(),
        text(input, "path").into(),
        input
            .field("entries")
            .items()
            .iter()
            .map(|entry| match entry.items() {
                [key, mapped] => (key.text().to_owned(), value(mapped)),
                other => panic!("an entry is a [key, value] pair: {other:?}"),
            })
            .collect(),
        input.field("case_sensitive").boolean(),
        None,
        None,
    );
    selected(input, declared, |mapping, value| {
        mapping.select(value).map(|_| ())
    })
}

/// `cut`'s declaration, `{source, path, breaks, labels, right}`, then the
/// selection of `select` when the case writes one.
fn cut(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["source", "path", "breaks", "labels", "right", "select"]);
    let declared = Cut::new(
        text(input, "source").into(),
        text(input, "path").into(),
        input
            .field("breaks")
            .items()
            .iter()
            .map(|boundary| match boundary {
                Json::Number(token) => token.parse().expect("a break"),
                other => panic!("a break is a number: {other:?}"),
            })
            .collect(),
        input
            .field("labels")
            .items()
            .iter()
            .map(|label| label.text().to_owned())
            .collect(),
        input.field("right").boolean(),
        None,
    );
    selected(input, declared, |cut, value| cut.select(value).map(|_| ()))
}

/// The admission refusal, else the refusal of selecting `select`.
fn selected<T>(
    input: &Json,
    declared: Result<T, AdmissionError>,
    select: impl Fn(&T, ValueRef<'_>) -> Result<(), Diagnostic>,
) -> Vec<Diagnostic> {
    let declared = match declared {
        Ok(declared) => declared,
        Err(AdmissionError::Diagnostic(diagnostic)) => return vec![diagnostic],
        Err(AdmissionError::Shape) => panic!("a declaration of the admitted shape"),
    };
    let Some(selected) = input.get("select") else {
        return Vec::new();
    };
    let selected = value(selected);
    select(&declared, ValueRef::from(&selected))
        .err()
        .into_iter()
        .collect()
}

/// A finding of a run, `{finding, path, ...}`: `multiple_values` with
/// `identifier` and `count`, `unmatched_key` with `dataset` and
/// `intermediate`, `multiple_matches` with those and `count`,
/// `baseline_ambiguity` with `column`, `date` and `count`, and
/// `reduction_input_type` with `expression`, `reducer`, `source` and
/// `actual`.
fn dataset_finding(input: &Json) -> Vec<Diagnostic> {
    let path = text(input, "path");
    let count = || input.field("count").integer();
    let date;
    let finding = match text(input, "finding") {
        "multiple_values" => {
            input.closed(&["finding", "path", "identifier", "count"]);
            DatasetFinding::MultipleValues {
                path,
                identifier: text(input, "identifier"),
                count: count(),
            }
        }
        "unmatched_key" => {
            input.closed(&["finding", "path", "dataset", "intermediate"]);
            DatasetFinding::UnmatchedKey {
                path,
                dataset: text(input, "dataset"),
                intermediate: text(input, "intermediate"),
            }
        }
        "multiple_matches" => {
            input.closed(&["finding", "path", "dataset", "intermediate", "count"]);
            DatasetFinding::MultipleMatches {
                path,
                dataset: text(input, "dataset"),
                intermediate: text(input, "intermediate"),
                count: count(),
            }
        }
        "baseline_ambiguity" => {
            input.closed(&["finding", "path", "column", "date", "count"]);
            date = value(input.field("date"));
            DatasetFinding::BaselineAmbiguity {
                path,
                column: text(input, "column"),
                date: &date,
                count: count(),
            }
        }
        "reduction_input_type" => {
            input.closed(&[
                "finding",
                "path",
                "expression",
                "reducer",
                "source",
                "actual",
            ]);
            DatasetFinding::ReductionInputType {
                path,
                expression: text(input, "expression"),
                reducer: match text(input, "reducer") {
                    "SUM" => NumericReducer::Sum,
                    "MEAN" => NumericReducer::Mean,
                    other => panic!("no numeric reducer {other:?}"),
                },
                source: text(input, "source"),
                actual: value_type(input.field("actual")),
            }
        }
        other => panic!("no dataset finding {other:?}"),
    };
    vec![finding.diagnostic().expect("a complete context")]
}

/// The evaluation of a predicate, `{expression, path, bindings}`, where each
/// name `bindings` gives reads its value and every other name is absent.
fn predicate(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["expression", "path", "bindings"]);
    let mut bindings = bindings(input.field("bindings"));
    let plan = compile(
        text(input, "expression"),
        Context::Predicate,
        text(input, "path"),
    )
    .expect("a predicate that compiles");
    match plan.evaluate_truth(&mut bindings) {
        Ok(_) => Vec::new(),
        Err(error) => vec![error.diagnostic().expect("a language condition")],
    }
}
