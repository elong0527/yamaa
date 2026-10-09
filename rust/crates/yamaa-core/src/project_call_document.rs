//! Read normalized versionless calls without resolving code or source records.
//! Plain text is an authored variable reference; explicit literal/date/datetime
//! wrappers retain their exact scalar kind and source node.
use crate::{
    project_function::{CallArgument, CallFinding, Function},
    project_function_document::{Finding, Kind, Reader},
    schema::{Document, DocumentNode as N},
    value::{Value, ValueType},
};
use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

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
        if self.name != function.definition().name {
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
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub arguments: usize,
    pub text_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            arguments: 1024,
            text_bytes: 1_048_576,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Limit(&'static str),
    Findings(Vec<Finding>),
}

/// Admit only name/args and the closed literal/date/datetime wrappers.
/// Retain every independent invalid scalar leaf; no partial
/// call is admitted. Preflight bounds ownership before the first string copy.
pub fn decode(document: &Document, root: usize, path: &str, limits: Limits) -> Result<Call, Error> {
    text_cost(document, root, path, limits)?;
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
                        if !matches!(document.nodes().get(value), Some(N::Text(_))) {
                            reader.fault(&argument_path, value, Kind::NormalizedShape);
                            Input::Literal(Value::Missing)
                        } else {
                            Input::Literal(reader.scalar(value, &argument_path))
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
        Err(Error::Findings(findings))
    }
}

/// Conservative bytes for ownership and complete diagnostics. A surrounding
/// compiler can charge all calls before decoding the first call.
pub fn text_cost(
    document: &Document,
    root: usize,
    path: &str,
    limits: Limits,
) -> Result<usize, Error> {
    let argument_nodes =
        document
            .field(root, "args")
            .and_then(|node| match document.nodes().get(node) {
                Some(N::Mapping(items)) => Some(items.as_slice()),
                _ => None,
            });
    if argument_nodes.is_some_and(|items| items.len() > limits.arguments) {
        return Err(Error::Limit("arguments"));
    }
    // Charge only this call's fixed-depth leaves. Repeated calls must not scan
    // or charge unrelated document nodes. Include conservative space for each
    // temporary diagnostic path and every possible retained Reader finding.
    let mut text = 0;
    charge_text(&mut text, path.len(), limits)?;
    charge_paths(&mut text, path.len(), 5, 4, limits)?;
    if let Some(node) = document.field(root, "name") {
        charge_leaf(document, node, &mut text, limits)?;
    }
    let name_length = document
        .field(root, "name")
        .and_then(|node| match document.nodes().get(node) {
            Some(N::Text(name)) => Some(name.len()),
            _ => None,
        })
        .unwrap_or(0);
    // The compiler's issue context repeats the function name for each scalar
    // fault. Charge that projection before returning any decode findings.
    let copies = argument_nodes
        .map_or(0, <[_]>::len)
        .checked_mul(2)
        .and_then(|n| n.checked_add(1))
        .ok_or(Error::Limit("text"))?;
    charge_text(
        &mut text,
        name_length
            .checked_mul(copies)
            .ok_or(Error::Limit("text"))?,
        limits,
    )?;
    charge_paths(&mut text, path.len(), 5, 2, limits)?;
    if let Some(items) = argument_nodes {
        for &(name_node, node) in items {
            let name_length = match document.nodes().get(name_node) {
                Some(N::Text(name)) => name.len(),
                _ => 0,
            };
            charge_leaf(document, name_node, &mut text, limits)?;
            charge_paths(&mut text, path.len(), 5, 2, limits)?;
            let suffix = name_length.checked_add(6).ok_or(Error::Limit("text"))?;
            charge_paths(&mut text, path.len(), suffix, 3, limits)?;
            let scalar = document.field(node, "literal").unwrap_or(node);
            charge_leaf(document, scalar, &mut text, limits)?;
            if let Some(value) = document
                .field(scalar, "date")
                .or_else(|| document.field(scalar, "datetime"))
            {
                charge_leaf(document, value, &mut text, limits)?;
            }
        }
    }
    Ok(text)
}

fn charge_text(used: &mut usize, amount: usize, limits: Limits) -> Result<(), Error> {
    *used = used
        .checked_add(amount)
        .filter(|&n| n <= limits.text_bytes)
        .ok_or(Error::Limit("text"))?;
    Ok(())
}
fn charge_paths(
    used: &mut usize,
    prefix: usize,
    suffix: usize,
    copies: usize,
    limits: Limits,
) -> Result<(), Error> {
    let amount = prefix
        .checked_add(suffix)
        .and_then(|n| n.checked_mul(copies))
        .ok_or(Error::Limit("text"))?;
    charge_text(used, amount, limits)
}
fn charge_leaf(
    document: &Document,
    node: usize,
    used: &mut usize,
    limits: Limits,
) -> Result<(), Error> {
    if let Some(N::Text(value) | N::Integer(value)) = document.nodes().get(node) {
        charge_text(used, value.len(), limits)?;
    }
    Ok(())
}
