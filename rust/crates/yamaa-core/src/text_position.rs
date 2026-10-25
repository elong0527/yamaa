//! The positional text operations: a piece of a text by scalar position
//! (TEXT-13) or between words (TEXT-16), and the number of scalars in a text
//! (TEXT-14). A position counts Unicode scalar values from 1, never bytes, so
//! a text splits the same way on every host.
use crate::json::Value as V;
use crate::portable_regex::is_space;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    value::Value,
};

/// A `str_substr` declaration the language refuses: a `start`, `piece` or
/// `length` below one (TEXT-13, TEXT-16), a `start` beside `piece`, or
/// neither of them, or a `piece` without `length` (TEXT-16). `Shape` is a
/// declaration the tree's admission never builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DeclarationError {
    #[error("text substring shape")]
    Shape,
    #[error("text substring start")]
    Start,
    #[error("text substring length")]
    Length,
    #[error("text substring piece")]
    Piece,
    #[error("text substring start beside piece")]
    StartBesidePiece,
    #[error("text substring without start or piece")]
    StartRequired,
    #[error("text substring piece without length")]
    LengthRequired,
}

/// `str_substr`: at most `length` scalars of a text, from the scalar at
/// `start` or as the `piece`-th piece broken between words. A position
/// beyond `usize` stays valid and is held as `None`: no text reaches such a
/// `start` or `piece`, and such a `length` takes the rest, as an omitted one
/// does.
#[derive(Clone, Debug, PartialEq)]
pub struct StringSubstr {
    source: String,
    path: String,
    begin: Begin,
    length: Option<usize>,
    missing: Option<Value>,
}

/// Where a `str_substr` piece begins: at a scalar position (TEXT-13), or
/// after the pieces before it (TEXT-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Begin {
    Start(Option<usize>),
    Piece(Option<usize>),
}

/// The fields of a `str_substr` declaration as written.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fields<'a> {
    pub start: Option<&'a str>,
    pub length: Option<&'a str>,
    pub piece: Option<&'a str>,
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
    /// The declaration with its fields as written. Which fields are
    /// declared is judged before their values, and a `start` or `piece`
    /// below one is refused before a `length` below one.
    pub fn new(
        source: String,
        path: String,
        fields: Fields<'_>,
        missing: Option<Value>,
    ) -> Result<Self, DeclarationError> {
        if source.is_empty() || path.is_empty() {
            return Err(DeclarationError::Shape);
        }
        let begin = match (fields.start, fields.piece, fields.length) {
            (Some(_), Some(_), _) => return Err(DeclarationError::StartBesidePiece),
            (None, None, _) => return Err(DeclarationError::StartRequired),
            (None, Some(_), None) => return Err(DeclarationError::LengthRequired),
            (Some(start), None, _) => Begin::Start(position(start, DeclarationError::Start)?),
            (None, Some(piece), Some(_)) => Begin::Piece(position(piece, DeclarationError::Piece)?),
        };
        let length = match fields.length {
            Some(length) => position(length, DeclarationError::Length)?,
            None => None,
        };
        Ok(Self {
            source,
            path,
            begin,
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
            ValueRef::Str(text) => Ok(Step::Value(match self.begin {
                Begin::Start(start) => self.at_start(text, start),
                Begin::Piece(piece) => self.piece(text, piece),
            })),
            other => Err(Error::Diagnostic(self.input_diagnostic(kind(other)))),
        }
    }
    /// The scalars of `text` from `start`, at most `length` of them, or
    /// missing when `start` lies past its last scalar, as every `start` lies
    /// past the end of the empty string.
    fn at_start(&self, text: &str, start: Option<usize>) -> Value {
        let Some(start) = start else {
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
    /// The `piece`-th piece of `text` broken between words, or missing past
    /// its last piece, as every `piece` lies past the end of the empty
    /// string (TEXT-16).
    fn piece(&self, text: &str, piece: Option<usize>) -> Value {
        let Some(piece) = piece else {
            return Value::Missing;
        };
        let length = self.length.unwrap_or(usize::MAX);
        let mut rest = text;
        for _ in 1..piece {
            if rest.is_empty() {
                return Value::Missing;
            }
            rest = &rest[word_break(rest, length).1..];
        }
        if rest.is_empty() {
            return Value::Missing;
        }
        Value::Str(rest[..word_break(rest, length).0].to_owned())
    }
}

/// The byte offsets where the first piece of a nonempty `rest` ends and the
/// next begins: all of a text of at most `length` scalars; else after the
/// last word end among its first `length` scalars, with the whitespace after
/// it dropped; else after exactly `length` scalars (TEXT-16).
fn word_break(rest: &str, length: usize) -> (usize, usize) {
    let mut scalars = rest.char_indices().peekable();
    let mut word_end = None;
    let mut cut = rest.len();
    let mut taken = 0;
    while let Some((offset, scalar)) = scalars.next() {
        if taken == length {
            cut = offset;
            break;
        }
        taken += 1;
        let next = scalars
            .peek()
            .map(|&(next, scalar)| (next, is_space(scalar)));
        if let Some((next, true)) = next {
            if !is_space(scalar) {
                word_end = Some(next);
            }
        }
    }
    if cut == rest.len() {
        return (cut, cut);
    }
    match word_end {
        Some(end) => {
            let spaces = rest[end..]
                .char_indices()
                .find(|&(_, scalar)| !is_space(scalar))
                .map_or(rest.len(), |(offset, _)| end + offset);
            (end, spaces)
        }
        None => (cut, cut),
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
    /// Every reached refusal but `Shape` has a normative projection, at the
    /// field it names.
    pub fn diagnostic(self, path: &str) -> Option<Diagnostic> {
        let (code, field) = match self {
            Self::Start => (C::StrSubstrPosition, "start"),
            Self::Length => (C::StrSubstrPosition, "length"),
            Self::Piece => (C::StrSubstrPiece, "piece"),
            Self::StartBesidePiece => (C::StrSubstrStartBesidePiece, "start"),
            Self::StartRequired => (C::StrSubstrFieldRequired, "start"),
            Self::LengthRequired => (C::StrSubstrFieldRequired, "length"),
            Self::Shape => return None,
        };
        if path.is_empty() {
            return None;
        }
        let context: Vec<(&str, String)> = match self {
            Self::Start | Self::Length | Self::Piece => {
                vec![("expected", format!("a positive integer {field}"))]
            }
            Self::StartBesidePiece => vec![
                ("class", "str_substr".into()),
                ("field", field.into()),
                ("identifier", field.into()),
            ],
            _ => vec![("class", "str_substr".into()), ("field", field.into())],
        };
        Some(Diagnostic {
            code,
            spec_paths: vec![format!("{path}.{field}")],
            context: context
                .into_iter()
                .map(|(key, value)| (key.into(), V::String(value)))
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
