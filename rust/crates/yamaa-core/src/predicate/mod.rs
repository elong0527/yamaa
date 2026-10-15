//! Typed three-valued predicates, independent of syntax, tables and host runtimes.
//!
//! The immutable arena validates structure before resolution.
//! Evaluation preserves written operand occurrences, including BETWEEN's repeated
//! subject. A future compiler owns grammar, binding and literal validation.

use std::cmp::Ordering;
use std::vec;

use crate::limits::{LimitExceeded, Limits};
use crate::regex;
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
/// Literal regular expressions are compiled during admission, before resolution.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Contains {
        value: Scalar,
        pattern: String,
    },
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

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("predicate invalid root")]
    InvalidRoot,
    #[error("predicate invalid child")]
    InvalidChild,
    #[error("predicate empty identifier")]
    EmptyIdentifier,
    #[error("predicate empty membership")]
    EmptyMembership,
    #[error("predicate regex: node={node:?}, error={error:?}")]
    Regex {
        node: usize,
        error: regex::CompileError,
    },
    #[error("predicate limit: {0:?}")]
    Limit(LimitExceeded),
}

/// The owning scope resolves names; its failures remain opaque and move intact.
pub trait Resolver {
    type Error;
    /// Resolve this occurrence once, preserving absence versus present missing.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error>;

    /// Optionally lend a normalized cell until this operand is copied.
    /// Existing owning ports retain their original one-call behavior by default.
    fn resolve_value(&mut self, identifier: &str) -> Result<Resolved<'_>, Self::Error> {
        self.resolve(identifier).map(|selection| match selection {
            Selection::Absent => Resolved::Absent,
            Selection::Present(value) => Resolved::Owned(value),
        })
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

/// A borrow survives only until the evaluator has checked and copied this operand.
pub enum Resolved<'a> {
    Absent,
    Owned(Value),
    Borrowed(ValueRef<'a>),
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
    ContainsExpectedText { actual: ValueType },
    DanglingEscape,
}

impl Condition {
    /// Predicate binding/type/pattern failures retain the reference validation phase.
    pub fn phase(&self) -> &'static str {
        self.diagnostic_code().definition().phase
    }
    /// Stable language vocabulary, separate from resource or port failures.
    pub fn condition(&self) -> &'static str {
        self.diagnostic_code().definition().condition
    }
    /// Owning normative predicate requirement; no lifecycle handler applies here.
    pub fn requirement(&self) -> &'static str {
        self.diagnostic_code()
            .definition()
            .requirement
            .expect("predicate requirement")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind<E> {
    Condition(Condition),
    Resolution { identifier: String, error: E },
    Regex(regex::MatchError),
}

/// Original source context and structural route, without fabricating a source span.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "predicate: spec_path={spec_path:?}, expression={expression:?}, route={route:?}, kind={kind:?}"
)]
pub struct EvaluationError<E> {
    pub spec_path: String,
    pub expression: String,
    pub route: Vec<Route>,
    pub kind: ErrorKind<E>,
}

/// Own the validated arena and its compiled literal patterns.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    nodes: Vec<Node>,
    compiled_patterns: Vec<Option<regex::Pattern>>,
    root: usize,
    spec_path: String,
    expression: String,
}

impl Plan {
    /// Return the original owning site without exposing mutable plan state.
    pub fn expression(&self) -> &str {
        &self.expression
    }
    pub fn spec_path(&self) -> &str {
        &self.spec_path
    }

