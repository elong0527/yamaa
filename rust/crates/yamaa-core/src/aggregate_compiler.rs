//! Bounded aggregate expressions reuse the shared numeric evaluator and source spans.
use crate::{
    aggregate_parser::{parse_aggregate, ParseError, ParsedAggregate, ParsedKind, Reducer},
    numeric_compiler::{compile_aggregate_subtree, CompileError, CompileLimits, CompiledNumeric},
};
use alloc::{string::String, vec::Vec};

#[derive(Clone, Debug, PartialEq)]
pub enum AggregateOperand {
    /// Bare values retain text, temporal and numeric types for MIN/MAX/ONLY.
    Value(String),
    /// COUNT(dataset.*) counts selected records independently of missing cells.
    Records(String),
    Numeric(CompiledNumeric),
}
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledReduction {
    pub reducer: Reducer,
    /// An opaque resolver identity containing a call, disjoint from field names.
    pub reference: String,
    pub operand: AggregateOperand,
}
#[derive(Clone, Debug, PartialEq)]
pub enum AggregateResult {
    Reduction(usize),
    Numeric(CompiledNumeric),
}
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledAggregate {
    expression: String,
    path: String,
    reductions: Vec<CompiledReduction>,
    result: AggregateResult,
    identifiers: Vec<String>,
    record_relations: Vec<String>,
    ungrouped: Vec<String>,
    work: usize,
}
impl CompiledAggregate {
    pub fn expression(&self) -> &str {
        &self.expression
    }
    pub fn spec_path(&self) -> &str {
        &self.path
    }
    pub fn reductions(&self) -> &[CompiledReduction] {
        &self.reductions
    }
    pub fn result(&self) -> &AggregateResult {
        &self.result
    }
    pub fn identifiers(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(String::as_str)
    }
    pub fn record_relations(&self) -> impl Iterator<Item = &str> {
        self.record_relations.iter().map(String::as_str)
    }
    pub fn ungrouped_identifiers(&self) -> impl Iterator<Item = &str> {
        self.ungrouped.iter().map(String::as_str)
    }
    pub fn node_count(&self) -> usize {
        self.work
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregateCompileError {
    Parse(ParseError),
    Numeric(CompileError),
    MetadataLimit {
        limit: usize,
        required: Option<usize>,
    },
}
/// Capacity policy for retained authored text across independent numeric plans.
pub const MAX_AGGREGATE_METADATA_BYTES: usize = 16_777_216;

fn ungroup(parsed: &ParsedAggregate, mut id: usize) -> usize {
    while let ParsedKind::Group { operand } = parsed.nodes()[id].kind {
        id = operand;
    }
    id
}
fn spelling(parsed: &ParsedAggregate, id: usize) -> &str {
    let span = parsed.nodes()[id].span;
    &parsed.expression()[span.start..span.end]
}

/// Admit every reducer and operand, including syntax behind lazy numeric calls.
/// Bare reducer results retain their type; only surrounding arithmetic coerces.
pub fn compile_aggregate(
    expression: &str,
    path: &str,
    limits: CompileLimits,
) -> Result<CompiledAggregate, AggregateCompileError> {
    let parsed = parse_aggregate(expression, limits.parse).map_err(AggregateCompileError::Parse)?;
    let root = ungroup(&parsed, parsed.root());
    let numeric_copies = parsed
        .nodes()
        .iter()
        .filter(|node| match node.kind {
            ParsedKind::Reduction { operand, .. } => !matches!(
                parsed.nodes()[ungroup(&parsed, operand)].kind,
                ParsedKind::Identifier | ParsedKind::Star
            ),
            _ => false,
        })
        .count()
        + usize::from(!matches!(
            parsed.nodes()[root].kind,
            ParsedKind::Reduction { .. }
        ));
    let required = expression
        .len()
        .checked_add(path.len())
        .and_then(|bytes| bytes.checked_mul(numeric_copies.saturating_add(1)));
    if required.is_none_or(|bytes| bytes > MAX_AGGREGATE_METADATA_BYTES) {
        return Err(AggregateCompileError::MetadataLimit {
            limit: MAX_AGGREGATE_METADATA_BYTES,
            required,
        });
    }
    let mut reductions = Vec::new();
    let mut reduction_nodes = Vec::new();
    let mut resolutions: usize = 0;
    for (id, node) in parsed.nodes().iter().enumerate() {
        let ParsedKind::Reduction {
            reducer, operand, ..
        } = node.kind
        else {
            continue;
        };
        let operand = ungroup(&parsed, operand);
        let operand = match parsed.nodes()[operand].kind {
            ParsedKind::Identifier => {
                resolutions = resolutions.saturating_add(1);
                AggregateOperand::Value(spelling(&parsed, operand).into())
            }
            ParsedKind::Star => {
                let written = spelling(&parsed, operand);
                AggregateOperand::Records(written[..written.len() - 2].into())
            }
            _ => {
                let numeric = compile_aggregate_subtree(&parsed, operand, path, limits.resolutions)
                    .map_err(AggregateCompileError::Numeric)?;
                resolutions = resolutions.saturating_add(numeric.resolution_count());
                AggregateOperand::Numeric(numeric)
            }
        };
        reduction_nodes.push(id);
        reductions.push(CompiledReduction {
            reducer,
            reference: spelling(&parsed, id).into(),
            operand,
        });
    }
    let result = match parsed.nodes()[root].kind {
        ParsedKind::Reduction { .. } => AggregateResult::Reduction(
            reduction_nodes
                .iter()
                .position(|&id| id == root)
                .expect("compiled reduction root"),
        ),
        _ => {
            let numeric = compile_aggregate_subtree(&parsed, root, path, limits.resolutions)
                .map_err(AggregateCompileError::Numeric)?;
            resolutions = resolutions.saturating_add(numeric.resolution_count());
            AggregateResult::Numeric(numeric)
        }
    };
    if resolutions > limits.resolutions {
        return Err(AggregateCompileError::Numeric(
            CompileError::ResolutionLimit {
                limit: limits.resolutions,
                required: resolutions,
            },
        ));
    }
    Ok(CompiledAggregate {
        expression: expression.into(),
        path: path.into(),
        reductions,
        result,
        identifiers: parsed.identifiers().map(String::from).collect(),
        record_relations: parsed.star_datasets().map(String::from).collect(),
        ungrouped: parsed.ungrouped_identifiers().map(String::from).collect(),
        work: parsed.nodes().len(),
    })
}
