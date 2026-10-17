//! The one evaluator: truth for predicates, numbers for arithmetic, and the
//! written values of comparison operands.
//!
//! Evaluation is eager and keeps written order: every operand of `AND`, `OR`
//! and arithmetic is evaluated even when the result is already known, and a
//! failure stops the evaluation. Each identifier occurrence, including the
//! repeated subject of `BETWEEN`, is resolved afresh; nothing is cached.

use std::cmp::Ordering;

use super::bind::CompiledExpression;
use super::syntax::{Comparison, NodeKind, SourceSpan};
use super::Function;
use crate::numeric::{
    self, ArithmeticErrorKind, BinaryOperator, IntegralFunction, MathFunction, Number,
    UnaryOperator,
};
use crate::table::ValueRef;
use crate::value::{compare_present, Selection, Value, ValueType};

/// Predicate truth is distinct from the runtime bool value and missing scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }
    /// Negation keeps an unknown truth unknown.
    pub fn negate(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
    /// Combine evaluated operands under the three-valued AND table.
    pub fn and(self, right: Self) -> Self {
        match (self, right) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::True,
        }
    }
    /// Combine evaluated operands under the three-valued OR table.
    pub fn or(self, right: Self) -> Self {
        match (self, right) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::False,
        }
    }
}

/// The owning site resolves names; its failures move intact and stay opaque.
pub trait Resolver {
    type Error;
    /// Resolve one occurrence, keeping absence apart from a present missing value.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error>;

    /// Optionally lend a normalized cell until the evaluator copies it.
    fn resolve_value(&mut self, identifier: &str) -> Result<Resolved<'_>, Self::Error> {
        self.resolve(identifier).map(|selection| match selection {
            Selection::Absent => Resolved::Absent,
            Selection::Present(value) => Resolved::Owned(value),
        })
    }
}

/// A borrow survives only until the evaluator has copied the operand.
pub enum Resolved<'a> {
    Absent,
    Owned(Value),
    Borrowed(ValueRef<'a>),
}

/// One structural step from a node to the operand that failed. Truth nodes and
/// arithmetic nodes keep their own historical spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Child,
    Left,
    Right,
    Value,
    Item(usize),
    Lower,
    RepeatedValue,
    Upper,
    Pattern,
    Unary,
    LeftOperand,
    RightOperand,
}

/// A failure the language owns, apart from the resolver's own failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    /// A comparison operand names no available value.
    UnknownField {
        identifier: String,
    },
    IncompatiblePair {
        left: ValueType,
        right: ValueType,
    },
    /// A non-text `LIKE` operand.
    ExpectedText {
        actual: ValueType,
    },
    /// A non-text `str_contains` source.
    ContainsExpectedText {
        actual: ValueType,
    },
    DanglingEscape,
    /// A numeric operand names no available value.
    NumericUnknownField {
        identifier: String,
    },
    /// A numeric operand holds a value of another type.
    NumericInput {
        identifier: String,
        actual: ValueType,
    },
    Arithmetic(ArithmeticErrorKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind<E> {
    Condition(Condition),
    Resolution { identifier: String, error: E },
}

/// The failing site: provenance, the route from the root, and for an
/// arithmetic node the exact source extent of the node that failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "expression: spec_path={spec_path:?}, expression={expression:?}, route={route:?}, source_span={source_span:?}, kind={kind:?}"
)]
pub struct EvaluationError<E> {
    pub spec_path: String,
    pub expression: String,
    pub route: Vec<Step>,
    pub source_span: Option<SourceSpan>,
    pub kind: ErrorKind<E>,
}

impl CompiledExpression {
    /// Evaluate a predicate afresh; reuse never memoizes values or failures.
    pub fn evaluate_truth<R: Resolver>(
        &self,
        resolver: &mut R,
    ) -> Result<Truth, Box<EvaluationError<R::Error>>> {
        Evaluator::new(self, resolver, &[]).truth(self.syntax.root())
    }

    /// Evaluate a numeric expression in written association.
    pub fn evaluate_number<R: Resolver>(
        &self,
        resolver: &mut R,
    ) -> Result<Number, Box<EvaluationError<R::Error>>> {
        Evaluator::new(self, resolver, &[]).number(self.syntax.root())
    }

    /// Evaluate an aggregate's arithmetic once every reduction has its value.
    pub fn evaluate_aggregate<R: Resolver>(
        &self,
        resolver: &mut R,
        reductions: &[Value],
    ) -> Result<Number, Box<EvaluationError<R::Error>>> {
        Evaluator::new(self, resolver, reductions).number(self.syntax.root())
    }

    /// Evaluate one reduction's numeric argument for one record.
    pub fn evaluate_operand<R: Resolver>(
        &self,
        reduction: usize,
        resolver: &mut R,
    ) -> Result<Number, Box<EvaluationError<R::Error>>> {
        let node = self.reductions[reduction].operand_node;
        Evaluator::new(self, resolver, &[]).number(node)
    }
}

