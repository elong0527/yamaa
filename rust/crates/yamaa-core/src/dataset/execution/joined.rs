//! Lazy implicit donor lookup shares missing, multiplicity and port evidence.
use super::*;
pub(super) fn read<T: TableAccess + ?Sized>(
    current: &T,
    joined: &crate::dataset::JoinedRead,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    read_options(
        current, joined, None, None, assignment, candidate, plan, row, state,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_options<T: TableAccess + ?Sized>(
    current: &T,
    joined: &crate::dataset::JoinedRead,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let (source, table) = match joined.source {
        AggregateSource::Primary => (usize::MAX, state.primary),
        AggregateSource::Secondary(index) => (index, state.secondary[index]),
    };
    lookup::read_joined(
        current,
        table,
        source,
        joined,
        assignment,
        candidate,
        plan,
        row,
        filter,
        selection,
        state.handlers,
    )
}
