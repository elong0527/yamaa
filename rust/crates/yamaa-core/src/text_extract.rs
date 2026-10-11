//! Bounded extraction through the shared portable pattern and matcher authorities.
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    predicate, regex,
    table::ValueRef,
    value::Value,
};
use alloc::{format, string::String, vec};

pub const MAX_DECLARATION_BYTES: usize = 16_777_216;
#[derive(Clone, Debug, PartialEq)]
pub struct StringExtract {
    source: String,
    path: String,
    pattern: String,
    compiled: Option<regex::Pattern>,
    group: usize,
    deferred: Option<Diagnostic>,
    missing: Option<Value>,
    no_match: Option<Value>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Shape,
    Limit,
    Regex(regex::CompileError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    Capture(Option<&'a str>),
    Recovered(&'static str),
    Missing,
    NoMatch(&'a str),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Diagnostic(Diagnostic),
    Limit(predicate::LimitError),
}
impl StringExtract {
    /// Account for metadata/context and several retained parser-arena copies.
    /// The conservative multiplier bounds arena retention; it is not CPU timing.
    pub fn declaration_bytes(
        source: &str,
        path: &str,
        pattern: &str,
        group: &str,
        handlers: impl IntoIterator<Item = usize>,
    ) -> Option<usize> {
        [source.len(), path.len(), group.len()]
            .into_iter()
            .chain(handlers)
            .try_fold(0usize, usize::checked_add)
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| pattern.len().checked_mul(512)?.checked_add(n))
            .and_then(|n| n.checked_add(2048))
    }
    /// Public use owns its bounded metadata before any later diagnostic copies.
    /// Invalid pattern/group facts are retained, allowing source/scope precedence.
    pub fn new(
        source: String,
        path: String,
        pattern: String,
        group: String,
        missing: Option<Value>,
        no_match: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        Self::with_budget(
            source,
            path,
            pattern,
            group,
            missing,
            no_match,
            &mut regex::CompileBudget::new(Default::default()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_budget(
        source: String,
        path: String,
        pattern: String,
        group: String,
        missing: Option<Value>,
        no_match: Option<Value>,
        budget: &mut regex::CompileBudget,
    ) -> Result<Self, AdmissionError> {
        if source.is_empty() || path.is_empty() || !decimal(&group) {
            return Err(AdmissionError::Shape);
        }
        let handler_bytes = [&missing, &no_match].into_iter().map(|value| match value {
            Some(Value::Str(text)) => text.len(),
            _ => 0,
        });
        if Self::declaration_bytes(&source, &path, &pattern, &group, handler_bytes)
            .is_none_or(|n| n > MAX_DECLARATION_BYTES)
        {
            return Err(AdmissionError::Limit);
        }
        let mut deferred = None;
        let compiled =
            match regex::Pattern::compile_with_budget(&pattern, Default::default(), budget) {
                Ok(compiled) => Some(compiled),
                Err(regex::CompileError::Invalid { reason, .. }) => {
                    // This projection belongs to extraction; checks retain their own
                    // pattern-only cause. The rejection is bounded static parser data.
                    let reason = match reason {
                        "unclosed character class" => "a character class is never closed",
                        "trailing backslash" => "a trailing backslash escapes nothing",
                        "variable-length lookbehind" => {
                            "a lookbehind whose length can vary is not allowed"
                        }
                        reason => reason,
                    };
                    deferred = Some(diagnostic(
                        C::RegexInvalidPattern,
                        &format!("{path}.pattern"),
                        [
                            ("pattern", V::Scalar(Value::Str(pattern.clone()))),
                            ("reason", V::Scalar(Value::Str(reason.into()))),
                        ],
                    ));
                    None
                }
                Err(error) => return Err(AdmissionError::Regex(error)),
            };
        let selected = if group.starts_with('-') && group[1..].bytes().all(|b| b == b'0') {
            Some(0)
        } else {
            group.parse::<usize>().ok()
        };
        if let Some(compiled) = &compiled {
            if selected.is_none_or(|group| group > compiled.group_count()) {
                deferred = Some(diagnostic(
                    C::ExtractGroup,
                    &format!("{path}.group"),
                    [
                        (
                            "group",
                            group.parse::<i64>().map_or_else(
                                |_| V::Integer(group.clone()),
                                |n| V::Scalar(Value::Int(n)),
                            ),
                        ),
                        (
                            "group_count",
                            V::Scalar(Value::Int(compiled.group_count() as i64)),
                        ),
                        ("pattern", V::Scalar(Value::Str(pattern.clone()))),
                    ],
                ));
            }
        }
        Ok(Self {
            source,
            path,
            pattern,
            compiled,
            group: selected.unwrap_or(0),
            deferred,
            missing,
            no_match,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn declaration_diagnostic(&self) -> Option<&Diagnostic> {
        self.deferred.as_ref()
    }
    pub fn handler(&self, name: &str) -> Option<&Value> {
        match name {
            "missing" => self.missing.as_ref(),
            "no_match" => self.no_match.as_ref(),
            _ => None,
        }
    }
    pub fn unknown_diagnostic(&self) -> Diagnostic {
        diagnostic(
            C::ExtractUnknown,
            &format!("{}.source", self.path),
            [("identifier", V::Scalar(Value::Str(self.source.clone())))],
        )
    }
    pub fn input_diagnostic(&self, actual: &'static str) -> Diagnostic {
        diagnostic(
            C::StrCaseInput,
            &format!("{}.source", self.path),
            [
                ("source", V::Scalar(Value::Str(self.source.clone()))),
                ("expected", V::Scalar(Value::Str("str".into()))),
                ("actual", V::Scalar(Value::Str(actual.into()))),
            ],
        )
    }
    /// Matching borrows captures and never allocates a result or handler literal.
    /// A resource failure cannot be mistaken for an absent match.
    pub fn apply<'a>(
        &self,
        value: ValueRef<'a>,
        limits: regex::MatchLimits,
        budget: &mut predicate::Budget,
    ) -> Result<Step<'a>, Error> {
        if let Some(diagnostic) = &self.deferred {
            return Err(Error::Diagnostic(diagnostic.clone()));
        }
        let subject = match value {
            ValueRef::Missing => {
                return Ok(if self.missing.is_some() {
                    Step::Recovered("missing")
                } else {
                    Step::Missing
                });
            }
            ValueRef::Str(subject) => subject,
            other => {
                return Err(Error::Diagnostic(self.input_diagnostic(match other {
                    ValueRef::Int(_) => "int",
                    ValueRef::Float(_) => "float",
                    ValueRef::Bool(_) => "bool",
                    ValueRef::Date(_) => "date",
                    ValueRef::DateTime(_) => "datetime",
                    _ => unreachable!(),
                })));
            }
        };
        match budget
            .search_pattern(
                self.compiled.as_ref().expect("admitted pattern"),
                subject,
                limits,
            )
            .map_err(Error::Limit)?
        {
            Some(found) => Ok(Step::Capture(found.groups[self.group])),
            None if self.no_match.is_some() => Ok(Step::Recovered("no_match")),
            None => Ok(Step::NoMatch(subject)),
        }
    }
    /// Caller admits cumulative copying; standalone use also bounds complete
    /// context before allocating subject/pattern/path. None is resource refusal.
    pub fn failure_text_bytes(&self, step: Step<'_>) -> Option<usize> {
        let subject = match step {
            Step::NoMatch(subject) => Some(subject),
            Step::Missing => None,
            _ => return None,
        };
        self.source
            .len()
            .checked_add(self.path.len())?
            .checked_add(self.pattern.len())?
            .checked_add(subject.map_or(0, str::len))?
            .checked_add(512)
    }
    pub fn failure_diagnostic(&self, step: Step<'_>, maximum: usize) -> Option<Diagnostic> {
        let subject = match step {
            Step::NoMatch(subject) => Some(subject),
            Step::Missing => None,
            _ => return None,
        };
        let bytes = self.failure_text_bytes(step)?;
        if bytes > maximum.min(MAX_DECLARATION_BYTES) {
            return None;
        }
        Some(match subject {
            Some(subject) => diagnostic(
                C::ExtractNoMatch,
                &self.path,
                [
                    ("value", V::Scalar(Value::Str(subject.into()))),
                    ("pattern", V::Scalar(Value::Str(self.pattern.clone()))),
                ],
            ),
            None => diagnostic(
                C::StrCaseMissing,
                &self.path,
                [("variable", V::Scalar(Value::Str(self.source.clone())))],
            ),
        })
    }
}
fn decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}
fn diagnostic<const N: usize>(code: C, path: &str, fields: [(&str, V); N]) -> Diagnostic {
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
