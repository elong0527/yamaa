//! Closed string-template parsing and whole-output admission, without data effects.
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    numeric_parser::SourceSpan,
    table::ValueRef,
    value::Value,
};
use alloc::{format, string::String, vec, vec::Vec};
pub const MAX_DECLARATION_BYTES: usize = 16_777_216;
pub const MAX_COMPILE_WORK: usize = 100_000_000;
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Shape,
    Limit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterializeError {
    Shape,
    Limit,
}
/// One counter shared by every declaration in a compilation request.
pub(crate) struct CompileBudget(usize);
impl CompileBudget {
    pub(crate) fn new() -> Self {
        Self(0)
    }
    pub(crate) fn charge(&mut self, amount: usize) -> Result<(), AdmissionError> {
        self.0 = self
            .0
            .checked_add(amount)
            .filter(|n| *n <= MAX_COMPILE_WORK)
            .ok_or(AdmissionError::Limit)?;
        Ok(())
    }
}
impl StringTemplate {
    /// Includes parser spans, identifiers, operands, diagnostics and retained plan copies.
    pub fn declaration_bytes(template: &str, path: &str, handler_bytes: usize) -> Option<usize> {
        template
            .len()
            .checked_mul(128)?
            .checked_add(
                path.len()
                    .checked_mul(template.len().checked_add(16)?)?
                    .checked_mul(8)?,
            )?
            .checked_add(handler_bytes.checked_mul(8)?)?
            .checked_add(2048)
    }
    pub fn new(
        template: String,
        path: String,
        missing: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        Self::with_budget(template, path, missing, &mut CompileBudget::new())
    }
    pub(crate) fn with_budget(
        template: String,
        path: String,
        missing: Option<Value>,
        budget: &mut CompileBudget,
    ) -> Result<Self, AdmissionError> {
        if path.is_empty() {
            return Err(AdmissionError::Shape);
        }
        let handler = match &missing {
            Some(Value::Str(v)) => v.len(),
            _ => 0,
        };
        if Self::declaration_bytes(&template, &path, handler)
            .is_none_or(|n| n > MAX_DECLARATION_BYTES)
        {
            return Err(AdmissionError::Limit);
        }
        budget.charge(template.len())?;
        let bytes = template.as_bytes();
        let mut spans = Vec::new();
        spans
            .try_reserve(bytes.len())
            .map_err(|_| AdmissionError::Limit)?;
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
            let mut context = vec![("reason", V::Scalar(Value::Str(reason.into())))];
            if let Some(name) = placeholder {
                context.push(("placeholder", V::Scalar(Value::Str(name.into()))));
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
        parts
            .try_reserve(spans.len())
            .map_err(|_| AdmissionError::Limit)?;
        identifiers
            .try_reserve(spans.len())
            .map_err(|_| AdmissionError::Limit)?;
        for (start, end, kind) in spans {
            budget.charge(1)?;
            parts.push(match kind {
                0 => Part::Text { start, end },
                1 => Part::Brace(char::from(bytes[start])),
                _ => {
                    let name = &template[start..end];
                    let mut selected = None;
                    for (index, existing) in identifiers.iter().enumerate() {
                        budget.charge(
                            name.len()
                                .checked_add(existing.len())
                                .ok_or(AdmissionError::Limit)?,
                        )?;
                        if existing == name {
                            selected = Some(index);
                            break;
                        }
                    }
                    let index = selected.unwrap_or(identifiers.len());
                    if selected.is_none() {
                        identifiers.push(name.into());
                    }
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
                ("source", V::Scalar(Value::Str(source.clone()))),
                ("expected", V::Scalar(Value::Str("str".into()))),
                ("actual", V::Scalar(Value::Str(actual.into()))),
            ],
        ))
    }
    pub fn unknown_diagnostic(&self, index: usize) -> Option<Diagnostic> {
        let source = self.identifiers.get(index)?;
        Some(diagnostic(
            C::ExtractUnknown,
            &format!("{}.template", self.path),
            [("identifier", V::Scalar(Value::Str(source.clone())))],
        ))
    }
    pub fn missing_diagnostic(&self, index: usize) -> Option<Diagnostic> {
        Some(diagnostic(
            C::StrCaseMissing,
            &self.path,
            [(
                "variable",
                V::Scalar(Value::Str(self.identifiers.get(index)?.clone())),
            )],
        ))
    }
    /// Values are in occurrence order, deliberately allowing repeated independent reads.
    pub fn output_bytes(&self, values: &[Value]) -> Result<usize, MaterializeError> {
        if self.deferred.is_some() {
            return Err(MaterializeError::Shape);
        }
        let mut occurrence = 0;
        let mut total = 0usize;
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
            total = total
                .checked_add(text.len())
                .ok_or(MaterializeError::Limit)?;
        }
        if occurrence != values.len() {
            return Err(MaterializeError::Shape);
        }
        Ok(total)
    }
    /// Complete UTF-8 byte admission precedes the sole output allocation and copy.
    pub fn materialize(
        &self,
        values: &[Value],
        maximum: usize,
    ) -> Result<String, MaterializeError> {
        let total = self.output_bytes(values)?;
        if total > maximum {
            return Err(MaterializeError::Limit);
        }
        let mut result = String::new();
        result
            .try_reserve_exact(total)
            .map_err(|_| MaterializeError::Limit)?;
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