fn own(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Missing,
        ValueRef::Str(value) => Value::Str(value.into()),
        ValueRef::Int(value) => Value::Int(value),
        ValueRef::Float(value) => Value::Float(value),
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(value) => Value::Date(value),
        ValueRef::DateTime(value) => Value::DateTime(value),
    }
}

struct Evaluator<'a, R: Resolver> {
    compiled: &'a CompiledExpression,
    resolver: &'a mut R,
    reductions: &'a [Value],
    route: Vec<Step>,
}

impl<'a, R: Resolver> Evaluator<'a, R> {
    fn new(compiled: &'a CompiledExpression, resolver: &'a mut R, reductions: &'a [Value]) -> Self {
        Self {
            compiled,
            resolver,
            reductions,
            route: Vec::new(),
        }
    }

    fn failure(
        &self,
        kind: ErrorKind<R::Error>,
        source_span: Option<SourceSpan>,
    ) -> Box<EvaluationError<R::Error>> {
        Box::new(EvaluationError {
            spec_path: self.compiled.spec_path.clone(),
            expression: self.compiled.expression().into(),
            route: self.route.clone(),
            source_span,
            kind,
        })
    }

    fn condition(
        &self,
        condition: Condition,
        source_span: Option<SourceSpan>,
    ) -> Box<EvaluationError<R::Error>> {
        self.failure(ErrorKind::Condition(condition), source_span)
    }

    /// Visit an operand and restore the route for its following sibling.
    fn nested<T>(&mut self, step: Step, visit: impl FnOnce(&mut Self) -> T) -> T {
        self.route.push(step);
        let result = visit(self);
        self.route.pop();
        result
    }

    /// Resolve one occurrence and copy any lent cell at once.
    fn resolve(&mut self, identifier: &str) -> Result<Option<Value>, R::Error> {
        Ok(match self.resolver.resolve_value(identifier)? {
            Resolved::Absent => None,
            Resolved::Owned(value) => Some(value),
            Resolved::Borrowed(value) => Some(own(value)),
        })
    }

    fn truth(&mut self, id: usize) -> Result<Truth, Box<EvaluationError<R::Error>>> {
        let compiled = self.compiled;
        let truth = match &compiled.syntax.nodes()[id].kind {
            NodeKind::Group(inner) => return self.truth(*inner),
            NodeKind::Boolean(value) => return Ok(Truth::from_bool(*value)),
            NodeKind::Not(operand) => {
                return self
                    .nested(Step::Child, |e| e.truth(*operand))
                    .map(Truth::negate)
            }
            NodeKind::And(left, right) | NodeKind::Or(left, right) => {
                let left = self.nested(Step::Left, |e| e.truth(*left))?;
                let right = self.nested(Step::Right, |e| e.truth(*right))?;
                return Ok(match compiled.syntax.nodes()[id].kind {
                    NodeKind::And(..) => left.and(right),
                    _ => left.or(right),
                });
            }
            NodeKind::Compare {
                operator,
                left,
                right,
            } => {
                let left = self.operand(*left, Step::Left)?;
                let right = self.operand(*right, Step::Right)?;
                compare(*operator, &left, &right)
            }
            NodeKind::IsNull { value, negated } => {
                let value = self.operand(*value, Step::Value)?;
                Ok(Truth::from_bool(
                    matches!(value, Value::Missing) != *negated,
                ))
            }
            NodeKind::In {
                value,
                items,
                negated,
            } => {
                let value = self.operand(*value, Step::Value)?;
                let mut truth = Truth::False;
                for (index, item) in items.iter().enumerate() {
                    let item = self.operand(*item, Step::Item(index))?;
                    let equal = compare(Comparison::Equal, &value, &item)
                        .map_err(|condition| self.condition(condition, None))?;
                    truth = truth.or(equal);
                }
                Ok(if *negated { truth.negate() } else { truth })
            }
            NodeKind::Between {
                value,
                lower,
                upper,
                negated,
            } => {
                // Resolve all four occurrences before either comparison.
                let first = self.operand(*value, Step::Value)?;
                let lower = self.operand(*lower, Step::Lower)?;
                let second = self.operand(*value, Step::RepeatedValue)?;
                let upper = self.operand(*upper, Step::Upper)?;
                compare(Comparison::GreaterEqual, &first, &lower)
                    .and_then(|left| {
                        compare(Comparison::LessEqual, &second, &upper).map(|right| left.and(right))
                    })
                    .map(|truth| if *negated { truth.negate() } else { truth })
            }
            NodeKind::Like {
                value,
                pattern,
                escape,
                negated,
            } => {
                let value = self.operand(*value, Step::Value)?;
                let pattern = self.operand(*pattern, Step::Pattern)?;
                match (&value, &pattern) {
                    (Value::Missing, _) | (_, Value::Missing) => Ok(Truth::Unknown),
                    (Value::Str(value), Value::Str(pattern)) => like(value, pattern, *escape),
                    _ => Err(Condition::ExpectedText {
                        actual: if matches!(value, Value::Str(_)) {
                            pattern.value_type()
                        } else {
                            value.value_type()
                        }
                        .expect("missing handled above"),
                    }),
                }
                .map(|truth| if *negated { truth.negate() } else { truth })
            }
            NodeKind::Call { arguments, .. } => {
                let value = self.operand(arguments[0], Step::Value)?;
                match value {
                    Value::Missing => Ok(Truth::Unknown),
                    Value::Str(value) => {
                        let pattern = compiled.patterns[id].as_ref().expect("admitted pattern");
                        Ok(Truth::from_bool(pattern.is_match(&value)))
                    }
                    other => Err(Condition::ContainsExpectedText {
                        actual: other.value_type().expect("missing handled above"),
                    }),
                }
            }
            _ => unreachable!("the binder admits only truth values here"),
        };
        truth.map_err(|condition| self.condition(condition, None))
    }

