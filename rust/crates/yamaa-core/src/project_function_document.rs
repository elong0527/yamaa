//! Read the scalars of a structurally normalized document: the reader the
//! project front ends ([`crate::front_end::v0_5::project`],
//! [`crate::front_end::v1::project`]) and project call decoding share. A value
//! the engine cannot run with, or a node whose shape the schema should have
//! refused, is a finding at its authored path.
use crate::{
    schema::{Document, DocumentNode as N},
    value::{Value, ValueType},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NormalizedShape,
    IntegerRange,
    InvalidDate,
    InvalidDateTime,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub node: usize,
    pub kind: Kind,
    /// The text an out-of-range integer, an invalid date or an invalid
    /// datetime is written as; a diagnostic reports it.
    pub value: Option<String>,
}
pub(crate) struct Reader<'a> {
    document: &'a Document,
    findings: Vec<Finding>,
}
impl Reader<'_> {
    pub(crate) fn new(document: &Document) -> Reader<'_> {
        Reader {
            document,
            findings: Vec::new(),
        }
    }
    pub(crate) fn finish(self) -> Vec<Finding> {
        self.findings
    }
    pub(crate) fn optional_text(
        &mut self,
        parent: usize,
        field: &str,
        path: &str,
    ) -> Option<String> {
        self.document
            .field(parent, field)
            .map(|node| self.text(node, path))
    }
    pub(crate) fn integer(&mut self, node: usize, path: &str) -> Option<i64> {
        let previous_findings = self.findings.len();
        match self.scalar(node, path) {
            Value::Int(value) => Some(value),
            _ => {
                if self.findings.len() == previous_findings {
                    self.fault(path, node, Kind::NormalizedShape);
                }
                None
            }
        }
    }

    pub(crate) fn fault(&mut self, path: &str, node: usize, kind: Kind) {
        self.findings.push(Finding {
            path: path.into(),
            node,
            kind,
            value: None,
        });
    }
    /// A scalar the engine cannot run with, written as `value`.
    fn refuse(&mut self, path: &str, node: usize, kind: Kind, value: &str) {
        self.invalid(path, node, kind, Some(value.into()));
    }
    fn invalid(&mut self, path: &str, node: usize, kind: Kind, value: Option<String>) {
        self.findings.push(Finding {
            path: path.into(),
            node,
            kind,
            value,
        });
    }
    /// The text of a node that is text.
    fn written_text(&self, node: usize) -> Option<String> {
        match self.document.nodes().get(node) {
            Some(N::Text(text)) => Some(text.clone()),
            _ => None,
        }
    }
    /// Whether a finding already names `path`.
    pub(crate) fn has_finding_at(&self, path: &str) -> bool {
        self.findings.iter().any(|finding| finding.path == path)
    }
    pub(crate) fn field(&mut self, parent: usize, name: &str, path: &str) -> Option<usize> {
        let field = self.document.field(parent, name);
        if field.is_none() {
            self.fault(path, parent, Kind::NormalizedShape);
        }
        field
    }
    pub(crate) fn text(&mut self, node: usize, path: &str) -> String {
        match self.document.nodes().get(node) {
            Some(N::Text(value)) => value.clone(),
            _ => {
                self.fault(path, node, Kind::NormalizedShape);
                String::new()
            }
        }
    }
    pub(crate) fn text_field(&mut self, parent: usize, field: &str, path: &str) -> String {
        self.field(parent, field, path)
            .map(|node| self.text(node, path))
            .unwrap_or_default()
    }
    pub(crate) fn boolean(&mut self, parent: usize, field: &str, path: &str) -> bool {
        let Some(node) = self.field(parent, field, path) else {
            return false;
        };
        match self.document.nodes().get(node) {
            Some(N::Boolean(value)) => *value,
            _ => {
                self.fault(path, node, Kind::NormalizedShape);
                false
            }
        }
    }
    pub(crate) fn sequence(&mut self, parent: usize, field: &str, path: &str) -> Vec<usize> {
        let Some(node) = self.field(parent, field, path) else {
            return Vec::new();
        };
        match self.document.nodes().get(node) {
            Some(N::Sequence(items)) => items.clone(),
            _ => {
                self.fault(path, node, Kind::NormalizedShape);
                Vec::new()
            }
        }
    }
    /// A 1.0 function-file value, typed by the type its definition declares
    /// (P-FN-1): text is a date or a datetime where that is the declared
    /// type, and no value is a tagged mapping. Any other scalar keeps its
    /// YAML type, which admission checks against the declaration; a value
    /// with no declared type, such as an argument no parameter names, too.
    pub(crate) fn declared(&mut self, node: usize, path: &str, kind: Option<ValueType>) -> Value {
        let document = self.document;
        match (document.nodes().get(node), kind) {
            (Some(N::Text(text)), Some(ValueType::Date)) => match text.parse() {
                Ok(value) => Value::Date(value),
                Err(_) => {
                    self.refuse(path, node, Kind::InvalidDate, text);
                    Value::Missing
                }
            },
            (Some(N::Text(text)), Some(ValueType::DateTime)) => match text.parse() {
                Ok(value) => Value::DateTime(value),
                Err(_) => {
                    self.refuse(path, node, Kind::InvalidDateTime, text);
                    Value::Missing
                }
            },
            (Some(N::Mapping(_)), _) => {
                self.fault(path, node, Kind::NormalizedShape);
                Value::Missing
            }
            _ => self.scalar(node, path),
        }
    }
    pub(crate) fn scalar(&mut self, node: usize, path: &str) -> Value {
        match self.document.nodes().get(node) {
            Some(N::Null) => Value::Missing,
            Some(N::Boolean(value)) => Value::Bool(*value),
            Some(N::Text(value)) => Value::Str(value.clone()),
            Some(N::Integer(value)) => match value.parse::<i64>() {
                Ok(value) => Value::Int(value),
                Err(_) => {
                    self.refuse(path, node, Kind::IntegerRange, value);
                    Value::Missing
                }
            },
            Some(N::Float(value)) => Value::float(*value),
            Some(N::Mapping(_)) => {
                if let Some(value) = self.document.field(node, "date") {
                    let written = self.written_text(value);
                    let text = self.text(value, path);
                    match text.parse() {
                        Ok(value) => Value::Date(value),
                        Err(_) => {
                            self.invalid(path, node, Kind::InvalidDate, written);
                            Value::Missing
                        }
                    }
                } else if let Some(value) = self.document.field(node, "datetime") {
                    let written = self.written_text(value);
                    let text = self.text(value, path);
                    match text.parse() {
                        Ok(value) => Value::DateTime(value),
                        Err(_) => {
                            self.invalid(path, node, Kind::InvalidDateTime, written);
                            Value::Missing
                        }
                    }
                } else {
                    self.fault(path, node, Kind::NormalizedShape);
                    Value::Missing
                }
            }
            _ => {
                self.fault(path, node, Kind::NormalizedShape);
                Value::Missing
            }
        }
    }
}
