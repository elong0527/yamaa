//! Compile bounded numeric syntax into the implemented scalar evaluator subset.
//!
//! Unsupported functions are rejected before a plan can resolve any identifiers.
//! Literal overflow is deferred until evaluation reaches it in written order.
//! Compiled trees are private, so parse and resolution budgets cannot be bypassed
//! by mutation. Budgets bound structure and call counts, not resolver running time.

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::evaluation::{EvaluationError, NumericNode, NumericPlan, NumericResolver, Operand};
use crate::numeric::{BinaryOperator, IntegralFunction, Number, SelectionFunction, UnaryOperator};
use crate::numeric_parser::{
    parse_numeric, NumericFunction, ParseError, ParseLimits, ParsedKind, ParsedNumeric, SourceSpan,
};

/// Parse budgets also bound compilation/evaluation work; every identifier
/// occurrence consumes one resolution, including repeated names. The static
/// maximum is checked before evaluation, even if an earlier operand would fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileLimits {
    pub parse: ParseLimits,
    pub resolutions: usize,
}

impl Default for CompileLimits {
    /// Use the bounded parser defaults and at most 4,096 resolver calls per run.
    fn default() -> Self {
        Self {
            parse: ParseLimits::default(),
            resolutions: 4096,
        }
    }
}

/// A valid function not yet executable by this subset, in original source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedFunction {
    pub function: NumericFunction,
    /// Exact written function name, excluding its arguments and parentheses.
    pub name: SourceSpan,
}

/// Distinguish invalid syntax, parser policy, unsupported features and call policy.
/// None of these outcomes runs a resolver or selects a language lifecycle handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    Parse(ParseError),
    Unsupported { functions: Vec<UnsupportedFunction> },
    ResolutionLimit { limit: usize, required: usize },
}

/// Evaluator failure plus the exact source extent of the failed semantic node.
/// Group parentheses do not replace an inner node's span. An operator/call failure
/// spans the whole expression/call; a leaf failure spans only its written token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledEvaluationError<E> {
    pub source_span: SourceSpan,
    pub evaluation: Box<EvaluationError<E>>,
}

/// Immutable executable plan for arithmetic, ABS, MOD, NULLIF, COALESCE and
/// numeric extrema, CEIL, FLOOR, TRUNC and SQRT. Reusing a plan repeats resolution;
/// no values or errors are cached.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledNumeric {
    plan: NumericPlan,
    sources: Vec<(Vec<Operand>, SourceSpan)>,
    resolutions: usize,
}

impl CompiledNumeric {
    /// Original source text, retained without normalization or reassociation.
    pub fn expression(&self) -> &str {
        &self.plan.expression
    }

    /// Specification provenance copied into every evaluation failure.
    pub fn spec_path(&self) -> &str {
        &self.plan.spec_path
    }

    /// Static maximum resolver calls for one evaluation, not distinct name count.
    pub fn resolution_count(&self) -> usize {
        self.resolutions
    }

    /// Evaluate within the compiled structural bounds and attach the failed span.
    /// Opaque resolver errors move intact through the wrapper, without a Clone bound.
    pub fn evaluate<R: NumericResolver>(
        &self,
        resolver: &mut R,
    ) -> Result<Number, CompiledEvaluationError<R::Error>> {
        self.plan.evaluate(resolver).map_err(|evaluation| {
            let source_span = self
                .sources
                .iter()
                .find(|(path, _)| *path == evaluation.location.operands)
                .expect("every compiled semantic node has a source span")
                .1;
            CompiledEvaluationError {
                source_span,
                evaluation: Box::new(evaluation),
            }
        })
    }
}

/// Parse and preflight the whole expression, then lower only supported syntax.
/// Parser failures take priority; unsupported calls precede the resolution policy.
/// No arithmetic folding, name binding, resolver invocation or handler accounting
/// occurs here. Node/token/byte limits and the hard depth ceiling come from parsing.
pub fn compile_numeric(
    text: &str,
    spec_path: &str,
    limits: CompileLimits,
) -> Result<CompiledNumeric, CompileError> {
    let parsed = parse_numeric(text, limits.parse).map_err(CompileError::Parse)?;
    let mut functions = Vec::new();
    let mut resolutions = 0;
    for node in parsed.nodes() {
        match &node.kind {
            ParsedKind::Call { function, name, .. }
                if !matches!(
                    function,
                    NumericFunction::Abs
                        | NumericFunction::Sqrt
                        | NumericFunction::Ceil
                        | NumericFunction::Floor
                        | NumericFunction::Trunc
                        | NumericFunction::Mod
                        | NumericFunction::Greatest
                        | NumericFunction::Least
                        | NumericFunction::NullIf
                        | NumericFunction::Coalesce
                ) =>
            {
                functions.push(UnsupportedFunction {
                    function: *function,
                    name: *name,
                });
            }
            ParsedKind::Identifier => resolutions += 1,
            _ => {}
        }
    }
    if !functions.is_empty() {
        functions.sort_by_key(|function| function.name.start);
        return Err(CompileError::Unsupported { functions });
    }
    if resolutions > limits.resolutions {
        return Err(CompileError::ResolutionLimit {
            limit: limits.resolutions,
            required: resolutions,
        });
    }
    let mut sources = Vec::new();
    let root = lower(&parsed, parsed.root(), &mut Vec::new(), &mut sources);
    Ok(CompiledNumeric {
        plan: NumericPlan {
            spec_path: spec_path.into(),
            expression: text.into(),
            root,
        },
        sources,
        resolutions,
    })
}

