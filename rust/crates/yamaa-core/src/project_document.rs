//! Decode the project file's own declarations from its normalized document.
//! Environment and codelist paths remain authored text; the native capture port
//! resolves them from the project file. Study and section metadata stay in the
//! retained document for submission composition.
use crate::{
    project_environment_document::{prefix, Declaration},
    project_function::Language,
    project_function_document::{Finding, Kind, Reader},
    project_terminology::Source as TerminologySource,
    project_terminology_document,
    schema::{Document, DocumentNode as N},
};

#[derive(Clone, Debug, PartialEq)]
pub struct CodelistDeclaration {
    pub node: usize,
    pub declaration: Declaration<TerminologySource>,
}
/// The environment file the project names for each host language, as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environments {
    pub python: Option<String>,
    pub r: Option<String>,
}
impl Environments {
    /// Each host reads only the entry its own key names.
    pub fn get(&self, host: Language) -> Option<&str> {
        match host {
            Language::Python => self.python.as_deref(),
            Language::R => self.r.as_deref(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSources {
    pub environments: Environments,
    pub codelists: Vec<CodelistDeclaration>,
}
pub fn decode(document: &Document, root: usize) -> Result<ProjectSources, Vec<Finding>> {
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let mut environments = Environments::default();
    if let Some(node) = document.field(root, "environments") {
        environments.python = reader.optional_text(node, "python", "environments.python");
        environments.r = reader.optional_text(node, "r", "environments.r");
    }
    let mut leaves = Vec::new();
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
                        leaves.extend(prefix(findings, &path));
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
    let mut findings = reader.finish();
    findings.extend(leaves);
    if findings.is_empty() {
        Ok(ProjectSources {
            environments,
            codelists,
        })
    } else {
        Err(findings)
    }
}
