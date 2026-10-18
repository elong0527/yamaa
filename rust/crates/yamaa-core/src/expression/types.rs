//! Static operand types: the type facts a site checks against its names before
//! any value is read, and the result type `compute` and `aggregate` declare.

use super::bind::{children, AggregateResult, CompiledExpression, ReductionOperand};
use super::evaluate::Step;
use super::syntax::{NodeKind, TemporalKind};
use super::{Context, Function, Reducer};
use crate::numeric::BinaryOperator;
use crate::reduction::NumericReducer;
use crate::result_type::ResultType;
use crate::value::{comparable_types, ValueType};

/// A static type fault, found from the site's declared types alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeFinding {
    /// An operand of arithmetic, of a numeric function or of a numeric root
    /// that is not numeric, named by its written text.
    NumericOperand { source: String, actual: ValueType },
    /// Two comparison operands whose types are not mutually comparable.
    IncompatiblePair {
        left: ValueType,
        right: ValueType,
        route: Vec<Step>,
    },
    /// A non-text `LIKE` operand.
    ExpectedText { actual: ValueType, route: Vec<Step> },
    /// A non-text `str_contains` source.
    ContainsExpectedText { actual: ValueType, route: Vec<Step> },
    /// `SUM` or `MEAN` over a bare name that is not numeric.
    ReductionInput {
        reducer: NumericReducer,
        source: String,
        actual: ValueType,
    },
}

fn numeric(kind: ValueType) -> bool {
    matches!(kind, ValueType::Int | ValueType::Float)
}

/// An arithmetic operand's possible numbers. A name of no numeric type is a
/// fault its site reports; its arithmetic still yields a number.
fn number(kind: ResultType) -> ResultType {
    match kind.numeric() {
        numbers if numbers.is_missing() && !kind.is_missing() => ResultType::NUMBER,
        numbers => numbers,
    }
}

/// VALUE-10's `+`, `-`, `*`, `ABS` and `MOD`: `int` for `int` operands,
/// `float` when either is a `float`, and missing when either always is. An
/// operand of several types gives the result for each of them.
fn promote(left: ResultType, right: ResultType) -> ResultType {
    if left.is_missing() || right.is_missing() {
        return ResultType::MISSING;
    }
    let int = left.contains(ValueType::Int) && right.contains(ValueType::Int);
    let float = left.contains(ValueType::Float) || right.contains(ValueType::Float);
    match (int, float) {
        (true, true) => ResultType::NUMBER,
        (true, false) => ResultType::INT,
        _ => ResultType::FLOAT,
    }
}

impl CompiledExpression {
    /// The result type of `compute` and `aggregate` (EXPR-14) from the types
    /// `lookup` gives each name: VALUE-10 for operators and functions and
    /// FORMULA-23 for reductions. `/` and the real-valued functions are
    /// `float`, and `NULLIF` is a `float` when either argument is.
    pub fn result_type(&self, lookup: impl Fn(&str) -> ResultType) -> ResultType {
        self.infer(self.syntax.root(), &lookup)
    }

    /// The expression as the site that binds it types each name: every
    /// `NULLIF` whose result type is `float` widens the `int` it returns
    /// under VALUE-8, also when its `float` argument is missing.
    pub fn typed(&self, lookup: impl Fn(&str) -> ResultType) -> Self {
        let mut typed = self.clone();
        typed.widened = (0..self.syntax.nodes().len())
            .filter(|&id| {
                matches!(
                    &self.syntax.nodes()[id].kind,
                    NodeKind::Call { name, .. }
                        if Function::lookup(self.syntax.slice(*name)) == Some(Function::NullIf)
                ) && self.infer(id, &lookup) == ResultType::FLOAT
            })
            .collect();
        typed
    }

