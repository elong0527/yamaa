//! The four locale-independent ASCII substitutions admitted by text declarations.
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    table::ValueRef,
    value::Value,
};
use alloc::string::String;
use alloc::{format, vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Sentence,
    Title,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputLimit {
    pub required: usize,
}

pub const MAX_DECLARATION_BYTES: usize = 16_777_216;
/// A closed case declaration owns bounded metadata and preserves an omitted handler.
#[derive(Clone, Debug, PartialEq)]
pub struct StringCase {
    source: String,
    path: String,
    mode: Case,
    missing: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Value(Value),
    Recovered,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Diagnostic(Diagnostic),
    Limit(OutputLimit),
}
impl StringCase {
    pub(crate) fn metadata_fits(source: &str, path: &str, missing: Option<&str>) -> bool {
        [Some(source), Some(path), missing]
            .into_iter()
            .flatten()
            .try_fold(0usize, |n, text| n.checked_add(text.len()))
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(512))
            .is_some_and(|n| n <= MAX_DECLARATION_BYTES)
    }
    /// Refuse malformed or oversized owned metadata before future context copies.
    pub fn new(source: String, path: String, mode: Case, missing: Option<Value>) -> Option<Self> {
        let missing_text = match &missing {
            Some(Value::Str(text)) => Some(text.as_str()),
            _ => None,
        };
        if source.is_empty()
            || path.is_empty()
            || !Self::metadata_fits(&source, &path, missing_text)
        {
            return None;
        }
        Some(Self {
            source,
            path,
            mode,
            missing,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn missing(&self) -> Option<&Value> {
        self.missing.as_ref()
    }
    pub(crate) fn input_diagnostic(&self, actual: &'static str) -> Diagnostic {
        diagnostic(
            C::StrCaseInput,
            &format!("{}.source", self.path),
            &[
                ("source", Value::Str(self.source.clone())),
                ("expected", Value::Str("str".into())),
                ("actual", Value::Str(actual.into())),
            ],
        )
    }
    /// Admit text work/storage before calling; maximum bounds the new output copy.
    /// The engine owns cumulative work and scalar-text quotas. Recovered literals
    /// remain borrowed in the declaration until their caller admits and copies them.
    pub fn apply(&self, value: ValueRef<'_>, maximum: usize) -> Result<Step, Error> {
        match value {
            ValueRef::Missing if self.missing.is_some() => Ok(Step::Recovered),
            ValueRef::Missing => Err(Error::Diagnostic(diagnostic(
                C::StrCaseMissing,
                &self.path,
                &[("variable", Value::Str(self.source.clone()))],
            ))),
            ValueRef::Str(text) => transform(text, self.mode, maximum)
                .map(|text| Step::Value(Value::Str(text)))
                .map_err(Error::Limit),
            other => Err(Error::Diagnostic(self.input_diagnostic(match other {
                ValueRef::Int(_) => "int",
                ValueRef::Float(_) => "float",
                ValueRef::Bool(_) => "bool",
                ValueRef::Date(_) => "date",
                ValueRef::DateTime(_) => "datetime",
                _ => unreachable!(),
            }))),
        }
    }
}
fn diagnostic(code: C, path: &str, fields: &[(&str, Value)]) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: fields
            .iter()
            .map(|(name, value)| ((*name).into(), V::Scalar(value.clone())))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// ASCII substitution preserves every other scalar and its UTF-8 byte width.
/// Admission precedes the output allocation, including empty and NUL text.
pub fn transform(input: &str, mode: Case, maximum: usize) -> Result<String, OutputLimit> {
    if input.len() > maximum {
        return Err(OutputLimit {
            required: input.len(),
        });
    }
    let mut output = String::with_capacity(input.len());
    let mut in_word = false;
    for (index, character) in input.chars().enumerate() {
        let substituted = match mode {
            Case::Upper => character.to_ascii_uppercase(),
            Case::Lower => character.to_ascii_lowercase(),
            Case::Sentence if index == 0 => character.to_ascii_uppercase(),
            Case::Sentence => character.to_ascii_lowercase(),
            Case::Title if character.is_ascii_alphabetic() => {
                let result = if in_word {
                    character.to_ascii_lowercase()
                } else {
                    character.to_ascii_uppercase()
                };
                in_word = true;
                result
            }
            Case::Title => {
                in_word = false;
                character
            }
        };
        output.push(substituted);
    }
    Ok(output)
}
