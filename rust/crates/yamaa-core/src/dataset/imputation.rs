//! Declarative completion, precision and imputation flags read collected
//! ISO 8601 (SDTM `--DTC`) text up to its first unknown field and preserve
//! each collected field. `date_text` reads text written in one of its
//! declared date forms into such text; no other operation here reads any
//! other form.
use crate::json::Value as V;
use crate::{
    date_forms::DateForms,
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    temporal::{month_length, Date, DateTime, Precision},
    value::{ColumnType, Value},
};

/// The temporal type an operation declares with `type`, never inferred from text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalType {
    Date,
    DateTime,
}

impl TemporalType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::DateTime => "datetime",
        }
    }
    pub fn column_type(self) -> ColumnType {
        match self {
            Self::Date => ColumnType::Date,
            Self::DateTime => ColumnType::DateTime,
        }
    }
}

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeRule {
    First,
    Last,
}
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum DatePolicyError {
    #[error("dataset imputation diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
    #[error("dataset imputation shape")]
    Shape,
}

/// What `impute` may invent and how: the declared target type, the coarsest
/// collected precision it completes, and the month, day and time rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub target: TemporalType,
    pub month: Option<u8>,
    pub day: Option<DayRule>,
    pub minimum: Precision,
    pub time: Option<TimeRule>,
}

/// Validate written completion policy without narrowing arbitrary-width integers.
pub fn completion_policy(
    target: TemporalType,
    month: Option<&str>,
    day: Option<DayDeclaration<'_>>,
    time: Option<TimeRule>,
    path: &str,
) -> Result<Completion, DatePolicyError> {
    completion_policy_findings(target, month, day, time, path)?.map_err(|findings| {
        DatePolicyError::Diagnostic(
            findings
                .into_iter()
                .next()
                .expect("invalid policy has a finding"),
        )
    })
}

/// Retain independent written month and day findings in declaration order.
/// The rules imply the coarsest collected precision `impute` completes
/// (TIME-8): a year alone with `month` and `day`, a year and month with `day`
/// alone, and a whole date with neither; `month` without `day` completes no
/// date and is refused (TIME-10). The type-dependent `time` field is checked
/// where the operation is read.
pub(crate) fn completion_policy_findings(
    target: TemporalType,
    month: Option<&str>,
    day: Option<DayDeclaration<'_>>,
    time: Option<TimeRule>,
    path: &str,
) -> Result<Result<Completion, std::vec::Vec<Diagnostic>>, DatePolicyError> {
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
        || (target == TemporalType::DateTime) != time.is_some()
        || month.is_some_and(|value| !canonical_integer(value))
        || matches!(day, Some(DayDeclaration::Number(value)) if !canonical_integer(value))
    {
        return Err(DatePolicyError::Shape);
    }
    fn number(value: &str) -> V {
        value.parse::<i64>().map_or_else(
            |_| V::Number((value).parse().expect("canonical integer")),
            V::from,
        )
    }
    fn error(code: C, path: &str, fields: std::vec::Vec<(&str, V)>) -> Diagnostic {
        Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    let minimum = match (month, day) {
        (Some(_), _) => Precision::Year,
        (None, Some(_)) => Precision::Month,
        (None, None) => Precision::Day,
    };
    let month = month
        .map(|value| {
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
                })
        })
        .transpose();
    let day = match day {
        None if minimum == Precision::Year => Err(error(
            C::ImputeMonthWithoutDay,
            path,
            vec![("field", V::String("day".into()))],
        )),
        None => Ok(None),
        Some(DayDeclaration::Position("first")) => Ok(Some(DayRule::First)),
        Some(DayDeclaration::Position("last")) => Ok(Some(DayRule::Last)),
        Some(DayDeclaration::Position(value)) => Err(error(
            C::ImputationDayRule,
            &format!("{path}.day"),
            vec![
                ("value", V::String(value.into())),
                (
                    "permitted",
                    V::Array(vec![V::String("first".into()), V::String("last".into())]),
                ),
            ],
        )),
        Some(DayDeclaration::Number(value)) => value
            .parse::<u8>()
            .ok()
            .filter(|n| (1..=31).contains(n))
            .map(|day| Some(DayRule::Number(day)))
            .ok_or_else(|| {
                error(
                    C::ImputationDayRange,
                    path,
                    vec![
                        ("field", V::String("day".into())),
                        ("value", number(value)),
                        ("permitted", V::String("1 to 31".into())),
                    ],
                )
            }),
    };
    match (month, day) {
        (Ok(month), Ok(day)) => Ok(Ok(Completion {
            target,
            month,
            day,
            minimum,
            time,
        })),
        (month, day) => Ok(Err(month.err().into_iter().chain(day.err()).collect())),
    }
}

