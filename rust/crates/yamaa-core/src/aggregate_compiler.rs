//! Bounded aggregate expressions reuse the shared numeric evaluator and source spans.
use crate::{
    aggregate_parser::{parse_aggregate, ParseError, ParsedAggregate, ParsedKind, Reducer},
    numeric_compiler::{compile_aggregate_subtree, CompiledNumeric},
};

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
}
impl CompiledAggregate {
    /// Surrounding numeric arithmetic has a closed declared kind. A bare reducer
    /// retains its donor kind and cannot acquire a type without that schema.
    pub fn numeric_result_type(&self) -> Option<crate::value::ColumnType> {
        match &self.result {
            AggregateResult::Numeric(plan) => Some(plan.result_type()),
            AggregateResult::Reduction(_) => None,
        }
    }

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
}
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
pub fn compile_aggregate(expression: &str, path: &str) -> Result<CompiledAggregate, ParseError> {
    let parsed = parse_aggregate(expression)?;
    let root = ungroup(&parsed, parsed.root());
    let mut reductions = Vec::new();
    let mut reduction_nodes = Vec::new();
    for (id, node) in parsed.nodes().iter().enumerate() {
        let ParsedKind::Reduction {
            reducer, operand, ..
        } = node.kind
        else {
            continue;
        };
        let operand = ungroup(&parsed, operand);
        let operand = match parsed.nodes()[operand].kind {
            ParsedKind::Identifier => AggregateOperand::Value(spelling(&parsed, operand).into()),
            ParsedKind::Star => {
                let written = spelling(&parsed, operand);
                AggregateOperand::Records(written[..written.len() - 2].into())
            }
            _ => {
                let numeric = compile_aggregate_subtree(&parsed, operand, path);
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
            let numeric = compile_aggregate_subtree(&parsed, root, path);
            AggregateResult::Numeric(numeric)
        }
    };
    Ok(CompiledAggregate {
        expression: expression.into(),
        path: path.into(),
        reductions,
        result,
        identifiers: parsed.identifiers().map(String::from).collect(),
        record_relations: parsed.star_datasets().map(String::from).collect(),
        ungrouped: parsed.ungrouped_identifiers().map(String::from).collect(),
    })
}
