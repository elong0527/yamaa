//! Bound complete-date operations retain authored operands and calendar semantics.
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    table::ValueRef,
    temporal::Date,
    value::Value,
};
use alloc::{format, string::String, vec, vec::Vec};

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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidTemporal {
    Shape,
    Bounds,
    Limit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Temporal {
    kind: TemporalKind,
    sources: Vec<String>,
    path: String,
}
impl Temporal {
    pub fn new(
        kind: TemporalKind,
        sources: Vec<String>,
        path: String,
    ) -> Result<Self, InvalidTemporal> {
        let context_bytes = sources
            .iter()
            .try_fold(path.len(), |total, source| total.checked_add(source.len()))
            .and_then(|bytes| bytes.checked_mul(6))
            .and_then(|bytes| bytes.checked_add(1024));
        if context_bytes.is_none_or(|bytes| bytes > crate::diagnostic::MAX_PROJECTION_BYTES) {
            return Err(InvalidTemporal::Limit);
        }
        if path.is_empty()
            || sources.iter().any(String::is_empty)
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
            sources,
            path,
        })
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
                .map(|(key, value)| (key.into(), ContextValue::Scalar(Value::Str(value.into()))))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    pub(crate) fn bounds_diagnostic(path: &str, value: &str) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::TemporalBounds,
            spec_paths: vec![format!("{path}.bounds")],
            context: vec![
                (
                    "value".into(),
                    ContextValue::Scalar(Value::Str(value.into())),
                ),
                (
                    "permitted".into(),
                    ContextValue::Sequence(vec![ContextValue::Scalar(Value::Str(
                        "exclusive".into(),
                    ))]),
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
    /// Admitted operation arity is enforced by the immutable dataset plan.
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
        }))
    }
}
impl TemporalKind {
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Difference { .. } => &["start", "end"],
            Self::StudyDay => &["date", "reference"],
            Self::EpochDay => &["source"],
        }
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
fn ordinal(date: Date) -> i64 {
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