    fn infer(&self, id: usize, lookup: &impl Fn(&str) -> ResultType) -> ResultType {
        match &self.syntax.nodes()[id].kind {
            NodeKind::Number { fractional, .. } => {
                if *fractional {
                    ResultType::FLOAT
                } else {
                    ResultType::INT
                }
            }
            NodeKind::Null => ResultType::MISSING,
            NodeKind::Identifier => number(lookup(self.syntax.text(id))),
            NodeKind::Group(operand) | NodeKind::Unary { operand, .. } => {
                self.infer(*operand, lookup)
            }
            NodeKind::Binary {
                operator,
                left,
                right,
            } => {
                let (left, right) = (self.infer(*left, lookup), self.infer(*right, lookup));
                match operator {
                    BinaryOperator::Divide if left.is_missing() || right.is_missing() => {
                        ResultType::MISSING
                    }
                    BinaryOperator::Divide => ResultType::FLOAT,
                    _ => promote(left, right),
                }
            }
            NodeKind::Call { name, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|&argument| self.infer(argument, lookup))
                    .collect::<Vec<_>>();
                match Function::lookup(self.syntax.slice(*name)) {
                    Some(Function::NullIf) => {
                        let (left, right) = (arguments[0], arguments[1]);
                        if right.is_missing() {
                            left
                        } else {
                            promote(left, right)
                        }
                    }
                    Some(Function::Abs) => arguments[0],
                    Some(Function::Mod) => promote(arguments[0], arguments[1]),
                    _ if arguments.iter().any(|argument| argument.is_missing()) => {
                        ResultType::MISSING
                    }
                    _ => ResultType::FLOAT,
                }
            }
            NodeKind::Reduction { reducer, .. } => {
                let reduction = &self.reductions[self.reduction_at(id)];
                let reduced = match &reduction.operand {
                    ReductionOperand::Value(name) => lookup(name),
                    ReductionOperand::Records(_) => ResultType::UNKNOWN,
                    ReductionOperand::Numeric => self.infer(reduction.operand_node, lookup),
                };
                match reducer {
                    Reducer::Count => ResultType::INT,
                    Reducer::Mean => ResultType::FLOAT,
                    Reducer::Sum => number(reduced),
                    Reducer::Min | Reducer::Max | Reducer::Only => reduced,
                }
            }
            _ => ResultType::UNKNOWN,
        }
    }

    /// Every static type fault, in the order a site reports them. `kind`
    /// gives a name's declared type, or None when it is not known. With
    /// `collected_sources`, a qualified name a grouped reduction collects is
    /// checked on its present values at run time instead.
    pub fn type_findings(
        &self,
        kind: impl Fn(&str) -> Option<ValueType>,
        collected_sources: bool,
    ) -> Vec<TypeFinding> {
        let mut findings = Vec::new();
        match self.context {
            Context::Compute => {
                for name in self.identifiers() {
                    self.numeric_name(name, &kind, &mut findings);
                }
            }
            Context::Aggregate => {
                for reduction in &self.reductions {
                    match &reduction.operand {
                        ReductionOperand::Numeric => {
                            let mut names = Vec::new();
                            self.names(reduction.operand_node, &mut names);
                            for name in names {
                                self.numeric_name(name, &kind, &mut findings);
                            }
                        }
                        ReductionOperand::Value(name) => {
                            let reducer = match reduction.reducer {
                                Reducer::Sum => NumericReducer::Sum,
                                Reducer::Mean => NumericReducer::Mean,
                                _ => continue,
                            };
                            if collected_sources && name.contains('.') {
                                continue;
                            }
                            if let Some(actual) = kind(name).filter(|actual| !numeric(*actual)) {
                                findings.push(TypeFinding::ReductionInput {
                                    reducer,
                                    source: name.clone(),
                                    actual,
                                });
                            }
                        }
                        ReductionOperand::Records(_) => {}
                    }
                }
                if self.result == AggregateResult::Numeric {
                    for name in self.ungrouped_identifiers() {
                        self.numeric_name(name, &kind, &mut findings);
                    }
                }
            }
            Context::Predicate => {
                let mut checker = Checker {
                    compiled: self,
                    kind: &kind,
                    findings: &mut findings,
                    seen: Vec::new(),
                    route: Vec::new(),
                };
                checker.truth(self.syntax.root());
            }
        }
        findings
    }

    fn numeric_name(
        &self,
        name: &str,
        kind: &impl Fn(&str) -> Option<ValueType>,
        findings: &mut Vec<TypeFinding>,
    ) {
        if let Some(actual) = kind(name).filter(|actual| !numeric(*actual)) {
            findings.push(TypeFinding::NumericOperand {
                source: name.into(),
                actual,
            });
        }
    }

    /// Distinct names of a subtree in written order.
    fn names<'a>(&'a self, node: usize, names: &mut Vec<&'a str>) {
        let kind = &self.syntax.nodes()[node].kind;
        if *kind == NodeKind::Identifier {
            let name = self.syntax.text(node);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for child in children(kind) {
            self.names(child, names);
        }
    }
}

/// The predicate type facts: numeric operands, comparable pairs and text.
struct Checker<'a, K> {
    compiled: &'a CompiledExpression,
    kind: &'a K,
    findings: &'a mut Vec<TypeFinding>,
    seen: Vec<String>,
    route: Vec<Step>,
}