    /// Admit a caller-authored typed arena without data access or constant folding.
    /// This does not parse syntax or replace the eventual shared specification compiler.
    pub fn new(
        nodes: Vec<Node>,
        root: usize,
        spec_path: String,
        expression: String,
    ) -> Result<Self, PlanError> {
        if root >= nodes.len() {
            return Err(PlanError::InvalidRoot);
        }
        let mut depths: Vec<usize> = Vec::with_capacity(nodes.len());
        let mut compiled_patterns = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            let mut compiled_pattern = None;
            let mut depth = 1;
            let mut children = Vec::new();
            let mut scalars = Vec::new();
            let mut membership: &[Scalar] = &[];
            match node {
                Node::Contains { value, pattern } => {
                    scalars.push(value);
                    compiled_pattern = Some(
                        regex::Pattern::compile(pattern)
                            .map_err(|error| PlanError::Regex { node: index, error })?,
                    );
                }
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
                let child = depths.get(child).ok_or(PlanError::InvalidChild)?;
                depth = depth.max(child + 1);
            }
            for scalar in scalars.into_iter().chain(membership) {
                if let Scalar::Identifier(name) = scalar {
                    if name.is_empty() {
                        return Err(PlanError::EmptyIdentifier);
                    }
                }
            }
            Limits::semantic_depth(depth).map_err(PlanError::Limit)?;
            depths.push(depth);
            compiled_patterns.push(compiled_pattern);
        }
        Ok(Self {
            nodes,
            compiled_patterns,
            root,
            spec_path,
            expression,
        })
    }

    /// Evaluate afresh; reuse never memoizes values, failures or resource consumption.
    pub fn evaluate<R: Resolver>(
        &self,
        resolver: &mut R,
    ) -> Result<Truth, EvaluationError<R::Error>> {
        self.node(self.root, resolver, &mut Vec::new())
    }

    /// Enumerate all admitted identifier occurrences for whole-plan scope binding.
    /// Arena order is structural, not execution order; BETWEEN's subject appears twice.
    pub fn identifiers<'a>(&'a self) -> Vec<&'a str> {
        let mut result = Vec::new();
        let mut add = |scalar: &'a Scalar| {
            if let Scalar::Identifier(name) = scalar {
                result.push(name.as_str());
            }
        };
        for node in &self.nodes {
            match node {
                Node::Boolean(_) | Node::Not(_) | Node::And(..) | Node::Or(..) => {}
                Node::Compare { left, right, .. } => {
                    add(left);
                    add(right);
                }
                Node::IsNull { value, .. } | Node::Contains { value, .. } => add(value),
                Node::In { value, items, .. } => {
                    add(value);
                    for item in items {
                        add(item);
                    }
                }
                Node::Between {
                    value,
                    lower,
                    upper,
                    ..
                } => {
                    add(value);
                    add(lower);
                    add(value);
                    add(upper);
                }
                Node::Like { value, pattern, .. } => {
                    add(value);
                    add(pattern);
                }
            }
        }
        result
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
    ) -> Result<Truth, EvaluationError<R::Error>> {
        route.push(position);
        let result = self.node(index, resolver, route);
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
    ) -> Result<Value, EvaluationError<R::Error>> {
        let mut location = route.to_vec();
        location.push(position);
        match scalar {
            Scalar::Literal(value) => Ok(value.clone()),
            Scalar::Identifier(identifier) => match resolver.resolve_value(identifier) {
                Ok(Resolved::Owned(value)) => Ok(value),
                Ok(Resolved::Borrowed(value)) => Ok(own(value)),
                Ok(Resolved::Absent) => Err(self.error(
                    ErrorKind::Condition(Condition::UnknownField {
                        identifier: identifier.clone(),
                    }),
                    &location,
                )),
                Err(error) => Err(self.error(
                    ErrorKind::Resolution {
                        identifier: identifier.clone(),
                        error,
                    },
                    &location,
                )),
            },
        }
    }

    /// Keep eager written order; errors stop execution but truth values do not.
    fn node<R: Resolver>(
        &self,
        index: usize,
        resolver: &mut R,
        route: &mut Vec<Route>,
    ) -> Result<Truth, EvaluationError<R::Error>> {
        let result = match &self.nodes[index] {
            Node::Contains { value, .. } => {
                let value = self.scalar(value, Route::Value, resolver, route)?;
                match value {
                    Value::Missing => Ok(Truth::Unknown),
                    Value::Str(value) => {
                        let pattern = self.compiled_patterns[index]
                            .as_ref()
                            .expect("admitted pattern");
                        let matched = pattern
                            .search(&value)
                            .map_err(|error| self.error(ErrorKind::Regex(error), route))?;
                        Ok(Truth::from_bool(matched.is_some()))
                    }
                    other => Err(Condition::ContainsExpectedText {
                        actual: other.value_type().expect("missing handled above"),
                    }),
                }
            }
            Node::Boolean(value) => return Ok(Truth::from_bool(*value)),
            Node::Not(child) => {
                return self
                    .child(*child, Route::Child, resolver, route)
                    .map(Truth::negate)
            }
            Node::And(left, right) | Node::Or(left, right) => {
                let left = self.child(*left, Route::Left, resolver, route)?;
                let right = self.child(*right, Route::Right, resolver, route)?;
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
                let left = self.scalar(left, Route::Left, resolver, route)?;
                let right = self.scalar(right, Route::Right, resolver, route)?;
                compare(*operator, &left, &right)
            }
            Node::IsNull { value, negated } => {
                let value = self.scalar(value, Route::Value, resolver, route)?;
                Ok(Truth::from_bool(
                    matches!(value, Value::Missing) != *negated,
                ))
            }
            Node::In {
                value,
                items,
                negated,
            } => {
                let value = self.scalar(value, Route::Value, resolver, route)?;
                let mut truth = Truth::False;
                for (index, item) in items.iter().enumerate() {
                    let item = self.scalar(item, Route::Item(index), resolver, route)?;
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
                let first = self.scalar(value, Route::Value, resolver, route)?;
                let lower = self.scalar(lower, Route::Lower, resolver, route)?;
                let second = self.scalar(value, Route::RepeatedValue, resolver, route)?;
                let upper = self.scalar(upper, Route::Upper, resolver, route)?;
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
                let value = self.scalar(value, Route::Value, resolver, route)?;
                let pattern = self.scalar(pattern, Route::Pattern, resolver, route)?;
                let truth = if matches!(value, Value::Missing) || matches!(pattern, Value::Missing)
                {
                    Truth::Unknown
                } else if let (Value::Str(value), Value::Str(pattern)) = (&value, &pattern) {
                    like(value, pattern, *escape)
                        .map_err(|e| self.error(ErrorKind::Condition(e), route))?
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

/// Unicode scalar full matching with O(pattern) storage.
/// All tokens are checked before matching, including a trailing escaped character.
fn like(value: &str, pattern: &str, escape: Option<char>) -> Result<Truth, Condition> {
    // The per-match step boundary belongs to regex; LIKE has no separate work quota.
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

mod type_facts;
