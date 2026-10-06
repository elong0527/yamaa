//! Owned decoded documents retain mapping order and arbitrary-width integers.
//! Parsing YAML, including duplicate keys and non-finite normalization, is an adapter concern.

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use num_bigint::BigInt;

/// One decoded occurrence. Children precede parents and have exactly one owner.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentNode {
    Null,
    Boolean(bool),
    /// Canonical decimal spelling; no machine-width truncation at the schema boundary.
    Integer(String),
    /// Finite binary64; adapters normalize non-finite source values to null.
    Float(f64),
    Text(String),
    Sequence(Vec<usize>),
    Mapping(Vec<(usize, usize)>),
}

impl DocumentNode {
    /// Preserve the public schema diagnostic vocabulary, including bool versus int.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean(_) => "bool",
            Self::Integer(_) => "int",
            Self::Float(_) => "float",
            Self::Text(_) => "str",
            Self::Sequence(_) => "sequence",
            Self::Mapping(_) => "mapping",
        }
    }

    /// Unicode strings count scalar values; container lengths count their members.
    pub fn length(&self) -> Option<usize> {
        match self {
            Self::Text(text) => Some(text.chars().count()),
            Self::Sequence(items) => Some(items.len()),
            Self::Mapping(items) => Some(items.len()),
            _ => None,
        }
    }
}

/// Decoded storage admission is separate from schema validation and runtime values.
#[derive(Clone, Copy, Debug)]
pub struct DocumentLimits {
    pub nodes: usize,
    pub text_bytes: usize,
    pub edges: usize,
    pub depth: usize,
}

impl Default for DocumentLimits {
    /// Bound storage and stack depth without restricting integers to a runtime scalar width.
    fn default() -> Self {
        Self {
            nodes: 131_072,
            text_bytes: 8_388_608,
            edges: 262_144,
            depth: 64,
        }
    }
}

/// Resource exhaustion never masquerades as a specification diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentResource {
    Nodes,
    TextBytes,
    Edges,
    Depth,
}

/// Invalid caller-authored arenas are transport defects, not authored schema errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentError {
    InvalidRoot,
    InvalidChild,
    SharedChild,
    UnreachableNode,
    InvalidInteger,
    NonFinite,
    NonScalarKey,
    DuplicateKey,
    Limit {
        resource: DocumentResource,
        limit: usize,
    },
}

/// Immutable tree with stable occurrence indices for recursive-alias guards.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    nodes: Vec<DocumentNode>,
    root: usize,
}

/// Mapping-key equality follows decoded scalar equality, without content hashing.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Key<'a> {
    Null,
    Number(String),
    Fraction(u64),
    Text(&'a str),
}

/// Ordered scalar-key identity shared by decoded-document and source admission.
/// The representation is private so adapters cannot invent a different numeric
/// equality rule. Strings are borrowed; no content hashing is involved.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct ScalarKey<'a>(Key<'a>);

impl DocumentNode {
    /// Return a key for an admitted scalar; containers have no scalar identity.
    pub fn scalar_key(&self) -> Option<ScalarKey<'_>> {
        match self {
            Self::Float(value) if !value.is_finite() => return None,
            Self::Integer(text) if !canonical_integer(text) => return None,
            _ => {}
        }
        scalar_key(self).map(ScalarKey)
    }
}

fn scalar_key(node: &DocumentNode) -> Option<Key<'_>> {
    Some(match node {
        DocumentNode::Null => Key::Null,
        DocumentNode::Boolean(value) => Key::Number(if *value { "1" } else { "0" }.into()),
        DocumentNode::Integer(text) => Key::Number(text.clone()),
        DocumentNode::Float(value) => float_key(*value),
        DocumentNode::Text(text) => Key::Text(text),
        _ => return None,
    })
}

