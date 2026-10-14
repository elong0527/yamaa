//! Lazy implicit donor lookup shares missing, multiplicity and port evidence.
use super::*;
pub(super) fn read<E>(
    joined: &yamaa_core::dataset::JoinedRead,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, E>,
) -> Result<Value, Box<ExecutionError<E>>> {
    read_options(joined, None, None, assignment, candidate, plan, row, state)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_options<E>(
    joined: &yamaa_core::dataset::JoinedRead,
    filter: Option<&yamaa_core::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, E>,
) -> Result<Value, Box<ExecutionError<E>>> {
    state.budget.work(1, joined.keys.len())?;
    let (source, table) = match joined.source {
        AggregateSource::Primary => (usize::MAX, state.primary),
        AggregateSource::Secondary(index) => (index, state.secondary[index]),
    };
    lookup::read_joined(
        table,
        source,
        joined,
        assignment,
        candidate,
        plan,
        row,
        state.budget,
        filter,
        selection,
        state.handlers,
    )
}
