//! Study dataset-name vocabulary from original normalized metadata only.
use crate::ast;
use crate::schema::{Document, DocumentNode as N};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Inventory {
    names: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    #[error("submission inventory shape")]
    Shape,
}
impl Inventory {
    /// The caller's capture supplies admitted specification documents; no data table or
    /// filename can stand in for the dataset's declared domain identity.
    pub fn from_specifications(documents: &[&Document]) -> Result<Self, MetadataError> {
        let mut selected = BTreeSet::new();
        for document in documents {
            let id = document
                .field(document.root(), "domain")
                .ok_or(MetadataError::Shape)?;
            let N::Text(name) = &document.nodes()[id] else {
                return Err(MetadataError::Shape);
            };
            selected.insert(name.as_str());
        }
        Ok(Self {
            names: selected.into_iter().map(String::from).collect(),
        })
    }
    pub fn names(&self) -> &[String] {
        &self.names
    }
}
/// Only governed column/value declarations can request this metadata closure.
pub fn requested(document: &Document) -> bool {
    let enabled = |node| {
        document
            .field(node, "inventory_vocabulary")
            .is_some_and(|id| document.nodes()[id] == N::Boolean(true))
    };
    let columns = document
        .field(document.root(), "columns")
        .and_then(|id| match &document.nodes()[id] {
            N::Sequence(columns) => Some(columns.as_slice()),
            _ => None,
        })
        .unwrap_or(&[]);
    columns.iter().any(|&column| {
        document.field(column, "submission").is_some_and(enabled)
            || crate::submission_values::entries(document, column)
                .iter()
                .any(|entry| enabled(entry.node))
    })
}
/// Select only the specifications each section lists, in written order.
/// Full Define/family admission is deliberately left with its existing
/// composition use case (SUB-23).
pub fn references(project: &ast::Project) -> Result<Vec<&str>, MetadataError> {
    let declarations = project
        .submission
        .as_ref()
        .map_err(|_| MetadataError::Shape)?;
    Ok(declarations
        .sections
        .iter()
        .flat_map(|section| section.specs.iter().map(|spec| spec.text.as_str()))
        .collect())
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("submission inventory port: written={written:?}, error={error:?}")]
    Port { written: String, error: E },
    #[error("submission inventory metadata: {0:?}")]
    Metadata(MetadataError),
}
pub struct Captured<S> {
    pub inventory: Inventory,
    pub specifications: Vec<S>,
}
/// Capture each specification the project's sections list, in written
/// order, with `capture`, and read the study's dataset names from the
/// document `document` shows of each.
pub fn capture<S, E>(
    project: &ast::Project,
    mut capture: impl FnMut(&str) -> Result<S, E>,
    document: impl Fn(&S) -> &Document,
) -> Result<Captured<S>, Error<E>> {
    let references = references(project).map_err(Error::Metadata)?;
    let mut specifications = Vec::new();
    for written in references {
        let specification = capture(written).map_err(|error| Error::Port {
            written: written.into(),
            error,
        })?;
        specifications.push(specification);
    }
    let documents = specifications.iter().map(document).collect::<Vec<_>>();
    let inventory = Inventory::from_specifications(&documents).map_err(Error::Metadata)?;
    Ok(Captured {
        inventory,
        specifications,
    })
}
