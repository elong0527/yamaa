//! Closed string-template parsing and materialization without data effects.
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    expression::SourceSpan,
    limits::{LimitExceeded, Limits},
    table::ValueRef,
    value::Value,
};
use std::vec;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Text { start: usize, end: usize },
    Brace(char),
    Placeholder(usize),
}
#[derive(Clone, Debug, PartialEq)]
pub struct StringTemplate {
    template: String,
    path: String,
    parts: Vec<Part>,
    identifiers: Vec<String>,
    missing: Option<Value>,
    deferred: Option<Diagnostic>,
    syntax_failure_span: Option<SourceSpan>,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    #[error("text template shape")]
    Shape,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MaterializeError {
    #[error("text template shape")]
    Shape,
    /// A result longer than `Limits::TEXT_BYTES` (TEXT-4).
    #[error("text template: {0}")]
    Limit(LimitExceeded),
    #[error("text template output allocation failed")]
    Allocation,
}
impl StringTemplate {
    pub fn new(
        template: String,
        path: String,
        missing: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        if path.is_empty() {
            return Err(AdmissionError::Shape);
        }
        let bytes = template.as_bytes();
        let mut spans = Vec::new();
        let mut at = 0;
        let mut invalid = None;
        // Complete syntax precedes every identifier comparison and owned name copy.
        while at < bytes.len() {
            let start = at;
            match bytes[at] {
                b'{' | b'}' if bytes.get(at + 1) == Some(&bytes[at]) => {
                    spans.push((start, at + 2, 1u8));
                    at += 2;
                }
                b'{' => {
                    let Some(end) = bytes[at + 1..]
                        .iter()
                        .position(|b| *b == b'}')
                        .map(|n| at + 1 + n)
                    else {
                        invalid = Some((
                            "unmatched_brace",
                            None,
                            SourceSpan {
                                start,
                                end: bytes.len(),
                            },
                        ));
                        break;
                    };
                    let name = &template[at + 1..end];
                    if !valid_name(name) {
                        invalid = Some((
                            "invalid_placeholder",
                            Some(name),
                            SourceSpan {
                                start,
                                end: end + 1,
                            },
                        ));
                        break;
                    }
                    spans.push((at + 1, end, 2));
                    at = end + 1;
                }
                b'}' => {
                    invalid = Some(("unmatched_brace", None, SourceSpan { start, end: at + 1 }));
                    break;
                }
                _ => {
                    at += 1;
                    while at < bytes.len() && !matches!(bytes[at], b'{' | b'}') {
                        at += 1;
                    }
                    spans.push((start, at, 0));
                }
            }
        }
        if let Some((reason, placeholder, syntax_failure_span)) = invalid {
            let mut context = vec![("reason", V::String(reason.into()))];
            if let Some(name) = placeholder {
                context.push(("placeholder", V::String(name.into())));
            }
            let deferred = Some(diagnostic(C::TemplateInvalid, &path, context));
            return Ok(Self {
                template,
                path,
                parts: Vec::new(),
                identifiers: Vec::new(),
                missing,
                deferred,
                syntax_failure_span: Some(syntax_failure_span),
            });
        }
        let mut identifiers: Vec<String> = Vec::new();
        let mut parts = Vec::new();
        let mut name_indices = std::collections::BTreeMap::new();
        for (start, end, kind) in spans {
            parts.push(match kind {
                0 => Part::Text { start, end },
                1 => Part::Brace(char::from(bytes[start])),
                _ => {
                    let name = &template[start..end];
                    let index = *name_indices.entry(name).or_insert_with(|| {
                        let index = identifiers.len();
                        identifiers.push(name.into());
                        index
                    });
                    Part::Placeholder(index)
                }
            });
        }
        Ok(Self {
            template,
            path,
            parts,
            identifiers,
            missing,
            deferred: None,
            syntax_failure_span: None,
        })
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }
    pub fn identifiers(&self) -> &[String] {
        &self.identifiers
    }
    pub fn missing(&self) -> Option<&Value> {
        self.missing.as_ref()
    }
    pub fn declaration_diagnostic(&self) -> Option<&Diagnostic> {
        self.deferred.as_ref()
    }
    /// Half-open UTF-8 byte span of a deferred syntax refusal in the original text.
    pub fn syntax_failure_span(&self) -> Option<SourceSpan> {
        self.syntax_failure_span
    }
    pub fn literal(&self, part: &Part) -> Option<&str> {
        match part {
            Part::Text { start, end } => self.template.get(*start..*end),
            Part::Brace('{') => Some("{"),
            Part::Brace('}') => Some("}"),
            _ => None,
        }
    }
    pub fn input_diagnostic(&self, index: usize, actual: &'static str) -> Option<Diagnostic> {
        let source = self.identifiers.get(index)?;
        if !matches!(
            actual,
            "str" | "int" | "float" | "bool" | "date" | "datetime"
        ) {
            return None;
        }
        Some(diagnostic(
            C::StrCaseInput,
            &format!("{}.template", self.path),
            [
                ("source", V::String(source.clone())),
                ("expected", V::String("str".into())),
                ("actual", V::String(actual.into())),
            ],
        ))
    }
    pub fn unknown_diagnostic(&self, index: usize) -> Option<Diagnostic> {
        let source = self.identifiers.get(index)?;
        Some(diagnostic(
            C::ExtractUnknown,
            &format!("{}.template", self.path),
            [("identifier", V::String(source.clone()))],
        ))
    }
    /// A template still fails on a missing placeholder without its `missing`
    /// handler; it never gives missing by default (TEXT-4).
    pub fn missing_diagnostic(&self, index: usize) -> Option<Diagnostic> {
        Some(diagnostic(
            C::TemplateMissing,
            &self.path,
            [("variable", V::String(self.identifiers.get(index)?.clone()))],
        ))
    }
    /// Values are in occurrence order, deliberately allowing repeated independent reads.
    /// The total is none beyond the platform's lengths.
    pub fn output_bytes(&self, values: &[Value]) -> Result<Option<usize>, MaterializeError> {
        if self.deferred.is_some() {
            return Err(MaterializeError::Shape);
        }
        let mut occurrence = 0;
        let mut total = Some(0usize);
        for part in &self.parts {
            let text = match part {
                Part::Placeholder(_) => {
                    let Some(Value::Str(text)) = values.get(occurrence) else {
                        return Err(MaterializeError::Shape);
                    };
                    occurrence += 1;
                    text.as_str()
                }
                _ => self.literal(part).ok_or(MaterializeError::Shape)?,
            };
            total = total.and_then(|total| total.checked_add(text.len()));
        }
        if occurrence != values.len() {
            return Err(MaterializeError::Shape);
        }
        Ok(total)
    }
    /// Complete UTF-8 byte admission, which refuses a result longer than
    /// `Limits::TEXT_BYTES`, precedes the sole output allocation and copy.
    pub fn materialize(&self, values: &[Value]) -> Result<String, MaterializeError> {
        let total = self.output_bytes(values)?;
        Limits::text_bytes(total).map_err(MaterializeError::Limit)?;
        let total = total.expect("an admitted length");
        let mut result = String::new();
        result
            .try_reserve_exact(total)
            .map_err(|_| MaterializeError::Allocation)?;
        let mut occurrence = 0;
        for part in &self.parts {
            if matches!(part, Part::Placeholder(_)) {
                let Value::Str(text) = &values[occurrence] else {
                    unreachable!()
                };
                occurrence += 1;
                result.push_str(text);
            } else {
                result.push_str(self.literal(part).expect("admitted span"));
            }
        }
        Ok(result)
    }
    pub fn input_kind(value: ValueRef<'_>) -> Option<&'static str> {
        match value {
            ValueRef::Missing | ValueRef::Str(_) => None,
            ValueRef::Int(_) => Some("int"),
            ValueRef::Float(_) => Some("float"),
            ValueRef::Bool(_) => Some("bool"),
            ValueRef::Date(_) => Some("date"),
            ValueRef::DateTime(_) => Some("datetime"),
        }
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|segment| {
            let mut b = segment.bytes();
            b.next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
                && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
}
fn diagnostic(
    code: C,
    path: &str,
    fields: impl IntoIterator<Item = (&'static str, V)>,
) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        source_span: None,
        operand_route: None,
    }
}
