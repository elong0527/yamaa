//! The binder: one check of a parsed expression against its context.
//!
//! A context refuses some constructs anywhere; those are found first, by
//! source position. Then one postorder walk checks every function and
//! reduction, and that truth values and plain values stand where each belongs.
//! Literals are decoded and regular-expression literals compiled once.

use super::syntax::{parse, NodeKind, Parsed, SourceSpan, TemporalKind};
use super::{
    CompileError, Construct, Context, Failure, Function, FunctionResult, Reducer, Resource,
};
use crate::portable_regex::{self, Pattern};
use crate::value::Value;

/// What a reduction reads from each record of its relation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReductionOperand {
    /// A bare name: its values keep their type for `MIN`, `MAX` and `ONLY`,
    /// and `STR_JOIN` writes each as its canonical text.
    Value(String),
    /// `COUNT(D.*)` counts the records of relation `D`.
    Records(String),
    /// Any other argument is a numeric expression evaluated per record.
    Numeric,
}

/// One reduction of an aggregate expression, in written order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduction {
    pub reducer: Reducer,
    /// The written text, such as `SUM(EX.EXDOSE)`, which diagnostics name.
    pub reference: String,
    pub operand: ReductionOperand,
    pub(super) node: usize,
    pub(super) operand_node: usize,
}

/// A bare reduction keeps the type it reduces; anything else is numeric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateResult {
    Reduction(usize),
    Numeric,
}

/// An admitted expression: its parse, context, decoded literals and compiled
/// patterns. Evaluation never reparses, folds or caches a value.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledExpression {
    pub(super) context: Context,
    pub(super) spec_path: String,
    pub(super) syntax: Parsed,
    pub(super) literals: Vec<Option<Value>>,
    pub(super) patterns: Vec<Option<Pattern>>,
    pub(super) reductions: Vec<Reduction>,
    pub(super) result: AggregateResult,
    /// The `NULLIF` nodes, in node order, whose result type is `float` where
    /// the expression is bound ([`CompiledExpression::typed`]).
    pub(super) widened: Vec<usize>,
    identifiers: Vec<String>,
    record_relations: Vec<String>,
    ungrouped: Vec<String>,
}

impl CompiledExpression {
    pub fn context(&self) -> Context {
        self.context
    }
    /// The original text, retained without normalization.
    pub fn expression(&self) -> &str {
        self.syntax.expression()
    }
    /// The specification path copied into every evaluation failure.
    pub fn spec_path(&self) -> &str {
        &self.spec_path
    }
    /// The admitted parse.
    pub fn syntax(&self) -> &Parsed {
        &self.syntax
    }
    /// Distinct case-sensitive names in first written occurrence order.
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(String::as_str)
    }
    /// Distinct `COUNT(D.*)` relations in written order.
    pub fn record_relations(&self) -> impl Iterator<Item = &str> {
        self.record_relations.iter().map(String::as_str)
    }
    /// Names outside every reduction, which an enclosing group must supply.
    pub fn ungrouped_identifiers(&self) -> impl Iterator<Item = &str> {
        self.ungrouped.iter().map(String::as_str)
    }
    pub fn reductions(&self) -> &[Reduction] {
        &self.reductions
    }
    pub fn result(&self) -> AggregateResult {
        self.result
    }
    pub(super) fn reduction_at(&self, node: usize) -> usize {
        self.reductions
            .iter()
            .position(|reduction| reduction.node == node)
            .expect("an admitted reduction node")
    }
}

