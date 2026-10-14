//! Declarative partial-date completion and precision preserve collected fields.
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    table::ValueRef,
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::Value,
};
use alloc::{format, string::String, vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayRule {
    First,
    Last,
    Number(u8),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayDeclaration<'a> {
    Number(&'a str),
    Position(&'a str),
}
/// Maximum borrowed policy/context bytes admitted before scans or copying.
pub const MAX_IMPUTATION_POLICY_BYTES: usize = 16_777_216;
#[derive(Clone, Debug, PartialEq)]
pub enum DatePolicyError {
    Diagnostic(Diagnostic),
    Limit,
    Shape,
}
/// Validate written calendar policy without narrowing arbitrary-width integers.
pub fn date_policy(
    month: Option<&str>,
    day: DayDeclaration<'_>,
    minimum: DatePrecision,
    path: &str,
) -> Result<ImputationKind, DatePolicyError> {
    let day_text = match day {
        DayDeclaration::Number(value) | DayDeclaration::Position(value) => value,
    };
    path.len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(month.map_or(0, str::len)))
        .and_then(|n| n.checked_add(day_text.len()))
        .and_then(|n| n.checked_add(512))
        .filter(|n| *n <= MAX_IMPUTATION_POLICY_BYTES)
        .ok_or(DatePolicyError::Limit)?;
    fn canonical_integer(text: &str) -> bool {
        let digits = text.strip_prefix('-').unwrap_or(text);
        if digits == "0" {
            return text == "0";
        }
        !digits.is_empty()
            && digits.as_bytes()[0] != b'0'
            && digits.bytes().all(|b| b.is_ascii_digit())
    }
    if path.is_empty()
        || minimum == DatePrecision::Day
        || month.is_some_and(|value| !canonical_integer(value))
        || matches!(day, DayDeclaration::Number(value) if !canonical_integer(value))
    {
        return Err(DatePolicyError::Shape);
    }
    fn number(value: &str) -> V {
        value
            .parse::<i64>()
            .map_or_else(|_| V::Integer(value.into()), |n| V::Scalar(Value::Int(n)))
    }
    fn error(code: C, path: &str, fields: alloc::vec::Vec<(&str, V)>) -> DatePolicyError {
        DatePolicyError::Diagnostic(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
            source_span: None,
            operand_route: None,
        })
    }
    let month = match (minimum, month) {
        (DatePrecision::Month, Some(value)) => {
            return Err(error(
                C::ImputationMonthNotPermitted,
                &format!("{path}.month"),
                vec![("month", number(value))],
            ))
        }
        (DatePrecision::Year, None) => {
            return Err(error(
                C::ImputationMonthRequired,
                &format!("{path}.month"),
                vec![(
                    "minimum_source_precision",
                    V::Scalar(Value::Str("year".into())),
                )],
            ))
        }
        (_, Some(value)) => Some(
            value
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=12).contains(n))
                .ok_or_else(|| {
                    error(
                        C::ImputationMonthRange,
                        &format!("{path}.month"),
                        vec![("month", number(value))],
                    )
                })?,
        ),
        (_, None) => None,
    };
    let day = match day {
        DayDeclaration::Position("first") => DayRule::First,
        DayDeclaration::Position("last") => DayRule::Last,
        DayDeclaration::Position(value) => {
            return Err(error(
                C::ImputationDayRule,
                &format!("{path}.day"),
                vec![
                    ("value", V::Scalar(Value::Str(value.into()))),
                    (
                        "permitted",
                        V::Sequence(vec![
                            V::Scalar(Value::Str("first".into())),
                            V::Scalar(Value::Str("last".into())),
                        ]),
                    ),
                ],
            ))
        }
        DayDeclaration::Number(value) => DayRule::Number(
            value
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=31).contains(n))
                .ok_or_else(|| {
                    error(
                        C::ImputationDayRange,
                        path,
                        vec![
                            ("field", V::Scalar(Value::Str("day".into()))),
                            ("value", number(value)),
                            ("permitted", V::Scalar(Value::Str("1 to 31".into()))),
                        ],
                    )
                })?,
        ),
    };
    Ok(ImputationKind::Date {
        month,
        day,
        minimum,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeRule {
    First,
    Last,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImputationKind {
    Date {
        month: Option<u8>,
        day: DayRule,
        minimum: DatePrecision,
    },
    DatePrecision,
    DateTime {
        time: TimeRule,
    },
    DateTimePrecision,
    ToDate,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Imputation {
    kind: ImputationKind,
    source: String,
    bound: Option<String>,
    path: String,
    missing: Option<Value>,
    invalid: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ImputationStep {
    Value(Value),
    Recovered { handler: &'static str },
    Bound(PartialDate),
}
/// A pending completion retains collected precision before calendar validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialDate {
    year: u16,
    month: u8,
    day: u8,
    precision: DatePrecision,
}
impl PartialDate {
    pub fn collected_precision(self) -> DatePrecision {
        self.precision
    }
    fn complete(self, path: &str) -> Result<Date, Diagnostic> {
        Date::new(self.year, self.month, self.day, self.precision).map_err(|_| {
            let value = match self.precision {
                DatePrecision::Year => format!("{:04}", self.year),
                DatePrecision::Month => format!("{:04}-{:02}", self.year, self.month),
                DatePrecision::Day => unreachable!("partial completion"),
            };
            diagnostic(
                C::DateImputeCalendar,
                path,
                &[
                    ("value", Value::Str(value)),
                    (
                        "completed",
                        Value::Str(format!(
                            "{:04}-{:02}-{:02}",
                            self.year, self.month, self.day
                        )),
                    ),
                ],
            )
        })
    }
}
impl Imputation {
    /// Admission includes temporary and retained declaration/context text copies.
    pub(crate) fn metadata_fits(
        source: &str,
        bound: Option<&str>,
        path: &str,
        missing: Option<&str>,
        invalid: Option<&str>,
    ) -> bool {
        [Some(source), bound, Some(path), missing, invalid]
            .into_iter()
            .flatten()
            .try_fold(0usize, |bytes, text| bytes.checked_add(text.len()))
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(512))
            .is_some_and(|n| n <= MAX_IMPUTATION_POLICY_BYTES)
    }
    /// Refuse malformed or oversized owned declarations before later copying.
    /// Runtime borrowed-value quotas remain the evaluator caller's authority.
    pub fn new(
        kind: ImputationKind,
        source: String,
        bound: Option<String>,
        path: String,
        missing: Option<Value>,
        invalid: Option<Value>,
    ) -> Option<Self> {
        fn handler_text(value: &Value) -> Option<&str> {
            match value {
                Value::Str(text) => Some(text.as_str()),
                _ => None,
            }
        }
        if !Self::metadata_fits(
            &source,
            bound.as_deref(),
            &path,
            missing.as_ref().and_then(handler_text),
            invalid.as_ref().and_then(handler_text),
        ) || source.is_empty()
            || path.is_empty()
            || bound.as_ref().is_some_and(String::is_empty)
        {
            return None;
        }
        match kind {
            ImputationKind::Date {
                month,
                day,
                minimum,
            } => {
                if !matches!(
                    (minimum, month),
                    (DatePrecision::Year, Some(1..=12)) | (DatePrecision::Month, None)
                ) || matches!(day, DayRule::Number(0 | 32..=255))
                {
                    return None;
                }
            }
            _ if bound.is_some() => return None,
            _ => (),
        }
        if matches!(kind, ImputationKind::ToDate) && (missing.is_some() || invalid.is_some()) {
            return None;
        }
        Some(Self {
            kind,
            source,
            bound,
            path,
            missing,
            invalid,
        })
    }
    pub fn kind(&self) -> ImputationKind {
        self.kind
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn bound_source(&self) -> Option<&str> {
        self.bound.as_deref()
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn handler(&self, name: &str) -> Option<&Value> {
        match name {
            "missing" => self.missing.as_ref(),
            "invalid" => self.invalid.as_ref(),
            _ => None,
        }
    }
    pub fn operation(&self) -> &'static str {
        match self.kind {
            ImputationKind::Date { .. } => "date_impute",
            ImputationKind::DatePrecision => "date_precision",
            ImputationKind::DateTime { .. } => "datetime_impute",
            ImputationKind::DateTimePrecision => "datetime_precision",
            ImputationKind::ToDate => "to_date",
        }
    }
    pub fn expected(&self) -> &'static str {
        match self.kind {
            ImputationKind::Date { .. } | ImputationKind::DateTime { .. } => "str",
            ImputationKind::DatePrecision => "str or date",
            ImputationKind::DateTimePrecision => "str or datetime",
            ImputationKind::ToDate => "datetime, ISO date text, or ISO 8601 datetime text",
        }
    }
    pub fn accepts(&self, kind: crate::value::ColumnType) -> bool {
        use crate::value::ColumnType as T;
        match self.kind {
            ImputationKind::Date { .. } | ImputationKind::DatePrecision => {
                matches!(kind, T::Str | T::Date)
            }
            ImputationKind::DateTime { .. }
            | ImputationKind::DateTimePrecision
            | ImputationKind::ToDate => matches!(kind, T::Str | T::DateTime),
        }
    }
    pub(crate) fn input_diagnostic(&self, actual: &str) -> Diagnostic {
        let code = match self.kind {
            ImputationKind::DateTime { .. } => C::DatetimeImputeInput,
            ImputationKind::DateTimePrecision => C::DatetimePrecisionInput,
            ImputationKind::ToDate => C::ToDateInput,
            _ => C::TemporalOperand,
        };
        diagnostic(
            code,
            &format!("{}.source", self.path),
            &[
                ("source", Value::Str(self.source.clone())),
                ("expected", Value::Str(self.expected().into())),
                ("actual", Value::Str(actual.into())),
            ],
        )
    }
    pub(crate) fn bound_input_diagnostic(&self, actual: &str) -> Diagnostic {
        diagnostic(
            C::TemporalOperand,
            &format!("{}.not_before", self.path),
            &[
                (
                    "source",
                    Value::Str(self.bound.clone().expect("declared bound")),
                ),
                ("expected", Value::Str("date".into())),
                ("actual", Value::Str(actual.into())),
            ],
        )
    }
    fn recover(
        &self,
        name: &'static str,
        text: Option<&str>,
    ) -> Result<ImputationStep, Diagnostic> {
        if self.handler(name).is_some() {
            return Ok(ImputationStep::Recovered { handler: name });
        }
        let code = match (self.kind, name) {
            (ImputationKind::DateTime { .. }, "missing") => C::DatetimeImputeMissing,
            (ImputationKind::DateTime { .. }, _) => C::DatetimeImputeInvalid,
            (ImputationKind::DateTimePrecision, "missing") => C::DatetimePrecisionMissing,
            (ImputationKind::DateTimePrecision, _) => C::DatetimePrecisionInvalid,
            (_, "missing") => C::DateImputeMissing,
            _ => C::DateImputeInvalid,
        };
        let mut fields = vec![
            ("operation", Value::Str(self.operation().into())),
            ("source", Value::Str(self.source.clone())),
        ];
        if let Some(text) = text {
            fields.push(("value", Value::Str(text.into())));
        }
        Err(diagnostic(code, &self.path, &fields))
    }
    /// Source completion is a separate step so unused bounds have no read effects.
    /// The caller must admit borrowed source text work and storage before calling;
    /// the engine does so with its cumulative work and scalar-text budgets. This
    /// pure service preserves semantic findings and does not own runtime quotas.
    pub fn source_value(&self, value: ValueRef<'_>) -> Result<ImputationStep, Diagnostic> {
        use ImputationKind as K;
        if value == ValueRef::Missing {
            return if matches!(self.kind, K::ToDate) {
                Ok(ImputationStep::Value(Value::Missing))
            } else {
                self.recover("missing", None)
            };
        }
        let result = match (self.kind, value) {
            (K::Date { .. }, ValueRef::Date(date)) => Value::Date(date),
            (K::DatePrecision, ValueRef::Date(date)) => date_code(date.collected_precision()),
            (K::DateTime { .. }, ValueRef::DateTime(date)) => Value::DateTime(date),
            (K::DateTimePrecision, ValueRef::DateTime(date)) => {
                datetime_code(date.collected_precision())
            }
            (K::ToDate, ValueRef::DateTime(moment)) => Value::Date(moment_date(moment)),
            (K::ToDate, ValueRef::Str(text)) => {
                if let Ok(date) = text.parse::<Date>() {
                    Value::Date(date)
                } else if let Ok(moment) = text.parse::<DateTime>() {
                    Value::Date(moment_date(moment))
                } else {
                    return Err(diagnostic(
                        C::ToDateInvalid,
                        &self.path,
                        &[
                            ("operation", Value::Str("to_date".into())),
                            ("value", Value::Str(text.into())),
                        ],
                    ));
                }
            }
            (
                K::Date {
                    month,
                    day,
                    minimum,
                },
                ValueRef::Str(text),
            ) => {
                let Some((year, collected_month, collected_day, precision)) = prefix(text) else {
                    return self.recover("invalid", Some(text));
                };
                if precision == DatePrecision::Year && minimum == DatePrecision::Month {
                    return Ok(ImputationStep::Value(Value::Missing));
                }
                if let Some(day) = collected_day {
                    return Ok(ImputationStep::Value(Value::Date(
                        Date::new(year, collected_month.unwrap(), day, DatePrecision::Day).unwrap(),
                    )));
                }
                let month = collected_month
                    .or(month)
                    .expect("admitted completion month");
                let day = match day {
                    DayRule::First => 1,
                    DayRule::Last => month_length(year, month),
                    DayRule::Number(day) => day,
                };
                let partial = PartialDate {
                    year,
                    month,
                    day,
                    precision,
                };
                if self.bound.is_some() {
                    return Ok(ImputationStep::Bound(partial));
                }
                Value::Date(partial.complete(&self.path)?)
            }
            (K::DatePrecision, ValueRef::Str(text)) => {
                let Some((_, _, _, precision)) = prefix(text) else {
                    return self.recover("invalid", Some(text));
                };
                date_code(precision)
            }
            (K::DateTime { time }, ValueRef::Str(text)) => {
                if let Ok(moment) = text.parse::<DateTime>() {
                    Value::DateTime(moment)
                } else if let Ok(date) = text.parse::<Date>() {
                    let (hour, minute, second) = match time {
                        TimeRule::First => (0, 0, 0),
                        TimeRule::Last => (23, 59, 59),
                    };
                    Value::DateTime(
                        DateTime::new(date, hour, minute, second, DateTimePrecision::Day).unwrap(),
                    )
                } else {
                    return self.recover("invalid", Some(text));
                }
            }
            (K::DateTimePrecision, ValueRef::Str(text)) => {
                if text.parse::<DateTime>().is_ok() {
                    datetime_code(DateTimePrecision::Second)
                } else if text.parse::<Date>().is_ok() {
                    datetime_code(DateTimePrecision::Day)
                } else {
                    return self.recover("invalid", Some(text));
                }
            }
            (_, other) => return Err(self.input_diagnostic(type_name(other))),
        };
        Ok(ImputationStep::Value(result))
    }
    /// Move only imputed components within the collected year or month.
    pub fn apply_bound(
        &self,
        completed: PartialDate,
        bound: ValueRef<'_>,
    ) -> Result<Value, Diagnostic> {
        let lower = match bound {
            ValueRef::Missing => None,
            ValueRef::Date(date) => Some(date),
            other => return Err(self.bound_input_diagnostic(type_name(other))),
        };
        let completed = completed.complete(&self.path)?;
        let Some(lower) = lower else {
            return Ok(Value::Date(completed));
        };
        if completed >= lower {
            return Ok(Value::Date(completed));
        }
        let (year, month, _) = completed.fields();
        let upper = match completed.collected_precision() {
            DatePrecision::Year => Date::new(year, 12, 31, DatePrecision::Day).unwrap(),
            DatePrecision::Month => {
                Date::new(year, month, month_length(year, month), DatePrecision::Day).unwrap()
            }
            DatePrecision::Day => unreachable!("complete dates skip bounds"),
        };
        if lower > upper {
            return Ok(Value::Missing);
        }
        let (year, month, day) = lower.fields();
        Ok(Value::Date(
            Date::new(year, month, day, completed.collected_precision()).unwrap(),
        ))
    }
}
fn moment_date(moment: DateTime) -> Date {
    let (year, month, day, _, _, _) = moment.fields();
    Date::new(year, month, day, DatePrecision::Day).unwrap()
}
fn date_code(precision: DatePrecision) -> Value {
    Value::Str(
        match precision {
            DatePrecision::Year => "Y",
            DatePrecision::Month => "M",
            DatePrecision::Day => "D",
        }
        .into(),
    )
}
fn datetime_code(precision: DateTimePrecision) -> Value {
    Value::Str(
        match precision {
            DateTimePrecision::Day => "D",
            DateTimePrecision::Second => "S",
        }
        .into(),
    )
}
fn type_name(value: ValueRef<'_>) -> &'static str {
    match value {
        ValueRef::Str(_) => "str",
        ValueRef::Int(_) => "int",
        ValueRef::Float(_) => "float",
        ValueRef::Bool(_) => "bool",
        ValueRef::Date(_) => "date",
        ValueRef::DateTime(_) => "datetime",
        ValueRef::Missing => "missing",
    }
}
fn month_length(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => unreachable!("admitted month"),
    }
}
fn prefix(text: &str) -> Option<(u16, Option<u8>, Option<u8>, DatePrecision)> {
    let bytes = text.as_bytes();
    if !matches!(bytes.len(), 4 | 7 | 10) || !bytes[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let year = text[..4].parse::<u16>().ok().filter(|n| *n > 0)?;
    if bytes.len() == 4 {
        return Some((year, None, None, DatePrecision::Year));
    }
    if bytes[4] != b'-' || !bytes[5..7].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let month = text[5..7]
        .parse::<u8>()
        .ok()
        .filter(|n| (1..=12).contains(n))?;
    if bytes.len() == 7 {
        return Some((year, Some(month), None, DatePrecision::Month));
    }
    let date = text.parse::<Date>().ok()?;
    Some((year, Some(month), Some(date.fields().2), DatePrecision::Day))
}
pub(crate) fn diagnostic(code: C, path: &str, fields: &[(&str, Value)]) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: fields
            .iter()
            .map(|(key, value)| ((*key).into(), V::Scalar(value.clone())))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}
