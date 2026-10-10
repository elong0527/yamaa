//! Bounded closed specification submission metadata. No IO or execution grant.
use crate::schema::{Document, DocumentNode as N};
use alloc::vec::Vec;

mod decode;
mod model;
pub use decode::decode;
pub use model::*;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub fields: usize,
    pub columns: usize,
    pub declarations: usize,
    pub work: usize,
    pub nodes: usize,
    pub depth: usize,
    pub list_items: usize,
    pub text_bytes: usize,
    pub escaped_bytes: usize,
    pub path_segments: usize,
    pub findings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fields: 65_536,
            columns: 16_384,
            declarations: 16_385,
            work: 67_108_864,
            nodes: 131_072,
            depth: 64,
            list_items: 131_072,
            text_bytes: 8_388_608,
            escaped_bytes: 83_886_080,
            path_segments: 655_360,
            findings: 65_536,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Fields,
    Columns,
    Declarations,
    Work,
    Nodes,
    Depth,
    ListItems,
    TextBytes,
    EscapedBytes,
    PathSegments,
    Findings,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Shape {
        node: usize,
    },
    Limit(Resource),
    /// Complete admitted structural findings, never a fitting prefix.
    Findings(Vec<Finding>),
}
#[derive(Debug)]
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
pub fn locate(
    input: &Document,
    root: usize,
    mut limits: Limits,
) -> Result<Locations<'_>, Error<'_>> {
    let refuse = |cause| Error { input, cause };
    let fields = match input.nodes().get(root) {
        Some(N::Mapping(fields)) => fields,
        _ => return Err(refuse(Cause::Shape { node: root })),
    };
    let scan = |limits: &mut Limits, fields: &[(usize, usize)]| -> Result<(), Cause> {
        limits.fields = limits
            .fields
            .checked_sub(fields.len())
            .ok_or(Cause::Limit(Resource::Fields))?;
        limits.work = fields
            .len()
            .checked_mul(16)
            .and_then(|n| limits.work.checked_sub(n))
            .ok_or(Cause::Limit(Resource::Work))?;
        for &(key, _) in fields {
            let N::Text(text) = &input.nodes()[key] else {
                return Err(Cause::Shape { node: key });
            };
            limits.work = text
                .len()
                .checked_mul(8)
                .and_then(|n| limits.work.checked_sub(n))
                .ok_or(Cause::Limit(Resource::Work))?;
        }
        Ok(())
    };
    scan(&mut limits, fields).map_err(refuse)?;
    let dataset = occurrence(input, root, FieldName::Submission);
    let columns_origin = occurrence(input, root, FieldName::Columns);
    let columns = match columns_origin.node {
        None => &[][..],
        Some(node) => match &input.nodes()[node] {
            N::Sequence(items) => items.as_slice(),
            _ => return Err(refuse(Cause::Shape { node })),
        },
    };
    limits.columns = limits
        .columns
        .checked_sub(columns.len())
        .ok_or_else(|| refuse(Cause::Limit(Resource::Columns)))?;
    limits.work = columns
        .len()
        .checked_mul(16)
        .and_then(|n| limits.work.checked_sub(n))
        .ok_or_else(|| refuse(Cause::Limit(Resource::Work)))?;
    for &node in columns {
        let N::Mapping(fields) = &input.nodes()[node] else {
            return Err(refuse(Cause::Shape { node }));
        };
        scan(&mut limits, fields).map_err(refuse)?;
    }
    // Count the whole declaration set before owning even location facts.
    let mut count = usize::from(dataset.node.is_some());
    for &node in columns {
        count = count
            .checked_add(usize::from(
                occurrence(input, node, FieldName::Submission)
                    .node
                    .is_some(),
            ))
            .ok_or_else(|| refuse(Cause::Limit(Resource::Declarations)))?;
    }
    limits
        .declarations
        .checked_sub(count)
        .ok_or_else(|| refuse(Cause::Limit(Resource::Declarations)))?;
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
