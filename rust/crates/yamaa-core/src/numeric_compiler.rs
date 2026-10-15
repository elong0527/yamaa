//! Compile bounded numeric syntax using the pinned portable math contract.
//!
//! Literal overflow is deferred until evaluation reaches it in written order.
//! Compiled trees retain the parser's fixed recursion guard and exact node spans.

use crate::evaluation::{EvaluationError, NumericNode, NumericPlan, NumericResolver, Operand};
use crate::numeric::{
    BinaryOperator, IntegralFunction, MathFunction, Number, SelectionFunction, UnaryOperator,
};
use crate::numeric_parser::{
    parse_numeric, NumericFunction, ParseError, ParsedKind, ParsedNumeric, SourceSpan,
};

/// Evaluator failure plus the exact source extent of the failed semantic node.
/// Group parentheses do not replace an inner node's span. An operator/call failure
/// spans the whole expression/call; a leaf failure spans only its written token.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("numeric compiler: source_span={source_span:?}, evaluation={evaluation:?}")]
pub struct CompiledEvaluationError<E> {
    pub source_span: SourceSpan,
    pub evaluation: Box<EvaluationError<E>>,
}

/// Immutable executable plan for the complete numeric vocabulary. Reusing a plan
/// repeats resolution; no values or errors are cached.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledNumeric {
    plan: NumericPlan,
    sources: Vec<(Vec<Operand>, SourceSpan)>,
    identifiers: Vec<String>,
}

/// Closed declared result kind for numeric computations, before any values exist.
pub const NUMERIC_RESULT_TYPE: crate::value::ColumnType = crate::value::ColumnType::Float;

impl CompiledNumeric {
    /// The declared numeric result kind does not execute or resolve this plan.
    pub fn result_type(&self) -> crate::value::ColumnType {
        NUMERIC_RESULT_TYPE
    }

    /// Original source text, retained without normalization or reassociation.
    pub fn expression(&self) -> &str {
        &self.plan.expression
    }

    /// Specification provenance copied into every evaluation failure.
    pub fn spec_path(&self) -> &str {
        &self.plan.spec_path
    }

    /// Distinct case-sensitive names in written first-occurrence order for static binding.
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(String::as_str)
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

/// Parse and preflight the whole expression before resolving any identifiers.
/// No arithmetic folding, name binding, resolver invocation or handler accounting
/// occurs here. The fixed recursion guard belongs to the parser.
pub fn compile_numeric(text: &str, spec_path: &str) -> Result<CompiledNumeric, ParseError> {
    let parsed = parse_numeric(text)?;
    Ok(compile_parsed(&parsed, spec_path))
}

/// R013 shares the same arithmetic evaluator and original spans. The parser has
/// already admitted the full aggregate; this projection cannot extend its grammar.
pub(crate) fn compile_aggregate_subtree(
    parsed: &crate::aggregate_parser::ParsedAggregate,
    root: usize,
    spec_path: &str,
) -> CompiledNumeric {
    compile_parsed(&ParsedNumeric::aggregate_subtree(parsed, root), spec_path)
}

fn compile_parsed(parsed: &ParsedNumeric, spec_path: &str) -> CompiledNumeric {
    let mut sources = Vec::new();
    let root = lower(parsed, parsed.root(), &mut Vec::new(), &mut sources);
    CompiledNumeric {
        plan: NumericPlan {
            spec_path: spec_path.into(),
            expression: parsed.expression().into(),
            root,
        },
        sources,
        identifiers: parsed.identifiers().map(String::from).collect(),
    }
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
            function: function @ (NumericFunction::Exp | NumericFunction::Ln),
            arguments,
            ..
        } => NumericNode::Math {
            function: if *function == NumericFunction::Exp {
                MathFunction::Exp
            } else {
                MathFunction::Ln
            },
            operand: child(parsed, arguments[0], Operand::Unary, route, sources),
        },
        ParsedKind::Call {
            function: NumericFunction::Power,
            arguments,
            ..
        } => NumericNode::Power {
            left: child(parsed, arguments[0], Operand::Left, route, sources),
            right: child(parsed, arguments[1], Operand::Right, route, sources),
        },
        ParsedKind::Call {
            function: NumericFunction::RoundHalfAwayFromZero,
            arguments,
            ..
        } => NumericNode::Round {
            value: child(parsed, arguments[0], Operand::Left, route, sources),
            digits: child(parsed, arguments[1], Operand::Right, route, sources),
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
