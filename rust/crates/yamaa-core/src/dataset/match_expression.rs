//! Lookup scalar expressions reuse the ordinary bound IR on resolved operands.
use super::*;
use crate::result_type::ResultType;

#[derive(Clone, Debug, PartialEq)]
pub struct MatchExpression {
    pub assignment: Assignment,
    pub reads: Vec<MatchRead>,
    pub schema: TableSchema,
    /// The expression's result type (EXPR-14), compared with the key column.
    pub result_type: ResultType,
    /// A relation window retains its original completed-row coordinates.
    pub windowed: bool,
}
impl MatchExpression {
    pub fn new(
        mut assignment: Assignment,
        reads: Vec<MatchRead>,
        schema: TableSchema,
        result_type: ResultType,
    ) -> Result<Self, PlanError> {
        let windowed = contains_window(&assignment.expression, 0)?;
        if !windowed {
            normalize(&mut assignment.expression, &reads)?;
        }
        Ok(Self {
            assignment,
            reads,
            schema,
            result_type,
            windowed,
        })
    }
}

fn slot(reads: &[MatchRead], read: &Read) -> Result<usize, PlanError> {
    reads
        .iter()
        .position(|bound| match (bound, read) {
            (MatchRead::Output { column: a, .. }, Read::Column(b)) => a == b,
            (
                MatchRead::Record { column: a, .. } | MatchRead::SecondaryRecord { column: a, .. },
                Read::Source(b),
            ) => a == b,
            (MatchRead::Record { column, identifier }, Read::Joined(joined)) => {
                joined.source == AggregateSource::Primary
                    && *column == joined.column
                    && identifier
                        .split_once('.')
                        .is_some_and(|(name, _)| name == joined.name)
            }
            (
                MatchRead::SecondaryRecord {
                    source,
                    column,
                    identifier,
                },
                Read::Joined(joined),
            ) => {
                joined.source == AggregateSource::Secondary(*source)
                    && *column == joined.column
                    && identifier
                        .split_once('.')
                        .is_some_and(|(name, _)| name == joined.name)
            }
            _ => false,
        })
        .ok_or(PlanError::InvalidIntermediate)
}
/// This is a coordinate rebase, not a second evaluator: ordinary scalar owners
/// continue to evaluate each admitted operation and local handler.
fn normalize(expression: &mut Expression, reads: &[MatchRead]) -> Result<(), PlanError> {
    match expression {
        Expression::Source(column)
        | Expression::Collect {
            column,
            filter: None,
            selection: None,
            ..
        } => *expression = Expression::Source(slot(reads, &Read::Source(*column))?),
        Expression::Column(column) => {
            *expression = Expression::Source(slot(reads, &Read::Column(*column))?)
        }
        Expression::Joined(joined) => {
            *expression = Expression::Source(slot(reads, &Read::Joined(joined.clone()))?)
        }
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrSubstr { source, .. }
        | Expression::StrLength { source, .. }
        | Expression::StrDecimal { source, .. }
        | Expression::Round { source, .. } => normalize(&mut source.expression, reads)?,
        Expression::Extreme { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::Temporal { operands, .. } => {
            for operand in operands {
                normalize(&mut operand.expression, reads)?;
            }
        }
        Expression::Compute(numeric) => {
            let bindings = numeric
                .bindings()
                .iter()
                .map(|binding| {
                    Ok(crate::bound_expression::Binding {
                        name: binding.name.clone(),
                        read: Read::Source(slot(reads, &binding.read)?),
                    })
                })
                .collect::<Result<Vec<_>, PlanError>>()?;
            *numeric = BoundNumeric::new(numeric.expression().clone(), bindings)
                .map_err(|_| PlanError::InvalidIntermediate)?;
        }
        Expression::Literal(_) => (),
        _ => return Err(PlanError::InvalidIntermediate),
    }
    Ok(())
}
fn contains_window(expression: &Expression, depth: usize) -> Result<bool, PlanError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PlanError::InvalidIntermediate);
    }
    match expression {
        Expression::Window(_) | Expression::ContextWindow(_) => Ok(true),
        Expression::Case(_) => Err(PlanError::InvalidIntermediate),
        Expression::Imputation { operation, .. } if operation.has_conditional_operands() => {
            Err(PlanError::InvalidIntermediate)
        }
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrSubstr { source, .. }
        | Expression::StrLength { source, .. }
        | Expression::StrDecimal { source, .. }
        | Expression::Round { source, .. } => contains_window(&source.expression, depth + 1),
        Expression::Extreme { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::Temporal { operands, .. } => {
            operands.iter().try_fold(false, |found, operand| {
                Ok(contains_window(&operand.expression, depth + 1)? || found)
            })
        }
        _ => Ok(false),
    }
}
