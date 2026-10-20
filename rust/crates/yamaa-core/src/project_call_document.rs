//! Read normalized versionless calls without resolving code or source records.
//! Plain text is an authored variable reference; explicit literal/date/datetime
//! wrappers retain their exact scalar kind and source node.
use crate::{
    project_function::{CallArgument, CallFinding, Function},
    project_function_document::{Finding, Kind, Reader},
    schema::{Document, DocumentNode as N},
    value::{Value, ValueType},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Literal(Value),
    Reference(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub name: String,
    pub node: usize,
    pub input: Input,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub name: String,
    pub name_node: usize,
    pub node: usize,
    pub arguments: Vec<Argument>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingFinding {
    FunctionNameMismatch,
    UnknownReference { argument: usize },
    Argument(CallFinding),
}
impl Call {
    /// Type metadata comes from the compiler's declared catalogue, never cells.
    /// Scope/dependency binding is a later compiler step over these references.
    pub fn validate(
        &self,
        function: &Function,
        references: &BTreeMap<String, ValueType>,
    ) -> Vec<BindingFinding> {
        let mut findings = Vec::new();
        if self.name != function.name() {
            findings.push(BindingFinding::FunctionNameMismatch);
            return findings;
        }
        let arguments = self
            .arguments
            .iter()
            .enumerate()
            .map(|(argument, item)| {
                let kind = match &item.input {
                    Input::Literal(value) => value.value_type(),
                    Input::Reference(name) => match references.get(name) {
                        Some(kind) => Some(*kind),
                        None => {
                            findings.push(BindingFinding::UnknownReference { argument });
                            None
                        }
                    },
                };
                CallArgument {
                    name: item.name.clone(),
                    kind,
                }
            })
            .collect::<Vec<_>>();
        findings.extend(
            function
                .bind_call(&arguments)
                .into_iter()
                .map(BindingFinding::Argument),
        );
        findings
    }
}
/// Admit only name/args and the closed literal/date/datetime wrappers.
/// Retain every independent invalid scalar leaf; no partial
/// call is admitted.
pub fn decode(document: &Document, root: usize, path: &str) -> Result<Call, Vec<Finding>> {
    let argument_nodes =
        document
            .field(root, "args")
            .and_then(|node| match document.nodes().get(node) {
                Some(N::Mapping(items)) => Some(items.as_slice()),
                _ => None,
            });
    let mut reader = Reader::new(document);
    let closed = matches!(document.nodes().get(root), Some(N::Mapping(items))
        if items.len() == 2 && items.iter().all(|&(key, _)|
            matches!(document.nodes().get(key), Some(N::Text(name)) if matches!(name.as_str(), "name" | "args"))));
    if !closed {
        reader.fault(path, root, Kind::NormalizedShape);
    }
    let name_node = reader
        .field(root, "name", &format!("{path}.name"))
        .unwrap_or(root);
    let name = reader.text(name_node, &format!("{path}.name"));
    let mut arguments = Vec::new();
    match argument_nodes {
        None => reader.fault(&format!("{path}.args"), root, Kind::NormalizedShape),
        Some(items) => {
            for &(name_node, node) in items {
                let name = reader.text(name_node, &format!("{path}.args"));
                let argument_path = format!("{path}.args.{name}");
                let input = match document.nodes().get(node) {
                    Some(N::Text(value)) => Input::Reference(value.clone()),
                    Some(N::Mapping(items)) if items.len() != 1 => {
                        reader.fault(&argument_path, node, Kind::NormalizedShape);
                        Input::Literal(Value::Missing)
                    }
                    Some(N::Mapping(_)) if document.field(node, "literal").is_some() => {
                        let value = document
                            .field(node, "literal")
                            .expect("checked literal wrapper");
                        match document.nodes().get(value) {
                            // `{literal: null}` passes the missing value explicitly.
                            Some(N::Null) => Input::Literal(Value::Missing),
                            Some(N::Text(_)) => {
                                Input::Literal(reader.scalar(value, &argument_path))
                            }
                            _ => {
                                reader.fault(&argument_path, value, Kind::NormalizedShape);
                                Input::Literal(Value::Missing)
                            }
                        }
                    }
                    _ => Input::Literal(reader.scalar(node, &argument_path)),
                };
                arguments.push(Argument { name, node, input });
            }
        }
    }
    let findings = reader.finish();
    if findings.is_empty() {
        Ok(Call {
            name,
            name_node,
            node: root,
            arguments,
        })
    } else {
        Err(findings)
    }
}