    fn operand(&mut self, id: usize, step: Step) -> Result<Value, Box<EvaluationError<R::Error>>> {
        self.nested(step, |e| e.value(id))
    }

    /// A comparison operand: a name keeps its type; arithmetic is numeric.
    fn value(&mut self, id: usize) -> Result<Value, Box<EvaluationError<R::Error>>> {
        let compiled = self.compiled;
        match &compiled.syntax.nodes()[id].kind {
            NodeKind::Group(inner) => self.value(*inner),
            NodeKind::Number { .. }
            | NodeKind::String(_)
            | NodeKind::Temporal { .. }
            | NodeKind::Null => Ok(compiled.literals[id].clone().expect("a decoded literal")),
            NodeKind::Identifier => {
                let identifier = compiled.syntax.text(id);
                match self.resolve(identifier) {
                    Ok(Some(value)) => Ok(value),
                    Ok(None) => Err(self.condition(
                        Condition::UnknownField {
                            identifier: identifier.into(),
                        },
                        None,
                    )),
                    Err(error) => Err(self.failure(
                        ErrorKind::Resolution {
                            identifier: identifier.into(),
                            error,
                        },
                        None,
                    )),
                }
            }
            _ => self.number(id).map(Value::from),
        }
    }

    fn number(&mut self, id: usize) -> Result<Number, Box<EvaluationError<R::Error>>> {
        let compiled = self.compiled;
        let node = &compiled.syntax.nodes()[id];
        let span = Some(node.span);
        let expression = compiled.expression();
        let arithmetic = match &node.kind {
            NodeKind::Group(inner) => return self.number(*inner),
            NodeKind::Number { .. }
            | NodeKind::Null
            | NodeKind::String(_)
            | NodeKind::Temporal { .. } => {
                let value = compiled.literals[id].as_ref().expect("a decoded literal");
                return Number::try_from(value).map_err(|actual| {
                    self.condition(
                        Condition::NumericInput {
                            identifier: compiled.syntax.text(id).into(),
                            actual,
                        },
                        span,
                    )
                });
            }
            NodeKind::Identifier => {
                let identifier = compiled.syntax.text(id);
                let value = match self.resolve(identifier) {
                    Ok(Some(value)) => value,
                    Ok(None) => {
                        return Err(self.condition(
                            Condition::NumericUnknownField {
                                identifier: identifier.into(),
                            },
                            span,
                        ))
                    }
                    Err(error) => {
                        return Err(self.failure(
                            ErrorKind::Resolution {
                                identifier: identifier.into(),
                                error,
                            },
                            span,
                        ))
                    }
                };
                return Number::try_from(&value).map_err(|actual| {
                    self.condition(
                        Condition::NumericInput {
                            identifier: identifier.into(),
                            actual,
                        },
                        span,
                    )
                });
            }
            NodeKind::Reduction { .. } => {
                let reduction = &compiled.reductions[compiled.reduction_at(id)];
                let value = &self.reductions[compiled.reduction_at(id)];
                return Number::try_from(value).map_err(|actual| {
                    self.condition(
                        Condition::NumericInput {
                            identifier: reduction.reference.clone(),
                            actual,
                        },
                        span,
                    )
                });
            }
            NodeKind::Unary { operator, operand } => {
                let value = self.nested(Step::Unary, |e| e.number(*operand))?;
                numeric::unary(*operator, value, expression)
            }
            NodeKind::Binary {
                operator,
                left,
                right,
            } => {
                let (left, right) = self.pair(*left, *right)?;
                numeric::binary(*operator, left, right, expression)
            }
            NodeKind::Call { name, arguments } => {
                let function =
                    Function::lookup(compiled.syntax.slice(*name)).expect("an admitted function");
                match function {
                    Function::Abs => {
                        let value = self.nested(Step::Unary, |e| e.number(arguments[0]))?;
                        numeric::unary(UnaryOperator::Abs, value, expression)
                    }
                    Function::Ceil | Function::Floor | Function::Trunc => {
                        let value = self.nested(Step::Unary, |e| e.number(arguments[0]))?;
                        return Ok(numeric::integral(
                            match function {
                                Function::Ceil => IntegralFunction::Ceil,
                                Function::Floor => IntegralFunction::Floor,
                                _ => IntegralFunction::Trunc,
                            },
                            value,
                        ));
                    }
                    Function::Exp | Function::Ln => {
                        let value = self.nested(Step::Unary, |e| e.number(arguments[0]))?;
                        numeric::math(
                            if function == Function::Exp {
                                MathFunction::Exp
                            } else {
                                MathFunction::Ln
                            },
                            value,
                            expression,
                        )
                    }
                    Function::Sqrt => {
                        let value = self.nested(Step::Unary, |e| e.number(arguments[0]))?;
                        numeric::sqrt(value, expression)
                    }
                    Function::Power => {
                        let (left, right) = self.pair(arguments[0], arguments[1])?;
                        numeric::power(left, right, expression)
                    }
                    Function::Mod => {
                        let (left, right) = self.pair(arguments[0], arguments[1])?;
                        numeric::binary(BinaryOperator::Modulo, left, right, expression)
                    }
                    Function::NullIf => {
                        let (left, right) = self.pair(arguments[0], arguments[1])?;
                        return Ok(numeric::null_if(left, right));
                    }
                    Function::StrContains => unreachable!("a truth value is not a number"),
                }
            }
            _ => unreachable!("the binder admits only numbers here"),
        };
        arithmetic.map_err(|error| self.condition(Condition::Arithmetic(error.kind), span))
    }

