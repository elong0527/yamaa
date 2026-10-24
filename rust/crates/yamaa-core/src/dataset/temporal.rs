//! Bound complete-date operations retain authored operands and calendar semantics.
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::{ConditionCode, Diagnostic},
    table::ValueRef,
    temporal::Date,
    value::Value,
};
use std::vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarUnit {
    Day,
    Week,
    Month,
    Year,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateBounds {
    Exclusive,
    Inclusive,
    Between,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalKind {
    Difference {
        unit: CalendarUnit,
        bounds: DateBounds,
    },
    StudyDay,
    EpochDay,
    DateShift,
    DateTimeDifference {
        unit: ElapsedUnit,
    },
    DateTimeShift {
        unit: ElapsedUnit,
    },
    DurationValue {
        unit: ElapsedUnit,
    },
    /// `duration_text`: a number of `unit`s as canonical ISO 8601 duration
    /// text, the inverse of `DurationValue`.
    DurationText {
        unit: ElapsedUnit,
    },
    /// `dtc`: collected date text from the year and the `depth - 1` finer
    /// parts declared after it, in [`DATE_PARTS`] order.
    DateText {
        depth: u8,
    },
    /// `time_of_day`: a datetime's whole seconds since midnight.
    TimeOfDay,
}
/// The parts `dtc` assembles, coarsest first.
pub static DATE_PARTS: [&str; 6] = ["year", "month", "day", "hour", "minute", "second"];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElapsedUnit {
    Second,
    Minute,
    Hour,
    Day,
}
impl ElapsedUnit {
    pub fn seconds(self) -> u32 {
        match self {
            Self::Second => 1,
            Self::Minute => 60,
            Self::Hour => 3600,
            Self::Day => 86400,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidTemporal {
    Shape,
    Bounds,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Temporal {
    pub(super) kind: TemporalKind,
    pub(super) sources: Vec<String>,
    pub(super) path: String,
    pub(super) literals: Vec<Option<Value>>,
    pub(super) invalid: Option<Value>,
    /// Each operand's path as its specification's surface writes it, in
    /// [`TemporalKind::fields`] order, for a 1.0 specification whose operand
    /// fields are renamed (P-OP-1); empty, each operand's path is the
    /// operation's path and the executor's field name.
    pub(super) operand_paths: Vec<String>,
}
impl Temporal {
    pub fn new(
        kind: TemporalKind,
        sources: Vec<String>,
        path: String,
    ) -> Result<Self, InvalidTemporal> {
        if path.is_empty()
            || sources.iter().any(String::is_empty)
            || matches!(kind, TemporalKind::DateText { depth } if !(1..=6).contains(&depth))
            || sources.len() != kind.fields().len()
        {
            return Err(InvalidTemporal::Shape);
        }
        if matches!(kind, TemporalKind::Difference { unit, bounds } if unit != CalendarUnit::Day && bounds != DateBounds::Exclusive)
        {
            return Err(InvalidTemporal::Bounds);
        }
        Ok(Self {
            kind,
            literals: vec![None; sources.len()],
            invalid: None,
            sources,
            path,
            operand_paths: Vec::new(),
        })
    }
    /// This operation with each operand's path as its specification writes
    /// it, one for each of [`TemporalKind::fields`].
    pub fn with_operand_paths(mut self, paths: Vec<String>) -> Self {
        debug_assert_eq!(paths.len(), self.kind.fields().len());
        self.operand_paths = paths;
        self
    }
    pub fn kind(&self) -> TemporalKind {
        self.kind
    }
    pub fn sources(&self) -> &[String] {
        &self.sources
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn operand_path(&self, index: usize) -> Option<String> {
        if let Some(path) = self.operand_paths.get(index) {
            return Some(path.clone());
        }
        self.kind
            .fields()
            .get(index)
            .map(|field| format!("{}.{}", self.path, field))
    }
    pub(crate) fn input_diagnostic(source: &str, path: &str, actual: &str) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::TemporalOperand,
            spec_paths: vec![path.into()],
            context: [("source", source), ("expected", "date"), ("actual", actual)]
                .into_iter()
                .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    /// A `unit` the declared `type` does not count in.
    pub(crate) fn unit_diagnostic(path: &str, value: &str, permitted: &[&str]) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::TemporalUnit,
            spec_paths: vec![format!("{path}.unit")],
            context: vec![
                ("value".into(), JsonValue::String(value.into())),
                (
                    "permitted".into(),
                    JsonValue::Array(
                        permitted
                            .iter()
                            .map(|unit| JsonValue::String((*unit).into()))
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    /// A field the declared `type` requires but the operation omits, or one
    /// the declared `type` does not take, which is the identifier written.
    pub(crate) fn type_field_diagnostic(
        required: bool,
        path: &str,
        field: &str,
        target: &str,
    ) -> Diagnostic {
        let mut context = vec![("field", field), ("type", target)];
        if !required {
            context.push(("identifier", field));
        }
        Diagnostic {
            code: if required {
                ConditionCode::TemporalFieldRequired
            } else {
                ConditionCode::TemporalFieldNotPermitted
            },
            spec_paths: vec![format!("{path}.{field}")],
            context: context
                .into_iter()
                .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    /// A `dtc` part declared without a coarser part, which it would never
    /// follow in the assembled text.
    pub(crate) fn date_part_gap_diagnostic(path: &str, field: &str, finer: &str) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::DateTextGap,
            spec_paths: vec![format!("{path}.{field}")],
            context: [("field", field), ("finer", finer)]
                .into_iter()
                .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    /// TIME-5's refusal of the endpoints a `diff` counts, at the field that
    /// writes them, naming the written value and the one value permitted.
    pub(crate) fn bounds_diagnostic(
        path: &str,
        field: &str,
        value: &str,
        permitted: &str,
    ) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::TemporalBounds,
            spec_paths: vec![format!("{path}.{field}")],
            context: vec![
                ("value".into(), JsonValue::String(value.into())),
                (
                    "permitted".into(),
                    JsonValue::Array(vec![JsonValue::String(permitted.into())]),
                ),
            ]
            .into_iter()
            .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    /// Each operand is checked in authored order, even when a preceding one is missing.
    pub fn operand(
        &self,
        index: usize,
        value: ValueRef<'_>,
    ) -> Option<Result<Option<Date>, Diagnostic>> {
        let source = self.sources.get(index)?;
        self.kind.fields().get(index)?;
        Some(match value {
            ValueRef::Missing => Ok(None),
            ValueRef::Date(value) => Ok(Some(value)),
            other => {
                let actual = match other {
                    ValueRef::Str(_) => "str",
                    ValueRef::Int(_) => "int",
                    ValueRef::Float(_) => "float",
                    ValueRef::Bool(_) => "bool",
                    ValueRef::DateTime(_) => "datetime",
                    _ => unreachable!("dates and missing handled above"),
                };
                Err(Self::input_diagnostic(
                    source,
                    &self.operand_path(index).expect("admitted operand index"),
                    actual,
                ))
            }
        })
    }
    /// The compiler binds one operand per source, so the arity holds.
    pub fn apply(&self, operands: &[Option<Date>]) -> Option<Value> {
        if operands.len() != self.sources.len() {
            return None;
        }
        if operands.iter().any(Option::is_none) {
            return Some(Value::Missing);
        }
        let first = operands[0].expect("present operand");
        Some(Value::Int(match self.kind {
            TemporalKind::Difference { unit, bounds } => {
                whole_units(first, operands[1].unwrap(), unit, bounds)
            }
            TemporalKind::StudyDay => {
                let difference = ordinal(first) - ordinal(operands[1].unwrap());
                if difference >= 0 {
                    difference + 1
                } else {
                    difference
                }
            }
            TemporalKind::EpochDay => ordinal(first) - 719_163,
            _ => return None,
        }))
    }
}
impl TemporalKind {
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Difference { .. } | Self::DateTimeDifference { .. } => &["start", "end"],
            Self::StudyDay => &["date", "reference"],
            Self::DateShift => &["source", "days"],
            Self::DateTimeShift { .. } => &["source", "amount"],
            Self::EpochDay
            | Self::DurationValue { .. }
            | Self::DurationText { .. }
            | Self::TimeOfDay => &["source"],
            Self::DateText { depth } => &DATE_PARTS[..usize::from(depth)],
        }
    }
    /// Whether the operands are read in order up to the first missing one:
    /// `dtc` reads no part after its first missing part (TIME-19). Every
    /// other operation reads each of its operands.
    pub fn stops_at_missing(self) -> bool {
        matches!(self, Self::DateText { .. })
    }
}
fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn month_length(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        _ => unreachable!("admitted month"),
    }
}
pub(super) fn ordinal(date: Date) -> i64 {
    let (year, month, day) = date.fields();
    let year = i64::from(year);
    let previous = year - 1;
    let before = 365 * previous + previous / 4 - previous / 100 + previous / 400;
    let months: i64 = (1..i64::from(month))
        .map(|month| month_length(year, month))
        .sum();
    before + months + i64::from(day)
}
fn whole_units(start: Date, end: Date, unit: CalendarUnit, bounds: DateBounds) -> i64 {
    if end < start {
        return -whole_units(end, start, unit, bounds);
    }
    let days = ordinal(end) - ordinal(start);
    match unit {
        CalendarUnit::Day => match bounds {
            DateBounds::Exclusive => days,
            DateBounds::Inclusive => days + 1,
            DateBounds::Between => (days - 1).max(0),
        },
        CalendarUnit::Week => days / 7,
        CalendarUnit::Month | CalendarUnit::Year => {
            let (start_year, start_month, start_day) = start.fields();
            let (end_year, end_month, end_day) = end.fields();
            let mut months = (i64::from(end_year) - i64::from(start_year)) * 12
                + i64::from(end_month)
                - i64::from(start_month);
            if months > 0
                && i64::from(start_day).min(month_length(i64::from(end_year), i64::from(end_month)))
                    > i64::from(end_day)
            {
                months -= 1;
            }
            if unit == CalendarUnit::Year {
                months / 12
            } else {
                months
            }
        }
    }
}
