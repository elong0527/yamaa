//! Typed three-valued predicates, independent of syntax, tables and host runtimes.
//!
//! The immutable arena admits structure and expanded work before resolution.
//! Evaluation preserves written operand occurrences, including BETWEEN's repeated
//! subject. A future compiler owns grammar, binding and literal validation.

use alloc::{string::String, vec, vec::Vec};
use core::cmp::Ordering;

use crate::value::{compare_present, Selection, Value, ValueType};

/// Predicate truth is distinct from the runtime bool value and missing scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    /// Map an ordinary comparison without introducing an unknown result.
    fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }

    /// REQ-0171 negation preserves unknown truth.
    pub fn negate(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }

    /// Combine already evaluated operands; this is not a short-circuit evaluator.
    pub fn and(self, right: Self) -> Self {
        match (self, right) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::True,
        }
    }

    /// Combine already evaluated operands under the REQ-0171 OR table.
    pub fn or(self, right: Self) -> Self {
        match (self, right) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::False,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// A normalized literal or a deferred occurrence in the consuming site's scope.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Literal(Value),
    Identifier(String),
}

/// Child indexes refer to earlier arena entries. Scalar occurrences are not cached.
/// This typed subset deliberately has no portable-regex call variant yet.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Boolean(bool),
    Not(usize),
    And(usize, usize),
    Or(usize, usize),
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
        escape: Option<char>,
        negated: bool,
    },
}

/// Caller-selected policies, not restrictions on the language itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub nodes: usize,
    pub depth: usize,
    pub work: usize,
    pub resolutions: usize,
    pub text_bytes: usize,
    pub like_work: usize,
}

impl Default for Limits {
    /// Bound expansion and dynamic text work independently of input row counts.
    fn default() -> Self {
        Self {
            nodes: 4096,
            depth: 64,
            work: 16384,
            resolutions: 4096,
            text_bytes: 1_048_576,
            like_work: 1_048_576,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Nodes,
    Depth,
    Work,
    Resolutions,
    TextBytes,
    LikeWork,
}

/// Resource refusal is never predicate false, unknown or a language diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitError {
    pub resource: Resource,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    InvalidRoot,
    InvalidChild,
    EmptyIdentifier,
    EmptyMembership,
    Limit(LimitError),
}

/// The owning scope resolves names; its failures remain opaque and move intact.
pub trait Resolver {
    type Error;
    /// Resolve this occurrence once, preserving absence versus present missing.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error>;
}

/// Structural occurrence, including the second BETWEEN subject read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Child,
    Left,
    Right,
    Value,
    Item(usize),
    Lower,
    RepeatedValue,
    Upper,
    Pattern,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    UnknownField { identifier: String },
    IncompatiblePair { left: ValueType, right: ValueType },
    ExpectedText { actual: ValueType },
    DanglingEscape,
}

impl Condition {
    /// Predicate binding/type/pattern failures retain the reference validation phase.
    pub fn phase(&self) -> &'static str {
        "validation"
    }

    /// Stable language condition vocabulary, separate from resource or port failures.
    pub fn condition(&self) -> &'static str {
        match self {
            Self::UnknownField { .. } => "unknown_field",
            Self::IncompatiblePair { .. } | Self::ExpectedText { .. } => "incompatible_input_type",
            Self::DanglingEscape => "invalid_predicate",
        }
    }

    /// Owning normative predicate requirement; no lifecycle handler is applied here.
    pub fn requirement(&self) -> &'static str {
        match self {
            Self::UnknownField { .. } => "REQ-0189",
            Self::IncompatiblePair { .. } | Self::ExpectedText { .. } => "REQ-0190",
            Self::DanglingEscape => "REQ-0191",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind<E> {
    Condition(Condition),
    Resolution { identifier: String, error: E },
    Limit(LimitError),
}

/// Original source context and structural route, without fabricating a source span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationError<E> {
    pub spec_path: String,
    pub expression: String,
    pub route: Vec<Route>,
    pub kind: ErrorKind<E>,
}

#[derive(Clone, Copy)]
struct Cost {
    depth: usize,
    work: usize,
    resolutions: usize,
}

/// Own all admitted nodes so later caller mutations cannot bypass static limits.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    nodes: Vec<Node>,
    root: usize,
    spec_path: String,
    expression: String,
    limits: Limits,
}

