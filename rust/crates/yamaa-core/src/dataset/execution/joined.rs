//! Lazy implicit donor lookup shares missing, multiplicity and port evidence.
use super::lookup::JoinSite;
use super::*;
/// A formula or operand read through the implicit join, answered at `site`
/// when the join finds no record (LOOKUP-24).
#[allow(clippy::too_many_arguments)]
pub(super) fn read<T: TableAccess + ?Sized>(
    current: &T,
    joined: &crate::dataset::JoinedRead,
    site: JoinSite<'_>,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    read_options(
        current, joined, None, None, None, site, assignment, candidate, plan, row, state,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_options<T: TableAccess + ?Sized>(
    current: &T,
    joined: &crate::dataset::JoinedRead,
    range: Option<&RelationRange>,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    site: JoinSite<'_>,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let range = lookup::range_value(range, current, candidate, plan, row, assignment, state)?;
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
        range,
        filter,
        selection,
        site,
        state.handlers,
    )
}
