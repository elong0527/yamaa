//! Replacement of every match through the shared portable pattern and matcher.
//! The search declaration, its deferred pattern finding and its source
//! diagnostics are the extraction declaration's; only the scan and the
//! literal replacement text are owned here.
use crate::{
    diagnostic::Diagnostic,
    limits::{LimitExceeded, Limits},
    table::ValueRef,
    text_extract::{AdmissionError, StringExtract},
    value::Value,
};

#[derive(Clone, Debug, PartialEq)]
pub struct StringReplace {
    search: StringExtract,
    replacement: String,
}
/// Matched spans borrow the subject; `render` joins them with the replacement.
/// `Recovered` is a missing source, answered by the declared `missing` or by
/// the default missing value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    Matched {
        subject: &'a str,
        spans: Vec<(usize, usize)>,
    },
    Recovered,
}
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("text replace diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// A result longer than `Limits::TEXT_BYTES` (TEXT-11).
    #[error("text replace: {0}")]
    Limit(LimitExceeded),
    #[error("text replace output allocation failed")]
    Allocation,
}
impl StringReplace {
    /// Own the declaration while retaining an invalid pattern for precedence.
    pub fn new(
        source: String,
        path: String,
        pattern: String,
        replacement: String,
        missing: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        Ok(Self {
            search: StringExtract::new(source, path, pattern, "0".into(), missing, None)?,
            replacement,
        })
    }
    /// The search declaration that binding and source diagnostics share.
    pub fn search(&self) -> &StringExtract {
        &self.search
    }
    pub fn source(&self) -> &str {
        self.search.source()
    }
    pub fn path(&self) -> &str {
        self.search.path()
    }
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
    pub fn missing(&self) -> Option<&Value> {
        self.search.handler("missing")
    }
    /// The declared `missing` literal, or missing when it is omitted.
    pub fn missing_substitute(&self) -> Value {
        self.search.substitute("missing")
    }
    /// Scan the whole subject before anything is copied.
    pub fn apply<'a>(&self, value: ValueRef<'a>) -> Result<Step<'a>, Error> {
        if let Some(diagnostic) = self.search.declaration_diagnostic() {
            return Err(Error::Diagnostic(diagnostic.clone()));
        }
        let subject = match value {
            ValueRef::Missing => return Ok(Step::Recovered),
            ValueRef::Str(subject) => subject,
            other => {
                return Err(Error::Diagnostic(self.search.input_diagnostic(
                    match other {
                        ValueRef::Int(_) => "int",
                        ValueRef::Float(_) => "float",
                        ValueRef::Bool(_) => "bool",
                        ValueRef::Date(_) => "date",
                        ValueRef::DateTime(_) => "datetime",
                        _ => unreachable!(),
                    },
                )));
            }
        };
        let spans = self
            .search
            .admitted()
            .expect("admitted pattern")
            .search_all(subject);
        Ok(Step::Matched { subject, spans })
    }
    /// The unmatched text, in order, with each match replaced by the literal
    /// text; a result longer than `Limits::TEXT_BYTES` is refused before it is
    /// built.
    pub fn render(&self, subject: &str, spans: &[(usize, usize)]) -> Result<String, RenderError> {
        let total = output_bytes(subject.len(), spans, self.replacement.len());
        Limits::text_bytes(total).map_err(RenderError::Limit)?;
        let total = total.expect("an admitted length");
        let mut output = String::new();
        output
            .try_reserve_exact(total)
            .map_err(|_| RenderError::Allocation)?;
        let mut kept = 0;
        for &(start, end) in spans {
            output.push_str(&subject[kept..start]);
            output.push_str(&self.replacement);
            kept = end;
        }
        output.push_str(&subject[kept..]);
        Ok(output)
    }
}