impl<K: Fn(&str) -> Option<ValueType>> Checker<'_, K> {
    fn node(&self, id: usize) -> &NodeKind {
        &self.compiled.syntax.nodes()[id].kind
    }

    fn truth(&mut self, id: usize) {
        match self.node(id).clone() {
            NodeKind::Group(inner) => self.truth(inner),
            NodeKind::Not(operand) => self.nested(Step::Child, operand),
            NodeKind::And(left, right) | NodeKind::Or(left, right) => {
                self.nested(Step::Left, left);
                self.nested(Step::Right, right);
            }
            NodeKind::Compare { left, right, .. } => {
                self.operands(&[left, right]);
                self.pair(left, right);
            }
            NodeKind::IsNull { value, .. } => self.operands(&[value]),
            NodeKind::In { value, items, .. } => {
                self.operands(&[value]);
                self.operands(&items);
                for item in items {
                    self.pair(value, item);
                }
            }
            NodeKind::Between {
                value,
                lower,
                upper,
                ..
            } => {
                self.operands(&[value, lower, upper]);
                self.pair(value, lower);
                self.pair(value, upper);
            }
            NodeKind::Like { value, pattern, .. } => {
                self.operands(&[value, pattern]);
                for operand in [value, pattern] {
                    if let Some(actual) = self.operand_type(operand) {
                        if actual != ValueType::Str {
                            self.findings.push(TypeFinding::ExpectedText {
                                actual,
                                route: self.route.clone(),
                            });
                        }
                    }
                }
            }
            NodeKind::Call { arguments, .. } => {
                self.operands(&arguments[..1]);
                if let Some(actual) = self.operand_type(arguments[0]) {
                    if actual != ValueType::Str {
                        self.findings.push(TypeFinding::ContainsExpectedText {
                            actual,
                            route: self.route.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn nested(&mut self, step: Step, id: usize) {
        self.route.push(step);
        self.truth(id);
        self.route.pop();
    }

    /// The numeric operands inside comparison operands.
    fn operands(&mut self, operands: &[usize]) {
        for &operand in operands {
            self.numeric_leaves(operand, false);
        }
    }

    /// Report each non-numeric operand of arithmetic once, in written order.
    fn numeric_leaves(&mut self, id: usize, numeric_position: bool) {
        let kind = self.node(id).clone();
        let actual = match &kind {
            NodeKind::Group(inner) => return self.numeric_leaves(*inner, numeric_position),
            NodeKind::Identifier => (self.kind)(self.compiled.syntax.text(id)),
            NodeKind::String(_) => Some(ValueType::Str),
            NodeKind::Temporal {
                kind: TemporalKind::Date,
                ..
            } => Some(ValueType::Date),
            NodeKind::Temporal {
                kind: TemporalKind::DateTime,
                ..
            } => Some(ValueType::DateTime),
            NodeKind::Unary { .. } | NodeKind::Binary { .. } | NodeKind::Call { .. } => {
                for child in children(&kind) {
                    self.numeric_leaves(child, true);
                }
                return;
            }
            _ => return,
        };
        if let Some(actual) = actual.filter(|actual| numeric_position && !numeric(*actual)) {
            let source = self.compiled.syntax.text(id).to_string();
            if !self.seen.contains(&source) {
                self.seen.push(source.clone());
                self.findings
                    .push(TypeFinding::NumericOperand { source, actual });
            }
        }
    }

    /// A comparison operand's static type; arithmetic over a faulty operand
    /// has none, so one fault is reported once.
    fn operand_type(&self, id: usize) -> Option<ValueType> {
        match self.node(id) {
            NodeKind::Group(inner) => self.operand_type(*inner),
            // A decoded literal: an overflowing float literal is missing.
            NodeKind::Number { .. } | NodeKind::String(_) | NodeKind::Temporal { .. } => {
                self.compiled.literals[id]
                    .as_ref()
                    .and_then(|value| value.value_type())
            }
            NodeKind::Identifier => (self.kind)(self.compiled.syntax.text(id)),
            NodeKind::Unary { .. } | NodeKind::Binary { .. } | NodeKind::Call { .. } => self
                .compiled
                .infer(id, &|name| ResultType::known((self.kind)(name)))
                .single()
                .filter(|kind| numeric(*kind))
                .filter(|_| !self.faulty(id)),
            _ => None,
        }
    }

    /// Whether arithmetic has an operand of a known non-numeric type.
    fn faulty(&self, id: usize) -> bool {
        match self.node(id) {
            NodeKind::Identifier => {
                (self.kind)(self.compiled.syntax.text(id)).is_some_and(|kind| !numeric(kind))
            }
            NodeKind::String(_) | NodeKind::Temporal { .. } => true,
            kind => children(kind).into_iter().any(|child| self.faulty(child)),
        }
    }

    fn pair(&mut self, left: usize, right: usize) {
        if let (Some(left), Some(right)) = (self.operand_type(left), self.operand_type(right)) {
            if !comparable_types(left, right) {
                self.findings.push(TypeFinding::IncompatiblePair {
                    left,
                    right,
                    route: self.route.clone(),
                });
            }
        }
    }
}
