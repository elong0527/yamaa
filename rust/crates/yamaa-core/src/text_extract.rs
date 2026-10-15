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
                                |_| {
                                    // Decimal declarations admit leading zeroes; JSON integers
                                    // require canonical spelling. Overflow leaves nonzero digits.
                                    let digits = group
                                        .strip_prefix('-')
                                        .unwrap_or(&group)
                                        .trim_start_matches('0');
                                    let canonical = if group.starts_with('-') {
                                        format!("-{digits}")
                                    } else {
                                        digits.into()
                                    };
                                    V::Number(canonical.parse().expect("validated decimal integer"))
                                },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(pattern: &str, group: &str) -> StringExtract {
        StringExtract::new(
            "SRC.TEXT".into(),
            "columns.TEXT.derivation.str_extract".into(),
            pattern.into(),
            group.into(),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn padded_overflow_group_retains_exact_integer_in_typed_finding() {
        for (group, expected) in [
            (
                format!("{}18446744073709551616", "0".repeat(128)),
                "18446744073709551616",
            ),
            (
                format!("-{}9223372036854775809", "0".repeat(128)),
                "-9223372036854775809",
            ),
        ] {
            let operation = operation("(a)", &group);
            let finding = operation.declaration_diagnostic().unwrap();
            assert_eq!(finding.code, C::ExtractGroup);
            assert_eq!(
                finding.spec_paths,
                ["columns.TEXT.derivation.str_extract.group"]
            );
            assert_eq!(
                finding.context,
                [
                    ("group".into(), V::Number(expected.parse().unwrap())),
                    ("group_count".into(), V::from(1)),
                    ("pattern".into(), V::String("(a)".into())),
                ]
                .into_iter()
                .collect()
            );
            assert!(finding.source_span.is_none());
            assert!(finding.operand_route.is_none());
            assert_eq!(
                operation.apply(ValueRef::Str("a")),
                Err(Error::Diagnostic(finding.clone()))
            );
        }
    }

    #[test]
    fn canonical_overflow_and_small_padded_groups_keep_numeric_facts() {
        for (group, expected) in [
            ("18446744073709551616", "18446744073709551616"),
            ("-9223372036854775809", "-9223372036854775809"),
            ("0002", "2"),
            ("-0001", "-1"),
        ] {
            let operation = operation("(a)", group);
            let finding = operation.declaration_diagnostic().unwrap();
            assert_eq!(finding.code, C::ExtractGroup);
            assert_eq!(
                finding.context["group"],
                V::Number(expected.parse().unwrap())
            );
        }
    }

    #[test]
    fn padded_valid_groups_and_signed_zero_keep_capture_semantics() {
        for (group, expected) in [
            ("0", "pa"),
            ("0000", "pa"),
            ("-0", "pa"),
            ("-0000", "pa"),
            ("1", "a"),
            ("0001", "a"),
        ] {
            let operation = operation("p(a)", group);
            assert!(operation.declaration_diagnostic().is_none());
            assert_eq!(
                operation.apply(ValueRef::Str("pa")),
                Ok(Step::Capture(Some(expected)))
            );
        }
    }

    #[test]
    fn group_shape_and_invalid_pattern_precedence_are_unchanged() {
        for group in ["", "-", "+1", "1.0", "1e2", " 1", "\u{ff11}"] {
            assert_eq!(
                StringExtract::new(
                    "SRC.TEXT".into(),
                    "path".into(),
                    "(a)".into(),
                    group.into(),
                    None,
                    None,
                ),
                Err(AdmissionError::Shape)
            );
        }
        let group = format!("{}18446744073709551616", "0".repeat(128));
        let operation = operation("[", &group);
        assert_eq!(
            operation.declaration_diagnostic().unwrap().code,
            C::RegexInvalidPattern
        );
    }
}
