//! Study dataset-name vocabulary from original normalized metadata only.
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
    /// Capture ports supply admitted specification documents; no data table or
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
/// Select only authored section specifications. Full Define/family admission is
/// deliberately left with its existing composition use case (SUB-23).
pub fn references(document: &Document) -> Result<Vec<&str>, MetadataError> {
    let locations = crate::submission_document::locate(document, document.root())
        .map_err(|_| MetadataError::Shape)?;
    let mut references = Vec::new();
    for &(_, section) in locations.sections() {
        let specs = document
            .field(section, "specs")
            .ok_or(MetadataError::Shape)?;
        let N::Sequence(specs) = &document.nodes()[specs] else {
            return Err(MetadataError::Shape);
        };
        for &spec in specs {
            let N::Text(written) = &document.nodes()[spec] else {
                return Err(MetadataError::Shape);
            };
            references.push(written.as_str());
        }
    }
    Ok(references)
}

pub trait Ports {
    type Specification;
    type Error;
    fn capture(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Specification, Self::Error>;
    fn document(specification: &Self::Specification) -> &Document;
    fn identity(specification: &Self::Specification) -> &str;
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
pub fn capture<P: Ports>(
    environment: &Document,
    declaring: &str,
    port: &mut P,
) -> Result<Captured<P::Specification>, Error<P::Error>> {
    let references = references(environment).map_err(Error::Metadata)?;
    let mut specifications = Vec::new();
    for written in references {
        let specification = port
            .capture(declaring, written)
            .map_err(|error| Error::Port {
                written: written.into(),
                error,
            })?;
        specifications.push(specification);
    }
    let documents = specifications.iter().map(P::document).collect::<Vec<_>>();
    let inventory = Inventory::from_specifications(&documents).map_err(Error::Metadata)?;
    Ok(Captured {
        inventory,
        specifications,
    })
}
