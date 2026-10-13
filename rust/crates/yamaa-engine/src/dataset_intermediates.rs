//! Lazy source eligibility and named record selection within one dataset attempt.
use super::*;
#[path = "dataset_match_values.rs"]
pub(super) mod match_values;
use alloc::{collections::BTreeMap, rc::Rc};

/// Keep only immutable row coordinates and the inherited handler outcome.
#[derive(Clone, Copy)]
enum Choice {
    Record { row: usize, multiple: bool },
    Absent,
}

/// Fresh for every attempt, with no entries for unused intermediate declarations.
#[derive(Default)]
pub(super) struct Run {
    eligible: BTreeMap<usize, Vec<usize>>,
    choices: BTreeMap<(SelectionIdentity, usize), Choice>,
    materialized: BTreeMap<(usize, Option<SelectionIdentity>), Rc<Dataset>>,
}

impl Run {
    /// Every completed template changes only SELF eligibility and match choices.
    pub(super) fn publish_self<E>(
        &mut self,
        plan: &DatasetPlan,
        budget: &mut Budget,
    ) -> Result<(), Box<ExecutionError<E>>> {
        if !plan
            .intermediates()
            .iter()
            .any(|item| item.source.is_self())
        {
            return Ok(());
        }
        budget.work(
            self.eligible
                .len()
                .checked_add(self.choices.len())
                .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
            1,
        )?;
        self.eligible
            .retain(|index, _| !plan.intermediates()[*index].source.is_self());
        self.choices
            .retain(|(_, index), _| !plan.intermediates()[*index].source.is_self());
        self.materialized
            .retain(|(index, _), _| !plan.intermediates()[*index].source.is_self());
        Ok(())
    }
}

