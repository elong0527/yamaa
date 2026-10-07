//! Lower bounded original predicate syntax into the shared executable plan.
//! No identifier resolution, constant folding or host semantic objects occur here.
use alloc::vec::Vec;

use crate::{
    predicate::{self, Node, Plan, Scalar},
    predicate_parser::{self, ParsedKind as K, ParsedPredicate, SourceSpan, TemporalKind},
    value::Value,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Limits {
    pub parse: predicate_parser::ParseLimits,
    pub plan: predicate::Limits,
}

/// Valid literals outside the current scalar carrier remain explicitly unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedLiteral {
    WideInteger,
    NonFiniteFloat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Parse(predicate_parser::ParseError),
    Plan(predicate::PlanError),
    UnsupportedLiteral {
        kind: UnsupportedLiteral,
        span: SourceSpan,
    },
    /// Parser/compiler structural disagreement, never a language diagnostic.
    Internal,
}

/// Parse every branch before lowering; the executable plan owns all provenance.
/// The parser's hard depth ceiling bounds recursion through original syntax.
pub fn compile(text: &str, path: &str, limits: Limits) -> Result<Plan, Error> {
    let parsed = predicate_parser::parse_predicate(text, limits.parse).map_err(Error::Parse)?;
    if path
        .len()
        .checked_add(text.len())
        .is_none_or(|n| n > limits.plan.text_bytes)
    {
        return Err(Error::Plan(predicate::PlanError::Limit(
            predicate::LimitError {
                resource: predicate::Resource::TextBytes,
                limit: limits.plan.text_bytes,
            },
        )));
    }
    let mut nodes = Vec::new();
    let root = lower(&parsed, parsed.root(), &mut nodes)?;
    Plan::new(nodes, root, path.into(), text.into(), limits.plan).map_err(Error::Plan)
}

fn scalar(parsed: &ParsedPredicate, index: usize) -> Result<Scalar, Error> {
    let node = &parsed.nodes()[index];
    let written = &parsed.expression()[node.span.start..node.span.end];
    let value = match &node.kind {
        K::Group(child) => return scalar(parsed, *child),
        K::Identifier => return Ok(Scalar::Identifier(written.into())),
        K::String(value) => Value::Str(value.clone()),
        K::Null => Value::Missing,
        K::Boolean(value) => Value::Bool(*value),
        K::Number { fractional: false } => {
            Value::Int(written.parse().map_err(|_| Error::UnsupportedLiteral {
                kind: UnsupportedLiteral::WideInteger,
                span: node.span,
            })?)
        }
        K::Number { fractional: true } => {
            let value: f64 = written.parse().map_err(|_| Error::Internal)?;
            if !value.is_finite() {
                return Err(Error::UnsupportedLiteral {
                    kind: UnsupportedLiteral::NonFiniteFloat,
                    span: node.span,
                });
            }
            Value::float(value)
        }
        K::Temporal {
            kind: TemporalKind::Date,
            value,
        } => Value::Date(value.parse().map_err(|_| Error::Internal)?),
        K::Temporal {
            kind: TemporalKind::DateTime,
            value,
        } => Value::DateTime(value.parse().map_err(|_| Error::Internal)?),
        _ => return Err(Error::Internal),
    };
    Ok(Scalar::Literal(value))
}

fn lower(parsed: &ParsedPredicate, index: usize, nodes: &mut Vec<Node>) -> Result<usize, Error> {
    let node = match &parsed.nodes()[index].kind {
        K::Group(child) => return lower(parsed, *child, nodes),
        K::Boolean(value) => Node::Boolean(*value),
        K::Not(child) => Node::Not(lower(parsed, *child, nodes)?),
        K::And(left, right) => {
            Node::And(lower(parsed, *left, nodes)?, lower(parsed, *right, nodes)?)
        }
        K::Or(left, right) => Node::Or(lower(parsed, *left, nodes)?, lower(parsed, *right, nodes)?),
        K::Compare {
            operator,
            left,
            right,
        } => Node::Compare {
            operator: *operator,
            left: scalar(parsed, *left)?,
            right: scalar(parsed, *right)?,
        },
        K::IsNull { value, negated } => Node::IsNull {
            value: scalar(parsed, *value)?,
            negated: *negated,
        },
        K::In {
            value,
            items,
            negated,
        } => Node::In {
            value: scalar(parsed, *value)?,
            items: items
                .iter()
                .map(|&item| scalar(parsed, item))
                .collect::<Result<_, _>>()?,
            negated: *negated,
        },
        K::Between {
            value,
            lower,
            upper,
            negated,
        } => Node::Between {
            value: scalar(parsed, *value)?,
            lower: scalar(parsed, *lower)?,
            upper: scalar(parsed, *upper)?,
            negated: *negated,
        },
        K::Like {
            value,
            pattern,
            escape,
            negated,
        } => Node::Like {
            value: scalar(parsed, *value)?,
            pattern: scalar(parsed, *pattern)?,
            escape: *escape,
            negated: *negated,
        },
        K::Contains { source, pattern } => {
            let K::String(pattern) = &parsed.nodes()[*pattern].kind else {
                return Err(Error::Internal);
            };
            Node::Contains {
                value: scalar(parsed, *source)?,
                pattern: pattern.clone(),
            }
        }
        _ => return Err(Error::Internal),
    };
    let index = nodes.len();
    nodes.push(node);
    Ok(index)
}
