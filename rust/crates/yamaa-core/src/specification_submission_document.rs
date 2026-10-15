//! Closed closed specification submission metadata. No IO or execution grant.
use crate::schema::{Document, DocumentNode as N};

mod decode;
mod model;
pub use decode::decode;
pub use model::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Shape {
        node: usize,
    },
    Resource(crate::limits::LimitExceeded),
    /// Complete admitted structural findings, never a fitting prefix.
    Findings(Vec<Finding>),
}
#[derive(Debug, thiserror::Error)]
#[error("specification submission document: cause={cause:?}")]
pub struct Error<'a> {
    pub input: &'a Document,
    pub cause: Cause,
}

/// Borrowed occurrence facts only. Null or malformed values are not accepted.
#[derive(Debug)]
pub struct Locations<'a> {
    input: &'a Document,
    dataset: Option<Origin>,
    columns: Vec<ColumnLocation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnLocation {
    pub node: usize,
    pub name: Origin,
    pub column_type: Origin,
    pub label: Origin,
    pub submission: Origin,
}
impl<'a> Locations<'a> {
    pub fn input(&self) -> &'a Document {
        self.input
    }
    pub fn dataset(&self) -> Option<Origin> {
        self.dataset
    }
    pub fn columns(&self) -> &[ColumnLocation] {
        &self.columns
    }
}

/// Admit every root/column field and key comparison before any location buffer
/// grows. The declaration scan grants no structural or semantic acceptance.
pub fn locate(input: &Document, root: usize) -> Result<Locations<'_>, Error<'_>> {
    let refuse = |cause| Error { input, cause };
    let fields = match input.nodes().get(root) {
        Some(N::Mapping(fields)) => fields,
        _ => return Err(refuse(Cause::Shape { node: root })),
    };
    let scan = |fields: &[(usize, usize)]| -> Result<(), Cause> {
        for &(key, _) in fields {
            if !matches!(input.nodes()[key], N::Text(_)) {
                return Err(Cause::Shape { node: key });
            }
        }
        Ok(())
    };
    scan(fields).map_err(refuse)?;
    let dataset = occurrence(input, root, FieldName::Submission);
    let columns_origin = occurrence(input, root, FieldName::Columns);
    let columns = match columns_origin.node {
        None => &[][..],
        Some(node) => match &input.nodes()[node] {
            N::Sequence(items) => items.as_slice(),
            _ => return Err(refuse(Cause::Shape { node })),
        },
    };
    crate::limits::Limits::columns(columns.len())
        .map_err(|error| refuse(Cause::Resource(error)))?;
    for &node in columns {
        let N::Mapping(fields) = &input.nodes()[node] else {
            return Err(refuse(Cause::Shape { node }));
        };
        scan(fields).map_err(refuse)?;
    }
    let mut owned = Vec::with_capacity(columns.len());
    for &node in columns {
        owned.push(ColumnLocation {
            node,
            name: occurrence(input, node, FieldName::Name),
            column_type: occurrence(input, node, FieldName::Type),
            label: occurrence(input, node, FieldName::Label),
            submission: occurrence(input, node, FieldName::Submission),
        });
    }
    Ok(Locations {
        input,
        dataset: dataset.node.map(|_| dataset),
        columns: owned,
    })
}

fn occurrence(input: &Document, container: usize, field: FieldName) -> Origin {
    let found = match input.nodes().get(container) {
        Some(N::Mapping(fields)) => fields.iter().find(
            |&&(key, _)| matches!(&input.nodes()[key], N::Text(text) if text == field.name()),
        ),
        _ => None,
    };
    Origin {
        container,
        field,
        key: found.map(|v| v.0),
        node: found.map(|v| v.1),
    }
}