/// The ADaM imputation flag an `imputation_flag` derives: `*DTF` or `*TMF`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagPart {
    Date,
    Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImputationKind {
    Impute(Completion),
    Precision {
        target: TemporalType,
    },
    Flag {
        part: FlagPart,
        seconds_collected: bool,
    },
    ToDate,
    /// `date_text`: text in one of the declared forms, read into the
    /// collected date text it denotes (TIME-22).
    DateText(DateForms),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Imputation {
    kind: ImputationKind,
    source: String,
    not_before: Option<String>,
    not_after: Option<String>,
    fallback: Option<String>,
    path: String,
    invalid: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ImputationStep {
    Value(Value),
    /// A declared `invalid` answers text that is not collected date text,
    /// such as an ISO 8601 interval or duration (TIME-13).
    Recovered {
        handler: &'static str,
    },
    /// A missing source: `impute`'s fallback answers it, otherwise the result
    /// is missing and counted as a default substitution.
    Missing,
    /// `to_date` read year or year-month text; the result is missing and counted.
    Partial,
    /// A completed value whose declared bounds still apply.
    Bound(Pending),
}

/// A completion awaiting its bounds, with the interval the collected
/// components admit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Date {
        completed: Date,
        earliest: Date,
        latest: Date,
    },
    DateTime {
        completed: DateTime,
        earliest: DateTime,
        latest: DateTime,
    },
}

impl Pending {
    pub fn collected_precision(self) -> Precision {
        match self {
            Self::Date { completed, .. } => completed.collected_precision(),
            Self::DateTime { completed, .. } => completed.collected_precision(),
        }
    }
}

/// The collected components of right-truncated `--DTC` text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Collected {
    year: u16,
    month: Option<u8>,
    day: Option<u8>,
    time: [Option<u8>; 3],
}

impl Collected {
    fn precision(self) -> Precision {
        match (self.month, self.day, self.time) {
            (_, _, [_, _, Some(_)]) => Precision::Second,
            (_, _, [_, Some(_), _]) => Precision::Minute,
            (_, _, [Some(_), _, _]) => Precision::Hour,
            (_, Some(_), _) => Precision::Day,
            (Some(_), _, _) => Precision::Month,
            _ => Precision::Year,
        }
    }
    fn date(self) -> Option<Date> {
        Date::new(self.year, self.month?, self.day?, Precision::Day).ok()
    }
    /// Collected year or year-month text, the part a calendar diagnostic names.
    fn prefix(self) -> String {
        match self.month {
            Some(month) => format!("{:04}-{month:02}", self.year),
            None => format!("{:04}", self.year),
        }
    }
}

/// Why text is not collected date text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotCollected {
    /// An ISO 8601 interval or duration, which names no single date.
    IntervalOrDuration,
    Invalid,
}

/// Read `YYYY[-MM[-DD[Thh[:mm[:ss]]]]]`, each field with the width and range
/// of a complete temporal value or written `-` when unknown, and reduce it to
/// its right-truncated prefix: the fields before the first unknown one.
/// `Ok(None)` is text whose year is unknown, so its prefix carries nothing.
fn collected(text: &str) -> Result<Option<Collected>, NotCollected> {
    let Some(fields) = crate::iso8601::collected(text) else {
        return Err(if crate::iso8601::interval_or_duration(text) {
            NotCollected::IntervalOrDuration
        } else {
            NotCollected::Invalid
        });
    };
    let prefix = fields
        .iter()
        .take_while(|field| field.is_some())
        .map(|field| field.expect("known prefix field"))
        .collect::<Vec<_>>();
    let field = |index: usize| prefix.get(index).map(|&value| value as u8);
    let Some(&year) = prefix.first() else {
        return Ok(None);
    };
    Ok(Some(Collected {
        year,
        month: field(1),
        day: field(2),
        time: [field(3), field(4), field(5)],
    }))
}