/// Bundle composition uses exactly the same scalar-key equality as document admission.
pub(super) fn scalar_keys_equal(a: &DocumentNode, b: &DocumentNode) -> bool {
    match (scalar_key(a), scalar_key(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Admit decimal spelling directly, avoiding expensive arbitrary-precision parsing.
fn canonical_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits == "0" {
        return text == "0";
    }
    !digits.is_empty() && digits.as_bytes()[0] != b'0' && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Integral binary64 keys compare exactly to integers, including values beyond i64.
fn float_key(value: f64) -> Key<'static> {
    let bits = value.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    if value == 0.0 {
        return Key::Number("0".into());
    }
    let significand = fraction | if raw_exponent == 0 { 0 } else { 1_u64 << 52 };
    let exponent = if raw_exponent == 0 {
        -1074
    } else {
        raw_exponent - 1023 - 52
    };
    let magnitude = if exponent >= 0 {
        BigInt::from(significand) << exponent as usize
    } else {
        let shift = (-exponent) as u32;
        if shift > 52 || significand & ((1_u64 << shift) - 1) != 0 {
            return Key::Fraction(bits);
        }
        BigInt::from(significand >> shift)
    };
    Key::Number(
        if value.is_sign_negative() {
            -magnitude
        } else {
            magnitude
        }
        .to_string(),
    )
}

/// Charge before allocation or traversal; successful prefixes are never refunded.
fn charge(
    used: &mut usize,
    amount: usize,
    limit: usize,
    resource: DocumentResource,
) -> Result<(), DocumentError> {
    *used = used
        .checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or(DocumentError::Limit { resource, limit })?;
    Ok(())
}

impl Document {
    /// Validate every occurrence and reject sharing, cycles, duplicate keys and unreachable data.
    pub fn new(
        nodes: Vec<DocumentNode>,
        root: usize,
        limits: DocumentLimits,
    ) -> Result<Self, DocumentError> {
        if root >= nodes.len() {
            return Err(DocumentError::InvalidRoot);
        }
        let mut count = 0;
        charge(
            &mut count,
            nodes.len(),
            limits.nodes,
            DocumentResource::Nodes,
        )?;
        let mut owned = vec![false; nodes.len()];
        let mut depths = Vec::with_capacity(nodes.len());
        let (mut bytes, mut edges) = (0, 0);
        for (index, node) in nodes.iter().enumerate() {
            let mut depth = 1;
            let mut child = |id: usize| -> Result<(), DocumentError> {
                if id >= index {
                    return Err(DocumentError::InvalidChild);
                }
                if owned[id] {
                    return Err(DocumentError::SharedChild);
                }
                owned[id] = true;
                depth = depth.max(depths[id] + 1);
                Ok(())
            };
            match node {
                DocumentNode::Text(text) | DocumentNode::Integer(text) => {
                    charge(
                        &mut bytes,
                        text.len(),
                        limits.text_bytes,
                        DocumentResource::TextBytes,
                    )?;
                    if matches!(node, DocumentNode::Integer(_)) && !canonical_integer(text) {
                        return Err(DocumentError::InvalidInteger);
                    }
                }
                DocumentNode::Float(value) if !value.is_finite() => {
                    return Err(DocumentError::NonFinite)
                }
                DocumentNode::Sequence(items) => {
                    charge(
                        &mut edges,
                        items.len(),
                        limits.edges,
                        DocumentResource::Edges,
                    )?;
                    for &id in items {
                        child(id)?;
                    }
                }
                DocumentNode::Mapping(items) => {
                    let amount = items.len().checked_mul(2).ok_or(DocumentError::Limit {
                        resource: DocumentResource::Edges,
                        limit: limits.edges,
                    })?;
                    charge(&mut edges, amount, limits.edges, DocumentResource::Edges)?;
                    let mut keys = BTreeSet::new();
                    for &(key, value) in items {
                        child(key)?;
                        child(value)?;
                        let key = scalar_key(&nodes[key]).ok_or(DocumentError::NonScalarKey)?;
                        if !keys.insert(key) {
                            return Err(DocumentError::DuplicateKey);
                        }
                    }
                }
                _ => {}
            }
            let limit = limits.depth.min(64);
            if depth > limit {
                return Err(DocumentError::Limit {
                    resource: DocumentResource::Depth,
                    limit,
                });
            }
            depths.push(depth);
        }
        if owned[root]
            || owned
                .iter()
                .enumerate()
                .any(|(id, used)| id != root && !used)
        {
            return Err(DocumentError::UnreachableNode);
        }
        Ok(Self { nodes, root })
    }

    /// Expose immutable decoded occurrences without host-library types.
    pub fn nodes(&self) -> &[DocumentNode] {
        &self.nodes
    }

    /// Return the stable root occurrence, including non-mapping documents.
    pub fn root(&self) -> usize {
        self.root
    }

    /// Resolve a textual field while preserving authored mapping order elsewhere.
    pub fn field(&self, mapping: usize, name: &str) -> Option<usize> {
        let DocumentNode::Mapping(items) = self.nodes.get(mapping)? else {
            return None;
        };
        items
            .iter()
            .find_map(|&(key, value)| match &self.nodes[key] {
                DocumentNode::Text(text) if text == name => Some(value),
                _ => None,
            })
    }
}
