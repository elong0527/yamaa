//! Aggregates: reduce the records of the current group, of a matched donor
//! relation or of the completed output, optionally enriching each record
//! with derivations first.
use super::buffer::Values;
use super::eval::owned;
use super::index::{Equality, Index};
use super::relations::{candidates, Probe, Refusal};
use super::{Cx, Fail, Run, Table};
use crate::aggregate_reduction::{reduce_values, AggregateReductionError, ReductionFields};
use crate::conversion::convert;
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, ExprKind, Op, OperandRole, ResolvedPlan};
use crate::dataset::{AggregatePhase, AggregateRelation, AggregateSource, BoundAggregate};
use crate::expression::{
    AggregateResult, Reducer, Reduction, ReductionOperand, Resolver as ExpressionResolver,
};
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::table_reduction::TableReductionError;
use crate::value::{Selection, Value, ValueType};
use std::collections::hash_map::Entry;
use std::collections::HashMap;

/// Where an aggregate's donor records come from, for port failures.
#[derive(Clone, Copy)]
enum Coordinate {
    Primary,
    Secondary(usize),
    Output,
}

/// The donor record an aggregate derivation enriches: its stored fields,
/// then the derived values completed so far.
pub(super) struct Record<'a, E> {
    donor: Table<'a, E>,
    donor_width: usize,
    pub(super) source_row: usize,
    derived: &'a [Value],
}

impl<'a, E> Record<'a, E> {
    /// The record a selected row stands on: one record per evaluation.
    pub(super) fn position(&self, _row: usize) -> usize {
        0
    }

    /// A field of the record: stored, or derived earlier in declaration order.
    pub(super) fn field(
        &self,
        plan: &ResolvedPlan,
        owner: ExprId,
        _position: usize,
        field: usize,
    ) -> Result<ValueRef<'a>, Fail<E>> {
        if field >= self.donor_width {
            return Ok(ValueRef::from(&self.derived[field - self.donor_width]));
        }
        self.donor.get(self.source_row, field).map_err(|error| {
            Box::new(ExecutionError::Cell {
                path: plan.expr(owner).path.to_string(),
                source_row: self.source_row,
                error,
            })
        })
    }

    /// A derived field of the record.
    pub(super) fn derived(&self, _position: usize, field: usize) -> ValueRef<'a> {
        ValueRef::from(&self.derived[field - self.donor_width])
    }

    /// The donor table the record belongs to.
    pub(super) fn table(&self) -> Table<'a, E> {
        self.donor
    }
}

/// The output rows each output aggregate ranges over, indexed by their group
/// values, and the answers of its filter per row, by aggregate and range.
#[derive(Default)]
pub(super) struct Outputs {
    groups: HashMap<(ExprId, usize, usize), Index>,
    filters: HashMap<(ExprId, usize, usize), Vec<Option<bool>>>,
}

/// The donor records of one aggregate over the current row.
struct Selected {
    members: Vec<usize>,
    representative: Option<usize>,
}

/// An aggregate node over rows.
pub(super) fn aggregate<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let mut values = Values::missing(0);
    for &row in rows {
        values.push_value(aggregate_value(run, cx, expr, row)?);
    }
    Ok(values)
}

fn aggregate_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    row: usize,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let Op::Aggregate(bound) = &plan.expr(expr).op else {
        unreachable!("an aggregate node holds its aggregate")
    };
    let (donor, coordinate) = match bound.relation() {
        AggregateRelation::CurrentGroup
        | AggregateRelation::Matched {
            source: AggregateSource::Primary,
            ..
        } => (cx.table, Coordinate::Primary),
        AggregateRelation::Matched {
            source: AggregateSource::Secondary(index),
            ..
        } => (cx.secondary[*index], Coordinate::Secondary(*index)),
        AggregateRelation::Output { .. } => (
            Table::Rows {
                frame: cx.frame,
                schema: cx.schema,
                start: cx.output.start,
                rows: cx.output.len(),
            },
            Coordinate::Output,
        ),
    };
    let selected = select(run, cx, expr, bound, donor, coordinate, row)?;
    if let Some(schema) = bound.derived_schema() {
        let values = derive_records(run, cx, expr, donor, &selected.members, row)?;
        let enriched = Enriched {
            donor,
            members: &selected.members,
            values: &values,
            schema,
        };
        return reduce_selected(run, cx, bound, &enriched, coordinate, &selected, row);
    }
    reduce_selected(run, cx, bound, &donor, coordinate, &selected, row)
}

