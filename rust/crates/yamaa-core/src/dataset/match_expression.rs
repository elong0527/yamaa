//! Lookup scalar expressions reuse the ordinary bound IR on resolved operands.
use super::*;
use crate::key_relation::ComparableType;

#[derive(Clone, Debug, PartialEq)]
pub struct MatchExpression {
    pub assignment: Assignment,
    pub reads: Vec<MatchRead>,
    pub schema: TableSchema,
    pub result_type: Option<ComparableType>,
    /// A relation window retains its original completed-row coordinates.
    pub windowed: bool,
}
impl MatchExpression {
    pub fn new(
        mut assignment: Assignment,
        reads: Vec<MatchRead>,
        schema: TableSchema,
        result_type: Option<ComparableType>,
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
    pub(super) fn validate(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        available: &[bool],
    ) -> Result<(), PlanError> {
        if contains_window(&self.assignment.expression, 0)? != self.windowed
            || self.reads.len() != self.schema.columns().len()
        {
            return Err(PlanError::InvalidIntermediate);
        }
        for (read, field) in self.reads.iter().zip(self.schema.columns()) {
            if read.identifier() != field.name
                || read.kind(primary, secondary, output, available)? != field.kind
            {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if self.assignment.column != output.columns().len() {
            return Err(PlanError::InvalidIntermediate);
        }
        let mut slots = if self.windowed {
            available.to_vec()
        } else {
            vec![false; available.len()]
        };
        validate_assignment_at(
            &self.assignment,
            &mut slots,
            if self.windowed { primary } else { &self.schema },
            primary,
            secondary,
            &[],
            output,
            &RowMode::Records,
            0,
            false,
            true,
        )
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
        | Expression::StrContains { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::Round { source, .. } => normalize(&mut source.expression, reads)?,
        Expression::Extreme { operands, .. }
        | Expression::StrConcat { operands, .. }
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
        Expression::Imputation { operation, .. } if operation.bound_source().is_some() => {
            Err(PlanError::InvalidIntermediate)
        }
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrContains { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::Round { source, .. } => contains_window(&source.expression, depth + 1),
        Expression::Extreme { operands, .. }
        | Expression::StrConcat { operands, .. }
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