/// Checked accounting avoids both counter wrapping and saturating a successful run.
fn charge(
    used: &mut usize,
    amount: usize,
    limit: usize,
    resource: Resource,
) -> Result<(), LimitError> {
    let next = used
        .checked_add(amount)
        .filter(|&next| next <= limit)
        .ok_or(LimitError { resource, limit })?;
    *used = next;
    Ok(())
}

impl Plan {
    /// Admit a caller-authored typed arena without data access or constant folding.
    /// This does not parse syntax or replace the eventual shared specification compiler.
    pub fn new(
        nodes: Vec<Node>,
        root: usize,
        spec_path: String,
        expression: String,
        limits: Limits,
    ) -> Result<Self, PlanError> {
        if root >= nodes.len() {
            return Err(PlanError::InvalidRoot);
        }
        let mut count = 0;
        charge(&mut count, nodes.len(), limits.nodes, Resource::Nodes).map_err(PlanError::Limit)?;
        let mut text = 0;
        for value in [&spec_path, &expression] {
            charge(
                &mut text,
                value.len(),
                limits.text_bytes,
                Resource::TextBytes,
            )
            .map_err(PlanError::Limit)?;
        }
        let mut costs: Vec<Cost> = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let mut cost = Cost {
                depth: 1,
                work: 1,
                resolutions: 0,
            };
            let mut children = Vec::new();
            let mut scalars = Vec::new();
            let mut membership: &[Scalar] = &[];
            match node {
                Node::Boolean(_) => {}
                Node::Not(child) => children.push(*child),
                Node::And(left, right) | Node::Or(left, right) => children.extend([*left, *right]),
                Node::Compare { left, right, .. } => scalars.extend([left, right]),
                Node::IsNull { value, .. } => scalars.push(value),
                Node::In { value, items, .. } => {
                    if items.is_empty() {
                        return Err(PlanError::EmptyMembership);
                    }
                    scalars.push(value);
                    membership = items;
                }
                Node::Between {
                    value,
                    lower,
                    upper,
                    ..
                } => scalars.extend([value, lower, value, upper]),
                Node::Like { value, pattern, .. } => scalars.extend([value, pattern]),
            }
            for child in children {
                let child = costs.get(child).ok_or(PlanError::InvalidChild)?;
                cost.depth = cost.depth.max(child.depth + 1);
                charge(&mut cost.work, child.work, limits.work, Resource::Work)
                    .map_err(PlanError::Limit)?;
                charge(
                    &mut cost.resolutions,
                    child.resolutions,
                    limits.resolutions,
                    Resource::Resolutions,
                )
                .map_err(PlanError::Limit)?;
            }
            for scalar in scalars.into_iter().chain(membership) {
                charge(&mut cost.work, 1, limits.work, Resource::Work).map_err(PlanError::Limit)?;
                let bytes = match scalar {
                    Scalar::Identifier(name) => {
                        if name.is_empty() {
                            return Err(PlanError::EmptyIdentifier);
                        }
                        charge(
                            &mut cost.resolutions,
                            1,
                            limits.resolutions,
                            Resource::Resolutions,
                        )
                        .map_err(PlanError::Limit)?;
                        name.len()
                    }
                    Scalar::Literal(Value::Str(value)) => value.len(),
                    _ => 0,
                };
                charge(&mut text, bytes, limits.text_bytes, Resource::TextBytes)
                    .map_err(PlanError::Limit)?;
            }
            let depth_limit = limits.depth.min(64);
            if cost.depth > depth_limit {
                return Err(PlanError::Limit(LimitError {
                    resource: Resource::Depth,
                    limit: depth_limit,
                }));
            }
            if cost.work > limits.work {
                return Err(PlanError::Limit(LimitError {
                    resource: Resource::Work,
                    limit: limits.work,
                }));
            }
            costs.push(cost);
        }
        Ok(Self {
            nodes,
            root,
            spec_path,
            expression,
            limits,
        })
    }

    /// Evaluate afresh; reuse never memoizes values, failures or resource consumption.
    pub fn evaluate<R: Resolver>(
        &self,
        resolver: &mut R,
    ) -> Result<Truth, EvaluationError<R::Error>> {
        self.node(self.root, resolver, &mut Vec::new(), &mut Budget::default())
    }

    /// Copy only portable provenance while moving an opaque resolver error unchanged.
    fn error<E>(&self, kind: ErrorKind<E>, route: &[Route]) -> EvaluationError<E> {
        EvaluationError {
            spec_path: self.spec_path.clone(),
            expression: self.expression.clone(),
            route: route.into(),
            kind,
        }
    }

    /// Visit a Boolean child and restore its sibling's structural location.
    fn child<R: Resolver>(
        &self,
        index: usize,
        position: Route,
        resolver: &mut R,
        route: &mut Vec<Route>,
        budget: &mut Budget,
    ) -> Result<Truth, EvaluationError<R::Error>> {
        route.push(position);
        let result = self.node(index, resolver, route, budget);
        route.pop();
        result
    }

    /// Resolve each scalar occurrence and bound text before local cloning/processing.
    fn scalar<R: Resolver>(
        &self,
        scalar: &Scalar,
        position: Route,
        resolver: &mut R,
        route: &[Route],
        budget: &mut Budget,
    ) -> Result<Value, EvaluationError<R::Error>> {
        let mut location = route.to_vec();
        location.push(position);
        let value = match scalar {
            Scalar::Literal(value) => {
                if let Value::Str(text) = value {
                    charge(
                        &mut budget.text,
                        text.len(),
                        self.limits.text_bytes,
                        Resource::TextBytes,
                    )
                    .map_err(|e| self.error(ErrorKind::Limit(e), &location))?;
                }
                return Ok(value.clone());
            }
            Scalar::Identifier(identifier) => {
                match resolver.resolve(identifier).map_err(|error| {
                    self.error(
                        ErrorKind::Resolution {
                            identifier: identifier.clone(),
                            error,
                        },
                        &location,
                    )
                })? {
                    Selection::Absent => {
                        return Err(self.error(
                            ErrorKind::Condition(Condition::UnknownField {
                                identifier: identifier.clone(),
                            }),
                            &location,
                        ))
                    }
                    Selection::Present(value) => value,
                }
            }
        };
        if let Value::Str(text) = &value {
            charge(
                &mut budget.text,
                text.len(),
                self.limits.text_bytes,
                Resource::TextBytes,
            )
            .map_err(|e| self.error(ErrorKind::Limit(e), &location))?;
        }
        Ok(value)
    }

    /// Keep eager written order; errors stop execution but truth values do not.
    fn node<R: Resolver>(
        &self,
        index: usize,
        resolver: &mut R,
        route: &mut Vec<Route>,
        budget: &mut Budget,
    ) -> Result<Truth, EvaluationError<R::Error>> {
        let result = match &self.nodes[index] {
            Node::Boolean(value) => return Ok(Truth::from_bool(*value)),
            Node::Not(child) => {
                return self
                    .child(*child, Route::Child, resolver, route, budget)
                    .map(Truth::negate)
            }
            Node::And(left, right) | Node::Or(left, right) => {
                let left = self.child(*left, Route::Left, resolver, route, budget)?;
                let right = self.child(*right, Route::Right, resolver, route, budget)?;
                return Ok(if matches!(self.nodes[index], Node::And(..)) {
                    left.and(right)
                } else {
                    left.or(right)
                });
            }
            Node::Compare {
                operator,
                left,
                right,
            } => {
                let left = self.scalar(left, Route::Left, resolver, route, budget)?;
                let right = self.scalar(right, Route::Right, resolver, route, budget)?;
                compare(*operator, &left, &right)
            }
            Node::IsNull { value, negated } => {
                let value = self.scalar(value, Route::Value, resolver, route, budget)?;
                Ok(Truth::from_bool(
                    matches!(value, Value::Missing) != *negated,
                ))
            }
            Node::In {
                value,
                items,
                negated,
            } => {
                let value = self.scalar(value, Route::Value, resolver, route, budget)?;
                let mut truth = Truth::False;
                for (index, item) in items.iter().enumerate() {
                    let item = self.scalar(item, Route::Item(index), resolver, route, budget)?;
                    truth = truth.or(compare(Comparison::Equal, &value, &item)
                        .map_err(|e| self.error(ErrorKind::Condition(e), route))?);
                }
                Ok(if *negated { truth.negate() } else { truth })
            }
            Node::Between {
                value,
                lower,
                upper,
                negated,
            } => {
                let first = self.scalar(value, Route::Value, resolver, route, budget)?;
                let lower = self.scalar(lower, Route::Lower, resolver, route, budget)?;
                let second = self.scalar(value, Route::RepeatedValue, resolver, route, budget)?;
                let upper = self.scalar(upper, Route::Upper, resolver, route, budget)?;
                // Resolve all four occurrences before either comparison. A failure
                // resolving the upper endpoint outranks an incompatible lower pair.
                let truth = compare(Comparison::GreaterEqual, &first, &lower).and_then(|left| {
                    compare(Comparison::LessEqual, &second, &upper).map(|right| left.and(right))
                });
                truth.map(|truth| if *negated { truth.negate() } else { truth })
            }
            Node::Like {
                value,
                pattern,
                escape,
                negated,
            } => {
                let value = self.scalar(value, Route::Value, resolver, route, budget)?;
                let pattern = self.scalar(pattern, Route::Pattern, resolver, route, budget)?;
                let truth = if matches!(value, Value::Missing) || matches!(pattern, Value::Missing)
                {
                    Truth::Unknown
                } else if let (Value::Str(value), Value::Str(pattern)) = (&value, &pattern) {
                    like(
                        value,
                        pattern,
                        *escape,
                        &mut budget.like_work,
                        self.limits.like_work,
                    )
                    .map_err(|e| self.error(e, route))?
                } else {
                    let actual = if !matches!(value, Value::Str(_)) {
                        value.value_type()
                    } else {
                        pattern.value_type()
                    };
                    return Err(self.error(
                        ErrorKind::Condition(Condition::ExpectedText {
                            actual: actual.expect("missing handled above"),
                        }),
                        route,
                    ));
                };
                Ok(if *negated { truth.negate() } else { truth })
            }
        };
        result.map_err(|e| self.error(ErrorKind::Condition(e), route))
    }
}

