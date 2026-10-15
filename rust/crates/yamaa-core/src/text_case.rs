//! The four locale-independent ASCII substitutions admitted by text declarations.
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    value::Value,
};
use serde_json::Value as V;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Sentence,
    Title,
}
/// A closed case declaration owns metadata and preserves an omitted handler.
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
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("text case diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
}
impl StringCase {
    /// Validate required declaration ownership without executing it.
    pub fn new(source: String, path: String, mode: Case, missing: Option<Value>) -> Option<Self> {
        if source.is_empty() || path.is_empty() {
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
    pub fn apply(&self, value: ValueRef<'_>) -> Result<Step, Error> {
        match value {
            ValueRef::Missing if self.missing.is_some() => Ok(Step::Recovered),
            ValueRef::Missing => Err(Error::Diagnostic(diagnostic(
                C::StrCaseMissing,
                &self.path,
                &[("variable", Value::Str(self.source.clone()))],
            ))),
            ValueRef::Str(text) => Ok(Step::Value(Value::Str(transform(text, self.mode)))),
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
            .map(|(name, value)| ((*name).into(), V::from(value.clone())))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// ASCII substitution preserves every other scalar and its UTF-8 byte width.
pub fn transform(input: &str, mode: Case) -> String {
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
    output
}