impl Completion {
    fn day_of(&self, year: u16, month: u8) -> u8 {
        match self.day.expect("admitted completion day") {
            DayRule::First => 1,
            DayRule::Last => month_length(year, month).expect("admitted month"),
            DayRule::Number(day) => day,
        }
    }
    /// Complete the date part from the declared rules, or the calendar diagnostic.
    fn complete_date(
        &self,
        collected: Collected,
        precision: Precision,
        path: &str,
    ) -> Result<Date, Diagnostic> {
        if let Some(date) = collected.date() {
            return Ok(date.with_precision(precision));
        }
        let month = collected
            .month
            .or(self.month)
            .expect("admitted completion month");
        let day = self.day_of(collected.year, month);
        Date::new(collected.year, month, day, precision.min(Precision::Day)).map_err(|_| {
            diagnostic(
                C::ImputeCalendar,
                path,
                &[
                    ("value", Value::Str(collected.prefix())),
                    (
                        "completed",
                        Value::Str(format!("{:04}-{month:02}-{day:02}", collected.year)),
                    ),
                ],
            )
        })
    }
    /// The earliest or latest day the collected date components admit.
    fn edge_date(collected: Collected, last: bool) -> Date {
        let month = collected.month.unwrap_or(if last { 12 } else { 1 });
        let day = collected.day.unwrap_or(if last {
            month_length(collected.year, month).expect("admitted month")
        } else {
            1
        });
        Date::new(collected.year, month, day, Precision::Day).expect("admitted collected date")
    }
    fn moment(date: Date, time: [u8; 3], precision: Precision) -> DateTime {
        DateTime::new(date, time[0], time[1], time[2], precision).expect("admitted time")
    }
    fn edge_time(collected: Collected, last: bool) -> [u8; 3] {
        let edge = if last { [23, 59, 59] } else { [0, 0, 0] };
        std::array::from_fn(|index| collected.time[index].unwrap_or(edge[index]))
    }
}