/// Source-only checks are eager even when no output expression reads the selector.
pub(super) fn verify<E>(
    plan: &DatasetPlan,
    self_only: bool,
    limits: Limits,
    state: &mut EvaluationState<'_, E>,
) -> Result<(), Box<ExecutionError<E>>> {
    let donors = self_relations::Donors::<E>::new(state.self_rows, plan.output());
    let context = Candidate {
        template: None,
        selection_identity: SelectionIdentity::Probe,
        members: Vec::new(),
        values: vec![Value::Missing; plan.output().columns().len()],
        completed: vec![false; plan.output().columns().len()],
    };
    let assignment = Assignment {
        column: 0,
        path: "intermediate-verification".into(),
        expression: Expression::Literal(Value::Missing),
    };
    let mut failures = Vec::new();
    for (index, item) in plan.intermediates().iter().enumerate() {
        if item.source.is_self() != self_only || item.unique.is_empty() {
            continue;
        }
        let prepared = materialize(index, &assignment, &context, plan, 0, limits, state)?;
        let materialized = prepared.as_ref().map(|data| Data::<E>::new(data));
        let table: &dyn TableAccess<Error = E> = if let Some(data) = &materialized {
            data
        } else {
            match &item.source {
                IntermediateSource::Input(index) => state.secondary[*index],
                IntermediateSource::Primary { .. } => state.primary,
                IntermediateSource::SelfRows { .. } => &donors,
            }
        };
        let name = source_name(item, plan);
        let eligible = eligibility(table, item, state.budget)?;
        let filtered = Filtered {
            table,
            rows: &eligible,
        };
        for check in &item.unique {
            state.budget.work(eligible.len(), check.columns.len())?;
            let mut columns = Vec::new();
            for &column in &check.columns {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
            let groups = partition(&filtered, &columns, limits.output_rows, limits.key_cells)
                .map_err(|error| Box::new(ExecutionError::Grouping(error)))?;
            let count = groups.iter().filter(|group| group.len() > 1).count();
            if count != 0 {
                failures.push(check.diagnostic(item, table.schema(), name, count));
            }
        }
        state.intermediates.eligible.insert(index, eligible);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Box::new(ExecutionError::IntermediateVerifications(
            failures,
        )))
    }
}
fn source_name<'a>(item: &'a Intermediate, plan: &'a DatasetPlan) -> &'a str {
    match &item.source {
        IntermediateSource::Input(index) => &plan.secondary()[*index].name,
        IntermediateSource::Primary { name, .. } => name,
        IntermediateSource::SelfRows { .. } => "SELF",
    }
}
pub(super) struct Data<'a, E> {
    dataset: &'a Dataset,
    error: core::marker::PhantomData<E>,
}
impl<'a, E> Data<'a, E> {
    pub(super) fn new(dataset: &'a Dataset) -> Self {
        Self {
            dataset,
            error: core::marker::PhantomData,
        }
    }
}
impl<E> TableAccess for Data<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        &self.dataset.schema
    }
    fn row_count(&self) -> usize {
        self.dataset.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.dataset
            .rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

/// An immutable donor snapshot is shared only across contexts supplying the same current row.
#[allow(clippy::too_many_arguments)]
fn materialize<E>(
    index: usize,
    assignment: &Assignment,
    context: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, E>,
) -> Result<Option<Rc<Dataset>>, Box<ExecutionError<E>>> {
    let item = &plan.intermediates()[index];
    let Some(derived) = &item.derivations else {
        return Ok(None);
    };
    let key = (
        index,
        (!derived.current.is_empty()).then_some(context.selection_identity),
    );
    if let Some(data) = state.intermediates.materialized.get(&key) {
        return Ok(Some(Rc::clone(data)));
    }
    let primary = row_drivers::Driver::Primary(state.primary);
    let mut current = Vec::new();
    for field in &derived.current {
        current.push(relation_predicates::resolve_current(
            field,
            &assignment.path,
            &primary,
            context,
            plan,
            row,
            assignment,
            relation_predicates::CurrentObservation::Operand,
            state,
        )?);
    }
    let donors = self_relations::Donors::<E>::new(state.self_rows, plan.output());
    let table: &dyn TableAccess<Error = E> = match &item.source {
        IntermediateSource::Input(index) => state.secondary[*index],
        IntermediateSource::Primary { .. } => state.primary,
        IntermediateSource::SelfRows { .. } => &donors,
    };
    let nested = &derived.plan;
    let width = nested.source().columns().len();
    let output_width = nested.output().columns().len();
    if table
        .row_count()
        .checked_mul(width)
        .is_none_or(|cells| cells > limits.output_cells)
    {
        return Err(Box::new(ExecutionError::Capacity));
    }
    state.budget.work(table.row_count(), width)?;
    let mut input = Dataset {
        schema: nested.source().clone(),
        rows: Vec::new(),
    };
    let mut candidates = Vec::new();
    input
        .rows
        .try_reserve_exact(table.row_count())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    candidates
        .try_reserve_exact(table.row_count())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for source_row in 0..table.row_count() {
        let mut values = vec![Value::Missing; width];
        for (column, target) in values.iter_mut().enumerate().take(derived.stored_columns) {
            *target = own(cell(table, item, source_row, column, state.budget)?);
            state.budget.value(target)?;
        }
        for (index, value) in current.iter().enumerate() {
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            values[output_width + index] = value.clone();
            state.budget.value(value)?;
        }
        let output = values[..output_width].to_vec();
        for value in &output {
            state.budget.value(value)?;
        }
        candidates.push(Candidate {
            template: Some(0),
            selection_identity: SelectionIdentity::Candidate(source_row),
            members: vec![source_row],
            values: output,
            completed: (0..output_width)
                .map(|column| column < derived.stored_columns)
                .collect(),
        });
        input.rows.push(values);
    }
    let nested_secondary = nested
        .secondary()
        .iter()
        .map(|source| {
            plan.secondary()
                .iter()
                .position(|outer| outer.name == source.name)
                .map_or(state.primary, |index| state.secondary[index])
        })
        .collect::<Vec<_>>();
    let mut intermediate_run = Run::default();
    // Raw donor fields have no output lifecycle conversion or handler ledger.
    let mut donor_handlers = HandlerCounter::default();
    for assignment in nested.columns() {
        let table = Data::<E>::new(&input);
        let mut numbers = windows::Evaluation::new(
            assignment,
            &candidates,
            nested,
            &table,
            limits,
            state.budget,
            &mut donor_handlers,
        )?;
        let mut nested_windows = windows::Cache::default();
        for row in 0..candidates.len() {
            let table = Data::<E>::new(&input);
            let value = if let Some(run) = &mut numbers {
                run.value(row, &candidates, &table, state.budget, assignment, nested)?
            } else {
                evaluate_raw_retained(
                    &table,
                    assignment,
                    &candidates[row],
                    nested,
                    row,
                    limits,
                    &mut EvaluationState {
                        primary: &table,
                        functions: state.functions,
                        secondary: &nested_secondary,
                        self_rows: &[],
                        budget: state.budget,
                        handlers: &mut donor_handlers,
                        intermediates: &mut intermediate_run,
                        output_rows: &candidates,
                        output_offset: 0,
                        windows: &mut nested_windows,
                        aggregate_row: None,
                    },
                    false,
                )
                .map_err(|error| donor_failure(error, item, plan, nested))?
            };
            state.budget.value(&value)?;
            state.budget.value(&value)?;
            input.rows[row][assignment.column] = value.clone();
            candidates[row].values[assignment.column] = value;
            candidates[row].completed[assignment.column] = true;
        }
    }
    for row in &input.rows {
        state.budget.discard_candidate(row);
    }
    let data = Rc::new(Dataset {
        schema: nested.output().clone(),
        rows: candidates
            .into_iter()
            .map(|candidate| candidate.values)
            .collect(),
    });
    state
        .intermediates
        .materialized
        .insert(key, Rc::clone(&data));
    Ok(Some(data))
}

/// A raw donor failure has donor provenance, rather than a destination identity.
fn donor_failure<E>(
    mut error: Box<ExecutionError<E>>,
    item: &Intermediate,
    outer: &DatasetPlan,
    nested: &DatasetPlan,
) -> Box<ExecutionError<E>> {
    match error.as_mut() {
        ExecutionError::Numeric { identity, .. }
        | ExecutionError::MultipleValues { identity, .. }
        | ExecutionError::MultipleMatches { identity, .. }
        | ExecutionError::UnmatchedKey { identity, .. }
        | ExecutionError::Reduction { identity, .. }
        | ExecutionError::SemanticDiagnostic { identity, .. } => *identity = None,
        ExecutionError::SecondaryCell { source, .. } => {
            let name = &nested.secondary()[*source].name;
            if let Some(index) = outer
                .secondary()
                .iter()
                .position(|source| source.name == *name)
            {
                *source = index;
            } else if let ExecutionError::SecondaryCell {
                path,
                source_row,
                error: original,
                ..
            } = core::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
            {
                *error = ExecutionError::Cell {
                    path,
                    source_row,
                    error: original,
                };
            }
        }
        ExecutionError::Cell { .. } => {
            if let IntermediateSource::Input(source) = item.source {
                if let ExecutionError::Cell {
                    path,
                    source_row,
                    error: original,
                } = core::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
                {
                    *error = ExecutionError::SecondaryCell {
                        source,
                        path,
                        source_row,
                        error: original,
                    };
                }
            }
        }
        _ => (),
    }
    error
}

pub(super) struct DriverRelation {
    pub(super) materialized: Option<Rc<Dataset>>,
    pub(super) eligible: Vec<usize>,
}

/// Row drivers consume the complete source-only donor set without executing a match.
pub(super) fn driver<E>(
    index: usize,
    plan: &DatasetPlan,
    limits: Limits,
    state: &mut EvaluationState<'_, E>,
) -> Result<DriverRelation, Box<ExecutionError<E>>> {
    let context = Candidate {
        template: None,
        selection_identity: SelectionIdentity::Probe,
        members: Vec::new(),
        values: vec![Value::Missing; plan.output().columns().len()],
        completed: vec![false; plan.output().columns().len()],
    };
    let assignment = Assignment {
        column: 0,
        path: plan.intermediates()[index].path.clone(),
        expression: Expression::Literal(Value::Missing),
    };
    let prepared = materialize(index, &assignment, &context, plan, 0, limits, state)?;
    let data = prepared.as_ref().map(|data| Data::<E>::new(data));
    let item = &plan.intermediates()[index];
    let table: &dyn TableAccess<Error = E> = if let Some(data) = &data {
        data
    } else {
        match item.source {
            IntermediateSource::Input(index) => state.secondary[index],
            IntermediateSource::Primary { .. } => state.primary,
            _ => return Err(Box::new(ExecutionError::SchemaMismatch)),
        }
    };
    let eligible = eligibility(table, item, state.budget)?;
    Ok(DriverRelation {
        materialized: prepared,
        eligible,
    })
}

pub(super) fn driver_groups<E>(
    table: &dyn TableAccess<Error = E>,
    eligible: &[usize],
    keys: &[usize],
    limits: Limits,
) -> Result<Vec<Vec<usize>>, Box<ExecutionError<E>>> {
    let filtered = Filtered {
        table,
        rows: eligible,
    };
    partition(&filtered, keys, limits.source_rows, limits.key_cells)
        .map(|groups| {
            groups
                .into_iter()
                .map(|group| group.into_iter().map(|row| eligible[row]).collect())
                .collect()
        })
        .map_err(|error| Box::new(ExecutionError::Grouping(error)))
}
struct Filtered<'a, E> {
    table: &'a dyn TableAccess<Error = E>,
    rows: &'a [usize],
}
impl<E> TableAccess for Filtered<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.table.schema()
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.table.cell(self.rows[row], column)
    }
}
fn eligibility<E>(
    table: &dyn TableAccess<Error = E>,
    item: &Intermediate,
    budget: &mut Budget,
) -> Result<Vec<usize>, Box<ExecutionError<E>>> {
    budget.work(table.row_count(), 1)?;
    let mut eligible = Vec::new();
    eligible
        .try_reserve_exact(table.row_count())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for source_row in 0..table.row_count() {
        if let Some(filter) = item.filter.as_ref().filter(|filter| !filter.correlated()) {
            let truth = crate::dataset_predicate::evaluate(
                filter.predicate(),
                table,
                source_row,
                &[],
                budget.predicate(),
            )
            .map_err(|error| match error.kind {
                yamaa_core::predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
                _ => Box::new(ExecutionError::Predicate { source_row, error }),
            })?;
            if truth != yamaa_core::predicate::Truth::True {
                continue;
            }
        }
        eligible.push(source_row);
    }
    Ok(eligible)
}