/// Admit one text for its context before any name is resolved or value read.
pub fn compile(
    text: &str,
    context: Context,
    spec_path: &str,
) -> Result<CompiledExpression, CompileError> {
    let syntax = parse(text)?;
    let mut binder = Binder {
        syntax: &syntax,
        context,
        patterns: vec![None; syntax.nodes().len()],
    };
    binder.refused_constructs()?;
    binder.visit(
        syntax.root(),
        match context {
            Context::Predicate => Need::Truth,
            Context::Aggregate => Need::Number,
            Context::Derivation => Need::Value,
        },
    )?;
    let patterns = binder.patterns;
    let literals = syntax
        .nodes()
        .iter()
        .map(|node| literal(&node.kind))
        .collect();
    let mut reductions = Vec::new();
    let mut reduced = vec![false; syntax.nodes().len()];
    for (node, entry) in syntax.nodes().iter().enumerate() {
        let NodeKind::Reduction {
            reducer, operand, ..
        } = entry.kind
        else {
            continue;
        };
        mark(&syntax, operand, &mut reduced);
        let operand_node = syntax.ungroup(operand);
        let operand = match syntax.nodes()[operand_node].kind {
            NodeKind::Identifier => ReductionOperand::Value(syntax.text(operand_node).into()),
            NodeKind::Star => {
                let written = syntax.text(operand_node);
                ReductionOperand::Records(written[..written.len() - 2].into())
            }
            _ => ReductionOperand::Numeric,
        };
        reductions.push(Reduction {
            reducer,
            reference: syntax.text(node).into(),
            operand,
            node,
            operand_node,
        });
    }
    let root = syntax.ungroup(syntax.root());
    let result = reductions
        .iter()
        .position(|reduction| reduction.node == root)
        .map_or(AggregateResult::Numeric, AggregateResult::Reduction);
    let mut ungrouped: Vec<String> = Vec::new();
    for (node, entry) in syntax.nodes().iter().enumerate() {
        if entry.kind == NodeKind::Identifier && !reduced[node] {
            let name = syntax.text(node);
            if !ungrouped.iter().any(|seen| seen == name) {
                ungrouped.push(name.into());
            }
        }
    }
    Ok(CompiledExpression {
        context,
        spec_path: spec_path.into(),
        identifiers: syntax.identifiers().map(String::from).collect(),
        record_relations: syntax.star_datasets().map(String::from).collect(),
        ungrouped,
        literals,
        patterns,
        reductions,
        result,
        widened: Vec::new(),
        syntax,
    })
}

/// Mark a subtree as read inside a reduction.
fn mark(syntax: &Parsed, node: usize, reduced: &mut [bool]) {
    reduced[node] = true;
    for child in children(&syntax.nodes()[node].kind) {
        mark(syntax, child, reduced);
    }
}

/// The children of a node in written order.
pub(super) fn children(kind: &NodeKind) -> Vec<usize> {
    match kind {
        NodeKind::Number { .. }
        | NodeKind::String(_)
        | NodeKind::Temporal { .. }
        | NodeKind::Boolean(_)
        | NodeKind::Null
        | NodeKind::Identifier
        | NodeKind::Star => Vec::new(),
        NodeKind::Group(operand)
        | NodeKind::Not(operand)
        | NodeKind::Unary { operand, .. }
        | NodeKind::Reduction { operand, .. } => vec![*operand],
        NodeKind::IsNull { value, .. } => vec![*value],
        NodeKind::And(left, right)
        | NodeKind::Or(left, right)
        | NodeKind::Compare { left, right, .. }
        | NodeKind::Binary { left, right, .. } => vec![*left, *right],
        NodeKind::In { value, items, .. } => std::iter::once(*value)
            .chain(items.iter().copied())
            .collect(),
        NodeKind::Between {
            value,
            lower,
            upper,
            ..
        } => vec![*value, *lower, *upper],
        NodeKind::Like { value, pattern, .. } => vec![*value, *pattern],
        NodeKind::Call { arguments, .. } => arguments.clone(),
    }
}

/// Decode a literal once; evaluation copies the decoded value.
fn literal(kind: &NodeKind) -> Option<Value> {
    Some(match kind {
        NodeKind::Number {
            fractional: false,
            text,
        } => Value::Int(text.parse().expect("an admitted integer literal")),
        // An overflowing float literal follows the non-finite-to-missing rule.
        NodeKind::Number {
            fractional: true,
            text,
        } => Value::float(text.parse().expect("the number grammar")),
        NodeKind::String(text) => Value::Str(text.clone()),
        NodeKind::Temporal {
            kind: TemporalKind::Date,
            value,
        } => Value::Date(value.parse().expect("an admitted date literal")),
        NodeKind::Temporal {
            kind: TemporalKind::DateTime,
            value,
        } => Value::DateTime(value.parse().expect("an admitted datetime literal")),
        NodeKind::Null => Value::Missing,
        NodeKind::Boolean(value) => Value::Bool(*value),
        _ => return None,
    })
}

