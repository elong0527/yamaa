//! Relation selection and eager reducer collection share the scalar numeric route.
use super::*;
use crate::table_grouping::Key;
use yamaa_core::{
    aggregate_compiler::{AggregateOperand, AggregateResult, CompiledReduction},
    aggregate_parser::Reducer,
    aggregate_reduction::{reduce_values, AggregateReductionError},
    evaluation::NumericResolver,
    value::Selection,
};

struct Output<'a, E> {
    schema: &'a TableSchema,
    rows: &'a [Candidate],
    marker: core::marker::PhantomData<E>,
}
impl<E> TableAccess for Output<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.rows
            .get(row)
            .and_then(|record| record.values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
#[derive(Clone, Copy)]
enum Coordinate {
    Primary,
    Secondary(usize),
    Output,
}
struct SelectedRelation {
    members: Vec<usize>,
    representative: Option<usize>,
}

pub(super) fn evaluate<T: TableAccess + ?Sized>(
    bound: &BoundAggregate,
    primary: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, bound.expression().node_count())?;
    match bound.relation() {
        AggregateRelation::CurrentGroup => run(
            bound,
            primary,
            Coordinate::Primary,
            primary,
            candidate,
            plan,
            row,
            state,
        ),
        AggregateRelation::Matched { source, .. } => match source {
            AggregateSource::Primary => run(
                bound,
                primary,
                Coordinate::Primary,
                primary,
                candidate,
                plan,
                row,
                state,
            ),
            AggregateSource::Secondary(index) => {
                let donor = state.secondary[*index];
                run(
                    bound,
                    donor,
                    Coordinate::Secondary(*index),
                    primary,
                    candidate,
                    plan,
                    row,
                    state,
                )
            }
        },
        AggregateRelation::Output { .. } => {
            let donor = Output {
                schema: plan.output(),
                rows: state.output_rows,
                marker: core::marker::PhantomData,
            };
            run(
                bound,
                &donor,
                Coordinate::Output,
                primary,
                candidate,
                plan,
                row,
                state,
            )
        }
    }
}

