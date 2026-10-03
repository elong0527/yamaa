//! Typed evaluation of the implemented numeric subset, independent of parsing.
//!
//! Nodes preserve written association. Resolution is left-to-right and stops at
//! the first failure, but missing operands do not skip later operands. The numeric
//! compiler supplies validated, bounded trees and maps failures to source spans.

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::numeric::{self, ArithmeticErrorKind, BinaryOperator, Number, UnaryOperator};
use crate::value::{Selection, ValueType};

/// Numeric IR for normalized values, deferred literal failures and arithmetic.
/// ABS and MOD use the existing unary/binary primitive variants. This is not a parser
/// or a claim that the rest of the numeric function vocabulary is implemented.
#[derive(Clone, Debug, PartialEq)]
pub enum NumericNode {
    Literal(Number),
    /// Deferred positive literal overflow, preserving exact canonical decimal digits.
    IntegerOverflowLiteral(String),
    Identifier(String),
    Unary {
        operator: UnaryOperator,
        operand: Box<NumericNode>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<NumericNode>,
        right: Box<NumericNode>,
    },
}

/// Resolve normalized core values without importing a table or host framework.
/// An error is transported intact; absence and a present missing value are distinct.
pub trait NumericResolver {
    type Error;

    /// Resolve one occurrence, in written order, without implicit memoization.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error>;
}

/// Structural position in the typed tree, not a source character offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Unary,
    Left,
    Right,
}

/// Provenance supplied by the compiler and the exact operand route that failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationLocation {
    pub spec_path: String,
    pub expression: String,
    pub operands: Vec<Operand>,
}

/// Failures owned by numeric evaluation rather than by the resolution boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumericCondition {
    UnknownField {
        identifier: String,
    },
    IncompatibleInput {
        identifier: String,
        actual: ValueType,
    },
    Arithmetic(ArithmeticErrorKind),
    LiteralOverflow {
        value: String,
    },
}

impl NumericCondition {
    /// Distinguish invalid bindings from arithmetic failures during derivation.
    pub fn phase(&self) -> &'static str {
        match self {
            Self::UnknownField { .. } | Self::IncompatibleInput { .. } => "validation",
            Self::Arithmetic(_) | Self::LiteralOverflow { .. } => "derivation",
        }
    }

    /// Return the same condition vocabulary entry as the Python reference.
    pub fn condition(&self) -> &'static str {
        match self {
            Self::UnknownField { .. } => "unknown_field",
            Self::IncompatibleInput { .. } => "incompatible_input_type",
            Self::Arithmetic(ArithmeticErrorKind::IntegerOverflow { .. })
            | Self::LiteralOverflow { .. } => "integer_overflow",
            Self::Arithmetic(ArithmeticErrorKind::DivisionByZero) => "division_by_zero",
        }
    }

    /// Identify the requirement owning the failure, without applying a handler.
    pub fn requirement(&self) -> &'static str {
        match self {
            Self::UnknownField { .. } => "REQ-0443",
            Self::IncompatibleInput { .. } => "REQ-0444",
            Self::Arithmetic(ArithmeticErrorKind::IntegerOverflow { .. })
            | Self::LiteralOverflow { .. } => "REQ-0434",
            Self::Arithmetic(ArithmeticErrorKind::DivisionByZero) => "REQ-0430",
        }
    }
}

/// Retain a resolver's own failure without reclassifying it as unknown or missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluationErrorKind<E> {
    Numeric(NumericCondition),
    Resolution { identifier: String, error: E },
}

/// An evaluation failure with provenance, ready for later diagnostic transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationError<E> {
    pub location: EvaluationLocation,
    pub kind: EvaluationErrorKind<E>,
}

/// A typed expression associated with its original text and specification path.
/// The caller supplies the tree; parsing, binding and handler accounting are separate.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericPlan {
    pub spec_path: String,
    pub expression: String,
    pub root: NumericNode,
}

impl NumericPlan {
    /// Evaluate without reassociation, implicit conversion, caching or handler effects.
    pub fn evaluate<R: NumericResolver>(
        &self,
        resolver: &mut R,
    ) -> Result<Number, EvaluationError<R::Error>> {
        self.evaluate_node(&self.root, resolver, &mut Vec::new())
    }

    /// Snapshot the failed node's route while preserving the original failure data.
    fn failure<E>(&self, kind: EvaluationErrorKind<E>, path: &[Operand]) -> EvaluationError<E> {
        EvaluationError {
            location: EvaluationLocation {
                spec_path: self.spec_path.clone(),
                expression: self.expression.clone(),
                operands: path.into(),
            },
            kind,
        }
    }

    /// Descend into a child and restore the route for its following sibling.
    fn child<R: NumericResolver>(
        &self,
        node: &NumericNode,
        position: Operand,
        resolver: &mut R,
        path: &mut Vec<Operand>,
    ) -> Result<Number, EvaluationError<R::Error>> {
        path.push(position);
        let result = self.evaluate_node(node, resolver, path);
        path.pop();
        result
    }

    /// Resolve and evaluate a node; both operands precede missing propagation.
    fn evaluate_node<R: NumericResolver>(
        &self,
        node: &NumericNode,
        resolver: &mut R,
        path: &mut Vec<Operand>,
    ) -> Result<Number, EvaluationError<R::Error>> {
        let arithmetic = match node {
            NumericNode::Literal(number) => return Ok(*number),
            NumericNode::IntegerOverflowLiteral(value) => {
                return Err(self.failure(
                    EvaluationErrorKind::Numeric(NumericCondition::LiteralOverflow {
                        value: value.clone(),
                    }),
                    path,
                ));
            }
            NumericNode::Identifier(identifier) => {
                let selected = resolver.resolve(identifier).map_err(|error| {
                    self.failure(
                        EvaluationErrorKind::Resolution {
                            identifier: identifier.clone(),
                            error,
                        },
                        path,
                    )
                })?;
                let condition = match selected {
                    Selection::Absent => NumericCondition::UnknownField {
                        identifier: identifier.clone(),
                    },
                    Selection::Present(value) => match Number::try_from(&value) {
                        Ok(number) => return Ok(number),
                        Err(actual) => NumericCondition::IncompatibleInput {
                            identifier: identifier.clone(),
                            actual,
                        },
                    },
                };
                return Err(self.failure(EvaluationErrorKind::Numeric(condition), path));
            }
            NumericNode::Unary { operator, operand } => {
                let value = self.child(operand, Operand::Unary, resolver, path)?;
                numeric::unary(*operator, value, &self.expression)
            }
            NumericNode::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.child(left, Operand::Left, resolver, path)?;
                let right = self.child(right, Operand::Right, resolver, path)?;
                numeric::binary(*operator, left, right, &self.expression)
            }
        };
        arithmetic.map_err(|error| {
            self.failure(
                EvaluationErrorKind::Numeric(NumericCondition::Arithmetic(error.kind)),
                path,
            )
        })
    }
}