fn output_bytes(
    subject_bytes: usize,
    spans: &[(usize, usize)],
    replacement_bytes: usize,
) -> Option<usize> {
    let kept = spans
        .iter()
        .try_fold(subject_bytes, |kept, &(start, end)| {
            kept.checked_sub(end.checked_sub(start)?)
        })?;
    kept.checked_add(replacement_bytes.checked_mul(spans.len())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::ConditionCode;

    fn operation(pattern: &str, replacement: &str, missing: Option<Value>) -> StringReplace {
        StringReplace::new(
            "SRC.TEXT".into(),
            "columns.TEXT.derivation.str_replace".into(),
            pattern.into(),
            replacement.into(),
            missing,
        )
        .unwrap()
    }
    fn replaced(pattern: &str, replacement: &str, subject: &str) -> String {
        let op = operation(pattern, replacement, None);
        match op.apply(ValueRef::Str(subject)) {
            Ok(Step::Matched { subject, spans }) => op.render(subject, &spans).unwrap(),
            other => panic!("{pattern} {subject:?}: {other:?}"),
        }
    }

    #[test]
    fn every_match_is_replaced_left_to_right_without_overlap() {
        for (pattern, replacement, subject, expected) in [
            (r"\s+", " ", "  HEAD   ACHE ", " HEAD ACHE "),
            ("^ | $", "", " HEAD ACHE ", "HEAD ACHE"),
            ("[^0-9]", "", "AE-0-12", "012"),
            ("[^0-9]", "", "NONE", ""),
            ("aa", "b", "aaaaa", "bba"),
            ("a|ab", "-", "abab", "-b-b"),
            ("x", "-", "abc", "abc"),
            ("^a", "", "aaa", "aa"),
            ("a$", "", "aaa", "aa"),
            // An assertion at the resumed position still sees the scalar before it.
            (r"\Ba", "-", "aaa", "a--"),
            (r"\bAE\b", "X", "AE AEX AE", "X AEX X"),
        ] {
            assert_eq!(
                replaced(pattern, replacement, subject),
                expected,
                "{pattern} {subject:?}"
            );
        }
    }

    #[test]
    fn empty_matches_are_replaced_and_step_over_one_scalar() {
        for (pattern, subject, expected) in [
            ("x*", "ab", "-a-b-"),
            ("a*", "baa", "-b--"),
            ("a*", "bbb", "-b-b-b-"),
            ("x*", "", "-"),
            ("", "\u{1D400}\u{e9}", "-\u{1D400}-\u{e9}-"),
            ("(?:)", "\0", "-\0-"),
            (r"\b", "ab cd", "-ab- -cd-"),
        ] {
            assert_eq!(replaced(pattern, "-", subject), expected, "{pattern}");
        }
    }

    #[test]
    fn replacement_text_is_literal_and_may_grow_the_value() {
        assert_eq!(replaced("(b)", "$1", "abc"), "a$1c");
        assert_eq!(replaced("(?<x>b)", "$<x>$&$$", "abc"), "a$<x>$&$$c");
        assert_eq!(replaced("b", r"\1{b}", "abc"), r"a\1{b}c");
        assert_eq!(replaced(".", "\u{1D400}", "ab"), "\u{1D400}\u{1D400}");
    }

    #[test]
    fn complete_output_size_counts_utf8_and_empty_matches_before_allocation() {
        let subject = "\u{e9}\u{10400}";
        let op = operation("x*", "\u{10400}", None);
        let Step::Matched { spans, .. } = op.apply(ValueRef::Str(subject)).unwrap() else {
            panic!("empty matches")
        };
        assert_eq!(output_bytes(subject.len(), &spans, 4), Some(18));
        assert_eq!(
            op.render(subject, &spans).unwrap(),
            "\u{10400}\u{e9}\u{10400}\u{10400}\u{10400}"
        );
        assert_eq!(output_bytes(6, &[(0, 2), (2, 6)], 0), Some(0));
        assert_eq!(output_bytes(6, &[], usize::MAX), Some(6));
        assert_eq!(output_bytes(0, &[(0, 0)], usize::MAX), Some(usize::MAX));
        assert_eq!(output_bytes(0, &[(0, 0), (0, 0)], usize::MAX), None);
        assert_eq!(output_bytes(1, &[(0, 0)], usize::MAX), None);
    }

    #[test]
    fn missing_handler_and_type_failures_are_distinct() {
        for handler in [Value::Missing, Value::Str("".into()), Value::Int(1)] {
            let declared = operation("a", "b", Some(handler.clone()));
            assert_eq!(declared.apply(ValueRef::Missing), Ok(Step::Recovered));
            assert_eq!(declared.missing(), Some(&handler));
            assert_eq!(declared.missing_substitute(), handler);
        }
        // An omitted handler answers a missing source with missing (COLUMN-10).
        let op = operation("a", "b", None);
        assert_eq!(op.apply(ValueRef::Missing), Ok(Step::Recovered));
        assert_eq!(op.missing(), None);
        assert_eq!(op.missing_substitute(), Value::Missing);
        let Err(Error::Diagnostic(input)) = op.apply(ValueRef::Int(1)) else {
            panic!("non-string source")
        };
        assert_eq!(input.code, ConditionCode::StrCaseInput);
        assert_eq!(
            input.spec_paths,
            ["columns.TEXT.derivation.str_replace.source"]
        );
    }

    #[test]
    fn invalid_pattern_is_deferred_to_the_pattern_path() {
        let op = operation("[^0-9", "", None);
        let deferred = op.search().declaration_diagnostic().unwrap().clone();
        assert_eq!(deferred.code, ConditionCode::RegexInvalidPattern);
        assert_eq!(
            deferred.spec_paths,
            ["columns.TEXT.derivation.str_replace.pattern"]
        );
        assert_eq!(
            op.apply(ValueRef::Str("1")),
            Err(Error::Diagnostic(deferred))
        );
    }

    #[test]
    fn a_long_scan_completes_without_a_step_budget() {
        // A million and one empty matches, which the retired step guard
        // refused, each replaced by nothing so the text stays within its limit.
        let subject = "a".repeat(1_000_000);
        assert_eq!(replaced("x*", "", &subject), subject);
    }

    #[test]
    fn shape_errors_are_the_extraction_declaration_errors() {
        assert_eq!(
            StringReplace::new("".into(), "p".into(), "a".into(), "".into(), None),
            Err(AdmissionError::Shape)
        );
        let op = operation("a", "b", None);
        assert_eq!(op.source(), "SRC.TEXT");
        assert_eq!(op.replacement(), "b");
        assert!(op.search().declaration_diagnostic().is_none());
    }
}
