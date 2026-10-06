//! Closed decoded-tree transport. YAML scalar recognition remains outside this boundary.

use super::TransportError;
use serde::{Deserialize, Serialize};
use yamaa_core::schema::{Document, DocumentError, DocumentLimits, DocumentNode as N};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Node {
    Null {},
    Boolean { value: bool },
    Integer { value: String },
    Float { bits: String },
    Text { value: String },
    Sequence { items: Vec<usize> },
    Mapping { entries: Vec<(usize, usize)> },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tree {
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Tree {
    pub fn admit(self) -> Result<Result<Document, DocumentError>, TransportError> {
        Ok(Document::new(
            self.nodes
                .into_iter()
                .map(|node| {
                    Ok(match node {
                        Node::Null {} => N::Null,
                        Node::Boolean { value } => N::Boolean(value),
                        Node::Integer { value } => N::Integer(value),
                        Node::Float { bits } => {
                            if bits.len() != 16
                                || !bits
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                            {
                                return Err(TransportError::InvalidDocument);
                            }
                            let value = u64::from_str_radix(&bits, 16)
                                .map_err(|_| TransportError::InvalidDocument)?;
                            N::Float(f64::from_bits(value))
                        }
                        Node::Text { value } => N::Text(value),
                        Node::Sequence { items } => N::Sequence(items),
                        Node::Mapping { entries } => N::Mapping(entries),
                    })
                })
                .collect::<Result<Vec<_>, TransportError>>()?,
            self.root,
            DocumentLimits::default(),
        ))
    }
    pub fn from_core(document: &Document) -> Self {
        Self {
            root: document.root(),
            nodes: document
                .nodes()
                .iter()
                .map(|node| match node {
                    N::Null => Node::Null {},
                    N::Boolean(value) => Node::Boolean { value: *value },
                    N::Integer(value) => Node::Integer {
                        value: value.clone(),
                    },
                    N::Float(value) => Node::Float {
                        bits: format!("{:016x}", value.to_bits()),
                    },
                    N::Text(value) => Node::Text {
                        value: value.clone(),
                    },
                    N::Sequence(items) => Node::Sequence {
                        items: items.clone(),
                    },
                    N::Mapping(entries) => Node::Mapping {
                        entries: entries.clone(),
                    },
                })
                .collect(),
        }
    }
}
