//! Decode source declarations directly from the normalized environment document.
//! Source paths remain authored text; captured metadata never becomes read authority.
use crate::{
    project_environment::Submission,
    project_function::{Definition, Language},
    project_function_document::{self, Finding, Kind, Reader},
    project_terminology::Source as TerminologySource,
    project_terminology_document,
    schema::{Document, DocumentNode as N},
};
use alloc::{boxed::Box, format, string::String, vec::Vec};

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
#[derive(Clone, Debug, PartialEq)]
pub struct CodelistDeclaration {
    pub node: usize,
    pub declaration: Declaration<TerminologySource>,
}
/// Section/study node identities belong to the retained normalized input, and
/// their complete closed metadata is consumed by the submission compiler.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentSources {
    pub language: Option<Language>,
    pub lock: Option<String>,
    pub functions: Option<Vec<FunctionDeclaration>>,
    pub codelists: Vec<CodelistDeclaration>,
    pub study: Option<usize>,
    pub submissions: Vec<(Submission, usize)>,
}
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticLimits {
    pub findings: usize,
    pub text_bytes: usize,
}
impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            findings: 65_536,
            text_bytes: 16_777_216,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticResource {
    Findings,
    TextBytes,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Findings(Vec<Finding>),
    Limit(DiagnosticResource),
}
struct Budget(DiagnosticLimits);
impl Budget {
    fn reserve(&mut self, findings: &[Finding], prefix: &str) -> Result<(), Error> {
        let bytes = findings.iter().try_fold(0usize, |total, finding| {
            prefix
                .len()
                .checked_add(usize::from(!prefix.is_empty() && !finding.path.is_empty()))
                .and_then(|n| n.checked_add(finding.path.len()))
                .and_then(|n| n.checked_add(total))
                .ok_or(Error::Limit(DiagnosticResource::TextBytes))
        })?;
        let count = self
            .0
            .findings
            .checked_sub(findings.len())
            .ok_or(Error::Limit(DiagnosticResource::Findings))?;
        let text = self
            .0
            .text_bytes
            .checked_sub(bytes)
            .ok_or(Error::Limit(DiagnosticResource::TextBytes))?;
        self.0.findings = count;
        self.0.text_bytes = text;
        Ok(())
    }
}
fn prefix(findings: Vec<Finding>, path: &str, budget: &mut Budget) -> Result<Vec<Finding>, Error> {
    // Reserve every resulting field path before copying the first source prefix.
    budget.reserve(&findings, path)?;
    Ok(findings
        .into_iter()
        .map(|mut finding| {
            finding.path = if finding.path.is_empty() {
                path.into()
            } else {
                format!("{path}.{}", finding.path)
            };
            finding
        })
        .collect())
}
pub fn decode(document: &Document, root: usize) -> Result<EnvironmentSources, Error> {
    decode_with_limits(document, root, DiagnosticLimits::default())
}
pub fn decode_with_limits(
    document: &Document,
    root: usize,
    limits: DiagnosticLimits,
) -> Result<EnvironmentSources, Error> {
    let mut budget = Budget(limits);
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let language = document.field(root, "language").and_then(|node| {
        match reader.text(node, "language").as_str() {
            "python" => Some(Language::Python),
            "r" => Some(Language::R),
            _ => {
                reader.fault("language", node, Kind::NormalizedShape);
                None
            }
        }
    });
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
                                    leaves.extend(prefix(findings, &path, &mut budget)?);
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
    let mut codelists = Vec::new();
    if document.field(root, "codelists").is_some() {
        for (index, node) in reader
            .sequence(root, "codelists", "codelists")
            .into_iter()
            .enumerate()
        {
            let path = format!("codelists[{index}]");
            let declaration = match document.nodes().get(node) {
                Some(N::Text(written)) => Some(Declaration::Path(written.clone())),
                Some(N::Mapping(_)) => match project_terminology_document::decode(document, node) {
                    Ok(source) => Some(Declaration::Inline(Box::new(source))),
                    Err(findings) => {
                        leaves.extend(prefix(findings, &path, &mut budget)?);
                        None
                    }
                },
                _ => {
                    reader.fault(&path, node, Kind::NormalizedShape);
                    None
                }
            };
            if let Some(declaration) = declaration {
                codelists.push(CodelistDeclaration { node, declaration });
            }
        }
    }
    let study = document.field(root, "study");
    let submissions = [
        ("sdtm", Submission::Sdtm),
        ("adam", Submission::Adam),
        ("send", Submission::Send),
    ]
    .into_iter()
    .filter_map(|(key, kind)| document.field(root, key).map(|node| (kind, node)))
    .collect();
    let mut findings = reader.finish();
    budget.reserve(&findings, "")?;
    findings.extend(leaves);
    if findings.is_empty() {
        Ok(EnvironmentSources {
            language,
            lock,
            functions,
            codelists,
            study,
            submissions,
        })
    } else {
        Err(Error::Findings(findings))
    }
}