fn cell<'a, T: TableAccess + ?Sized>(
    table: &'a T,
    bound: &BoundAggregate,
    coordinate: Coordinate,
    row: usize,
    column: usize,
    budget: &mut Budget,
) -> Result<ValueRef<'a>, Box<ExecutionError<T::Error>>> {
    budget.work(1, 1)?;
    table.cell(row, column).map_err(|error| {
        Box::new(match coordinate {
            Coordinate::Secondary(source) => ExecutionError::SecondaryCell {
                path: bound.expression().spec_path().into(),
                source,
                source_row: row,
                error,
            },
            _ => ExecutionError::Cell {
                path: bound.expression().spec_path().into(),
                source_row: row,
                error,
            },
        })
    })
}
fn filter<T: TableAccess + ?Sized>(
    table: &T,
    bound: &BoundAggregate,
    source_row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<bool, Box<ExecutionError<T::Error>>> {
    if let Some(filter) = bound.filter() {
        return crate::dataset_predicate::evaluate(
            filter,
            table,
            source_row,
            &[],
            state.budget.predicate(),
        )
        .map(|truth| truth == yamaa_core::predicate::Truth::True)
        .map_err(|error| match error.kind {
            yamaa_core::predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
            _ => Box::new(ExecutionError::Predicate { source_row, error }),
        });
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn select<T: TableAccess + ?Sized, P: TableAccess<Error = T::Error> + ?Sized>(
    bound: &BoundAggregate,
    donor: &T,
    coordinate: Coordinate,
    primary: &P,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<SelectedRelation, Box<ExecutionError<T::Error>>> {
    let capacity = if matches!(bound.relation(), AggregateRelation::CurrentGroup) {
        candidate.members.len()
    } else {
        donor.row_count()
    };
    state.budget.work(capacity, 1)?;
    let mut members = Vec::new();
    members
        .try_reserve_exact(capacity)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    match bound.relation() {
        AggregateRelation::CurrentGroup => {
            for &source_row in &candidate.members {
                if filter(donor, bound, source_row, state)? {
                    members.push(source_row);
                }
            }
            Ok(SelectedRelation {
                members,
                representative: candidate.members.first().copied(),
            })
        }
        AggregateRelation::Output { group_by } => {
            let mut first = None;
            for source_row in 0..donor.row_count() {
                let mut matches = true;
                for &column in group_by {
                    if Key::from(cell(
                        donor,
                        bound,
                        coordinate,
                        source_row,
                        column,
                        state.budget,
                    )?) != Key::from(ValueRef::from(&candidate.values[column]))
                    {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    first.get_or_insert(source_row);
                    if filter(donor, bound, source_row, state)? {
                        members.push(source_row);
                    }
                }
            }
            Ok(SelectedRelation {
                members,
                representative: first,
            })
        }
        AggregateRelation::Matched { keys, .. } => {
            let request =
                intermediates::match_values::Request::new(bound.expression().spec_path(), keys);
            let current = if keys.iter().any(|key| key.value.is_expression()) {
                Some(intermediates::match_values::resolve(
                    request, candidate, plan, row, primary, state,
                )?)
            } else {
                None
            };
            // Aggregate match values are completed before the right side is
            // selected. Its predicate visits only matched donor records.
            let values = match current {
                Some(current) => intermediates::match_values::evaluate(
                    request, &current, candidate, plan, row, state,
                )?,
                None => intermediates::match_values::ordinary(
                    request, candidate, plan, row, primary, state,
                )?,
            };
            if values.iter().any(|(_, value)| *value == Value::Missing) {
                return Ok(SelectedRelation {
                    members,
                    representative: None,
                });
            }
            let range_value = bound
                .range()
                .map(|range| {
                    let assignment = Assignment {
                        column: 0,
                        expression: Expression::Literal(Value::Missing),
                        path: range.path().into(),
                    };
                    relation_predicates::resolve_current(
                        range.current(),
                        range.path(),
                        primary,
                        candidate,
                        plan,
                        row,
                        &assignment,
                        relation_predicates::CurrentObservation::Operand,
                        state,
                    )
                })
                .transpose()?;
            let mut first = None;
            for source_row in 0..donor.row_count() {
                let mut matches = true;
                for (column, expected) in &values {
                    if yamaa_core::value::compare_present_refs(
                        cell(donor, bound, coordinate, source_row, *column, state.budget)?,
                        ValueRef::from(expected),
                    ) != Ok(core::cmp::Ordering::Equal)
                    {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    first.get_or_insert(source_row);
                    if filter(donor, bound, source_row, state)?
                        && narrow(
                            bound,
                            range_value.as_ref(),
                            donor,
                            coordinate,
                            source_row,
                            state,
                        )?
                    {
                        members.push(source_row);
                    }
                }
            }
            Ok(SelectedRelation {
                members,
                representative: first,
            })
        }
    }
}

fn narrow<T: TableAccess + ?Sized>(
    bound: &BoundAggregate,
    value: Option<&Value>,
    donor: &T,
    coordinate: Coordinate,
    source_row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<bool, Box<ExecutionError<T::Error>>> {
    use yamaa_core::dataset::RelationRangeFailure;
    let Some(range) = bound.range() else {
        return Ok(true);
    };
    range
        .allows(
            ValueRef::from(value.expect("bound range value")),
            |column| cell(donor, bound, coordinate, source_row, column, state.budget),
        )
        .map_err(|error| match error {
            RelationRangeFailure::Access(error) => error,
            RelationRangeFailure::Incomparable(types) => Box::new(
                ExecutionError::ScalarDiagnostic(types.diagnostic_for(range.path(), None)),
            ),
        })
}

struct Resolver<'a, T: TableAccess + ?Sized> {
    bound: &'a BoundAggregate,
    table: &'a T,
    coordinate: Coordinate,
    source_row: Option<usize>,
    reductions: &'a [Value],
    budget: &'a mut Budget,
}
impl<T: TableAccess + ?Sized> NumericResolver for Resolver<'_, T> {
    type Error = Box<ExecutionError<T::Error>>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.budget.work(1, 1)?;
        let value = if let Some(index) = self
            .bound
            .expression()
            .reductions()
            .iter()
            .position(|reduction| reduction.reference == identifier)
        {
            ValueRef::from(&self.reductions[index])
        } else if let Some(row) = self.source_row {
            cell(
                self.table,
                self.bound,
                self.coordinate,
                row,
                self.bound.column(identifier),
                self.budget,
            )?
        } else {
            ValueRef::Missing
        };
        if let ValueRef::Str(value) = value {
            self.budget.scalar_text(value.len())?;
        }
        Ok(Selection::Present(own(value)))
    }
}

#[allow(clippy::too_many_arguments)]
fn run<T: TableAccess + ?Sized, P: TableAccess<Error = T::Error> + ?Sized>(
    bound: &BoundAggregate,
    donor: &T,
    coordinate: Coordinate,
    primary: &P,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let SelectedRelation {
        members,
        representative,
    } = select(
        bound, donor, coordinate, primary, candidate, plan, row, state,
    )?;
    if let Some(schema) = bound.derived_schema() {
        let values = derive_records(bound, donor, &members, primary, candidate, plan, row, state)?;
        let enriched = Enriched {
            donor,
            members: &members,
            values: &values,
            schema,
        };
        return reduce_selected(
            bound,
            &enriched,
            coordinate,
            &members,
            representative,
            primary,
            candidate,
            plan,
            row,
            state,
        );
    }
    reduce_selected(
        bound,
        donor,
        coordinate,
        &members,
        representative,
        primary,
        candidate,
        plan,
        row,
        state,
    )
}
#[allow(clippy::too_many_arguments)]
fn reduce_selected<T: TableAccess + ?Sized, P: TableAccess<Error = T::Error> + ?Sized>(
    bound: &BoundAggregate,
    donor: &T,
    coordinate: Coordinate,
    members: &[usize],
    representative: Option<usize>,
    primary: &P,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let mut reductions = Vec::new();
    reductions
        .try_reserve_exact(bound.expression().reductions().len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for reduction in bound.expression().reductions() {
        let value = if members.is_empty() {
            Value::Missing
        } else {
            match &reduction.operand {
                AggregateOperand::Records(_) => yamaa_core::reduction::count_records(members.len())
                    .map(Value::from)
                    .map_err(|_| Box::new(ExecutionError::Capacity))?,
                AggregateOperand::Value(name) => {
                    let mut values = Vec::new();
                    values
                        .try_reserve_exact(members.len())
                        .map_err(|_| Box::new(ExecutionError::Allocation))?;
                    for &source_row in members {
                        values.push(cell(
                            donor,
                            bound,
                            coordinate,
                            source_row,
                            bound.column(name),
                            state.budget,
                        )?);
                    }
                    fold(
                        bound, reduction, &values, primary, candidate, plan, row, state,
                    )?
                }
                AggregateOperand::Numeric(expression) => {
                    let mut values = Vec::new();
                    values
                        .try_reserve_exact(members.len())
                        .map_err(|_| Box::new(ExecutionError::Allocation))?;
                    for &source_row in members {
                        state.budget.work(1, expression.node_count())?;
                        let result = expression.evaluate(&mut Resolver {
                            bound,
                            table: donor,
                            coordinate,
                            source_row: Some(source_row),
                            reductions: &[],
                            budget: state.budget,
                        });
                        values.push(numeric::project_result(
                            result,
                            candidate,
                            plan,
                            row,
                            state.budget,
                        )?);
                    }
                    let refs = values.iter().map(ValueRef::from).collect::<Vec<_>>();
                    fold(
                        bound, reduction, &refs, primary, candidate, plan, row, state,
                    )?
                }
            }
        };
        reductions.push(value);
    }
    // Every reducer runs before a lazy surrounding numeric expression resolves.
    match bound.expression().result() {
        AggregateResult::Reduction(index) => {
            Ok(core::mem::replace(&mut reductions[*index], Value::Missing))
        }
        AggregateResult::Numeric(expression) => {
            state.budget.work(1, expression.node_count())?;
            let result = expression.evaluate(&mut Resolver {
                bound,
                table: donor,
                coordinate,
                source_row: representative,
                reductions: &reductions,
                budget: state.budget,
            });
            numeric::project_result(result, candidate, plan, row, state.budget)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fold<T: TableAccess + ?Sized>(
    bound: &BoundAggregate,
    reduction: &CompiledReduction,
    values: &[ValueRef<'_>],
    primary: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let result = reduce_values(values, reduction.reducer, bound.expression().expression());
    let error = match result {
        Ok(value) => {
            if let ValueRef::Str(value) = value {
                state.budget.scalar_text(value.len())?;
            }
            return Ok(own(value));
        }
        Err(error) => error,
    };
    match error {
        AggregateReductionError::Numeric(
            yamaa_core::reduction::ReductionError::IncompatibleInputType { actual, .. },
        ) => Err(Box::new(ExecutionError::ReductionType {
            path: bound.expression().spec_path().into(),
            expression: bound.expression().expression().into(),
            reducer: if reduction.reducer == Reducer::Sum {
                NumericReducer::Sum
            } else {
                NumericReducer::Mean
            },
            source: match &reduction.operand {
                AggregateOperand::Value(name) => name.clone(),
                _ => bound.expression().expression().into(),
            },
            actual,
        })),
        AggregateReductionError::Numeric(error) => {
            let identity = if matches!(&error,yamaa_core::reduction::ReductionError::Arithmetic { error,.. } if error.phase() == "derivation")
            {
                failure_identity(candidate, plan.keys(), row, state.budget)?
            } else {
                None
            };
            Err(Box::new(ExecutionError::Reduction {
                path: bound.expression().spec_path().into(),
                error: TableReductionError::Reduction(error),
                identity,
            }))
        }
        AggregateReductionError::MultipleRecords { count } => {
            let context = bound.row_context().or_else(|| {
                candidate
                    .template
                    .and_then(|template| plan.row_context(template))
            });
            let mut group = Vec::new();
            if let Some(context) = context {
                group
                    .try_reserve_exact(context.groups.len())
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                for (name, column) in &context.groups {
                    let source = if bound.phase() == AggregatePhase::Derivation {
                        candidate
                            .template
                            .map(|template| plan.templates()[template].mode.source_index())
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let value = if source == 0 {
                        cell(
                            primary,
                            bound,
                            Coordinate::Primary,
                            candidate.members[0],
                            *column,
                            state.budget,
                        )?
                    } else {
                        cell(
                            state.secondary[source - 1],
                            bound,
                            Coordinate::Secondary(source - 1),
                            candidate.members[0],
                            *column,
                            state.budget,
                        )?
                    };
                    state.budget.identity_refs(core::iter::once(value))?;
                    group.push((name.clone(), own(value)));
                }
            }
            let diagnostic = yamaa_core::diagnostic::AggregateFinding::MultipleRecords {
                path: bound.operation_path(),
                expression: bound.expression().expression(),
                phase: bound.phase(),
                count,
                row: context.map(|context| context.row.as_str()),
                group: &group,
            }
            .diagnostic()
            .map_err(|_| Box::new(ExecutionError::Capacity))?;
            let identity = if bound.phase() == AggregatePhase::Derivation {
                failure_identity(candidate, plan.keys(), row, state.budget)?
            } else {
                None
            };
            Err(Box::new(ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            }))
        }
        AggregateReductionError::IncompatibleInputs { .. } => {
            let types = values
                .iter()
                .filter_map(|value| value.value_type())
                .collect::<Vec<_>>();
            let source = match &reduction.operand {
                AggregateOperand::Value(name) => name.as_str(),
                _ => bound.expression().expression(),
            };
            let diagnostic = yamaa_core::diagnostic::AggregateFinding::IncomparableSources {
                path: bound.expression().spec_path(),
                expression: bound.expression().expression(),
                source,
                types: &types,
            }
            .diagnostic()
            .map_err(|_| Box::new(ExecutionError::Capacity))?;
            Err(Box::new(ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity: None,
            }))
        }
    }
}

/// Keep the original donor records borrowed; retain only newly bound values.
struct Enriched<'a, T: TableAccess + ?Sized> {
    donor: &'a T,
    members: &'a [usize],
    values: &'a [Vec<Value>],
    schema: &'a TableSchema,
}
impl<T: TableAccess + ?Sized> TableAccess for Enriched<'_, T> {
    type Error = T::Error;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        self.donor.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<T::Error>> {
        if column < self.donor.schema().columns().len() {
            self.donor.cell(row, column)
        } else {
            self.members
                .binary_search(&row)
                .ok()
                .and_then(|index| self.values.get(index))
                .and_then(|record| record.get(column - self.donor.schema().columns().len()))
                .map(ValueRef::from)
                .ok_or(CellError::OutOfBounds { row, column })
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn derive_records<T: TableAccess + ?Sized, P: TableAccess<Error = T::Error> + ?Sized>(
    bound: &BoundAggregate,
    donor: &T,
    members: &[usize],
    primary: &P,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Vec<Vec<Value>>, Box<ExecutionError<T::Error>>> {
    use crate::numeric_lifecycle::{recover_conversion, ConversionRecoveryError};
    state.budget.work(
        members.len(),
        bound
            .derivations()
            .len()
            .checked_add(candidate.values.len())
            .ok_or_else(|| Box::new(ExecutionError::Capacity))?,
    )?;
    let mut enriched = Vec::new();
    enriched
        .try_reserve_exact(members.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    if members.is_empty() {
        return Ok(enriched);
    }
    // Record derivations operate on donor fields while named reads match against
    // the enclosing row. Restore nested evaluation context even when it fails.
    let previous = state.aggregate_row.take();
    for value in &candidate.values {
        if let Value::Str(text) = value {
            state.budget.scalar_text(text.len())?;
        }
    }
    state.aggregate_row = Some(Candidate {
        template: candidate.template,
        selection_identity: candidate.selection_identity,
        members: candidate.members.clone(),
        values: candidate.values.clone(),
        completed: candidate.completed.clone(),
    });
    let result = (|| {
        for &source_row in members {
            for value in &candidate.values {
                if let Value::Str(text) = value {
                    state.budget.scalar_text(text.len())?;
                }
            }
            let mut record = Candidate {
                template: candidate.template,
                selection_identity: candidate.selection_identity,
                members: vec![source_row],
                values: candidate.values.clone(),
                completed: candidate.completed.clone(),
            };
            record.values.resize(
                candidate.values.len() + bound.derivations().len(),
                Value::Missing,
            );
            record.completed.resize(record.values.len(), false);
            for binding in bound.derivations() {
                let value = evaluate_raw(
                    donor,
                    &binding.assignment,
                    &record,
                    plan,
                    row,
                    state.budget.limits(),
                    state,
                )?;
                let converted = match convert(&value, binding.kind) {
                    Ok(value) => value,
                    Err(error) => {
                        if let Some(handler) = &binding.handler {
                            if let Value::Str(text) = &handler.value {
                                state.budget.scalar_text(text.len())?;
                            }
                        }
                        match recover_conversion(
                            binding.kind,
                            &binding.path,
                            error,
                            binding.handler.as_ref(),
                            state.handlers,
                        ) {
                            Ok(value) => value,
                            Err(error) => match *error {
                                ConversionRecoveryError::Accounting { error, .. } => {
                                    return Err(Box::new(ExecutionError::HandlerAccounting(error)))
                                }
                                ConversionRecoveryError::Conversion { spec_path, error }
                                | ConversionRecoveryError::HandlerConversion {
                                    spec_path,
                                    replacement: error,
                                    ..
                                } => {
                                    return Err(Box::new(ExecutionError::Conversion {
                                        path: spec_path,
                                        output_row: row,
                                        error,
                                        identity: failure_identity(
                                            candidate,
                                            plan.keys(),
                                            row,
                                            state.budget,
                                        )?,
                                    }))
                                }
                            },
                        }
                    }
                };
                if let Value::Str(text) = &converted {
                    state.budget.scalar_text(text.len())?;
                }
                record.values[binding.assignment.column] = converted;
                record.completed[binding.assignment.column] = true;
            }
            enriched.push(record.values.split_off(candidate.values.len()));
        }
        Ok(enriched)
    })();
    state.aggregate_row = previous;
    let _ = primary;
    result
}
