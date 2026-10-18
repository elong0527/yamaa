//! Decode the installed-code declarations of one linked environment file.
//! Source paths remain authored text; captured metadata never becomes read authority.
use crate::{
    project_function::Definition,
    project_function_document::{self, Finding, Kind, Reader},
    schema::{Document, DocumentNode as N},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Declaration<T> {
    Path(String),
    Inline(Box<T>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDeclaration {
    pub name: String,
    pub node: usize,
    pub declaration: Declaration<Definition>,
}
/// The environment file holds only installed code. The project key that links
/// it states its language; lock and function paths resolve from this file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnvironmentSources {
    pub lock: Option<String>,
    pub functions: Option<Vec<FunctionDeclaration>>,
}
pub(crate) fn prefix(findings: Vec<Finding>, path: &str) -> Vec<Finding> {
    findings
        .into_iter()
        .map(|mut finding| {
            finding.path = if finding.path.is_empty() {
                path.into()
            } else {
                format!("{path}.{}", finding.path)
            };
            finding
        })
        .collect()
}
pub fn decode(document: &Document, root: usize) -> Result<EnvironmentSources, Vec<Finding>> {
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let lock = reader.optional_text(root, "lock", "lock");
    let mut leaves = Vec::new();
    let functions = if let Some(node) = document.field(root, "functions") {
        let mut functions = Vec::new();
        match document.nodes().get(node) {
            Some(N::Mapping(entries)) => {
                for &(key, value) in entries {
                    let name = reader.text(key, "functions");
                    let path = format!("functions.{name}");
                    let declaration = match document.nodes().get(value) {
                        Some(N::Text(written)) => Some(Declaration::Path(written.clone())),
                        Some(N::Mapping(_)) => {
                            match project_function_document::decode(document, value, &name) {
                                Ok(definition) => Some(Declaration::Inline(Box::new(definition))),
                                Err(findings) => {
                                    leaves.extend(prefix(findings, &path));
                                    None
                                }
                            }
                        }
                        _ => {
                            reader.fault(&path, value, Kind::NormalizedShape);
                            None
                        }
                    };
                    if let Some(declaration) = declaration {
                        functions.push(FunctionDeclaration {
                            name,
                            node: value,
                            declaration,
                        });
                    }
                }
            }
            _ => reader.fault("functions", node, Kind::NormalizedShape),
        }
        Some(functions)
    } else {
        None
    };
    let mut findings = reader.finish();
    findings.extend(leaves);
    if findings.is_empty() {
        Ok(EnvironmentSources { lock, functions })
    } else {
        Err(findings)
    }
}