/// Whether a context refuses a construct wherever it is written.
fn refuses(context: Context, construct: Construct) -> bool {
    match context {
        Context::Predicate => false,
        Context::Aggregate => true,
        Context::Derivation => matches!(construct, Construct::Comparison | Construct::Boolean),
    }
}

/// What a position requires of the node written there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Need {
    /// An operand of `AND`, `OR` or `NOT`, or a predicate's root.
    Truth,
    /// A comparison or reduction operand, or an expression field's root, of
    /// any value type.
    Value,
    /// An operand of arithmetic or a numeric function, or a numeric root.
    Number,
}

struct Binder<'a> {
    syntax: &'a Parsed,
    context: Context,
    patterns: Vec<Option<Pattern>>,
}

impl Binder<'_> {
    fn invalid(&self, byte: usize, failure: Failure, message: impl Into<String>) -> CompileError {
        CompileError::Invalid {
            position: self.syntax.position(byte),
            failure,
            message: message.into(),
        }
    }

    /// `aggregate` refuses comparisons, Boolean logic and text or temporal
    /// literals wherever they are written, and an expression field refuses
    /// comparisons and Boolean logic; report the first one.
    fn refused_constructs(&self) -> Result<(), CompileError> {
        let refused = self
            .syntax
            .nodes()
            .iter()
            .filter_map(|node| {
                let construct = match node.kind {
                    NodeKind::Compare { .. }
                    | NodeKind::IsNull { .. }
                    | NodeKind::In { .. }
                    | NodeKind::Between { .. }
                    | NodeKind::Like { .. } => Construct::Comparison,
                    NodeKind::Not(_)
                    | NodeKind::And(..)
                    | NodeKind::Or(..)
                    | NodeKind::Boolean(_) => Construct::Boolean,
                    NodeKind::String(_) => Construct::String,
                    NodeKind::Temporal { .. } => Construct::Temporal,
                    _ => return None,
                };
                refuses(self.context, construct).then_some((node.site, construct))
            })
            .min_by_key(|(site, _)| *site);
        match refused {
            Some((site, construct)) => Err(self.invalid(
                site,
                Failure::ProhibitedConstruct { construct },
                format!("{} is not admitted here", construct.name()),
            )),
            None => Ok(()),
        }
    }

    /// Check children before their parent, so an inner fault is reported first.
    fn visit(&mut self, id: usize, need: Need) -> Result<(), CompileError> {
        let syntax = self.syntax;
        match &syntax.nodes()[id].kind {
            NodeKind::Group(inner) => self.visit(*inner, need),
            NodeKind::Number { .. }
            | NodeKind::String(_)
            | NodeKind::Temporal { .. }
            | NodeKind::Null
            | NodeKind::Identifier
            | NodeKind::Star => self.value(id, need),
            NodeKind::Boolean(_) => self.truth(id, need),
            NodeKind::Not(operand) => {
                self.visit(*operand, Need::Truth)?;
                self.truth(id, need)
            }
            NodeKind::And(left, right) | NodeKind::Or(left, right) => {
                self.visit(*left, Need::Truth)?;
                self.visit(*right, Need::Truth)?;
                self.truth(id, need)
            }
            NodeKind::Compare { .. }
            | NodeKind::IsNull { .. }
            | NodeKind::In { .. }
            | NodeKind::Between { .. }
            | NodeKind::Like { .. } => {
                for operand in children(&syntax.nodes()[id].kind) {
                    self.visit(operand, Need::Value)?;
                }
                self.truth(id, need)
            }
            NodeKind::Unary { operand, .. } => {
                self.visit(*operand, Need::Number)?;
                self.value(id, need)
            }
            NodeKind::Binary { left, right, .. } => {
                self.visit(*left, Need::Number)?;
                self.visit(*right, Need::Number)?;
                self.value(id, need)
            }
            NodeKind::Call { name, arguments } => self.call(id, *name, arguments, need),
            NodeKind::Reduction {
                reducer,
                name,
                operand,
            } => self.reduction(id, *reducer, *name, *operand, need),
        }
    }

    /// A value where a truth value belongs: an operator was expected after it.
    fn value(&self, id: usize, need: Need) -> Result<(), CompileError> {
        if need == Need::Truth {
            return Err(self.invalid(
                self.syntax.nodes()[id].follow,
                Failure::Syntax,
                "operand must be followed by a Boolean operator",
            ));
        }
        Ok(())
    }

    /// A truth value where a value belongs.
    fn truth(&self, id: usize, need: Need) -> Result<(), CompileError> {
        if need != Need::Truth {
            return Err(self.invalid(
                self.syntax.nodes()[id].site,
                Failure::Syntax,
                "a truth value is not an operand",
            ));
        }
        Ok(())
    }

    fn prohibited_function(&self, name: SourceSpan, argument_count: Option<usize>) -> CompileError {
        self.invalid(
            name.start,
            Failure::ProhibitedFunction {
                function: self.syntax.slice(name).into(),
                argument_count,
            },
            match argument_count {
                Some(_) => "the function takes another number of arguments",
                None => "the function is not admitted here",
            },
        )
    }

    fn call(
        &mut self,
        id: usize,
        name: SourceSpan,
        arguments: &[usize],
        need: Need,
    ) -> Result<(), CompileError> {
        // An unknown name, or a function its declaration does not admit in
        // this context, such as str_contains outside a predicate.
        let Some(function) = Function::lookup(self.syntax.slice(name))
            .filter(|function| function.contexts().contains(&self.context))
        else {
            for &argument in arguments {
                self.visit(argument, Need::Value)?;
            }
            return Err(self.prohibited_function(name, None));
        };
        match function.result() {
            // str_contains, the one truth-valued function: a value and a
            // pattern literal (FORMULA-8).
            FunctionResult::Boolean => {
                if !function.admits(arguments.len()) {
                    for &argument in arguments {
                        self.visit(argument, Need::Value)?;
                    }
                    return Err(self.prohibited_function(name, Some(arguments.len())));
                }
                self.visit(arguments[0], Need::Value)?;
                self.pattern(id, arguments[1])?;
                self.truth(id, need)
            }
            FunctionResult::Numeric => {
                for &argument in arguments {
                    self.visit(argument, Need::Number)?;
                }
                if !function.admits(arguments.len()) {
                    return Err(self.prohibited_function(name, Some(arguments.len())));
                }
                self.value(id, need)
            }
        }
    }

    /// A `str_contains` pattern is a string literal holding a portable regex,
    /// compiled once here.
    fn pattern(&mut self, call: usize, pattern: usize) -> Result<(), CompileError> {
        let node = &self.syntax.nodes()[pattern];
        let NodeKind::String(text) = &node.kind else {
            return Err(self.invalid(
                node.site,
                Failure::PatternNotLiteral,
                "str_contains pattern must be a string literal",
            ));
        };
        let site = node.span.start;
        let compiled = Pattern::compile(text).map_err(|error| match error {
            portable_regex::CompileError::Invalid { byte, reason } => self.invalid(
                site,
                Failure::InvalidRegex { byte, reason },
                format!("invalid regex in str_contains pattern: {reason}"),
            ),
            portable_regex::CompileError::Limit { resource, limit } => CompileError::Limit {
                position: self.syntax.position(site),
                resource: Resource::Regex(resource),
                limit,
            },
        })?;
        self.patterns[call] = Some(compiled);
        Ok(())
    }

    fn reduction(
        &mut self,
        id: usize,
        reducer: Reducer,
        name: SourceSpan,
        operand: usize,
        need: Need,
    ) -> Result<(), CompileError> {
        self.visit(operand, Need::Value)?;
        if self.context != Context::Aggregate {
            return Err(self.prohibited_function(name, None));
        }
        if let Some(inner) = self.first_reduction(operand) {
            return Err(self.invalid(
                name.start,
                Failure::NestedReduction {
                    outer: reducer,
                    inner,
                },
                "reductions do not nest",
            ));
        }
        self.value(id, need)
    }

    /// The first reduction in written order, searched depth-first.
    fn first_reduction(&self, node: usize) -> Option<Reducer> {
        let kind = &self.syntax.nodes()[node].kind;
        if let NodeKind::Reduction { reducer, .. } = kind {
            return Some(*reducer);
        }
        children(kind)
            .into_iter()
            .find_map(|child| self.first_reduction(child))
    }
}
