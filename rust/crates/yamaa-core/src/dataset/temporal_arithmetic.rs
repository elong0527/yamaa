//! Typed civil arithmetic and duration conversion share the existing temporal binder.
use super::temporal::{ordinal, ElapsedUnit, Temporal, TemporalKind, DATE_PARTS};
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    temporal::{Date, DateTime, Precision},
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
        if !matches!(
            self.kind,
            TemporalKind::DurationValue { .. } | TemporalKind::DateText { .. }
        ) {
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
            (TemporalKind::DateText { .. }, _) => "str or int",
            (TemporalKind::TimeOfDay, _) => "datetime",
            _ => "date",
        }
    }
    pub fn accepts(&self, index: usize, kind: ColumnType) -> bool {
        match self.expected(index) {
            "date" => kind == ColumnType::Date,
            "datetime" => kind == ColumnType::DateTime,
            "int" => kind == ColumnType::Int,
            "str" => kind == ColumnType::Str,
            "str or int" => matches!(kind, ColumnType::Str | ColumnType::Int),
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
            TemporalKind::DateText { .. } => C::DateTextInput,
            TemporalKind::TimeOfDay => C::TimeOfDayInput,
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
    /// A result outside its type's range or a text that does not read, with
    /// the facts its message names.
    fn failure(&self, code: C, context: &[(&str, JsonValue)]) -> Diagnostic {
        Diagnostic {
            code,
            spec_paths: vec![self.path.clone()],
            context: context
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone()))
                .collect(),
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
        // `dtc` reads no part after its first missing one, so checks none.
        let missing = values.iter().position(|value| *value == Value::Missing);
        let read = match missing {
            Some(index) if self.kind.stops_at_missing() => index + 1,
            _ => values.len(),
        };
        for (index, value) in values[..read].iter().enumerate() {
            self.validate_operand(index, value)
                .expect("admitted operand index")?;
        }
        // A missing part ends collected date text rather than the result.
        if let TemporalKind::DateText { .. } = self.kind {
            return self.date_text(values);
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
                    .ok_or_else(|| {
                        self.failure(C::DateShiftRange, &[("to", JsonValue::from("date"))])
                    })?
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
                    .ok_or_else(|| {
                        self.failure(
                            C::DatetimeShiftRange,
                            &[("to", JsonValue::from("datetime"))],
                        )
                    })?
            }
            TemporalKind::DurationValue { unit } => {
                let Value::Str(ref text) = values[0] else {
                    unreachable!()
                };
                match super::duration::read(text, unit) {
                    Some(value) => value,
                    None if self.invalid.is_some() => return Ok(TemporalStep::Invalid),
                    None => {
                        return Err(self.failure(
                            C::DurationInvalid,
                            &[
                                ("source", JsonValue::from(self.sources[0].as_str())),
                                ("value", JsonValue::from(text.as_str())),
                            ],
                        ))
                    }
                }
            }
            // A float whose seconds leave the binary64 range has no text.
            TemporalKind::DurationText { unit } => super::duration::write(&values[0], unit)
                .map(Value::Str)
                .ok_or_else(|| {
                    self.failure(C::DurationTextRange, &[("to", JsonValue::from("str"))])
                })?,
            TemporalKind::TimeOfDay => {
                let Value::DateTime(moment) = values[0] else {
                    unreachable!()
                };
                let (_, _, _, hour, minute, second) = moment.fields();
                Value::Int(i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
            }
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

impl Temporal {
    /// Assemble collected date text from the declared parts in order, stopping
    /// before the first missing part; a missing year gives missing. A `str`
    /// part is its field's digits at the field's width and an `int` part is
    /// written zero-padded, each within the field's range, and a day within
    /// its month. Parts after the first missing one are never read.
    fn date_text(&self, values: &[Value]) -> Result<TemporalStep, Diagnostic> {
        // (separator, width, lowest, highest) for each part of DATE_PARTS.
        const FIELDS: [(&str, usize, i64, i64); 6] = [
            ("", 4, 1, 9999),
            ("-", 2, 1, 12),
            ("-", 2, 1, 31),
            ("T", 2, 0, 23),
            (":", 2, 0, 59),
            (":", 2, 0, 59),
        ];
        let mut text = String::new();
        let mut read = [0i64; 2];
        for (index, value) in values.iter().enumerate() {
            let (separator, width, lowest, highest) = FIELDS[index];
            let number = match value {
                Value::Missing => break,
                Value::Str(digits) => (digits.len() == width
                    && digits.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| digits.parse::<i64>().expect("ASCII digits")),
                Value::Int(number) => Some(*number),
                _ => unreachable!("admitted date part"),
            };
            let highest = if index == 2 {
                crate::temporal::month_length(read[0] as u16, read[1] as u8)
                    .map_or(highest, i64::from)
            } else {
                highest
            };
            let Some(number) = number.filter(|number| (lowest..=highest).contains(number)) else {
                if self.invalid.is_some() {
                    return Ok(TemporalStep::Invalid);
                }
                return Err(Diagnostic {
                    code: C::DateTextInvalid,
                    spec_paths: vec![self.path.clone()],
                    context: [
                        ("operation".into(), JsonValue::String("dtc".into())),
                        ("part".into(), JsonValue::String(DATE_PARTS[index].into())),
                        (
                            "source".into(),
                            JsonValue::String(self.sources[index].clone()),
                        ),
                        ("value".into(), JsonValue::from(value.clone())),
                    ]
                    .into_iter()
                    .collect(),
                    source_span: None,
                    operand_route: None,
                });
            };
            if index < 2 {
                read[index] = number;
            }
            text.push_str(separator);
            text.push_str(&format!("{number:0width$}"));
        }
        Ok(TemporalStep::Value(if text.is_empty() {
            Value::Missing
        } else {
            Value::Str(text)
        }))
    }
}

fn date_from_ordinal(day: i64, precision: crate::temporal::Precision) -> Option<Date> {
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
    let date = Date::new(year, month, day, crate::temporal::Precision::Day).unwrap();
    (ordinal(date) - 1) * 86400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second)
}
fn datetime_from_seconds(seconds: i64, precision: Precision) -> Option<DateTime> {
    if !(0..315_537_897_600).contains(&seconds) {
        return None;
    }
    let date = date_from_ordinal(seconds / 86400 + 1, crate::temporal::Precision::Day)?;
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
/// Resolve the binary64 amount in seconds to whole seconds with the language's
/// one rounding (FORMULA-16) at zero places, then add it to the exact source.
/// The source second never affects the rounding. Bound before converting: an
/// admitted magnitude has at most twelve integer digits.
fn rounded_target(base: i64, delta: f64) -> Option<i64> {
    if !delta.is_finite() || !(-315_537_897_601.0..=315_537_897_601.0).contains(&delta) {
        return None;
    }
    let seconds = i64::try_from(&crate::decimal_rounding::rounded_magnitude(delta, 0)).ok()?;
    base.checked_add(if delta.is_sign_negative() {
        -seconds
    } else {
        seconds
    })
}
