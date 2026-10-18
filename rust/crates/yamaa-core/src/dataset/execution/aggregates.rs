//! Relation selection and eager reducer collection share the scalar numeric route.
use super::*;
use crate::table_grouping::Key;
use crate::{
    aggregate_reduction::{reduce_values, AggregateReductionError},
    expression::{
        AggregateResult, Reducer, Reduction, ReductionOperand, Resolver as ExpressionResolver,
    },
    value::Selection,
};

struct Output<'a, E> {
    schema: &'a TableSchema,
    rows: &'a [Candidate],
    marker: std::marker::PhantomData<E>,
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
                marker: std::marker::PhantomData,
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
) -> Result<ValueRef<'a>, Box<ExecutionError<T::Error>>> {
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
) -> Result<bool, Box<ExecutionError<T::Error>>> {
    if let Some(filter) = bound.filter() {
        return crate::dataset::execution::predicates::evaluate(filter, table, source_row, &[])
            .map(|truth| truth == crate::expression::Truth::True)
            .map_err(|error| {
                Box::new(ExecutionError::Predicate {
                    source_row,
                    error: *error,
                })
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
    let reserve = |capacity: usize| {
        let mut members = Vec::new();
        members
            .try_reserve_exact(capacity)
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        Ok::<_, Box<ExecutionError<T::Error>>>(members)
    };
    match bound.relation() {
        AggregateRelation::CurrentGroup => {
            let mut members = reserve(candidate.members.len())?;
            for &source_row in &candidate.members {
                if filter(donor, bound, source_row)? {
                    members.push(source_row);
                }
            }
            Ok(SelectedRelation {
                members,
                representative: candidate.members.first().copied(),
            })
        }
        AggregateRelation::Output { group_by } => {
            let mut members = reserve(donor.row_count())?;
            let mut first = None;
            for source_row in 0..donor.row_count() {
                let mut matches = true;
                for &column in group_by {
                    if Key::from(cell(donor, bound, coordinate, source_row, column)?)
                        != Key::from(ValueRef::from(&candidate.values[column]))
                    {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    first.get_or_insert(source_row);
                    if filter(donor, bound, source_row)? {
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
            // matched, and its filter narrows only matched records
            // (LOOKUP-25).
            let values = match current {
                Some(current) => intermediates::match_values::evaluate(
                    request, &current, candidate, plan, row, primary, state,
                )?,
                None => intermediates::match_values::ordinary(
                    request, candidate, plan, row, primary, state,
                )?,
            };
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
            let probe_keys = values
                .iter()
                .map(|(column, value)| (*column, ValueRef::from(value)))
                .collect::<Vec<_>>();
            let kinds = values
                .iter()
                .map(|(column, _)| {
                    donor
                        .schema()
                        .columns()
                        .get(*column)
                        .map(|field| field.kind.into())
                })
                .collect::<Vec<_>>();
            let probe = matching::Probe {
                keys: &probe_keys,
                kinds: &kinds,
                range: bound.range().zip(range_value.as_ref().map(ValueRef::from)),
            };
            if probe.incomplete() {
                return Ok(SelectedRelation {
                    members: Vec::new(),
                    representative: None,
                });
            }
            let found = matching::candidates(
                0..donor.row_count(),
                0,
                &probe,
                |source_row, column| cell(donor, bound, coordinate, source_row, column),
                |source_row| filter(donor, bound, source_row),
            )
            .map_err(|refusal| match refusal {
                matching::Refusal::Failed(error) => error,
                matching::Refusal::Key {
                    index,
                    current,
                    donor: record,
                } => {
                    let column = values[index].0;
                    let key = keys
                        .iter()
                        .find(|key| key.source_column == column)
                        .expect("resolved key belongs to the admitted aggregate");
                    Box::new(ExecutionError::ScalarDiagnostic(
                        crate::diagnostic::lookup::key_type(
                            &key.path,
                            crate::diagnostic::lookup::Owner::Unnamed,
                            &matching::key_source(key, &donor.schema().columns()[column].name),
                            crate::result_type::ResultType::known(current),
                            crate::result_type::ResultType::known(record),
                        ),
                    ))
                }
                matching::Refusal::Range(types) => Box::new(ExecutionError::ScalarDiagnostic(
                    types.diagnostic_for(bound.range().map_or("", |range| range.path()), None),
                )),
            })?;
            // Grouping columns are key columns: the first record whose keys
            // match carries their values (FORMULA-22).
            Ok(SelectedRelation {
                members: found.rows,
                representative: found.first_key_match,
            })
        }
    }
}

/// Read a relation record's field; the arithmetic over reductions reads the
/// group's first record, and nothing when the group is empty.
struct Resolver<'a, T: TableAccess + ?Sized> {
    bound: &'a BoundAggregate,
    table: &'a T,
    coordinate: Coordinate,
    source_row: Option<usize>,
}
impl<T: TableAccess + ?Sized> ExpressionResolver for Resolver<'_, T> {
    type Error = Box<ExecutionError<T::Error>>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let value = if let Some(row) = self.source_row {
            cell(
                self.table,
                self.bound,
                self.coordinate,
                row,
                self.bound.column(identifier),
            )?
        } else {
            ValueRef::Missing
        };
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
    for (index, reduction) in bound.expression().reductions().iter().enumerate() {
        let value = if members.is_empty() {
            Value::Missing
        } else {
            match &reduction.operand {
                ReductionOperand::Records(_) => crate::reduction::count_records(members.len())
                    .map(Value::from)
                    .map_err(|_| Box::new(ExecutionError::Capacity))?,
                ReductionOperand::Value(name) => {
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
                        )?);
                    }
                    fold(
                        bound, reduction, &values, primary, candidate, plan, row, state,
                    )?
                }
                ReductionOperand::Numeric => {
                    let mut values = Vec::new();
                    values
                        .try_reserve_exact(members.len())
                        .map_err(|_| Box::new(ExecutionError::Allocation))?;
                    for &source_row in members {
                        let result = bound.expression().evaluate_operand(
                            index,
                            &mut Resolver {
                                bound,
                                table: donor,
                                coordinate,
                                source_row: Some(source_row),
                            },
                        );
                        values.push(numeric::project_result(result, candidate, plan, row)?);
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
            Ok(std::mem::replace(&mut reductions[index], Value::Missing))
        }
        AggregateResult::Numeric => {
            let result = bound.expression().evaluate_aggregate(
                &mut Resolver {
                    bound,
                    table: donor,
                    coordinate,
                    source_row: representative,
                },
                &reductions,
            );
            numeric::project_result(result, candidate, plan, row)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fold<T: TableAccess + ?Sized>(
    bound: &BoundAggregate,
    reduction: &Reduction,
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
            return Ok(own(value));
        }
        Err(error) => error,
    };
    match error {
        AggregateReductionError::Numeric(
            crate::reduction::ReductionError::IncompatibleInputType { actual, .. },
        ) => Err(Box::new(ExecutionError::ReductionType {
            path: bound.expression().spec_path().into(),
            expression: bound.expression().expression().into(),
            reducer: if reduction.reducer == Reducer::Sum {
                NumericReducer::Sum
            } else {
                NumericReducer::Mean
            },
            source: match &reduction.operand {
                ReductionOperand::Value(name) => name.clone(),
                _ => bound.expression().expression().into(),
            },
            actual,
        })),
        AggregateReductionError::Numeric(error) => {
            let identity = if matches!(&error,crate::reduction::ReductionError::Arithmetic { error,.. } if error.phase() == "derivation")
            {
                failure_identity(candidate, plan.keys(), row)?
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
                        )?
                    } else {
                        cell(
                            state.secondary[source - 1],
                            bound,
                            Coordinate::Secondary(source - 1),
                            candidate.members[0],
                            *column,
                        )?
                    };

                    group.push((name.clone(), own(value)));
                }
            }
            let diagnostic = crate::diagnostic::AggregateFinding::MultipleRecords {
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
                failure_identity(candidate, plan.keys(), row)?
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
                ReductionOperand::Value(name) => name.as_str(),
                _ => bound.expression().expression(),
            };
            let diagnostic = crate::diagnostic::AggregateFinding::IncomparableSources {
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
    state.aggregate_row = Some(Candidate {
        template: candidate.template,
        selection_identity: candidate.selection_identity,
        members: candidate.members.clone(),
        values: candidate.values.clone(),
        completed: candidate.completed.clone(),
    });
    let result = (|| {
        for &source_row in members {
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
                let value = evaluate_raw(donor, &binding.assignment, &record, plan, row, state)?;
                let converted = match convert(&value, binding.kind) {
                    Ok(value) => value,
                    Err(error) => {
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
                                        identity: failure_identity(candidate, plan.keys(), row)?,
                                    }))
                                }
                            },
                        }
                    }
                };
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
