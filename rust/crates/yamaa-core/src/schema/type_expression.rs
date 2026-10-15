//! REQ-0261/0262 type syntax. Unions are ordered descriptor lists, not strings.

use alloc::{string::String, vec::Vec};
use core::ops::Range;

/// One immutable type occurrence; children precede their containing type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeNode {
    Name(Range<usize>),
    List(usize),
    Dictionary { key: usize, value: usize },
}

/// Explicit syntax limits, independent of subsequent document validation limits.
#[derive(Clone, Copy, Debug)]
pub struct TypeLimits {
    pub bytes: usize,
    pub nodes: usize,
    pub depth: usize,
}

impl Default for TypeLimits {
    /// Bound input before scanning and recursive descent before entering children.
    fn default() -> Self {
        Self {
            bytes: 65_536,
            nodes: 4_096,
            depth: 64,
        }
    }
}

/// Resource refusal is distinct from malformed schema type syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeResource {
    Bytes,
    Nodes,
    Depth,
}

/// Byte locations refer to the original, untrimmed UTF-8 expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeError {
    Invalid {
        byte: usize,
    },
    Limit {
        resource: TypeResource,
        limit: usize,
    },
}

/// Parsed type syntax retains authored names without resolving bundle references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeExpression {
    expression: String,
    nodes: Vec<TypeNode>,
    spans: Vec<Range<usize>>,
    root: usize,
}

impl TypeExpression {
    /// Parse a complete expression without accepting trailing tokens or union syntax.
    pub fn parse(expression: &str, limits: TypeLimits) -> Result<Self, TypeError> {
        if expression.len() > limits.bytes {
            return Err(TypeError::Limit {
                resource: TypeResource::Bytes,
                limit: limits.bytes,
            });
        }
        let mut parser = Parser {
            expression,
            at: 0,
            nodes: Vec::new(),
            spans: Vec::new(),
            admitted: 0,
            limits,
        };
        let root = parser.parse(1)?;
        parser.space();
        if parser.at != expression.len() {
            return Err(parser.invalid());
        }
        Ok(Self {
            expression: expression.into(),
            nodes: parser.nodes,
            spans: parser.spans,
            root,
        })
    }

    /// Return the caller's original spelling, including surrounding whitespace.
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// Expose immutable occurrence order for binding and independent inspection.
    pub fn nodes(&self) -> &[TypeNode] {
        &self.nodes
    }

    /// Return the index of the complete expression's type.
    pub fn root(&self) -> usize {
        self.root
    }

    /// Exact nested spelling without outer whitespace, for schema shorthand dispatch.
    pub fn node_text(&self, node: usize) -> Option<&str> {
        self.spans
            .get(node)
            .map(|span| &self.expression[span.clone()])
    }

    /// Yield named type occurrences in written order, including repeated names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().filter_map(|node| match node {
            TypeNode::Name(span) => Some(&self.expression[span.clone()]),
            _ => None,
        })
    }
}

struct Parser<'a> {
    expression: &'a str,
    at: usize,
    nodes: Vec<TypeNode>,
    spans: Vec<Range<usize>>,
    admitted: usize,
    limits: TypeLimits,
}

impl Parser<'_> {
    /// Advance only over whitespace; retain byte coordinates for all other tokens.
    fn space(&mut self) {
        while let Some(c) = self.expression[self.at..].chars().next() {
            // The existing schema reader's strip convention also includes these
            // four ASCII separators; escaped strings may contain them.
            if !c.is_whitespace() && !('\u{1c}'..='\u{1f}').contains(&c) {
                break;
            }
            self.at += c.len_utf8();
        }
    }

    /// Locate malformed syntax without including the caller's text in an error.
    fn invalid(&self) -> TypeError {
        TypeError::Invalid { byte: self.at }
    }

    /// Require one structural ASCII token, allowing surrounding inner whitespace.
    fn token(&mut self, expected: u8) -> Result<(), TypeError> {
        self.space();
        if self.expression.as_bytes().get(self.at) != Some(&expected) {
            return Err(self.invalid());
        }
        self.at += 1;
        Ok(())
    }

    /// Reserve each occurrence before descending; a hard cap protects the stack.
    fn parse(&mut self, depth: usize) -> Result<usize, TypeError> {
        let depth_limit = self.limits.depth.min(64);
        if depth > depth_limit {
            return Err(TypeError::Limit {
                resource: TypeResource::Depth,
                limit: depth_limit,
            });
        }
        if self.admitted >= self.limits.nodes {
            return Err(TypeError::Limit {
                resource: TypeResource::Nodes,
                limit: self.limits.nodes,
            });
        }
        self.admitted += 1;
        self.space();
        let start = self.at;
        let bytes = self.expression.as_bytes();
        if !bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        {
            return Err(self.invalid());
        }
        self.at += 1;
        while bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.at += 1;
        }
        let end = self.at;
        // A bracket belongs immediately after its collection keyword. Spaces
        // are allowed around its inner expressions and separating comma.
        let node = if bytes.get(self.at) == Some(&b'[') {
            self.at += 1;
            match &self.expression[start..end] {
                "list" => {
                    let child = self.parse(depth + 1)?;
                    self.token(b']')?;
                    TypeNode::List(child)
                }
                "dict" => {
                    let key = self.parse(depth + 1)?;
                    self.token(b',')?;
                    let value = self.parse(depth + 1)?;
                    self.token(b']')?;
                    TypeNode::Dictionary { key, value }
                }
                _ => return Err(TypeError::Invalid { byte: end }),
            }
        } else {
            TypeNode::Name(start..end)
        };
        let index = self.nodes.len();
        self.nodes.push(node);
        self.spans.push(start..self.at);
        Ok(index)
    }
}
