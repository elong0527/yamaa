//! Typed civil arithmetic and duration conversion share the existing temporal binder.
use super::temporal::{ordinal, ElapsedUnit, Temporal, TemporalKind};
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    temporal::{Date, DateTime, DateTimePrecision},
    value::{ColumnType, Value},
};
use std::vec;

#[derive(Debug, PartialEq)]
pub enum TemporalStep {
    Value(Value),
    Invalid,
}

impl Temporal {
    pub fn literal(&self, index: usize) -> Option<&Value> {
        self.literals.get(index)?.as_ref()
    }
    pub fn with_literal(mut self, index: usize, value: Value) -> Self {
        self.literals[index] = Some(value);
        self
    }
    pub fn with_invalid(mut self, value: Value) -> Result<Self, super::InvalidTemporal> {
        if !matches!(self.kind, TemporalKind::DurationValue { .. }) {
            return Err(super::InvalidTemporal::Shape);
        }
        self.invalid = Some(value);
        Ok(self)
    }
    pub fn invalid_handler(&self) -> Option<&Value> {
        self.invalid.as_ref()
    }
    pub fn expected(&self, index: usize) -> &'static str {
        match (self.kind, index) {
            (TemporalKind::DateShift, 1) => "int",
            (
                TemporalKind::DateTimeShift {
                    unit: ElapsedUnit::Second,
                },
                1,
            ) => "int",
            (TemporalKind::DateTimeShift { .. }, 1) | (TemporalKind::DurationText { .. }, _) => {
                "numeric"
            }
            (TemporalKind::DateTimeShift { .. } | TemporalKind::DateTimeDifference { .. }, _) => {
                "datetime"
            }
            (TemporalKind::DurationValue { .. }, _) => "str",
            _ => "date",
        }
    }
    pub fn accepts(&self, index: usize, kind: ColumnType) -> bool {
        match self.expected(index) {
            "date" => kind == ColumnType::Date,
            "datetime" => kind == ColumnType::DateTime,
            "int" => kind == ColumnType::Int,
            "str" => kind == ColumnType::Str,
            _ => matches!(kind, ColumnType::Int | ColumnType::Float),
        }
    }
    pub fn operand_diagnostic(&self, index: usize, actual: &str) -> Diagnostic {
        let code = match self.kind {
            TemporalKind::DateShift => C::DateShiftInput,
            TemporalKind::DateTimeDifference { .. } => C::DatetimeDifferenceInput,
            TemporalKind::DateTimeShift { .. } => C::DatetimeShiftInput,
            TemporalKind::DurationValue { .. } => C::DurationValueInput,
            TemporalKind::DurationText { .. } => C::DurationTextInput,
            _ => C::TemporalOperand,
        };
        Diagnostic {
            code,
            spec_paths: vec![self.operand_path(index).expect("admitted operand")],
            context: [
                ("source", self.sources[index].as_str()),
                ("expected", self.expected(index)),
                ("actual", actual),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
            .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    fn failure(&self, code: C) -> Diagnostic {
        Diagnostic {
            code,
            spec_paths: vec![self.path.clone()],
            context: Default::default(),
            source_span: None,
            operand_route: None,
        }
    }
    pub fn apply_values(&self, values: &[Value]) -> Option<Result<TemporalStep, Diagnostic>> {
        (values.len() == self.sources.len()).then(|| self.apply_admitted_values(values))
    }
    /// Validate each reached operand before the next one is evaluated.
    pub fn validate_operand(&self, index: usize, value: &Value) -> Option<Result<(), Diagnostic>> {
        self.sources.get(index)?;
        let actual = match value {
            Value::Missing => return Some(Ok(())),
            Value::Str(_) => (ColumnType::Str, "str"),
            Value::Int(_) => (ColumnType::Int, "int"),
            Value::Float(_) => (ColumnType::Float, "float"),
            Value::Date(_) => (ColumnType::Date, "date"),
            Value::DateTime(_) => (ColumnType::DateTime, "datetime"),
            Value::Bool(_) => return Some(Err(self.operand_diagnostic(index, "bool"))),
        };
        Some(if self.accepts(index, actual.0) {
            Ok(())
        } else {
            Err(self.operand_diagnostic(index, actual.1))
        })
    }
    fn apply_admitted_values(&self, values: &[Value]) -> Result<TemporalStep, Diagnostic> {
        for (index, value) in values.iter().enumerate() {
            self.validate_operand(index, value)
                .expect("admitted operand index")?;
        }
        if values.iter().any(|value| matches!(value, Value::Missing)) {
            return Ok(TemporalStep::Value(Value::Missing));
        }
        let result = match self.kind {
            TemporalKind::DateShift => {
                let (Value::Date(date), Value::Int(days)) = (&values[0], &values[1]) else {
                    unreachable!()
                };
                ordinal(*date)
                    .checked_add(*days)
                    .and_then(|day| date_from_ordinal(day, date.collected_precision()))
                    .map(Value::Date)
                    .ok_or_else(|| self.failure(C::DateShiftRange))?
            }
            TemporalKind::DateTimeDifference { unit } => {
                let (Value::DateTime(start), Value::DateTime(end)) = (&values[0], &values[1])
                else {
                    unreachable!()
                };
                let seconds = civil_seconds(*end) - civil_seconds(*start);
                if unit == ElapsedUnit::Second {
                    Value::Int(seconds)
                } else {
                    Value::float(seconds as f64 / f64::from(unit.seconds()))
                }
            }
            TemporalKind::DateTimeShift { unit } => {
                let Value::DateTime(source) = values[0] else {
                    unreachable!()
                };
                let base = civil_seconds(source);
                let target = match values[1] {
                    Value::Int(amount) => amount
                        .checked_mul(i64::from(unit.seconds()))
                        .and_then(|delta| base.checked_add(delta)),
                    Value::Float(amount) => {
                        rounded_target(base, amount.get() * f64::from(unit.seconds()))
                    }
                    _ => unreachable!(),
                };
                target
                    .and_then(|target| datetime_from_seconds(target, source.collected_precision()))
                    .map(Value::DateTime)
                    .ok_or_else(|| self.failure(C::DatetimeShiftRange))?
            }
            TemporalKind::DurationValue { unit } => {
                let Value::Str(ref text) = values[0] else {
                    unreachable!()
                };
                match super::duration::read(text, unit) {
                    Some(value) => value,
                    None if self.invalid.is_some() => return Ok(TemporalStep::Invalid),
                    None => return Err(self.failure(C::DurationInvalid)),
                }
            }
            TemporalKind::DurationText { unit } => super::duration::write(&values[0], unit)
                .map(Value::Str)
                .ok_or_else(|| self.failure(C::DurationTextRange))?,
            _ => {
                let dates: Vec<_> = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        self.operand(index, ValueRef::from(value)).unwrap().unwrap()
                    })
                    .collect();
                self.apply(&dates).expect("admitted temporal arity")
            }
        };
        Ok(TemporalStep::Value(result))
    }
}

