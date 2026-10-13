//! Study dataset-name vocabulary from original normalized metadata only.
use crate::{
    project_limits::{Budget, Limit, Limits},
    schema::{Document, DocumentNode as N},
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Inventory {
    names: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    Limit(Limit),
}
impl From<Limit> for Error {
    fn from(value: Limit) -> Self {
        Self::Limit(value)
    }
}
impl Inventory {
    /// Capture ports supply admitted specification documents; no data table or
    /// filename can stand in for the dataset's declared domain identity.
    pub fn from_specifications(documents: &[&Document], limits: Limits) -> Result<Self, Error> {
        let mut budget = Budget::new(limits);
        let mut selected = BTreeSet::new();
        for document in documents {
            budget.work(document.nodes().len())?;
            let id = document
                .field(document.root(), "domain")
                .ok_or(Error::Shape)?;
            let N::Text(name) = &document.nodes()[id] else {
                return Err(Error::Shape);
            };
            budget.text(name)?;
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
    if columns
        .iter()
        .any(|&column| document.field(column, "submission").is_some_and(enabled))
    {
        return true;
    }
    let rows = document
        .field(document.root(), "rows")
        .and_then(|id| match &document.nodes()[id] {
            N::Sequence(rows) => Some(rows.as_slice()),
            _ => None,
        })
        .unwrap_or(&[]);
    rows.iter().any(|&row| document.field(row, "submission").is_some_and(|id| {
        matches!(&document.nodes()[id], N::Mapping(entries) if entries.iter().any(|&(_, node)| enabled(node)))
    }))
}
/// Select only authored section specifications. Full Define/family admission is
/// deliberately left with its existing composition use case (REQ-0908).
pub fn references(document: &Document, limits: Limits) -> Result<Vec<&str>, Error> {
    let locations =
        crate::submission_document::locate(document, document.root(), Default::default())
            .map_err(|_| Error::Shape)?;
    let mut budget = Budget::new(limits);
    let mut references = Vec::new();
    for &(_, section) in locations.sections() {
        let specs = document.field(section, "specs").ok_or(Error::Shape)?;
        let N::Sequence(specs) = &document.nodes()[specs] else {
            return Err(Error::Shape);
        };
        for &spec in specs {
            let N::Text(written) = &document.nodes()[spec] else {
                return Err(Error::Shape);
            };
            budget.text(written)?;
            references.push(written.as_str());
        }
    }
    Ok(references)
}
