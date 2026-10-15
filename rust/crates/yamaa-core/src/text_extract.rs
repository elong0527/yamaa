//! Extraction through the shared portable pattern and matcher authorities.
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    regex,
    table::ValueRef,
    value::Value,
};
use serde_json::Value as V;

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
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    #[error("text extract shape")]
    Shape,
    #[error("text extract regex: {0:?}")]
    Regex(regex::CompileError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    Capture(Option<&'a str>),
    Recovered(&'static str),
    Missing,
    NoMatch(&'a str),
}
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("text extract diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
    #[error("text extract regex: {0:?}")]
    Regex(regex::MatchError),
}
impl StringExtract {
    /// Own the declaration while retaining invalid pattern/group facts for precedence.
    pub fn new(
        source: String,
        path: String,
        pattern: String,
        group: String,
        missing: Option<Value>,
        no_match: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        if source.is_empty() || path.is_empty() || !decimal(&group) {
            return Err(AdmissionError::Shape);
        }
        let mut deferred = None;
        let compiled = match regex::Pattern::compile(&pattern) {
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
                        ("pattern", V::String(pattern.clone())),
                        ("reason", V::String(reason.into())),
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
                                |_| V::Number((group.clone()).parse().expect("canonical integer")),
                                V::from,
                            ),
                        ),
                        ("group_count", V::from(compiled.group_count() as i64)),
                        ("pattern", V::String(pattern.clone())),
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
            [("identifier", V::String(self.source.clone()))],
        )
    }
    pub fn input_diagnostic(&self, actual: &'static str) -> Diagnostic {
        diagnostic(
            C::StrCaseInput,
            &format!("{}.source", self.path),
            [
                ("source", V::String(self.source.clone())),
                ("expected", V::String("str".into())),
                ("actual", V::String(actual.into())),
            ],
        )
    }
    /// Matching borrows captures and never allocates a result or handler literal.
    /// A resource failure cannot be mistaken for an absent match.
    pub fn apply<'a>(&self, value: ValueRef<'a>) -> Result<Step<'a>, Error> {
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
        match self
            .compiled
            .as_ref()
            .expect("admitted pattern")
            .search(subject)
            .map_err(Error::Regex)?
        {
            Some(found) => Ok(Step::Capture(found.groups[self.group])),
            None if self.no_match.is_some() => Ok(Step::Recovered("no_match")),
            None => Ok(Step::NoMatch(subject)),
        }
    }
    pub fn failure_diagnostic(&self, step: Step<'_>) -> Option<Diagnostic> {
        let subject = match step {
            Step::NoMatch(subject) => Some(subject),
            Step::Missing => None,
            _ => return None,
        };
        Some(match subject {
            Some(subject) => diagnostic(
                C::ExtractNoMatch,
                &self.path,
                [
                    ("value", V::String(subject.into())),
                    ("pattern", V::String(self.pattern.clone())),
                ],
            ),
            None => diagnostic(
                C::StrCaseMissing,
                &self.path,
                [("variable", V::String(self.source.clone()))],
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