fn cell<'t, T: TableAccess + ?Sized>(
    table: &'t T,
    bound: &BoundAggregate,
    coordinate: Coordinate,
    row: usize,
    column: usize,
) -> Result<ValueRef<'t>, Fail<T::Error>> {
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

fn filter<E>(
    table: Table<'_, E>,
    bound: &BoundAggregate,
    source_row: usize,
) -> Result<bool, Fail<E>> {
    if let Some(filter) = bound.filter() {
        return super::predicates::row_truth(filter, table, source_row, |_| ValueRef::Missing)
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

/// The records of the current group, of the output rows sharing the row's
/// group values, or of the donor relation matched on the row's keys.
fn select<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    bound: &BoundAggregate,
    donor: Table<'_, E>,
    coordinate: Coordinate,
    row: usize,
) -> Result<Selected, Fail<E>> {
    match bound.relation() {
        AggregateRelation::CurrentGroup => {
            let all = cx.frame.members(row);
            let mut members = Vec::with_capacity(all.len());
            for &source_row in all {
                if filter(donor, bound, source_row)? {
                    members.push(source_row);
                }
            }
            Ok(Selected {
                members,
                representative: all.first().copied(),
            })
        }
        AggregateRelation::Output { group_by } => {
            // The join operator (#2842): the output rows sharing the row's
            // group values, under grouping equality, from an index over the
            // rows the aggregate ranges over; the filter then runs on them
            // in stored order, once per row.
            let range = (expr, cx.output.start, cx.output.len());
            let index = match run.windows.outputs.groups.entry(range) {
                Entry::Occupied(index) => index.into_mut(),
                Entry::Vacant(slot) => slot.insert(Index::groups(
                    group_by,
                    0..donor.row_count(),
                    |source_row, column| cell(&donor, bound, coordinate, source_row, column),
                )?),
            };
            let matched = index
                .matching(
                    group_by.iter().map(|&column| cx.frame.get(row, column)),
                    Equality::Grouping,
                )
                .to_vec();
            let mut members = Vec::with_capacity(matched.len());
            for &source_row in &matched {
                if bound.filter().is_none() {
                    members.push(source_row);
                    continue;
                }
                let answers = match run.windows.outputs.filters.entry(range) {
                    Entry::Occupied(answers) => answers.into_mut(),
                    Entry::Vacant(slot) => slot.insert(super::filled(donor.row_count(), None)?),
                };
                let kept = match answers[source_row] {
                    Some(kept) => kept,
                    None => {
                        let kept = filter(donor, bound, source_row)?;
                        run.windows
                            .outputs
                            .filters
                            .get_mut(&range)
                            .expect("answers")[source_row] = Some(kept);
                        kept
                    }
                };
                if kept {
                    members.push(source_row);
                }
            }
            Ok(Selected {
                members,
                representative: matched.first().copied(),
            })
        }
        AggregateRelation::Matched { keys, .. } => {
            let path = bound.expression().spec_path();
            let ExprKind::Aggregate(relation) = run.plan.expr(expr).kind else {
                unreachable!("an aggregate node names its relation")
            };
            let values = super::relations::match_keys(run, cx, relation, path, keys, row)?;
            let range_value = match bound.range() {
                Some(range) => Some(super::relations::current_value(
                    run,
                    cx,
                    super::relations::Site {
                        expr: Some(expr),
                        path: range.path(),
                    },
                    range.current(),
                    range.path(),
                    Some(row),
                )?),
                None => None,
            };
            let probe_keys: Vec<(usize, ValueRef<'_>)> = values
                .iter()
                .map(|(column, value)| (*column, ValueRef::from(value)))
                .collect();
            let kinds: Vec<Option<ValueType>> = values
                .iter()
                .map(|(column, _)| {
                    donor
                        .schema()
                        .columns()
                        .get(*column)
                        .map(|field| field.kind.into())
                })
                .collect();
            let probe = Probe {
                keys: &probe_keys,
                kinds: &kinds,
                range: bound.range().zip(range_value.as_ref().map(ValueRef::from)),
            };
            if probe.incomplete() {
                return Ok(Selected {
                    members: Vec::new(),
                    representative: None,
                });
            }
            // The join operator (#2842), as for an implicit join.
            let bucket =
                super::joins::bucket(run, donor, &probe_keys, &kinds, |source_row, column| {
                    cell(&donor, bound, coordinate, source_row, column)
                })?;
            let indexed = Probe {
                keys: &[],
                kinds: &[],
                range: probe.range,
            };
            let (rows, probe) = match &bucket {
                Some(rows) => (super::joins::Rows::Bucket(rows.iter()), &indexed),
                None => (super::joins::Rows::Scan(0..donor.row_count()), &probe),
            };
            let found = candidates(
                rows,
                probe,
                |source_row, column| cell(&donor, bound, coordinate, source_row, column),
                |source_row, run: &mut Run<'_, '_, E>| {
                    if bound.filter().is_none() {
                        return Ok(true);
                    }
                    // The filter reads only the record: one answer each.
                    run.ports
                        .kept(expr, donor, source_row, || filter(donor, bound, source_row))
                },
                run,
            )
            .map_err(|refusal| match refusal {
                Refusal::Failed(error) => error,
                Refusal::Key {
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
                            &super::relations::key_source(
                                key,
                                &donor.schema().columns()[column].name,
                            ),
                            crate::result_type::ResultType::known(current),
                            crate::result_type::ResultType::known(record),
                        ),
                    ))
                }
                Refusal::Range(types) => Box::new(ExecutionError::ScalarDiagnostic(
                    types.diagnostic_for(bound.range().map_or("", |range| range.path()), None),
                )),
            })?;
            // Grouping columns are key columns: the first record whose keys
            // match carries their values (FORMULA-22).
            Ok(Selected {
                members: found.rows,
                representative: found.first_key_match,
            })
        }
    }
}

