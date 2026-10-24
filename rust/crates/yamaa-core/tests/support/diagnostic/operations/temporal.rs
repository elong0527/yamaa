//! The date and time entry points: civil arithmetic, `dtc` and
//! `time_of_day` (TIME-4, TIME-14, TIME-17, TIME-19, TIME-20), and date
//! completion (TIME-1 to TIME-18).
use super::{text, value, File};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    dataset::{
        completion_policy, CalendarUnit, Completion, DateBounds, DatePolicyError, DayDeclaration,
        ElapsedUnit, FlagPart, Imputation, ImputationKind, Temporal, TemporalKind, TemporalType,
        TimeRule,
    },
    diagnostic::Diagnostic,
    table::ValueRef,
};

pub(super) const FILES: &[File] = &[fixture!("temporal.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "impute" => impute(input),
        "impute_policy" => impute_policy(input),
        "temporal" => temporal(input),
        _ => return None,
    })
}

/// A date and time operation of `kind` at `path` applied to `values`, one
/// for each of the operation's operands, each read from a source named as
/// its operand, `{kind, path, values}`: `kind` is `"date_shift"`,
/// `"study_day"`, `"epoch_day"` or `"time_of_day"`, or one of
/// `{"difference": <unit>, "bounds": ...}`, `{"datetime_difference":
/// <unit>}`, `{"datetime_shift": <unit>}`, `{"duration_value": <unit>}` and
/// `{"date_text": <depth>}`.
fn temporal(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["kind", "path", "values"]);
    let elapsed = |unit: &Json| match unit.text() {
        "second" => ElapsedUnit::Second,
        "minute" => ElapsedUnit::Minute,
        "hour" => ElapsedUnit::Hour,
        "day" => ElapsedUnit::Day,
        other => panic!("no elapsed unit {other:?}"),
    };
    let kind = match input.field("kind") {
        Json::String(kind) => match kind.as_str() {
            "date_shift" => TemporalKind::DateShift,
            "study_day" => TemporalKind::StudyDay,
            "epoch_day" => TemporalKind::EpochDay,
            "time_of_day" => TemporalKind::TimeOfDay,
            other => panic!("no date and time operation {other:?}"),
        },
        kind => match kind.members() {
            [(name, unit), (bounds, closed)] if name == "difference" && bounds == "bounds" => {
                TemporalKind::Difference {
                    unit: match unit.text() {
                        "day" => CalendarUnit::Day,
                        "week" => CalendarUnit::Week,
                        "month" => CalendarUnit::Month,
                        "year" => CalendarUnit::Year,
                        other => panic!("no calendar unit {other:?}"),
                    },
                    bounds: match closed.text() {
                        "exclusive" => DateBounds::Exclusive,
                        "inclusive" => DateBounds::Inclusive,
                        "between" => DateBounds::Between,
                        other => panic!("no date bounds {other:?}"),
                    },
                }
            }
            [(name, unit)] if name == "datetime_difference" => TemporalKind::DateTimeDifference {
                unit: elapsed(unit),
            },
            [(name, unit)] if name == "datetime_shift" => TemporalKind::DateTimeShift {
                unit: elapsed(unit),
            },
            [(name, unit)] if name == "duration_value" => TemporalKind::DurationValue {
                unit: elapsed(unit),
            },
            [(name, depth)] if name == "date_text" => TemporalKind::DateText {
                depth: depth.integer(),
            },
            other => panic!("no date and time operation {other:?}"),
        },
    };
    let operation = Temporal::new(
        kind,
        kind.fields().iter().map(|field| (*field).into()).collect(),
        text(input, "path").into(),
    )
    .expect("an admitted operation");
    let values = input
        .field("values")
        .items()
        .iter()
        .map(value)
        .collect::<Vec<_>>();
    match operation
        .apply_values(&values)
        .expect("one value for each operand")
    {
        Ok(_) => Vec::new(),
        Err(diagnostic) => vec![diagnostic],
    }
}

/// `impute`'s completion policy, `{type, month, day, time, path}`: `day` is
/// `{"number": ...}` or `{"position": ...}`, and each of the three is
/// optional.
fn impute_policy(input: &Json) -> Vec<Diagnostic> {
    match policy(input) {
        Ok(_) => Vec::new(),
        Err(DatePolicyError::Diagnostic(diagnostic)) => vec![diagnostic],
        Err(DatePolicyError::Shape) => panic!("a policy of the admitted shape"),
    }
}

/// A completion operation applied to `value`, `{kind, source, path, value}`:
/// `kind` is `{"impute": <policy>}`, `{"precision": <type>}`, `{"flag":
/// <part>, "seconds_collected": ...}` or `"to_date"`.
fn impute(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["kind", "source", "path", "value"]);
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
    let input = input.closed(&["type", "month", "day", "time", "path"]);
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
