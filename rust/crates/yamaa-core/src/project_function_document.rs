//! Decode a structurally normalized definition directly from the shared document.
//! No host model, JSON plan, code discovery or default synthesis participates.
use crate::{
    project_function::{Case, Definition, Parameter},
    schema::{Document, DocumentNode as N},
    value::{ColumnType, Value, ValueType},
};
use alloc::{format, string::String, vec::Vec};

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
        });
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
    pub(crate) fn scalar(&mut self, node: usize, path: &str) -> Value {
        match self.document.nodes().get(node) {
            Some(N::Null) => Value::Missing,
            Some(N::Boolean(value)) => Value::Bool(*value),
            Some(N::Text(value)) => Value::Str(value.clone()),
            Some(N::Integer(value)) => match value.parse::<i64>() {
                Ok(value) => Value::Int(value),
                Err(_) => {
                    self.fault(path, node, Kind::IntegerRange);
                    Value::Missing
                }
            },
            Some(N::Float(value)) => Value::float(*value),
            Some(N::Mapping(_)) => {
                if let Some(value) = self.document.field(node, "date") {
                    let text = self.text(value, path);
                    match text.parse() {
                        Ok(value) => Value::Date(value),
                        Err(_) => {
                            self.fault(path, node, Kind::InvalidDate);
                            Value::Missing
                        }
                    }
                } else if let Some(value) = self.document.field(node, "datetime") {
                    let text = self.text(value, path);
                    match text.parse() {
                        Ok(value) => Value::DateTime(value),
                        Err(_) => {
                            self.fault(path, node, Kind::InvalidDateTime);
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
    fn parameter_kind(&mut self, parent: usize, path: &str) -> ValueType {
        let text = self.text_field(parent, "type", path);
        match text.as_str() {
            "str" => ValueType::Str,
            "int" => ValueType::Int,
            "float" => ValueType::Float,
            "bool" => ValueType::Bool,
            "date" => ValueType::Date,
            "datetime" => ValueType::DateTime,
            _ => {
                self.fault(path, parent, Kind::NormalizedShape);
                ValueType::Str
            }
        }
    }
    fn column_kind(&mut self, parent: usize, path: &str) -> ColumnType {
        let text = self.text_field(parent, "returns", path);
        match text.as_str() {
            "str" => ColumnType::Str,
            "int" => ColumnType::Int,
            "float" => ColumnType::Float,
            "date" => ColumnType::Date,
            "datetime" => ColumnType::DateTime,
            _ => {
                self.fault(path, parent, Kind::NormalizedShape);
                ColumnType::Str
            }
        }
    }
    fn parameters(&mut self, root: usize) -> Vec<Parameter> {
        self.sequence(root, "params", "params")
            .iter()
            .enumerate()
            .map(|(i, &node)| {
                let path = format!("params[{i}]");
                let name = self.text_field(node, "name", &format!("{path}.name"));
                let kind = self.parameter_kind(node, &format!("{path}.type"));
                let required = self.boolean(node, "required", &format!("{path}.required"));
                let default = self
                    .document
                    .field(node, "default")
                    .map(|value| self.scalar(value, &format!("{path}.default")));
                let accepts_missing =
                    self.boolean(node, "accepts_missing", &format!("{path}.accepts_missing"));
                Parameter {
                    name,
                    kind,
                    required,
                    default,
                    accepts_missing,
                }
            })
            .collect()
    }
    fn cases(&mut self, root: usize) -> Vec<Case> {
        self.sequence(root, "tests", "tests")
            .iter()
            .enumerate()
            .map(|(i, &node)| {
                let path = format!("tests[{i}]");
                let id = self.text_field(node, "id", &format!("{path}.id"));
                let covers = self
                    .sequence(node, "covers", &format!("{path}.covers"))
                    .iter()
                    .enumerate()
                    .map(|(i, &node)| self.text(node, &format!("{path}.covers[{i}]")))
                    .collect();
                let mut args = Vec::new();
                if let Some(args_node) = self.field(node, "args", &format!("{path}.args")) {
                    if let Some(N::Mapping(items)) = self.document.nodes().get(args_node) {
                        let items = items.clone();
                        for (key, value) in items {
                            let name = self.text(key, &format!("{path}.args"));
                            let value = self.scalar(value, &format!("{path}.args.{name}"));
                            args.push((name, value));
                        }
                    } else {
                        self.fault(&format!("{path}.args"), args_node, Kind::NormalizedShape);
                    }
                }
                let result = self
                    .field(node, "result", &format!("{path}.result"))
                    .map(|node| self.scalar(node, &format!("{path}.result")))
                    .unwrap_or(Value::Missing);
                Case {
                    id,
                    covers,
                    args,
                    result,
                }
            })
            .collect()
    }
}

/// Decode all scalar leaves before returning any definition. Failed values retain
/// exact node references; placeholder fields are never exposed as an accepted model.
pub fn decode(document: &Document, root: usize, name: &str) -> Result<Definition, Vec<Finding>> {
    let mut reader = Reader {
        document,
        findings: Vec::new(),
    };
    let function = reader.text_field(root, "function", "function");
    let description = reader.text_field(root, "description", "description");
    let params = reader.parameters(root);
    let returns = reader.column_kind(root, "returns");
    let may_return_missing = reader.boolean(root, "may_return_missing", "may_return_missing");
    let comparison_decimals = reader
        .field(root, "comparison_decimals", "comparison_decimals")
        .map(|node| match reader.scalar(node, "comparison_decimals") {
            Value::Int(value) => value,
            _ => {
                if !reader
                    .findings
                    .iter()
                    .any(|f| f.path == "comparison_decimals")
                {
                    reader.fault("comparison_decimals", node, Kind::NormalizedShape);
                }
                0
            }
        })
        .unwrap_or_default();
    let tests = reader.cases(root);
    if reader.findings.is_empty() {
        Ok(Definition {
            name: name.into(),
            function,
            description,
            params,
            returns,
            may_return_missing,
            comparison_decimals,
            tests,
        })
    } else {
        Err(reader.findings)
    }
}