/// Reuse a selected record while recording each reading's own inherited handler site.
#[allow(clippy::too_many_arguments)]
pub(super) fn read<T: TableAccess + ?Sized>(
    index: usize,
    column: usize,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    read_with_observations(
        index, column, assignment, candidate, plan, row, primary, state, true,
    )
}

/// Predicate normalization discards an inherited recovery observation, while
/// sharing the same eligibility and attempt-local record-choice cache.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_predicate<T: TableAccess + ?Sized>(
    index: usize,
    column: usize,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    read_with_observations(
        index, column, assignment, candidate, plan, row, primary, state, false,
    )
}
#[allow(clippy::too_many_arguments)]
fn read_with_observations<T: TableAccess + ?Sized>(
    index: usize,
    column: usize,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
    observe_handlers: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    // Aggregate derive bindings keep their donor record separate from the
    // current output row used by a named, kept intermediate selection.
    if let Some(current) = state.aggregate_row.take() {
        let primary = state.primary;
        let result = read_with_observations(
            index,
            column,
            assignment,
            &current,
            plan,
            row,
            primary,
            state,
            observe_handlers,
        );
        state.aggregate_row = Some(current);
        return result;
    }
    // Admission forbids named reads during destination-free key probes.
    if candidate.selection_identity == SelectionIdentity::Probe {
        return Err(Box::new(ExecutionError::SchemaMismatch));
    }
    // A linear comparison bound admits tree lookups before inspecting retained keys.
    state.budget.work(
        state
            .intermediates
            .choices
            .len()
            .checked_add(1)
            .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
        1,
    )?;
    let cache_key = (candidate.selection_identity, index);
    let item = &plan.intermediates()[index];
    let prepared = materialize(
        index,
        assignment,
        candidate,
        plan,
        row,
        state.budget.limits(),
        state,
    )?;
    let materialized = prepared.as_ref().map(|data| Data::<T::Error>::new(data));
    let donors = self_relations::Donors::<T::Error>::new(state.self_rows, plan.output());
    let table: &dyn TableAccess<Error = T::Error> = if let Some(data) = &materialized {
        data
    } else {
        match &item.source {
            IntermediateSource::Input(index) => state.secondary[*index],
            IntermediateSource::Primary { .. } => state.primary,
            IntermediateSource::SelfRows { .. } => &donors,
        }
    };
    let source_name = match &item.source {
        IntermediateSource::Input(index) => plan.secondary()[*index].name.as_str(),
        IntermediateSource::Primary { name, .. } => name.as_str(),
        IntermediateSource::SelfRows { .. } => "SELF",
    };
    let choice = if let Some(choice) = state.intermediates.choices.get(&cache_key) {
        *choice
    } else {
        // Newly admitted expressions use the original dependency/filter boundary.
        // Ordinary selectors retain the established filter-before-raw-key behavior.
        let current = if item.has_expression_keys() {
            Some(match_values::resolve(
                item, candidate, plan, row, primary, state,
            )?)
        } else {
            None
        };
        state.budget.work(
            state
                .intermediates
                .eligible
                .len()
                .checked_add(1)
                .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
            1,
        )?;
        if !state.intermediates.eligible.contains_key(&index)
            || item
                .derivations
                .as_ref()
                .is_some_and(|derived| !derived.current.is_empty())
        {
            let eligible = eligibility(table, item, state.budget)?;
            state.budget.work(
                state
                    .intermediates
                    .eligible
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
                1,
            )?;
            state.intermediates.eligible.insert(index, eligible);
        }
        let mut count = 0;
        let mut selected = None;
        let keys = match current {
            Some(current) => match_values::evaluate(item, &current, candidate, plan, row, state)?,
            None => match_values::ordinary(item, candidate, plan, row, primary, state)?,
        };
        let range_value = item
            .range
            .as_ref()
            .map(|range| {
                relation_predicates::resolve_current(
                    range.current(),
                    range.path(),
                    primary,
                    candidate,
                    plan,
                    row,
                    assignment,
                    relation_predicates::CurrentObservation::Operand,
                    state,
                )
            })
            .transpose()?;
        state.budget.work(
            state
                .intermediates
                .eligible
                .len()
                .checked_add(1)
                .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
            1,
        )?;
        let eligible_len = state.intermediates.eligible[&index].len();
        if range_value
            .as_ref()
            .is_none_or(|value| !matches!(value, Value::Missing))
            && !keys
                .iter()
                .any(|(_, value)| matches!(value, Value::Missing))
        {
            // First count every matching record without reading order or donor fields.
            let mut matching = Vec::new();
            state.budget.work(eligible_len, 1)?;
            matching
                .try_reserve_exact(eligible_len)
                .map_err(|_| Box::new(ExecutionError::Allocation))?;
            for position in 0..eligible_len {
                let source_row = state.intermediates.eligible[&index][position];
                let mut matches = true;
                for (column, expected) in &keys {
                    let value = cell(table, item, source_row, *column, state.budget)?;
                    if yamaa_core::value::compare_present_refs(value, ValueRef::from(expected))
                        != Ok(core::cmp::Ordering::Equal)
                    {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    if let Some(filter) = item.filter.as_ref().filter(|filter| filter.correlated())
                    {
                        let owner = match &item.source {
                            IntermediateSource::Input(index) => {
                                relation_predicates::DonorOwner::Secondary(*index)
                            }
                            IntermediateSource::Primary { .. } => {
                                relation_predicates::DonorOwner::Primary
                            }
                            IntermediateSource::SelfRows { .. } => {
                                relation_predicates::DonorOwner::Completed
                            }
                        };
                        if relation_predicates::evaluate(
                            filter,
                            relation_predicates::Donor {
                                table,
                                row: source_row,
                                owner,
                            },
                            primary,
                            candidate,
                            plan,
                            row,
                            assignment,
                            state,
                        )? != yamaa_core::predicate::Truth::True
                        {
                            continue;
                        }
                    }
                    if let (Some(range), Some(value)) = (&item.range, &range_value) {
                        state.budget.work(1, 2)?;
                        let allowed = range
                            .allows(ValueRef::from(value), |column| {
                                cell(table, item, source_row, column, state.budget)
                            })
                            .map_err(|failure| match failure {
                                RelationRangeFailure::Access(error) => error,
                                RelationRangeFailure::Incomparable(types) => {
                                    Box::new(ExecutionError::ScalarDiagnostic(
                                        types.diagnostic(range.path(), &item.identifier),
                                    ))
                                }
                            })?;
                        if !allowed {
                            continue;
                        }
                    }
                    matching.push(source_row);
                }
            }
            count = matching.len();
            selected = matching.first().copied();
            if count > 1 {
                if let Some(selection) = &item.selection {
                    selected = Some(select(table, item, &matching, selection, state.budget)?);
                } else {
                    return Err(Box::new(ExecutionError::MultipleMatches {
                        path: item.path.clone(),
                        dataset: source_name.into(),
                        intermediate: item.identifier.clone(),
                        match_count: count,
                        matched_key: matched_key(table, &keys, state.budget)?,
                        identity: failure_identity(candidate, plan.keys(), row, state.budget)?,
                    }));
                }
            }
        }
        let choice = if let Some(row) = selected {
            Choice::Record {
                row,
                multiple: count > 1,
            }
        } else if item.no_match.is_some() {
            Choice::Absent
        } else {
            return Err(Box::new(ExecutionError::UnmatchedKey {
                path: item.path.clone(),
                dataset: source_name.into(),
                intermediate: item.identifier.clone(),
                matched_key: matched_key(table, &keys, state.budget)?,
                identity: failure_identity(candidate, plan.keys(), row, state.budget)?,
            }));
        };
        state.budget.work(
            state
                .intermediates
                .choices
                .len()
                .checked_add(1)
                .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
            1,
        )?;
        state.intermediates.choices.insert(cache_key, choice);
        choice
    };
    let (value, handler) = match choice {
        Choice::Record { row, multiple } => (
            own(cell(table, item, row, column, state.budget)?),
            multiple.then_some(HandlerKind::MultipleMatches),
        ),
        Choice::Absent => {
            let value = item
                .no_match
                .as_ref()
                .expect("absence handler was admitted before caching");
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            (value.clone(), Some(HandlerKind::NoMatch))
        }
    };
    if let Some(handler) = handler.filter(|_| observe_handlers) {
        state
            .handlers
            .record(
                &alloc::format!("{}.{}", assignment.path, handler.name()),
                handler,
            )
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
    }
    Ok(value)
}

/// Retain exact key values separately from output-row identity in join diagnostics.
fn matched_key<E>(
    table: &dyn TableAccess<Error = E>,
    keys: &[(usize, Value)],
    budget: &mut Budget,
) -> Result<Vec<(String, Value)>, Box<ExecutionError<E>>> {
    budget.work(1, keys.len())?;
    budget.identity(keys.iter().map(|(_, value)| value))?;
    Ok(keys
        .iter()
        .map(|(column, value)| {
            (
                table.schema().columns()[*column].name.clone(),
                value.clone(),
            )
        })
        .collect())
}

/// Charge every opaque secondary read and preserve its source coordinates.
fn cell<'a, E>(
    table: &'a dyn TableAccess<Error = E>,
    item: &Intermediate,
    row: usize,
    column: usize,
    budget: &mut Budget,
) -> Result<ValueRef<'a>, Box<ExecutionError<E>>> {
    budget.work(1, 1)?;
    let value = table.cell(row, column).map_err(|error| {
        Box::new(match item.source.input_index() {
            Some(source) => ExecutionError::SecondaryCell {
                path: item.path.clone(),
                source,
                source_row: row,
                error,
            },
            None if matches!(item.source, IntermediateSource::Primary { .. }) => {
                ExecutionError::Cell {
                    path: item.path.clone(),
                    source_row: row,
                    error,
                }
            }
            None => ExecutionError::SchemaMismatch,
        })
    })?;
    if let ValueRef::Str(text) = value {
        budget.scalar_text(text.len())?;
    }
    Ok(value)
}

/// Select one stable extremum, retaining input position as the final tie breaker.
fn select<E>(
    table: &dyn TableAccess<Error = E>,
    item: &Intermediate,
    matching: &[usize],
    selection: &SourceSelection,
    budget: &mut Budget,
) -> Result<usize, Box<ExecutionError<E>>> {
    use core::cmp::Ordering;
    let mut best = matching[0];
    for &row in &matching[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            order = windows::compare(
                cell(table, item, row, term.column, budget)?,
                cell(table, item, best, term.column, budget)?,
                term,
            );
            if order != Ordering::Equal {
                break;
            }
        }
        order = order.then_with(|| row.cmp(&best));
        if (selection.keep == Keep::First && order == Ordering::Less)
            || (selection.keep == Keep::Last && order == Ordering::Greater)
        {
            best = row;
        }
    }
    Ok(best)
}
