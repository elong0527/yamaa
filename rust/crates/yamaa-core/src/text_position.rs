//! The positional text operations: a piece of a text by scalar position
//! (TEXT-13) and the number of scalars in a text (TEXT-14). A position counts
//! Unicode scalar values from 1, never bytes, so a text splits the same way
//! on every host.
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    value::Value,
};

/// A `str_substr` declaration the language refuses: a `start` or a `length`
/// below one (TEXT-13). `Shape` is a declaration the tree's admission never
/// builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DeclarationError {
    #[error("text substring shape")]
    Shape,
    #[error("text substring start")]
    Start,
    #[error("text substring length")]
    Length,
}

/// `str_substr`: at most `length` scalars of a text, from the scalar at
/// `start`. A position beyond `usize` stays valid and is held as `None`: no
/// text reaches such a `start`, and such a `length` takes the rest, as an
/// omitted one does.
#[derive(Clone, Debug, PartialEq)]
pub struct StringSubstr {
    source: String,
    path: String,
    start: Option<usize>,
    length: Option<usize>,
    missing: Option<Value>,
}

/// `str_length`: the number of scalars in a text.
#[derive(Clone, Debug, PartialEq)]
pub struct StringLength {
    source: String,
    path: String,
    missing: Option<Value>,
}

/// `Recovered` is a missing source, answered by the declared `missing` or by
/// the default missing value.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Value(Value),
    Recovered,
}

/// A source that is not text (TEXT-1).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("text position diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
}

impl StringSubstr {
    /// The declaration with `start` and `length` as written; a `start` below
    /// one is refused before a `length` below one.
    pub fn new(
        source: String,
        path: String,
        start: &str,
        length: Option<&str>,
        missing: Option<Value>,
    ) -> Result<Self, DeclarationError> {
        if source.is_empty() || path.is_empty() {
            return Err(DeclarationError::Shape);
        }
        let start = position(start, DeclarationError::Start)?;
        let length = match length {
            Some(length) => position(length, DeclarationError::Length)?,
            None => None,
        };
        Ok(Self {
            source,
            path,
            start,
            length,
            missing,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    /// The declared `missing` literal, or missing when it is omitted.
    pub fn missing_substitute(&self) -> Value {
        self.missing.clone().unwrap_or(Value::Missing)
    }
    pub(crate) fn input_diagnostic(&self, actual: &'static str) -> Diagnostic {
        input_diagnostic(&self.source, &self.path, actual)
    }
    pub fn apply(&self, value: ValueRef<'_>) -> Result<Step, Error> {
        match value {
            ValueRef::Missing => Ok(Step::Recovered),
            ValueRef::Str(text) => Ok(Step::Value(self.piece(text))),
            other => Err(Error::Diagnostic(self.input_diagnostic(kind(other)))),
        }
    }
    /// The scalars of `text` from `start`, at most `length` of them, or
    /// missing when `start` lies past its last scalar, as every `start` lies
    /// past the end of the empty string.
    fn piece(&self, text: &str) -> Value {
        let Some(start) = self.start else {
            return Value::Missing;
        };
        let mut offsets = text
            .char_indices()
            .map(|(offset, _)| offset)
            .skip(start - 1);
        let Some(begin) = offsets.next() else {
            return Value::Missing;
        };
        let end = self
            .length
            .and_then(|length| offsets.nth(length - 1))
            .unwrap_or(text.len());
        Value::Str(text[begin..end].to_owned())
    }
}

impl StringLength {
    /// Validate required declaration ownership without executing it.
    pub fn new(source: String, path: String, missing: Option<Value>) -> Option<Self> {
        if source.is_empty() || path.is_empty() {
            return None;
        }
        Some(Self {
            source,
            path,
            missing,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    /// The declared `missing` literal, or missing when it is omitted.
    pub fn missing_substitute(&self) -> Value {
        self.missing.clone().unwrap_or(Value::Missing)
    }
    pub(crate) fn input_diagnostic(&self, actual: &'static str) -> Diagnostic {
        input_diagnostic(&self.source, &self.path, actual)
    }
    pub fn apply(&self, value: ValueRef<'_>) -> Result<Step, Error> {
        match value {
            ValueRef::Missing => Ok(Step::Recovered),
            ValueRef::Str(text) => Ok(Step::Value(Value::Int(length(text)))),
            other => Err(Error::Diagnostic(self.input_diagnostic(kind(other)))),
        }
    }
}

/// The number of scalars in `text`, the count SUB-8's `length` bounds. A
/// text holds fewer scalars than bytes, so the count always fits.
fn length(text: &str) -> i64 {
    i64::try_from(text.chars().count()).unwrap_or(i64::MAX)
}

impl DeclarationError {
    /// Only a reached position below one has a normative projection.
    pub fn diagnostic(self, path: &str) -> Option<Diagnostic> {
        let field = match self {
            Self::Start => "start",
            Self::Length => "length",
            Self::Shape => return None,
        };
        if path.is_empty() {
            return None;
        }
        Some(Diagnostic {
            code: C::StrSubstrPosition,
            spec_paths: vec![format!("{path}.{field}")],
            context: [(
                "expected".into(),
                V::String(format!("a positive integer {field}")),
            )]
            .into_iter()
            .collect(),
            source_span: None,
            operand_route: None,
        })
    }
}

/// A position as written: its value, `None` beyond `usize`, or `below` for
/// zero or a negative integer. Lowering writes only integers.
fn position(text: &str, below: DeclarationError) -> Result<Option<usize>, DeclarationError> {
    let digits = |text: &str| {
        text.as_bytes()
            .first()
            .is_some_and(|first| matches!(first, b'1'..=b'9'))
            && text.bytes().all(|byte| byte.is_ascii_digit())
    };
    if text == "0" || text.strip_prefix('-').is_some_and(digits) {
        return Err(below);
    }
    if !digits(text) {
        return Err(DeclarationError::Shape);
    }
    Ok(text.parse().ok())
}

/// The refusal of a source that is not text (TEXT-1), at its `source` field.
pub(crate) fn input_diagnostic(source: &str, path: &str, actual: &'static str) -> Diagnostic {
    Diagnostic {
        code: C::StrCaseInput,
        spec_paths: vec![format!("{path}.source")],
        context: [
            ("source", Value::Str(source.into())),
            ("expected", Value::Str("str".into())),
            ("actual", Value::Str(actual.into())),
        ]
        .into_iter()
        .map(|(name, value)| (name.into(), V::from(value)))
        .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// The type name of a present value that is not text.
fn kind(value: ValueRef<'_>) -> &'static str {
    match value {
        ValueRef::Int(_) => "int",
        ValueRef::Float(_) => "float",
        ValueRef::Bool(_) => "bool",
        ValueRef::Date(_) => "date",
        ValueRef::DateTime(_) => "datetime",
        ValueRef::Missing | ValueRef::Str(_) => unreachable!("missing and text are answered"),
    }
}
