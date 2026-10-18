//! Lazy source eligibility and named record selection within one dataset attempt.
use super::*;
#[cfg(test)]
mod cache_tests;
mod key_index;
pub(super) mod match_values;
use std::{collections::BTreeMap, rc::Rc};

/// Keep only immutable row coordinates and the inherited handler outcome.
#[derive(Clone, Copy)]
enum Choice {
    Record { row: usize, multiple: bool },
    Absent,
}

/// Fresh for every attempt, with no entries for unused intermediate declarations.
#[derive(Default)]
pub(super) struct Run {
    /// Per record, whether a filter that reads only the donor records keeps
    /// it, decided the first time the record matches (LOOKUP-8).
    narrowed: BTreeMap<usize, Vec<Option<bool>>>,
    key_indexes: BTreeMap<usize, Option<key_index::KeyIndex>>,
    choices: BTreeMap<(SelectionIdentity, usize), Choice>,
    materialized: BTreeMap<(usize, Option<SelectionIdentity>), Rc<Dataset>>,
}

impl Run {
    /// A key window observes a different relation after row templates finish.
    /// Choices and correlated materializations from template-local windows
    /// cannot identify a choice in the complete output relation.
    pub(super) fn begin_columns<E>(
        &mut self,
        plan: &DatasetPlan,
    ) -> Result<(), Box<ExecutionError<E>>> {
        if !plan.intermediates().iter().flat_map(|item| &item.keys).any(
            |key| matches!(&key.value, MatchValue::Expression(expression) if expression.windowed),
        ) {
            return Ok(());
        }

        let mut affected = Vec::new();
        affected
            .try_reserve_exact(plan.intermediates().len())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        for item in plan.intermediates() {
            let mut windowed = item.keys.iter().any(
                |key| matches!(&key.value, MatchValue::Expression(expression) if expression.windowed),
            );
            for field in item.current_fields() {
                if let CurrentRead::Intermediate { index, .. } = field.read {
                    windowed |= affected.get(index).copied().unwrap_or(true);
                }
            }
            affected.push(windowed);
        }
        self.narrowed.retain(|index, _| !affected[*index]);
        self.key_indexes.retain(|index, _| !affected[*index]);
        self.choices.retain(|(_, index), _| !affected[*index]);
        self.materialized.retain(|(index, _), _| !affected[*index]);
        Ok(())
    }
    /// Every completed template changes only SELF eligibility and match choices.
    pub(super) fn publish_self<E>(
        &mut self,
        plan: &DatasetPlan,
    ) -> Result<(), Box<ExecutionError<E>>> {
        if !plan
            .intermediates()
            .iter()
            .any(|item| item.source.is_self())
        {
            return Ok(());
        }

        self.narrowed
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
        let prepared = materialize(index, &assignment, &context, plan, 0, state)?;
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
        let eligible = eligibility(table, item)?;
        let filtered = Filtered {
            table,
            rows: &eligible,
        };
        for check in &item.unique {
            let mut columns = Vec::new();
            for &column in &check.columns {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
            let groups = partition(&filtered, &columns)
                .map_err(|error| Box::new(ExecutionError::Grouping(error)))?;
            let count = groups.iter().filter(|group| group.len() > 1).count();
            if count != 0 {
                failures.push(check.diagnostic(item, table.schema(), name, count));
            }
        }
        // The check narrowed every record; a later match reuses each answer.
        let mut narrowed = vec![Some(false); table.row_count()];
        for row in eligible {
            narrowed[row] = Some(true);
        }
        state.intermediates.narrowed.insert(index, narrowed);
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
    error: std::marker::PhantomData<E>,
}
impl<'a, E> Data<'a, E> {
    pub(super) fn new(dataset: &'a Dataset) -> Self {
        Self {
            dataset,
            error: std::marker::PhantomData,
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
    Limits::table(table.row_count(), width)
        .map_err(|error| scoped_table_boundary(error, &item.path))?;
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
            *target = own(cell(table, item, source_row, column)?);
        }
        for (index, value) in current.iter().enumerate() {
            values[output_width + index] = value.clone();
        }
        let output = values[..output_width].to_vec();
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
    // Untyped donor expressions retain their existing raw-result lifecycle.
    // Every donor handler is reported at its own path, counted each time the
    // donor records are built (COLUMN-14); typed results convert against the
    // caller's counter.
    let mut donor_handlers = HandlerCounter::default();
    for assignment in nested.columns() {
        let table = Data::<E>::new(&input);
        let mut numbers =
            windows::Evaluation::new(assignment, &candidates, nested, &table, &mut donor_handlers)?;
        let mut nested_windows = windows::Cache::default();
        for row in 0..candidates.len() {
            let table = Data::<E>::new(&input);
            let evaluated = if let Some(run) = &mut numbers {
                run.value(row, &candidates, &table, assignment, nested)
            } else {
                evaluate_raw_retained(
                    &table,
                    assignment,
                    &candidates[row],
                    nested,
                    row,
                    &mut EvaluationState {
                        primary: &table,
                        functions: state.functions,
                        secondary: &nested_secondary,
                        self_rows: &[],
                        handlers: &mut donor_handlers,
                        intermediates: &mut intermediate_run,
                        output_rows: &candidates,
                        output_offset: 0,
                        windows: &mut nested_windows,
                        aggregate_row: None,
                    },
                    false,
                )
            };
            donor_handlers
                .transfer(state.handlers)
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            let value = evaluated.map_err(|error| donor_failure(error, item, plan, nested))?;
            let value = if derived.typed_columns.contains(&assignment.column) {
                finish(
                    value,
                    assignment,
                    &candidates[row],
                    nested,
                    row,
                    state.handlers,
                )
                .map_err(|error| donor_failure(error, item, plan, nested))?
            } else {
                value
            };

            input.rows[row][assignment.column] = value.clone();
            candidates[row].values[assignment.column] = value;
            candidates[row].completed[assignment.column] = true;
        }
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
        ExecutionError::Conversion { identity, .. }
        | ExecutionError::Numeric { identity, .. }
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
            } = std::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
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
                } = std::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
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
    let prepared = materialize(index, &assignment, &context, plan, 0, state)?;
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
    let eligible = eligibility(table, item)?;
    Ok(DriverRelation {
        materialized: prepared,
        eligible,
    })
}

pub(super) fn driver_groups<E>(
    table: &dyn TableAccess<Error = E>,
    eligible: &[usize],
    keys: &[usize],
) -> Result<Vec<Vec<usize>>, Box<ExecutionError<E>>> {
    let filtered = Filtered {
        table,
        rows: eligible,
    };
    partition(&filtered, keys)
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
/// Every donor record a filter that reads only the donor records keeps: the
/// relation a row driver traverses and a `unique` verification checks, not a
/// match.
fn eligibility<E>(
    table: &dyn TableAccess<Error = E>,
    item: &Intermediate,
) -> Result<Vec<usize>, Box<ExecutionError<E>>> {
    check_deferred_filter_types(item)?;

    let mut eligible = Vec::new();
    eligible
        .try_reserve_exact(table.row_count())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for source_row in 0..table.row_count() {
        if let Some(filter) = item.filter.as_ref().filter(|filter| !filter.correlated()) {
            if !keeps(table, filter, source_row)? {
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
    let item = &plan.intermediates()[index];
    let cache_key = (candidate.selection_identity, index);
    let prepared = materialize(index, assignment, candidate, plan, row, state)?;
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
        resolve_choice(
            index,
            assignment,
            candidate,
            plan,
            row,
            primary,
            state,
            table,
            item,
            source_name,
            cache_key,
        )?
    };
    let (value, handler) = match choice {
        Choice::Record { row, multiple } => (
            own(cell(table, item, row, column)?),
            multiple.then_some(HandlerKind::MultipleMatches),
        ),
        // The declared literal, or missing by default; counted either way.
        Choice::Absent => (
            item.no_match.clone().unwrap_or(Value::Missing),
            Some(HandlerKind::NoMatch),
        ),
    };
    if let Some(handler) = handler.filter(|_| observe_handlers) {
        state
            .handlers
            .record(
                &std::format!("{}.{}", assignment.path, handler.name()),
                handler,
            )
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
    }
    Ok(value)
}

fn check_deferred_filter_types<E>(item: &Intermediate) -> Result<(), Box<ExecutionError<E>>> {
    let findings = item
        .filter
        .as_ref()
        .map(|filter| filter.deferred_type_findings())
        .unwrap_or(&[]);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(Box::new(ExecutionError::IntermediateFilterTypes(
            findings.to_vec(),
        )))
    }
}

pub(super) fn check_unobserved_filter_types<E>(
    plan: &DatasetPlan,
) -> Result<(), Box<ExecutionError<E>>> {
    for item in plan.intermediates() {
        // Keep the first failing selector's complete bounded projection rather
        // than allocate an aggregate copy of every unused selector's findings.
        check_deferred_filter_types(item)?;
    }
    Ok(())
}

/// Retain exact key values separately from output-row identity in join diagnostics.
fn matched_key<E>(
    table: &dyn TableAccess<Error = E>,
    keys: &[(usize, Value)],
) -> Result<Vec<(String, Value)>, Box<ExecutionError<E>>> {
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

/// Preserve each opaque secondary read and its source coordinates.
fn cell<'a, E>(
    table: &'a dyn TableAccess<Error = E>,
    item: &Intermediate,
    row: usize,
    column: usize,
) -> Result<ValueRef<'a>, Box<ExecutionError<E>>> {
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
    Ok(value)
}

/// Resolve and cache the record choice for one intermediate read: match the
/// records on the resolved keys and range, narrow them by the filter, then
/// select (LOOKUP-25).
#[allow(clippy::too_many_arguments)]
fn resolve_choice<T: TableAccess + ?Sized>(
    index: usize,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
    table: &dyn TableAccess<Error = T::Error>,
    item: &Intermediate,
    source_name: &str,
    cache_key: (SelectionIdentity, usize),
) -> Result<Choice, Box<ExecutionError<T::Error>>> {
    // Expression match values resolve their current references first.
    let current = if item.has_expression_keys() {
        Some(match_values::resolve(
            item, candidate, plan, row, primary, state,
        )?)
    } else {
        None
    };
    check_deferred_filter_types(item)?;
    let keys = match current {
        Some(current) => {
            match_values::evaluate(item, &current, candidate, plan, row, primary, state)?
        }
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
    let probe_keys = keys
        .iter()
        .map(|(column, value)| (*column, ValueRef::from(value)))
        .collect::<Vec<_>>();
    let kinds = keys
        .iter()
        .map(|(column, _)| item.source_kind(plan.secondary(), *column).map(Into::into))
        .collect::<Vec<_>>();
    let probe = matching::Probe {
        keys: &probe_keys,
        kinds: &kinds,
        range: item
            .range
            .as_ref()
            .zip(range_value.as_ref().map(ValueRef::from)),
    };
    let mut count = 0;
    let mut selected = None;
    if !probe.incomplete() {
        // Small relations keep the scan; the index pays off only for repeated
        // reads of a sizable relation. Plain intermediates are indexed over the
        // full key tuple, for any number of key columns and any scalar type.
        // Expression keys, ranges, derivations and correlated filters stay on
        // the scan, as do SELF sources and key pairs whose types are not known
        // to compare (the scan owns that refusal).
        let use_index = table.row_count() >= key_index::MINIMUM_ROWS
            && !item.keys.is_empty()
            && !item.has_expression_keys()
            && !item.source.is_self()
            && item.range.is_none()
            && item.derivations.is_none()
            && item
                .filter
                .as_ref()
                .is_none_or(|filter| !filter.correlated())
            && keys.iter().all(|(column, expected)| {
                match (
                    expected.value_type(),
                    item.source_kind(plan.secondary(), *column),
                ) {
                    (Some(expected), Some(kind)) => {
                        crate::value::comparable_types(expected, kind.into())
                    }
                    (Some(_), None) => true,
                    _ => false,
                }
            });
        let mut indexed = None;
        if use_index {
            if let std::collections::btree_map::Entry::Vacant(entry) =
                state.intermediates.key_indexes.entry(index)
            {
                let rows = (0..table.row_count()).collect::<Vec<_>>();
                entry.insert(key_index::KeyIndex::build(table, item, &rows)?);
            }
            // A donor cell whose type fits no column type leaves the relation
            // unindexed; the scan below owns its outcome.
            if let Some(index) = state
                .intermediates
                .key_indexes
                .get(&index)
                .and_then(Option::as_ref)
            {
                indexed = Some(index.matching(&keys)?.to_vec());
            }
        }
        let capacity = indexed.as_ref().map_or(table.row_count(), Vec::len);
        // The index has already matched its bucket on every key pair, and an
        // indexed lookup declares no range: only narrowing remains.
        let bucket = matching::Probe {
            keys: &[],
            kinds: &[],
            range: None,
        };
        let (rows, probe): (Box<dyn Iterator<Item = usize>>, _) = match indexed {
            Some(rows) => (Box::new(rows.into_iter()), &bucket),
            None => (Box::new(0..table.row_count()), &probe),
        };
        // A filter that reads only the donor records keeps one answer per
        // record, unless the records are derived for this current record.
        let reusable = item
            .derivations
            .as_ref()
            .is_none_or(|derived| derived.current.is_empty());
        let found = matching::candidates(
            rows,
            capacity,
            probe,
            |source_row, column| cell(table, item, source_row, column),
            |source_row| {
                let Some(filter) = &item.filter else {
                    return Ok(true);
                };
                if filter.correlated() {
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
                    return Ok(relation_predicates::evaluate(
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
                    )? == crate::expression::Truth::True);
                }
                if reusable {
                    if let Some(Some(kept)) = state
                        .intermediates
                        .narrowed
                        .get(&index)
                        .and_then(|narrowed| narrowed.get(source_row))
                    {
                        return Ok(*kept);
                    }
                }
                let kept = keeps(table, filter, source_row)?;
                if reusable {
                    let narrowed = state
                        .intermediates
                        .narrowed
                        .entry(index)
                        .or_insert_with(|| vec![None; table.row_count()]);
                    if let Some(slot) = narrowed.get_mut(source_row) {
                        *slot = Some(kept);
                    }
                }
                Ok(kept)
            },
        )
        .map_err(|refusal| match refusal {
            matching::Refusal::Failed(error) => error,
            matching::Refusal::Key {
                index: pair,
                current,
                donor,
            } => {
                let column = keys[pair].0;
                let key = item
                    .keys
                    .iter()
                    .find(|key| key.source_column == column)
                    .expect("resolved key belongs to the admitted selector");
                Box::new(ExecutionError::ScalarDiagnostic(
                    crate::diagnostic::lookup::key_type(
                        &key.path,
                        crate::diagnostic::lookup::Owner::Intermediate(&item.identifier),
                        &matching::key_source(key, &table.schema().columns()[column].name),
                        crate::result_type::ResultType::known(current),
                        crate::result_type::ResultType::known(donor),
                    ),
                ))
            }
            matching::Refusal::Range(types) => {
                Box::new(ExecutionError::ScalarDiagnostic(types.diagnostic(
                    item.range.as_ref().map_or("", |range| range.path()),
                    &item.identifier,
                )))
            }
        })?;
        count = found.rows.len();
        selected = found.rows.first().copied();
        if count > 1 {
            if let Some(selection) = &item.selection {
                selected = Some(matching::select(
                    &found.rows,
                    selection,
                    |source_row, column| cell(table, item, source_row, column),
                )?);
            } else {
                return Err(Box::new(ExecutionError::MultipleMatches {
                    path: item.path.clone(),
                    dataset: source_name.into(),
                    intermediate: item.identifier.clone(),
                    match_count: count,
                    matched_key: matched_key(table, &keys)?,
                    identity: failure_identity(candidate, plan.keys(), row)?,
                }));
            }
        }
    }
    let choice = if let Some(row) = selected {
        Choice::Record {
            row,
            multiple: count > 1,
        }
    } else if !item.require_match {
        Choice::Absent
    } else {
        return Err(Box::new(ExecutionError::UnmatchedKey {
            path: item.path.clone(),
            dataset: source_name.into(),
            intermediate: item.identifier.clone(),
            matched_key: matched_key(table, &keys)?,
            identity: failure_identity(candidate, plan.keys(), row)?,
        }));
    };

    state.intermediates.choices.insert(cache_key, choice);
    Ok(choice)
}

/// Whether a filter that reads only the donor records keeps one of them.
fn keeps<E>(
    table: &dyn TableAccess<Error = E>,
    filter: &RelationPredicate,
    source_row: usize,
) -> Result<bool, Box<ExecutionError<E>>> {
    crate::dataset::execution::predicates::evaluate(filter.predicate(), table, source_row, &[])
        .map(|truth| truth == crate::expression::Truth::True)
        .map_err(|error| {
            Box::new(ExecutionError::Predicate {
                source_row,
                error: *error,
            })
        })
}
