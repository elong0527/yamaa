//! Canonical scalar text with Unicode-character left padding (TEXT-2).
use crate::{
    conversion::{convert, ConversionError},
    limits::{LimitExceeded, Limits},
    value::{ColumnType, Value},
};
use std::borrow::Cow;

/// Canonical primitive formatting has a fixed bound; authored text stays borrowed.
pub const MAX_PRIMITIVE_TEXT_BYTES: usize = 327;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DeclarationError {
    #[error("text pad shape")]
    Shape,
    #[error("text pad width")]
    Width,
}

/// Missing does not inspect width or allocate text. Present text has no handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringPad {
    source: String,
    path: String,
    width: Option<usize>,
}

/// A borrowed string or bounded canonical primitive, before output allocation.
#[derive(Debug)]
pub struct PreparedText<'a>(Text<'a>);
#[derive(Debug)]
enum Text<'a> {
    Missing,
    Present {
        text: Cow<'a, str>,
        padding: Option<usize>,
        required: Option<usize>,
    },
}

impl StringPad {
    /// A positive width beyond usize stays valid; present output refuses it
    /// as text longer than `Limits::TEXT_BYTES`.
    pub fn new(source: &str, path: &str, width: &str) -> Result<Self, DeclarationError> {
        if source.is_empty() || path.is_empty() {
            return Err(DeclarationError::Shape);
        }
        if width == "0"
            || width.strip_prefix('-').is_some_and(|n| {
                n.as_bytes()
                    .first()
                    .is_some_and(|b| matches!(b, b'1'..=b'9'))
                    && n.bytes().all(|b| b.is_ascii_digit())
            })
        {
            return Err(DeclarationError::Width);
        }
        if width.is_empty()
            || !matches!(width.as_bytes()[0], b'1'..=b'9')
            || !width.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(DeclarationError::Shape);
        }
        Ok(Self {
            source: source.into(),
            path: path.into(),
            width: width.parse().ok(),
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Authored text stays borrowed. Primitive conversion reuses the core conversion owner and
    /// allocates at most MAX_PRIMITIVE_TEXT_BYTES; bool preserves its existing error.
    pub fn prepare_text<'a>(&self, value: &'a Value) -> Result<PreparedText<'a>, ConversionError> {
        let text = match value {
            Value::Missing => return Ok(PreparedText(Text::Missing)),
            Value::Str(text) => Cow::Borrowed(text.as_str()),
            value => {
                let Value::Str(text) = convert(value, ColumnType::Str)? else {
                    unreachable!("present scalar converted to text")
                };
                debug_assert!(text.len() <= MAX_PRIMITIVE_TEXT_BYTES);
                Cow::Owned(text)
            }
        };
        let padding = self
            .width
            .map(|width| width.saturating_sub(text.chars().count()));
        let required = padding.and_then(|padding| text.len().checked_add(padding));
        Ok(PreparedText(Text::Present {
            text,
            padding,
            required,
        }))
    }
}

impl PreparedText<'_> {
    pub fn required_bytes(&self) -> Option<usize> {
        match &self.0 {
            Text::Missing => Some(0),
            Text::Present { required, .. } => *required,
        }
    }
    /// Refuse a result longer than `Limits::TEXT_BYTES` before its allocation
    /// or any authored copy (TEXT-2).
    pub fn render(self) -> Result<Value, LimitExceeded> {
        match self.0 {
            Text::Missing => Ok(Value::Missing),
            Text::Present {
                text,
                padding,
                required,
            } => {
                Limits::text_bytes(required)?;
                let padding = padding.expect("an admitted output has its padding");
                if padding == 0 {
                    return Ok(Value::Str(text.into_owned()));
                }
                let mut output = String::with_capacity(text.len() + padding);
                output.extend(std::iter::repeat_n(' ', padding));
                output.push_str(&text);
                Ok(Value::Str(output))
            }
        }
    }
}

impl DeclarationError {
    /// Only a reached positive-width failure has a normative projection.
    pub fn diagnostic(self, path: &str) -> Option<crate::diagnostic::Diagnostic> {
        if self != Self::Width || path.is_empty() {
            return None;
        }
        Some(crate::diagnostic::Diagnostic {
            code: crate::diagnostic::ConditionCode::StrPadWidth,
            spec_paths: std::vec![std::format!("{path}.width")],
            context: [(
                "expected".into(),
                crate::json::Value::String("a positive integer width".into()),
            )]
            .into_iter()
            .collect(),
            source_span: None,
            operand_route: None,
        })
    }
}