#[derive(Default)]
struct Budget {
    text: usize,
    like_work: usize,
}

/// REQ-0167 widens mixed numbers, unlike the exact comparator used by grouping.
fn compare(operator: Comparison, left: &Value, right: &Value) -> Result<Truth, Condition> {
    if matches!(left, Value::Missing) || matches!(right, Value::Missing) {
        return Ok(Truth::Unknown);
    }
    let ordering = match (left, right) {
        (Value::Int(left), Value::Float(right)) => {
            (*left as f64).partial_cmp(&right.get()).expect("finite")
        }
        (Value::Float(left), Value::Int(right)) => {
            left.get().partial_cmp(&(*right as f64)).expect("finite")
        }
        _ => compare_present(left, right).map_err(|_| Condition::IncompatiblePair {
            left: left.value_type().expect("present"),
            right: right.value_type().expect("present"),
        })?,
    };
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

/// Unicode scalar full matching with O(pattern) storage and explicit work policy.
/// All tokens are checked before matching, including a trailing escaped character.
fn like<E>(
    value: &str,
    pattern: &str,
    escape: Option<char>,
    used: &mut usize,
    limit: usize,
) -> Result<Truth, ErrorKind<E>> {
    let mut tokens = Vec::new();
    let mut escaped = false;
    for character in pattern.chars() {
        charge(used, 1, limit, Resource::LikeWork).map_err(ErrorKind::Limit)?;
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
        return Err(ErrorKind::Condition(Condition::DanglingEscape));
    }
    // Charge row initialization as well as every source scalar and DP cell.
    charge(used, tokens.len() + 1, limit, Resource::LikeWork).map_err(ErrorKind::Limit)?;
    let mut previous = vec![false; tokens.len() + 1];
    let mut next = vec![false; tokens.len() + 1];
    previous[0] = true;
    for (index, token) in tokens.iter().enumerate() {
        previous[index + 1] = matches!(token, Pattern::Any) && previous[index];
    }
    for character in value.chars() {
        charge(used, tokens.len() + 1, limit, Resource::LikeWork).map_err(ErrorKind::Limit)?;
        next[0] = false;
        for (index, token) in tokens.iter().enumerate() {
            next[index + 1] = match token {
                Pattern::Any => next[index] || previous[index + 1],
                Pattern::One => previous[index],
                Pattern::Literal(literal) => *literal == character && previous[index],
            };
        }
        core::mem::swap(&mut previous, &mut next);
    }
    Ok(Truth::from_bool(previous[tokens.len()]))
}
