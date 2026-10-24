//! The five locale-independent ASCII case modes admitted by text declarations.
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    value::Value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Sentence,
    Title,
    /// SAS `PROPCASE`: the first scalar, and each scalar directly after one
    /// of the delimiters, raised, and every other scalar lowered (TEXT-5).
    Proper(Delimiters),
}

/// The ASCII scalars after which `proper` raises a scalar: one bit for each
/// of the 128 ASCII scalars, so a set is one value and a check is one test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delimiters(u128);

impl Delimiters {
    /// SAS `PROPCASE`'s six, which `proper` takes when it lists none: the
    /// blank, `/`, `-`, `(`, `.` and the tab.
    pub const PROPCASE: Self =
        Self(bit(b' ') | bit(b'/') | bit(b'-') | bit(b'(') | bit(b'.') | bit(b'\t'));

    /// The delimiters `list` names: at least one ASCII scalar, each listed
    /// once; a list that names none, repeats one or holds a scalar outside
    /// ASCII names no set.
    pub fn new(list: &str) -> Option<Self> {
        let mut set = 0;
        for scalar in list.chars() {
            let member = u8::try_from(scalar).ok().filter(u8::is_ascii).map(bit)?;
            if set & member != 0 {
                return None;
            }
            set |= member;
        }
        (set != 0).then_some(Self(set))
    }

    /// Whether `scalar` is one of the delimiters, compared as the same scalar.
    fn contains(self, scalar: char) -> bool {
        u8::try_from(scalar)
            .ok()
            .filter(u8::is_ascii)
            .is_some_and(|byte| self.0 & bit(byte) != 0)
    }
}

const fn bit(byte: u8) -> u128 {
    1 << byte
}

/// `delimiters` written at `path`, the declaration's own path, beside `to`,
/// a mode that takes no list: the field is not one that mode has, as a field
/// a temporal operation's declared type does not take (TEXT-5, TIME-1).
pub(crate) fn delimiters_not_taken(path: &str, to: &str) -> Diagnostic {
    diagnostic(
        C::StrCaseDelimitersNotTaken,
        &format!("{path}.delimiters"),
        &[
            ("field", Value::Str("delimiters".into())),
            ("to", Value::Str(to.into())),
            ("identifier", Value::Str("delimiters".into())),
        ],
    )
}

/// A `delimiters` list written at `path`, the declaration's own path, that
/// names no ASCII scalar, repeats one or holds a scalar outside ASCII
/// (TEXT-5).
pub(crate) fn delimiters_invalid(path: &str) -> Diagnostic {
    diagnostic(
        C::StrCaseDelimiters,
        &format!("{path}.delimiters"),
        &[(
            "expected",
            Value::Str("a non-empty string of ASCII characters, each listed once".into()),
        )],
    )
}
/// A closed case declaration owns metadata and preserves an omitted handler.
#[derive(Clone, Debug, PartialEq)]
pub struct StringCase {
    source: String,
    path: String,
    mode: Case,
    missing: Option<Value>,
}
/// `Recovered` is a missing source, answered by the declared `missing` or by
/// the default missing value.
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
    /// The declared `missing` literal, or missing when it is omitted.
    pub fn missing_substitute(&self) -> Value {
        self.missing.clone().unwrap_or(Value::Missing)
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
            ValueRef::Missing => Ok(Step::Recovered),
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
    // The preceding scalar after downward substitution: `proper` identifies
    // delimiter positions in the lowered text before raising any scalar.
    let mut previous = None;
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
            Case::Proper(delimiters) => match previous {
                None => character.to_ascii_uppercase(),
                Some(before) if delimiters.contains(before) => character.to_ascii_uppercase(),
                Some(_) => character.to_ascii_lowercase(),
            },
        };
        previous = Some(character.to_ascii_lowercase());
        output.push(substituted);
    }
    output
}
