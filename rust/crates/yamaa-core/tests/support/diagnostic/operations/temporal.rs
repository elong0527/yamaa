//! The date completion entry points (TIME-1 to TIME-18).
use super::{text, value};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    dataset::{
        completion_policy, Completion, DatePolicyError, DayDeclaration, FlagPart, Imputation,
        ImputationKind, TemporalType, TimeRule,
    },
    diagnostic::Diagnostic,
    table::ValueRef,
};

/// `impute`'s completion policy, `{type, month, day, time, path}`: `day` is
/// `{"number": ...}` or `{"position": ...}`, and each of the three is
/// optional.
pub(super) fn impute_policy(input: &Json) -> Vec<Diagnostic> {
    match policy(input) {
        Ok(_) => Vec::new(),
        Err(DatePolicyError::Diagnostic(diagnostic)) => vec![diagnostic],
        Err(DatePolicyError::Shape) => panic!("a policy of the admitted shape"),
    }
}

/// A completion operation applied to `value`, `{kind, source, path, value}`:
/// `kind` is `{"impute": <policy>}`, `{"precision": <type>}`, `{"flag":
/// <part>, "seconds_collected": ...}` or `"to_date"`.
pub(super) fn impute(input: &Json) -> Vec<Diagnostic> {
    let kind = match input.field("kind") {
        Json::String(kind) if kind == "to_date" => ImputationKind::ToDate,
        kind => match kind.members() {
            [(name, policy_input)] if name == "impute" => {
                ImputationKind::Impute(policy(policy_input).expect("an admitted completion policy"))
            }
            [(name, target)] if name == "precision" => ImputationKind::Precision {
                target: temporal_type(target),
            },
            [(name, part), (seconds, collected)]
                if name == "flag" && seconds == "seconds_collected" =>
            {
                ImputationKind::Flag {
                    part: match part.text() {
                        "date" => FlagPart::Date,
                        "time" => FlagPart::Time,
                        other => panic!("no flag part {other:?}"),
                    },
                    seconds_collected: collected.boolean(),
                }
            }
            other => panic!("no completion operation {other:?}"),
        },
    };
    let operation = Imputation::new(
        kind,
        text(input, "source").into(),
        None,
        None,
        None,
        text(input, "path").into(),
        None,
    )
    .expect("an admitted operation");
    let value = value(input.field("value"));
    match operation.source_value(ValueRef::from(&value)) {
        Ok(_) => Vec::new(),
        Err(diagnostic) => vec![diagnostic],
    }
}

fn policy(input: &Json) -> Result<Completion, DatePolicyError> {
    let day = input.get("day").map(|day| match day.members() {
        [(name, number)] if name == "number" => DayDeclaration::Number(number.text()),
        [(name, position)] if name == "position" => DayDeclaration::Position(position.text()),
        other => panic!("no day declaration {other:?}"),
    });
    let time = input.get("time").map(|time| match time.text() {
        "first" => TimeRule::First,
        "last" => TimeRule::Last,
        other => panic!("no time rule {other:?}"),
    });
    completion_policy(
        temporal_type(input.field("type")),
        input.get("month").map(Json::text),
        day,
        time,
        text(input, "path"),
    )
}

#[track_caller]
fn temporal_type(json: &Json) -> TemporalType {
    match json.text() {
        "date" => TemporalType::Date,
        "datetime" => TemporalType::DateTime,
        other => panic!("no temporal type {other:?}"),
    }
}
