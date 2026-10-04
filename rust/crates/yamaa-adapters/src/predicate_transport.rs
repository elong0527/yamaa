//! Flat, bounded typed predicate admission for the dataset bridge, not a parser.
use crate::{dataset_transport::DatasetTransportError as Error, scalar_transport::ScalarValue};
use serde::Deserialize;
use yamaa_core::predicate as core;
use yamaa_engine::dataset_predicate as engine;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Predicate {
    path: String,
    text: String,
    nodes: Vec<Node>,
    root: usize,
    bindings: Vec<Binding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    name: String,
    read: Read,
}

impl Binding {
    /// Decode the shared closed dataset read coordinates after bounded name admission.
    pub(crate) fn prepare(self) -> Result<engine::Binding, Error> {
        name_limit(&self.name)?;
        Ok(engine::Binding {
            name: self.name,
            read: match self.read {
                Read::Source(index) => engine::Read::Source(index),
                Read::Column(index) => engine::Read::Column(index),
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Read {
    Source(usize),
    Column(usize),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Scalar {
    Literal(ScalarValue),
    Identifier(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Node {
    Boolean(bool),
    Not(usize),
    And([usize; 2]),
    Or([usize; 2]),
    Compare {
        operator: Comparison,
        left: Scalar,
        right: Scalar,
    },
    IsNull {
        value: Scalar,
        negated: bool,
    },
    In {
        value: Scalar,
        items: Vec<Scalar>,
        negated: bool,
    },
    Between {
        value: Scalar,
        lower: Scalar,
        upper: Scalar,
        negated: bool,
    },
    Like {
        value: Scalar,
        pattern: Scalar,
        escape: Option<String>,
        negated: bool,
    },
}

impl Scalar {
    /// Admit only lossless scalar encodings and bounded nonempty identifiers.
    fn core(self) -> Result<core::Scalar, Error> {
        match self {
            Self::Literal(value) => Ok(core::Scalar::Literal(
                value.into_core().map_err(|_| Error::InvalidScalar)?,
            )),
            Self::Identifier(name) => {
                name_limit(&name)?;
                Ok(core::Scalar::Identifier(name))
            }
        }
    }
}

/// Names are already bound; this boundary enforces size rather than parsing syntax.
fn name_limit(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::InvalidPlan);
    }
    if name.len() > 256 {
        return Err(Error::RequestLimit);
    }
    Ok(())
}

impl Node {
    /// Preserve every caller-authored occurrence without folding or data access.
    fn core(self) -> Result<core::Node, Error> {
        Ok(match self {
            Self::Boolean(value) => core::Node::Boolean(value),
            Self::Not(child) => core::Node::Not(child),
            Self::And([a, b]) => core::Node::And(a, b),
            Self::Or([a, b]) => core::Node::Or(a, b),
            Self::Compare {
                operator,
                left,
                right,
            } => core::Node::Compare {
                operator: match operator {
                    Comparison::Eq => core::Comparison::Equal,
                    Comparison::Ne => core::Comparison::NotEqual,
                    Comparison::Lt => core::Comparison::Less,
                    Comparison::Le => core::Comparison::LessEqual,
                    Comparison::Gt => core::Comparison::Greater,
                    Comparison::Ge => core::Comparison::GreaterEqual,
                },
                left: left.core()?,
                right: right.core()?,
            },
            Self::IsNull { value, negated } => core::Node::IsNull {
                value: value.core()?,
                negated,
            },
            Self::In {
                value,
                items,
                negated,
            } => {
                if items.len() > 4096 {
                    return Err(Error::RequestLimit);
                }
                core::Node::In {
                    value: value.core()?,
                    items: items
                        .into_iter()
                        .map(Scalar::core)
                        .collect::<Result<_, _>>()?,
                    negated,
                }
            }
            Self::Between {
                value,
                lower,
                upper,
                negated,
            } => core::Node::Between {
                value: value.core()?,
                lower: lower.core()?,
                upper: upper.core()?,
                negated,
            },
            Self::Like {
                value,
                pattern,
                escape,
                negated,
            } => {
                let escape = escape
                    .map(|text| {
                        let mut chars = text.chars();
                        let value = chars.next().ok_or(Error::InvalidPlan)?;
                        if chars.next().is_some() {
                            return Err(Error::InvalidPlan);
                        }
                        Ok(value)
                    })
                    .transpose()?;
                core::Node::Like {
                    value: value.core()?,
                    pattern: pattern.core()?,
                    escape,
                    negated,
                }
            }
        })
    }
}

impl Predicate {
    /// Validate the complete typed filter and bindings before decoding source IPC.
    pub(crate) fn prepare(self) -> Result<engine::BoundPredicate, Error> {
        if self.nodes.len() > 4096
            || self.bindings.len() > 4096
            || self.path.len() > 1024
            || self.text.len() > 65536
        {
            return Err(Error::RequestLimit);
        }
        if self.text.is_empty() {
            return Err(Error::InvalidPlan);
        }
        let bindings = self
            .bindings
            .into_iter()
            .map(Binding::prepare)
            .collect::<Result<_, Error>>()?;
        let nodes = self
            .nodes
            .into_iter()
            .map(Node::core)
            .collect::<Result<_, _>>()?;
        let plan = core::Plan::new(
            nodes,
            self.root,
            self.path,
            self.text,
            core::Limits::default(),
        )
        .map_err(|error| match error {
            core::PlanError::Limit(_) => Error::RequestLimit,
            _ => Error::InvalidPlan,
        })?;
        engine::BoundPredicate::new(plan, bindings).map_err(|_| Error::InvalidPlan)
    }
}