impl Imputation {
    /// Validate the declared policy and required source metadata.
    pub fn new(
        kind: ImputationKind,
        source: String,
        not_before: Option<String>,
        not_after: Option<String>,
        fallback: Option<String>,
        path: String,
        invalid: Option<Value>,
    ) -> Option<Self> {
        if source.is_empty()
            || path.is_empty()
            || [&not_before, &not_after, &fallback]
                .into_iter()
                .any(|operand| operand.as_ref().is_some_and(String::is_empty))
        {
            return None;
        }
        match kind {
            ImputationKind::Impute(completion) => {
                let month = matches!(
                    (completion.minimum, completion.month),
                    (Precision::Year, Some(1..=12)) | (Precision::Month | Precision::Day, None)
                );
                let day = match (completion.minimum, completion.day) {
                    (Precision::Day, None) => true,
                    (Precision::Year | Precision::Month, Some(DayRule::Number(day))) => {
                        (1..=31).contains(&day)
                    }
                    (Precision::Year | Precision::Month, Some(_)) => true,
                    _ => false,
                };
                let time =
                    (completion.target == TemporalType::DateTime) == completion.time.is_some();
                if !(month && day && time) {
                    return None;
                }
            }
            _ if not_before.is_some() || not_after.is_some() || fallback.is_some() => return None,
            _ => (),
        }
        if matches!(kind, ImputationKind::ToDate | ImputationKind::Flag { .. }) && invalid.is_some()
        {
            return None;
        }
        Some(Self {
            kind,
            source,
            not_before,
            not_after,
            fallback,
            path,
            invalid,
        })
    }
    pub fn kind(&self) -> ImputationKind {
        self.kind
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Declared bounds as `(field, source)`, `not_before` first.
    pub fn bounds(&self) -> impl Iterator<Item = (&'static str, &str)> + '_ {
        [
            ("not_before", self.not_before.as_deref()),
            ("not_after", self.not_after.as_deref()),
        ]
        .into_iter()
        .filter_map(|(field, source)| source.map(|source| (field, source)))
    }
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }
    /// Every operand as `(field, source)`: the source, then the declared
    /// bounds, then the fallback. Bound operands follow the source in this order.
    pub fn operands(&self) -> impl Iterator<Item = (&'static str, &str)> + '_ {
        std::iter::once(("source", self.source.as_str()))
            .chain(self.bounds())
            .chain(self.fallback().map(|source| ("fallback", source)))
    }
    /// Bounds and a fallback are read only when a source needs them.
    pub fn has_conditional_operands(&self) -> bool {
        self.operands().nth(1).is_some()
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn handler(&self, name: &str) -> Option<&Value> {
        match name {
            "invalid" => self.invalid.as_ref(),
            _ => None,
        }
    }
    /// Whether a missing source is counted as a default substitution at
    /// `<path>.missing`; a declared fallback is counted at `<path>.fallback`.
    pub fn counts_missing_source(&self) -> bool {
        matches!(
            self.kind,
            ImputationKind::Impute(_) | ImputationKind::Precision { .. }
        )
    }
    /// Crossed bounds are counted only where both bounds are declared.
    pub fn counts_crossed_bounds(&self) -> bool {
        self.not_before.is_some() && self.not_after.is_some()
    }
    pub fn operation(&self) -> &'static str {
        match self.kind {
            ImputationKind::Impute(_) => "impute",
            ImputationKind::Precision { .. } => "precision",
            ImputationKind::Flag { .. } => "imputation_flag",
            ImputationKind::ToDate => "to_date",
            ImputationKind::DateText(_) => "date_text",
        }
    }
    fn target(&self) -> Option<TemporalType> {
        match self.kind {
            ImputationKind::Impute(Completion { target, .. })
            | ImputationKind::Precision { target } => Some(target),
            ImputationKind::Flag { .. } | ImputationKind::ToDate | ImputationKind::DateText(_) => {
                None
            }
        }
    }
    pub fn expected(&self) -> &'static str {
        match self.kind {
            ImputationKind::Impute(Completion { target, .. })
            | ImputationKind::Precision { target } => match target {
                TemporalType::Date => "str or date",
                TemporalType::DateTime => "str or datetime",
            },
            ImputationKind::Flag {
                part: FlagPart::Date,
                ..
            } => "date or datetime",
            ImputationKind::Flag {
                part: FlagPart::Time,
                ..
            } => "datetime",
            ImputationKind::ToDate => "datetime, ISO date text, or ISO 8601 datetime text",
            ImputationKind::DateText(_) => "str",
        }
    }
    pub fn accepts(&self, kind: ColumnType) -> bool {
        use ColumnType as T;
        match self.kind {
            ImputationKind::Impute(Completion { target, .. })
            | ImputationKind::Precision { target } => {
                kind == T::Str || kind == target.column_type()
            }
            ImputationKind::Flag {
                part: FlagPart::Date,
                ..
            } => matches!(kind, T::Date | T::DateTime),
            ImputationKind::Flag {
                part: FlagPart::Time,
                ..
            } => kind == T::DateTime,
            ImputationKind::ToDate => matches!(kind, T::Str | T::DateTime),
            ImputationKind::DateText(_) => kind == T::Str,
        }
    }
    /// A bound or fallback has the declared target type.
    pub fn accepts_operand(&self, kind: ColumnType) -> bool {
        self.target()
            .is_some_and(|target| kind == target.column_type())
    }
    pub(crate) fn input_diagnostic(&self, actual: &str) -> Diagnostic {
        let code = match self.kind {
            ImputationKind::Impute(Completion {
                target: TemporalType::DateTime,
                ..
            }) => C::ImputeDatetimeInput,
            ImputationKind::Precision {
                target: TemporalType::DateTime,
            } => C::PrecisionDatetimeInput,
            ImputationKind::Flag { .. } => C::ImputationFlagInput,
            ImputationKind::ToDate => C::ToDateInput,
            ImputationKind::DateText(_) => C::DateFormsInput,
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
    /// A bound or fallback whose value is not of the declared target type.
    pub(crate) fn operand_input_diagnostic(
        &self,
        field: &str,
        source: &str,
        actual: &str,
    ) -> Diagnostic {
        diagnostic(
            C::TemporalOperand,
            &format!("{}.{field}", self.path),
            &[
                ("source", Value::Str(source.into())),
                (
                    "expected",
                    Value::Str(self.target().map_or("date", TemporalType::name).into()),
                ),
                ("actual", Value::Str(actual.into())),
            ],
        )
    }
    /// A declared `invalid` answers text that is not collected date text; an
    /// interval or duration without it fails with its own condition.
    fn recover(&self, text: &str, reason: NotCollected) -> Result<ImputationStep, Diagnostic> {
        if self.invalid.is_some() {
            return Ok(ImputationStep::Recovered { handler: "invalid" });
        }
        Err(diagnostic(
            match reason {
                NotCollected::IntervalOrDuration => C::ImputeIntervalText,
                NotCollected::Invalid => C::ImputeInvalid,
            },
            &self.path,
            &[
                ("operation", Value::Str(self.operation().into())),
                ("source", Value::Str(self.source.clone())),
                ("value", Value::Str(text.into())),
            ],
        ))
    }
    /// Source completion is a separate step so unused bounds and an unused
    /// fallback have no read effects. This pure service preserves semantic
    /// findings and does not own runtime quotas.
    pub fn source_value(&self, value: ValueRef<'_>) -> Result<ImputationStep, Diagnostic> {
        use ImputationKind as K;
        if value == ValueRef::Missing {
            return Ok(match self.kind {
                K::Impute(_) | K::Precision { .. } => ImputationStep::Missing,
                K::Flag { .. } | K::ToDate | K::DateText(_) => {
                    ImputationStep::Value(Value::Missing)
                }
            });
        }
        let result = match (self.kind, value) {
            (K::Impute(completion), ValueRef::Date(date))
                if completion.target == TemporalType::Date =>
            {
                Value::Date(date)
            }
            (K::Impute(completion), ValueRef::DateTime(moment))
                if completion.target == TemporalType::DateTime =>
            {
                Value::DateTime(moment)
            }
            (K::Impute(completion), ValueRef::Str(text)) => {
                return match collected(text) {
                    Ok(Some(collected)) => self.complete(completion, collected),
                    // A prefix that carries no year is coarser than every
                    // minimum precision.
                    Ok(None) => Ok(ImputationStep::Value(Value::Missing)),
                    Err(reason) => self.recover(text, reason),
                };
            }
            (
                K::Precision {
                    target: TemporalType::Date,
                },
                ValueRef::Date(date),
            ) => Value::Str(date.collected_precision().word().into()),
            (
                K::Precision {
                    target: TemporalType::DateTime,
                },
                ValueRef::DateTime(moment),
            ) => Value::Str(moment.collected_precision().word().into()),
            (K::Precision { target }, ValueRef::Str(text)) => {
                let precision = match collected(text) {
                    Ok(Some(collected)) => collected.precision(),
                    Ok(None) => Precision::None,
                    Err(reason) => return self.recover(text, reason),
                };
                let precision = match target {
                    TemporalType::Date => precision.min(Precision::Day),
                    TemporalType::DateTime => precision,
                };
                Value::Str(precision.word().into())
            }
            (
                K::Flag {
                    part: FlagPart::Date,
                    ..
                },
                ValueRef::Date(date),
            ) => flag(FlagPart::Date, true, date.collected_precision()),
            (
                K::Flag {
                    part,
                    seconds_collected,
                },
                ValueRef::DateTime(moment),
            ) => flag(part, seconds_collected, moment.collected_precision()),
            (K::ToDate, ValueRef::DateTime(moment)) => Value::Date(moment.date()),
            (K::ToDate, ValueRef::Str(text)) => match collected(text) {
                Ok(Some(collected)) => match collected.date() {
                    Some(date) => Value::Date(date),
                    None => return Ok(ImputationStep::Partial),
                },
                Ok(None) => return Ok(ImputationStep::Partial),
                // An interval or duration fails as `impute` and `precision`
                // fail on one (TIME-13); other text keeps its 0.4.0 failure.
                Err(NotCollected::IntervalOrDuration) => {
                    return Err(diagnostic(
                        C::ToDateIntervalText,
                        &self.path,
                        &[
                            ("operation", Value::Str("to_date".into())),
                            ("source", Value::Str(self.source.clone())),
                            ("value", Value::Str(text.into())),
                        ],
                    ))
                }
                Err(NotCollected::Invalid) => {
                    return Err(diagnostic(
                        C::ToDateInvalid,
                        &self.path,
                        &[
                            ("operation", Value::Str("to_date".into())),
                            ("value", Value::Str(text.into())),
                        ],
                    ))
                }
            },
            // Text in none of the declared forms is invalid text, which a
            // declared `invalid` answers (TIME-22).
            (K::DateText(forms), ValueRef::Str(text)) => match forms.read(text) {
                Some(collected) => Value::Str(collected),
                None if self.invalid.is_some() => {
                    return Ok(ImputationStep::Recovered { handler: "invalid" })
                }
                None => {
                    return Err(diagnostic(
                        C::DateFormsInvalid,
                        &self.path,
                        &[
                            ("operation", Value::Str("date_text".into())),
                            ("source", Value::Str(self.source.clone())),
                            ("value", Value::Str(text.into())),
                        ],
                    ))
                }
            },
            (_, other) => return Err(self.input_diagnostic(type_name(other))),
        };
        Ok(ImputationStep::Value(result))
    }
    /// Complete collected text under the declared rules. A source coarser than
    /// the minimum precision gives missing, and a complete source stands.
    fn complete(
        &self,
        completion: Completion,
        collected: Collected,
    ) -> Result<ImputationStep, Diagnostic> {
        let precision = collected.precision();
        let value = match completion.target {
            TemporalType::Date => {
                if let Some(date) = collected.date() {
                    return Ok(ImputationStep::Value(Value::Date(date)));
                }
                if precision < completion.minimum {
                    return Ok(ImputationStep::Value(Value::Missing));
                }
                let completed = completion.complete_date(collected, precision, &self.path)?;
                if self.bounds().next().is_some() {
                    return Ok(ImputationStep::Bound(Pending::Date {
                        completed,
                        earliest: Completion::edge_date(collected, false),
                        latest: Completion::edge_date(collected, true),
                    }));
                }
                Value::Date(completed)
            }
            TemporalType::DateTime => {
                if precision == Precision::Second {
                    let date = collected.date().expect("collected date");
                    let time = Completion::edge_time(collected, false);
                    return Ok(ImputationStep::Value(Value::DateTime(Completion::moment(
                        date, time, precision,
                    ))));
                }
                if precision < completion.minimum {
                    return Ok(ImputationStep::Value(Value::Missing));
                }
                let date = completion.complete_date(collected, precision, &self.path)?;
                let last = completion.time == Some(TimeRule::Last);
                let completed =
                    Completion::moment(date, Completion::edge_time(collected, last), precision);
                if self.bounds().next().is_some() {
                    let edge = |last| {
                        Completion::moment(
                            Completion::edge_date(collected, last),
                            Completion::edge_time(collected, last),
                            precision,
                        )
                    };
                    return Ok(ImputationStep::Bound(Pending::DateTime {
                        completed,
                        earliest: edge(false),
                        latest: edge(true),
                    }));
                }
                Value::DateTime(completed)
            }
        };
        Ok(ImputationStep::Value(value))
    }
    /// A fallback answers a missing source with its whole value, collected
    /// precision `none`; a missing fallback gives missing.
    pub fn fallback_value(&self, value: ValueRef<'_>) -> Result<Value, Diagnostic> {
        match (self.target(), value) {
            (_, ValueRef::Missing) => Ok(Value::Missing),
            (Some(TemporalType::Date), ValueRef::Date(date)) => {
                Ok(Value::Date(date.with_precision(Precision::None)))
            }
            (Some(TemporalType::DateTime), ValueRef::DateTime(moment)) => {
                Ok(Value::DateTime(moment.with_precision(Precision::None)))
            }
            (_, other) => Err(self.operand_input_diagnostic(
                "fallback",
                self.fallback().unwrap_or_default(),
                type_name(other),
            )),
        }
    }
    /// Move only imputed components within the interval the collected
    /// components admit. A missing bound, or one outside that interval, is no
    /// bound; crossed bounds resolve to `not_after`. Returns the bounded value
    /// and whether the bounds crossed.
    pub fn apply_bounds(
        &self,
        pending: Pending,
        not_before: ValueRef<'_>,
        not_after: ValueRef<'_>,
    ) -> Result<(Value, bool), Diagnostic> {
        let lower = self
            .not_before
            .as_deref()
            .map(|source| ("not_before", source));
        let upper = self
            .not_after
            .as_deref()
            .map(|source| ("not_after", source));
        let precision = pending.collected_precision();
        Ok(match pending {
            Pending::Date {
                completed,
                earliest,
                latest,
            } => {
                let read =
                    |declared: Option<(&str, &str)>, value: ValueRef<'_>| match (declared, value) {
                        (None, _) | (_, ValueRef::Missing) => Ok(None),
                        (_, ValueRef::Date(date)) => Ok(Some(date)),
                        (Some((field, source)), other) => {
                            Err(self.operand_input_diagnostic(field, source, type_name(other)))
                        }
                    };
                let (value, crossed) = clamp(
                    completed,
                    earliest,
                    latest,
                    read(lower, not_before)?,
                    read(upper, not_after)?,
                );
                (Value::Date(value.with_precision(precision)), crossed)
            }
            Pending::DateTime {
                completed,
                earliest,
                latest,
            } => {
                let read =
                    |declared: Option<(&str, &str)>, value: ValueRef<'_>| match (declared, value) {
                        (None, _) | (_, ValueRef::Missing) => Ok(None),
                        (_, ValueRef::DateTime(moment)) => Ok(Some(moment)),
                        (Some((field, source)), other) => {
                            Err(self.operand_input_diagnostic(field, source, type_name(other)))
                        }
                    };
                let (value, crossed) = clamp(
                    completed,
                    earliest,
                    latest,
                    read(lower, not_before)?,
                    read(upper, not_after)?,
                );
                (Value::DateTime(value.with_precision(precision)), crossed)
            }
        })
    }
}
/// `min(max(completed, lower), upper)` over the bounds that lie inside
/// `[earliest, latest]`, and whether those bounds crossed.
fn clamp<T: Ord + Copy>(
    completed: T,
    earliest: T,
    latest: T,
    lower: Option<T>,
    upper: Option<T>,
) -> (T, bool) {
    let inside = |bound: &T| earliest <= *bound && *bound <= latest;
    let (lower, upper) = (lower.filter(inside), upper.filter(inside));
    let crossed = matches!((lower, upper), (Some(lower), Some(upper)) if lower > upper);
    let raised = lower.map_or(completed, |lower| completed.max(lower));
    (upper.map_or(raised, |upper| raised.min(upper)), crossed)
}
/// The ADaM `*DTF` or `*TMF` code for an imputed value's collected precision.
fn flag(part: FlagPart, seconds_collected: bool, precision: Precision) -> Value {
    let code = match (part, precision) {
        (FlagPart::Date, Precision::None) => "Y",
        (FlagPart::Date, Precision::Year) => "M",
        (FlagPart::Date, Precision::Month) => "D",
        (FlagPart::Date, _) => return Value::Missing,
        (FlagPart::Time, Precision::Hour) => "M",
        (FlagPart::Time, Precision::Minute) if seconds_collected => "S",
        (FlagPart::Time, Precision::Minute | Precision::Second) => return Value::Missing,
        (FlagPart::Time, _) => "H",
    };
    Value::Str(code.into())
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
pub(crate) fn diagnostic(code: C, path: &str, fields: &[(&str, Value)]) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: fields
            .iter()
            .map(|(key, value)| ((*key).into(), V::from(value.clone())))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collected_text_reads_every_right_truncated_form_and_nothing_else() {
        for (text, precision) in [
            ("2025", Precision::Year),
            ("2025-03", Precision::Month),
            ("2025-03-04", Precision::Day),
            ("2025-03-04T08", Precision::Hour),
            ("2025-03-04T08:30", Precision::Minute),
            ("2025-03-04T08:30:15", Precision::Second),
            ("2024-02-29T23:59:59", Precision::Second),
            // An unknown component ends the right-truncated prefix.
            ("2025---04", Precision::Year),
            ("2025-03--T08:30", Precision::Month),
            ("2025-03-04T-:30", Precision::Day),
            ("2025-03-04T08:-:15", Precision::Hour),
            ("2025-03-04T08:30:-", Precision::Minute),
        ] {
            assert_eq!(
                collected(text).map(|reading| reading.map(Collected::precision)),
                Ok(Some(precision)),
                "{text}"
            );
        }
        // An unknown year leaves a prefix that carries nothing.
        for text in ["--03-04", "-----T08:30", "--02-29"] {
            assert_eq!(collected(text), Ok(None), "{text}");
        }
        for text in [
            "",
            "-",
            "-----T-",
            "0000",
            "202",
            "20250",
            "2025-00",
            "2025-13",
            "2025-1",
            "2025/03",
            "2025-02-29",
            "2025-04-31",
            "2025-03-00",
            "2025---32",
            "--02-30",
            "2025-03-04 08",
            "2025-03-04t08",
            "2025-03-04T24",
            "2025-03-04T08:60",
            "2025-03-04T08:30:60",
            "2025-03-04T08:30:15Z",
            "2025-03-04T08:30:15.5",
            "2025-",
            "UNK",
            "２０２５",
        ] {
            assert_eq!(collected(text), Err(NotCollected::Invalid), "{text:?}");
        }
        for text in [
            "2025-01-01/2025-01-10",
            "2025-01-01T08:00/PT2H",
            "P3D",
            "PT2H30M",
            "-PT15M",
        ] {
            assert_eq!(
                collected(text),
                Err(NotCollected::IntervalOrDuration),
                "{text:?}"
            );
        }
    }

    #[test]
    fn crossed_bounds_resolve_to_the_upper_bound_and_outside_bounds_are_ignored() {
        let day = |d| Date::new(2025, 2, d, Precision::Day).unwrap();
        let (earliest, latest, completed) = (day(1), day(28), day(15));
        assert_eq!(
            clamp(completed, earliest, latest, Some(day(20)), Some(day(10))),
            (day(10), true)
        );
        assert_eq!(
            clamp(completed, earliest, latest, Some(day(10)), Some(day(20))),
            (day(15), false)
        );
        let march = Date::new(2025, 3, 20, Precision::Day).unwrap();
        assert_eq!(
            clamp(completed, earliest, latest, Some(march), None),
            (day(15), false)
        );
        let january = Date::new(2025, 1, 5, Precision::Day).unwrap();
        assert_eq!(
            clamp(completed, earliest, latest, None, Some(january)),
            (day(15), false)
        );
    }

    #[test]
    fn flags_follow_the_one_precision_ladder() {
        use Precision as P;
        let code = |part, seconds, precision| match flag(part, seconds, precision) {
            Value::Str(code) => Some(code),
            _ => None,
        };
        let date = [
            P::None,
            P::Year,
            P::Month,
            P::Day,
            P::Hour,
            P::Minute,
            P::Second,
        ]
        .map(|precision| code(FlagPart::Date, true, precision));
        assert_eq!(
            date,
            [Some("Y"), Some("M"), Some("D"), None, None, None, None].map(|c| c.map(String::from))
        );
        let time = [
            P::None,
            P::Year,
            P::Month,
            P::Day,
            P::Hour,
            P::Minute,
            P::Second,
        ]
        .map(|precision| code(FlagPart::Time, true, precision));
        assert_eq!(
            time,
            [
                Some("H"),
                Some("H"),
                Some("H"),
                Some("H"),
                Some("M"),
                Some("S"),
                None
            ]
            .map(|c| c.map(String::from))
        );
        assert_eq!(code(FlagPart::Time, false, P::Minute), None);
        assert_eq!(code(FlagPart::Time, false, P::Hour), Some("M".into()));
    }
}