fn date_from_ordinal(day: i64, precision: crate::temporal::DatePrecision) -> Option<Date> {
    if !(1..=3_652_059).contains(&day) {
        return None;
    }
    let mut low: u16 = 1;
    let mut high: u16 = 9999;
    while low < high {
        let middle = (low + high).div_ceil(2);
        if ordinal(Date::new(middle, 1, 1, precision).ok()?) <= day {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let mut month = 1;
    while month < 12 && ordinal(Date::new(low, month + 1, 1, precision).ok()?) <= day {
        month += 1;
    }
    let date = Date::new(low, month, 1, precision).ok()?;
    Date::new(low, month, (day - ordinal(date) + 1) as u8, precision).ok()
}
fn civil_seconds(value: DateTime) -> i64 {
    let (year, month, day, hour, minute, second) = value.fields();
    let date = Date::new(year, month, day, crate::temporal::DatePrecision::Day).unwrap();
    (ordinal(date) - 1) * 86400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second)
}
fn datetime_from_seconds(seconds: i64, precision: DateTimePrecision) -> Option<DateTime> {
    if !(0..315_537_897_600).contains(&seconds) {
        return None;
    }
    let date = date_from_ordinal(seconds / 86400 + 1, crate::temporal::DatePrecision::Day)?;
    let time = seconds % 86400;
    DateTime::new(
        date,
        (time / 3600) as u8,
        (time / 60 % 60) as u8,
        (time % 60) as u8,
        precision,
    )
    .ok()
}
fn rounded_target(base: i64, delta: f64) -> Option<i64> {
    // Bound before casting and add the small fraction after the exact integer,
    // preserving ties even at year 9999. Parity belongs to the resulting second.
    if !delta.is_finite() || !(-315_537_897_601.0..=315_537_897_601.0).contains(&delta) {
        return None;
    }
    let lower = libm::floor(delta);
    let target = base.checked_add(lower as i64)?;
    let fraction = delta - lower;
    target.checked_add(i64::from(
        fraction > 0.5 || fraction == 0.5 && target % 2 != 0,
    ))
}
