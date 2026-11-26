//! A number written as text with a fixed count of decimals (TEXT-15). The
//! one rounding and the exact decimal writing are CSV display precision's
//! ([`fixed_point`], CSV-9), and the left padding is `str_pad`'s
//! ([`StringPad`], TEXT-2), so each has one implementation.
use crate::json::Value as Json;
use crate::{
    csv_precision::fixed_point,
    diagnostic::{ConditionCode as C, Diagnostic},
    limits::Limits,
    table::ValueRef,
    text_missing::{Error, MissingCountedText, Step},
    text_pad::StringPad,
    value::{FiniteFloat, Value},
};

/// Why a declaration has no operation.
#[derive(Clone, Debug, PartialEq)]
pub enum Admission {
    /// A shape the schema and the lowering never give.
    Shape,
    /// `decimals` below zero or `width` below one, each at its own field, in
    /// field order.
    Findings(Vec<Diagnostic>),
}

/// A declaration of `str_decimal`. A count too large for any text stays
/// valid, as a `str_pad` width does: missing needs none, and a present value
/// refuses the text it cannot hold.
#[derive(Clone, Debug, PartialEq)]
pub struct StringDecimal {
    source: String,
    path: String,
    /// `decimals`, or none beyond the platform's lengths.
    decimals: Option<usize>,
    /// The padding to `width`.
    pad: Option<StringPad>,
    missing: Option<Value>,
}

/// The sign of a canonical integer as the decoder writes it: `0`, digits
/// without a leading zero, or those digits after `-`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sign {
    Negative,
    Zero,
    Positive,
}

fn sign(text: &str) -> Result<Sign, Admission> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (Sign::Negative, digits),
        None if text == "0" => return Ok(Sign::Zero),
        None => (Sign::Positive, text),
    };
    match digits.as_bytes() {
        [b'1'..=b'9', rest @ ..] if rest.iter().all(u8::is_ascii_digit) => Ok(sign),
        _ => Err(Admission::Shape),
    }
}

/// The refusal of the integer `written` at `<path>.<field>` under `code`.
fn declaration(code: C, path: &str, field: &str, expected: &str, written: &str) -> Diagnostic {
    let actual = written.parse().map_or(Json::Null, Json::Number);
    Diagnostic::new(
        code,
        vec![format!("{path}.{field}")],
        [
            ("expected".into(), Json::String(expected.into())),
            ("actual".into(), actual),
        ]
        .into(),
    )
}

/// The refusal at `<path>.decimals`, under `code`, of a `decimals` count
/// below zero, the one judgement of the verbs that write a number to that many
/// places (TEXT-15, TIME-21); `Shape` for integer text the lowering never
/// keeps.
pub(crate) fn negative_decimals(
    code: C,
    path: &str,
    written: &str,
) -> Result<Option<Diagnostic>, Admission> {
    Ok((sign(written)? == Sign::Negative)
        .then(|| declaration(code, path, "decimals", "a non-negative integer", written)))
}

impl StringDecimal {
    /// Admit the canonical integers the lowering keeps as written: `decimals`
    /// of zero or more and an optional `width` of one or more.
    pub fn new(
        source: String,
        path: String,
        decimals: &str,
        width: Option<&str>,
        missing: Option<Value>,
    ) -> Result<Self, Admission> {
        if source.is_empty() || path.is_empty() {
            return Err(Admission::Shape);
        }
        let mut findings = Vec::new();
        findings.extend(negative_decimals(
            C::StrDecimalDeclaration,
            &path,
            decimals,
        )?);
        if let Some(width) = width {
            if sign(width)? != Sign::Positive {
                findings.push(declaration(
                    C::StrDecimalDeclaration,
                    &path,
                    "width",
                    "a positive integer",
                    width,
                ));
            }
        }
        if !findings.is_empty() {
            return Err(Admission::Findings(findings));
        }
        let pad = width
            .map(|width| StringPad::new(&source, &path, width).map_err(|_| Admission::Shape))
            .transpose()?;
        Ok(Self {
            decimals: decimals.parse().ok(),
            source,
            path,
            pad,
            missing,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    /// The refusal of a source of type `actual` at `<path>.source`.
    pub fn input_diagnostic(source: &str, path: &str, actual: &str) -> Diagnostic {
        Diagnostic::new(
            C::StrDecimalInput,
            vec![format!("{path}.source")],
            [
                ("source", source),
                ("expected", "numeric"),
                ("actual", actual),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), Json::String(value.into())))
            .collect(),
        )
    }
    fn refusal(&self, actual: &str) -> Error {
        Error::Diagnostic(Self::input_diagnostic(&self.source, &self.path, actual))
    }
}

impl MissingCountedText for StringDecimal {
    fn path(&self) -> &str {
        &self.path
    }
    fn missing(&self) -> Option<&Value> {
        self.missing.as_ref()
    }
    /// Missing precedes the source's type. An `int` widens to the nearest
    /// binary64 (VALUE-8) before the one rounding, as a numeric operand of a
    /// `float` result does. Text longer than `Limits::TEXT_BYTES` is refused
    /// before it is built, whether its decimals or its width would make it
    /// so; the writing of the decimals does not count a length it refuses.
    fn apply(&self, value: ValueRef<'_>) -> Result<Step, Error> {
        let number = match value {
            ValueRef::Missing => return Ok(Step::Recovered),
            ValueRef::Int(value) => {
                FiniteFloat::new(value as f64).expect("an int widens to a finite binary64")
            }
            ValueRef::Float(value) => value,
            ValueRef::Str(_) => return Err(self.refusal("str")),
            ValueRef::Bool(_) => return Err(self.refusal("bool")),
            ValueRef::Date(_) => return Err(self.refusal("date")),
            ValueRef::DateTime(_) => return Err(self.refusal("datetime")),
            ValueRef::Time(_) => return Err(self.refusal("time")),
        };
        let uncounted =
            || Error::Limit(Limits::text_bytes(None).expect_err("no uncounted text is admitted"));
        let decimals = self.decimals.ok_or_else(uncounted)?;
        let text =
            Value::Str(fixed_point(number, decimals, Limits::TEXT_BYTES).map_err(|_| uncounted())?);
        let Some(pad) = &self.pad else {
            return Ok(Step::Value(text));
        };
        pad.prepare_text(&text)
            .expect("text needs no conversion")
            .render()
            .map(Step::Value)
            .map_err(Error::Limit)
    }
}