/// Read a relation record's field; the arithmetic over reductions reads the
/// group's first record, and nothing when the group is empty.
struct Fields<'r, T: TableAccess + ?Sized> {
    bound: &'r BoundAggregate,
    table: &'r T,
    coordinate: Coordinate,
    source_row: Option<usize>,
}
impl<T: TableAccess + ?Sized> ExpressionResolver for Fields<'_, T> {
    type Error = Fail<T::Error>;
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
        Ok(Selection::Present(owned(value)))
    }
}

fn reduce_selected<E, T: TableAccess<Error = E> + ?Sized>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    bound: &BoundAggregate,
    donor: &T,
    coordinate: Coordinate,
    selected: &Selected,
    row: usize,
) -> Result<Value, Fail<E>> {
    // `STR_JOIN` reads the records in the declared order; every other
    // reducer, and a tie, keeps stored order (LOOKUP-10, VALUE-23).
    let joins = bound
        .expression()
        .reductions()
        .iter()
        .any(|reduction| reduction.reducer == Reducer::StrJoin);
    let ordered = if joins && !bound.order().is_empty() {
        Some(order(donor, bound, coordinate, &selected.members)?)
    } else {
        None
    };
    let mut reductions = Vec::with_capacity(bound.expression().reductions().len());
    for (index, reduction) in bound.expression().reductions().iter().enumerate() {
        let members = match &ordered {
            Some(ordered) if reduction.reducer == Reducer::StrJoin => ordered,
            _ => &selected.members,
        };
        let value = if members.is_empty() {
            Value::Missing
        } else {
            match &reduction.operand {
                ReductionOperand::Records(_) => crate::reduction::count_records(members.len())
                    .map(Value::from)
                    .map_err(|_| Box::new(ExecutionError::Capacity))?,
                ReductionOperand::Value(name) => {
                    let mut values = Vec::with_capacity(members.len());
                    for &source_row in members {
                        values.push(cell(
                            donor,
                            bound,
                            coordinate,
                            source_row,
                            bound.column(name),
                        )?);
                    }
                    fold(run, cx, bound, reduction, &values, row)?
                }
                ReductionOperand::Numeric => {
                    let mut values = Vec::with_capacity(members.len());
                    for &source_row in members {
                        let result = bound.expression().evaluate_operand(
                            index,
                            &mut Fields {
                                bound,
                                table: donor,
                                coordinate,
                                source_row: Some(source_row),
                            },
                        );
                        values.push(super::predicates::project_result(run, cx, result, row)?);
                    }
                    let refs: Vec<ValueRef<'_>> = values.iter().map(ValueRef::from).collect();
                    fold(run, cx, bound, reduction, &refs, row)?
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
                &mut Fields {
                    bound,
                    table: donor,
                    coordinate,
                    source_row: selected.representative,
                },
                &reductions,
            );
            super::predicates::project_result(run, cx, result, row)
        }
    }
}

/// The selected records in the order `STR_JOIN` joins them: by each term of
/// the declared order in turn (VALUE-22), a stable sort keeping stored order
/// among records equal on every term (VALUE-23).
fn order<T: TableAccess + ?Sized>(
    donor: &T,
    bound: &BoundAggregate,
    coordinate: Coordinate,
    members: &[usize],
) -> Result<Vec<usize>, Fail<T::Error>> {
    let terms = bound.order();
    let mut keyed = Vec::with_capacity(members.len());
    for &member in members {
        let mut values = Vec::with_capacity(terms.len());
        for term in terms {
            values.push(cell(donor, bound, coordinate, member, term.column)?);
        }
        keyed.push((member, values));
    }
    super::relations::sort(&mut keyed, terms, |(_, values), term| values[term])
        .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
    Ok(keyed.into_iter().map(|(member, _)| member).collect())
}

fn fold<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    bound: &BoundAggregate,
    reduction: &Reduction,
    values: &[ValueRef<'_>],
    row: usize,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let fields = ReductionFields {
        separator: bound.separator(),
        distinct: bound.distinct(),
        quantile: bound.quantile(),
    };
    let error = match reduce_values(
        values,
        reduction.reducer,
        fields,
        bound.expression().expression(),
    ) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    match error {
        // No column holds a value without text, such as a truth value.
        AggregateReductionError::Text { error, .. } => Err(Box::new(ExecutionError::Conversion {
            path: bound.expression().spec_path().into(),
            output_row: row,
            error,
            identity: run.failure_identity(cx, row),
        })),
        AggregateReductionError::Numeric(
            crate::reduction::ReductionError::IncompatibleInputType { actual, .. },
        ) => Err(Box::new(ExecutionError::ReductionType {
            path: bound.expression().spec_path().into(),
            expression: bound.expression().expression().into(),
            reducer: reduction.reducer,
            source: match &reduction.operand {
                ReductionOperand::Value(name) => name.clone(),
                _ => bound.expression().expression().into(),
            },
            actual,
        })),
        AggregateReductionError::Numeric(error) => {
            let identity = if matches!(&error, crate::reduction::ReductionError::Arithmetic { error, .. } if error.phase() == "derivation")
            {
                run.failure_identity(cx, row)
            } else {
                None
            };
            Err(Box::new(ExecutionError::Reduction {
                path: bound.expression().spec_path().into(),
                error: TableReductionError::Reduction(error),
                identity,
            }))
        }
        AggregateReductionError::Limit(limit) => Err(Box::new(ExecutionError::text(
            limit,
            bound.expression().spec_path(),
        ))),
        AggregateReductionError::MultipleRecords { count } => {
            let template = cx.frame.template(row);
            let template_node = template.map(|template| {
                let id = plan
                    .region_templates(cx.region)
                    .nth(template)
                    .expect("a region template");
                plan.template(id)
            });
            let context = bound
                .row_context()
                .or_else(|| template_node.and_then(|node| node.row_context.as_ref()));
            let mut group = Vec::new();
            if let Some(context) = context {
                for (name, column) in &context.groups {
                    let source = if bound.phase() == AggregatePhase::Derivation {
                        template
                            .map(|template| run.template_source(cx.region, template))
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let member = cx.frame.members(row)[0];
                    let value = if source == 0 {
                        cell(&cx.table, bound, Coordinate::Primary, member, *column)?
                    } else {
                        cell(
                            &cx.secondary[source - 1],
                            bound,
                            Coordinate::Secondary(source - 1),
                            member,
                            *column,
                        )?
                    };
                    group.push((name.clone(), owned(value)));
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
                run.failure_identity(cx, row)
            } else {
                None
            };
            Err(Box::new(ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            }))
        }
        AggregateReductionError::IncompatibleInputs { .. } => {
            let types: Vec<ValueType> = values
                .iter()
                .filter_map(|value| value.value_type())
                .collect();
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

/// The donor records borrowed, with only the newly derived values retained.
struct Enriched<'r, T: TableAccess> {
    donor: T,
    members: &'r [usize],
    values: &'r [Vec<Value>],
    schema: &'r TableSchema,
}
impl<T: TableAccess> TableAccess for Enriched<'_, T> {
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

/// Enrich every selected record with the derivations in declaration order;
/// a typed derivation converts its result before a later one reads it.
fn derive_records<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    donor: Table<'_, E>,
    members: &[usize],
    row: usize,
) -> Result<Vec<Vec<Value>>, Fail<E>> {
    let plan = run.plan;
    let derivations: Vec<ExprId> = plan
        .operands(expr)
        .iter()
        .filter(|operand| operand.role == OperandRole::Derivation)
        .map(|operand| operand.expr)
        .collect();
    let donor_width = donor.schema().columns().len();
    let mut enriched = Vec::with_capacity(members.len());
    for &source_row in members {
        let mut derived = vec![Value::Missing; derivations.len()];
        for (position, &wrapper) in derivations.iter().enumerate() {
            let Op::Derivation { kind, handler, .. } = &plan.expr(wrapper).op else {
                unreachable!("a derivation wrapper holds its conversion")
            };
            let inner = plan.operands(wrapper)[0].expr;
            let record = Record {
                donor,
                donor_width,
                source_row,
                derived: &derived,
            };
            let scope = Cx {
                record: Some(&record),
                table: donor,
                ..cx.clone()
            };
            let value = run.eval(&scope, inner, &[row], false)?.value(0);
            let converted = match convert(&value, *kind) {
                Ok(value) => value,
                Err(error) => {
                    let Some(handler) = handler else {
                        return Err(Box::new(ExecutionError::Conversion {
                            path: plan.expr(wrapper).path.to_string(),
                            output_row: row,
                            error,
                            identity: run.failure_identity(cx, row),
                        }));
                    };
                    run.fire_at(
                        Some(wrapper),
                        &handler.spec_path,
                        crate::numeric_lifecycle::HandlerKind::Unconvertible,
                    )?;
                    convert(&handler.value, *kind).map_err(|replacement| {
                        Box::new(ExecutionError::Conversion {
                            path: handler.spec_path.clone(),
                            output_row: row,
                            error: replacement,
                            identity: run.failure_identity(cx, row),
                        })
                    })?
                }
            };
            derived[position] = converted;
        }
        enriched.push(derived);
    }
    Ok(enriched)
}