/// Convert unsigned validated digits without arbitrary precision or float narrowing.
/// An out-of-range spelling becomes a deferred failure, including before unary '-'.
fn integer_literal(written: &str) -> NumericNode {
    let digits = written.trim_start_matches('0');
    if digits.is_empty() {
        return NumericNode::Literal(Number::Int(0));
    }
    if digits.len() <= 19 {
        if let Ok(value) = digits.parse::<i64>() {
            return NumericNode::Literal(Number::Int(value));
        }
    }
    NumericNode::IntegerOverflowLiteral(String::from(digits))
}

/// Lower a child while recording the evaluator's unchanged structural operand path.
fn child(
    parsed: &ParsedNumeric,
    id: usize,
    operand: Operand,
    route: &mut Vec<Operand>,
    sources: &mut Vec<(Vec<Operand>, SourceSpan)>,
) -> Box<NumericNode> {
    route.push(operand);
    let node = lower(parsed, id, route, sources);
    route.pop();
    Box::new(node)
}

/// Lower one bounded arena node, retaining groups only in the original source.
fn lower(
    parsed: &ParsedNumeric,
    id: usize,
    route: &mut Vec<Operand>,
    sources: &mut Vec<(Vec<Operand>, SourceSpan)>,
) -> NumericNode {
    let node = &parsed.nodes()[id];
    if let ParsedKind::Group { operand } = node.kind {
        return lower(parsed, operand, route, sources);
    }
    sources.push((route.clone(), node.span));
    let written = &parsed.expression()[node.span.start..node.span.end];
    match &node.kind {
        ParsedKind::Number { fractional: false } => integer_literal(written),
        ParsedKind::Number { fractional: true } => NumericNode::Literal(Number::float(
            written.parse().expect("validated numeric float grammar"),
        )),
        ParsedKind::Null => NumericNode::Literal(Number::Missing),
        ParsedKind::Identifier => NumericNode::Identifier(written.into()),
        ParsedKind::Unary { operator, operand } => NumericNode::Unary {
            operator: *operator,
            operand: child(parsed, *operand, Operand::Unary, route, sources),
        },
        ParsedKind::Binary {
            operator,
            left,
            right,
        } => NumericNode::Binary {
            operator: *operator,
            left: child(parsed, *left, Operand::Left, route, sources),
            right: child(parsed, *right, Operand::Right, route, sources),
        },
        ParsedKind::Call {
            function: NumericFunction::Abs,
            arguments,
            ..
        } => NumericNode::Unary {
            operator: UnaryOperator::Abs,
            operand: child(parsed, arguments[0], Operand::Unary, route, sources),
        },
        ParsedKind::Call {
            function:
                function @ (NumericFunction::Ceil | NumericFunction::Floor | NumericFunction::Trunc),
            arguments,
            ..
        } => NumericNode::Integral {
            function: match function {
                NumericFunction::Ceil => IntegralFunction::Ceil,
                NumericFunction::Floor => IntegralFunction::Floor,
                _ => IntegralFunction::Trunc,
            },
            operand: child(parsed, arguments[0], Operand::Unary, route, sources),
        },
        ParsedKind::Call {
            function: NumericFunction::Sqrt,
            arguments,
            ..
        } => NumericNode::Sqrt {
            operand: child(parsed, arguments[0], Operand::Unary, route, sources),
        },
        ParsedKind::Call {
            function: NumericFunction::Mod,
            arguments,
            ..
        } => NumericNode::Binary {
            operator: BinaryOperator::Modulo,
            left: child(parsed, arguments[0], Operand::Left, route, sources),
            right: child(parsed, arguments[1], Operand::Right, route, sources),
        },
        ParsedKind::Call {
            function: NumericFunction::NullIf,
            arguments,
            ..
        } => NumericNode::NullIf {
            left: child(parsed, arguments[0], Operand::Left, route, sources),
            right: child(parsed, arguments[1], Operand::Right, route, sources),
        },
        ParsedKind::Call {
            function,
            arguments,
            ..
        } if matches!(
            function,
            NumericFunction::Greatest | NumericFunction::Least | NumericFunction::Coalesce
        ) =>
        {
            let function = match function {
                NumericFunction::Greatest => SelectionFunction::Greatest,
                NumericFunction::Least => SelectionFunction::Least,
                _ => SelectionFunction::Coalesce,
            };
            let arguments = arguments
                .iter()
                .enumerate()
                .map(|(index, &id)| {
                    route.push(Operand::Argument(index));
                    let node = lower(parsed, id, route, sources);
                    route.pop();
                    node
                })
                .collect();
            NumericNode::Selection {
                function,
                arguments,
            }
        }
        ParsedKind::Call { .. } | ParsedKind::Group { .. } => {
            unreachable!("preflighted calls and unwrapped groups")
        }
    }
}
