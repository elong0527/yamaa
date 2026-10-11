//! Canonical scalar text with Unicode-character left padding (REQ-1261).
use crate::{
    conversion::{convert, ConversionError},
    value::{ColumnType, Value},
};
use alloc::{borrow::Cow, string::String};

pub const MAX_DECLARATION_BYTES: usize = 16_777_216;
/// Canonical primitive formatting has a fixed bound; authored text stays borrowed.
pub const MAX_PRIMITIVE_TEXT_BYTES: usize = 327;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclarationError {
    Shape,
    MetadataLimit,
    Width,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputLimit {
    pub required: Option<usize>,
}

/// Missing does not inspect width or allocate text. Present text has no handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringPad {
    source: String,
    path: String,
    width_text: String,
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
    pub(crate) fn metadata_bytes(source: &str, path: &str, width: &str) -> Option<usize> {
        source
            .len()
            .checked_add(path.len())?
            .checked_add(width.len())?
            .checked_mul(8)?
            .checked_add(512)
    }
    /// Admit all metadata before scanning the mathematical integer or copying text.
    /// A positive width beyond usize stays valid; present output refuses overflow.
    pub fn new(source: &str, path: &str, width: &str) -> Result<Self, DeclarationError> {
        if Self::metadata_bytes(source, path, width).is_none_or(|n| n > MAX_DECLARATION_BYTES) {
            return Err(DeclarationError::MetadataLimit);
        }
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
            width_text: width.into(),
            width: width.parse().ok(),
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn width_text(&self) -> &str {
        &self.width_text
    }

    /// The caller admits authored subject scan work before this call. No authored
    /// text is copied. Primitive conversion reuses the core conversion owner and
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
    /// Refuse the complete output size before its allocation or any authored copy.
    pub fn render(self, maximum: usize) -> Result<Value, OutputLimit> {
        match self.0 {
            Text::Missing => Ok(Value::Missing),
            Text::Present {
                text,
                padding,
                required,
            } => {
                let Some(required) = required.filter(|&n| n <= maximum) else {
                    return Err(OutputLimit { required });
                };
                let padding = padding.expect("representable required output");
                if padding == 0 {
                    return Ok(Value::Str(text.into_owned()));
                }
                let mut output = String::with_capacity(required);
                output.extend(core::iter::repeat_n(' ', padding));
                output.push_str(&text);
                Ok(Value::Str(output))
            }
        }
    }
}

impl DeclarationError {
    /// Only a reached positive-width failure has a normative projection.
    pub fn diagnostic(self, path: &str) -> Option<crate::diagnostic::Diagnostic> {
        if self != Self::Width
            || path.is_empty()
            || path
                .len()
                .checked_mul(8)
                .and_then(|n| n.checked_add(512))
                .is_none_or(|n| n > MAX_DECLARATION_BYTES)
        {
            return None;
        }
        Some(crate::diagnostic::Diagnostic {
            code: crate::diagnostic::ConditionCode::StrPadWidth,
            spec_paths: alloc::vec![alloc::format!("{path}.width")],
            context: [(
                "expected".into(),
                crate::diagnostic::ContextValue::Scalar(Value::Str(
                    "a positive integer width".into(),
                )),
            )]
            .into_iter()
            .collect(),
            source_span: None,
            operand_route: None,
        })
    }
}