    /// Both operands are evaluated before missing propagates.
    fn pair(
        &mut self,
        left: usize,
        right: usize,
    ) -> Result<(Number, Number), Box<EvaluationError<R::Error>>> {
        let left = self.nested(Step::LeftOperand, |e| e.number(left))?;
        let right = self.nested(Step::RightOperand, |e| e.number(right))?;
        Ok((left, right))
    }
}

/// A mixed int and float pair compares by exact value; no operand is widened.
fn compare(operator: Comparison, left: &Value, right: &Value) -> Result<Truth, Condition> {
    if matches!(left, Value::Missing) || matches!(right, Value::Missing) {
        return Ok(Truth::Unknown);
    }
    let ordering = compare_present(left, right).map_err(|_| Condition::IncompatiblePair {
        left: left.value_type().expect("present"),
        right: right.value_type().expect("present"),
    })?;
    Ok(Truth::from_bool(match operator {
        Comparison::Equal => ordering == Ordering::Equal,
        Comparison::NotEqual => ordering != Ordering::Equal,
        Comparison::Less => ordering == Ordering::Less,
        Comparison::LessEqual => ordering != Ordering::Greater,
        Comparison::Greater => ordering == Ordering::Greater,
        Comparison::GreaterEqual => ordering != Ordering::Less,
    }))
}

#[derive(Clone, Copy)]
enum Pattern {
    Any,
    One,
    Literal(char),
}

/// Full matching scalar by scalar, in time linear in the value and pattern.
/// Every pattern token is checked first, including a trailing escape.
fn like(value: &str, pattern: &str, escape: Option<char>) -> Result<Truth, Condition> {
    let mut tokens = Vec::new();
    let mut escaped = false;
    for character in pattern.chars() {
        if escaped {
            tokens.push(Pattern::Literal(character));
            escaped = false;
        } else if Some(character) == escape {
            escaped = true;
        } else {
            tokens.push(match character {
                '%' => Pattern::Any,
                '_' => Pattern::One,
                _ => Pattern::Literal(character),
            });
        }
    }
    if escaped {
        return Err(Condition::DanglingEscape);
    }
    let mut previous = vec![false; tokens.len() + 1];
    let mut next = vec![false; tokens.len() + 1];
    previous[0] = true;
    for (index, token) in tokens.iter().enumerate() {
        previous[index + 1] = matches!(token, Pattern::Any) && previous[index];
    }
    for character in value.chars() {
        next[0] = false;
        for (index, token) in tokens.iter().enumerate() {
            next[index + 1] = match token {
                Pattern::Any => next[index] || previous[index + 1],
                Pattern::One => previous[index],
                Pattern::Literal(literal) => *literal == character && previous[index],
            };
        }
        std::mem::swap(&mut previous, &mut next);
    }
    Ok(Truth::from_bool(previous[tokens.len()]))
}
