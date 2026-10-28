//! Windows over completed rows: one run per window and unit, partitioned
//! once, each partition completed when its first row is read.
use super::buffer::Values;
use super::{Cx, Fail, Run};
use crate::dataset::execution::{ExecutionError, Resource};
use crate::dataset::resolved::{ExprId, ExprKind, Op, WindowId};
use crate::dataset::{Window, WindowKind};
use crate::limits::Limits;
use crate::table::{TableAccess, TableSchema, ValueRef};
use crate::value::Value;
use std::cmp::Ordering;
use std::collections::HashMap;

/// The window runs of the unit being evaluated.
#[derive(Default)]
pub(super) struct Runs {
    runs: HashMap<WindowId, WindowRun>,
    /// Output aggregates' indexes and filter answers, kept for the unit as
    /// window runs are: the frame values they read do not change in one.
    pub(super) outputs: super::aggregates::Outputs,
}

/// A small cached answer: never a cloned payload.
#[derive(Clone, Copy)]
enum Answer {
    Missing,
    Flag,
    AmbiguousBaseline {
        row: usize,
        date: usize,
        count: usize,
    },
    Number(i64),
    Cell {
        row: usize,
        column: usize,
    },
}

/// One window over the rows of its scope: frame rows `start..start + len`,
/// positions index them. A context window appends the collected driver
/// fields after the output columns.
struct WindowRun {
    window: Window,
    start: usize,
    width: usize,
    collected: Vec<Vec<Value>>,
    schema: Option<TableSchema>,
    groups: Vec<Option<Vec<usize>>>,
    group_for_row: Vec<usize>,
    answers: Vec<Answer>,
}

impl WindowRun {
    fn get<'f>(
        &'f self,
        cx: &'f Cx<'_, impl Sized>,
        position: usize,
        column: usize,
    ) -> ValueRef<'f> {
        if column < self.width {
            cx.frame.get(self.start + position, column)
        } else {
            ValueRef::from(&self.collected[position][column - self.width])
        }
    }
}

/// The window payload of a window node, and its assignment's column.
fn payload(
    plan: &crate::dataset::resolved::ResolvedPlan,
    expr: ExprId,
) -> (&Window, Option<&crate::dataset::ContextWindow>, usize) {
    match &plan.expr(expr).op {
        Op::Window { window, column } => (window, None, *column),
        Op::ContextWindow { context, column } => (&context.window, Some(context), *column),
        _ => unreachable!("a window node holds its window"),
    }
}

fn window_id(plan: &crate::dataset::resolved::ResolvedPlan, expr: ExprId) -> Option<WindowId> {
    match plan.expr(expr).kind {
        ExprKind::Window(window) | ExprKind::ContextWindow(window) => Some(window),
        _ => None,
    }
}

/// Partition the scope once, before any row's result is converted. A step
/// whose binding is a window builds it before its first row.
pub(super) fn prepare<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
) -> Result<(), Fail<E>> {
    let Some(id) = window_id(run.plan, expr) else {
        return Ok(());
    };
    let built = build(run, cx, expr)?;
    run.windows.runs.insert(id, built);
    Ok(())
}

fn build<E>(run: &mut Run<'_, '_, E>, cx: &Cx<'_, E>, expr: ExprId) -> Result<WindowRun, Fail<E>> {
    let plan = run.plan;
    let (window, context, _) = payload(plan, expr);
    let path = plan.expr(expr).path.to_string();
    let start = cx.output.start;
    let len = cx.output.len();
    let width = cx.frame.width();
    let mut collected = Vec::new();
    let mut schema = None;
    if let Some(context) = context {
        let augmented = context
            .schema(cx.schema, cx.table.schema())
            .map_err(|_| Box::new(ExecutionError::Capacity))?;
        Limits::table(len, augmented.columns().len()).map_err(|error| {
            Box::new(ExecutionError::ScopedLimit {
                resource: match error.resource {
                    "columns" => Resource::SourceColumns,
                    "table_rows" => Resource::SourceRows,
                    "table_cells" => Resource::SourceCells,
                    _ => unreachable!("table boundary resource"),
                },
                limit: error.limit,
                required: error.actual,
                spec_paths: vec![path.clone()],
            })
        })?;
        collected.reserve(len);
        for position in 0..len {
            let row = start + position;
            let mut values = Vec::with_capacity(context.sources.len());
            for (name, column) in &context.sources {
                let value = super::predicates::collect_row(
                    run,
                    cx,
                    super::predicates::Collection {
                        column: *column,
                        identifier: name,
                        filter: None,
                        selection: None,
                        path: &path,
                        site: None,
                    },
                    row,
                )
                .map_err(|mut error| {
                    // The collection's row is its position in the window's scope.
                    if let ExecutionError::MultipleValues {
                        identity: Some(identity),
                        ..
                    } = error.as_mut()
                    {
                        identity.position = position;
                    }
                    error
                })?;
                values.push(value);
            }
            collected.push(values);
        }
        schema = Some(augmented);
    }
    if i64::try_from(len).is_err() {
        return Err(Box::new(ExecutionError::Capacity));
    }
    let mut built = WindowRun {
        window: window.clone(),
        start,
        width,
        collected,
        schema,
        groups: Vec::new(),
        group_for_row: Vec::new(),
        answers: vec![Answer::Missing; len],
    };
    let groups: Vec<Vec<usize>> = if window.group_by.is_empty() {
        vec![(0..len).collect()]
    } else {
        let width = built.schema.as_ref().unwrap_or(cx.schema).columns().len();
        super::group::admit(&window.group_by, width, len, None)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
        let scope = &built;
        super::group::partition(len, window.group_by.len(), |position, key| {
            scope.get(cx, position, window.group_by[key])
        })
        .map_err(|_| {
            Box::new(ExecutionError::OutputGrouping(
                crate::table_grouping::GroupingError::Allocation,
            ))
        })?
        .into_vecs()
    };
    let mut group_for_row = super::filled(len, 0)?;
    for (group, members) in groups.iter().enumerate() {
        for &position in members {
            group_for_row[position] = group;
        }
    }
    built.groups = groups.into_iter().map(Some).collect();
    built.group_for_row = group_for_row;
    Ok(built)
}

/// A binding whose expression is a window: its prepared run's answers.
pub(super) fn direct<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Option<Values>, Fail<E>> {
    let Some(id) = window_id(run.plan, expr) else {
        return Ok(None);
    };
    if !run.windows.runs.contains_key(&id) {
        prepare(run, cx, expr)?;
    }
    answers(run, cx, expr, id, rows).map(Some)
}

/// A window read inside another expression: its run is built when a row
/// first reaches it. Building takes the whole run, so no map entry is held.
#[allow(clippy::map_entry)]
pub(super) fn value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let id = window_id(run.plan, expr).expect("a window node");
    if !run.windows.runs.contains_key(&id) {
        let built = build(run, cx, expr)?;
        run.windows.runs.insert(id, built);
    }
    answers(run, cx, expr, id, rows)
}

fn answers<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    id: WindowId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let mut built = run
        .windows
        .runs
        .remove(&id)
        .expect("the reached window has a run");
    let result = (|| {
        let mut values = Values::missing(0);
        for &row in rows {
            let position = row - built.start;
            complete(&mut built, cx, position)?;
            values.push_value(answer(plan, &built, cx, expr, position)?);
        }
        Ok(values)
    })();
    run.windows.runs.insert(id, built);
    result
}

/// Complete the partition of one position, sorting it and evaluating every
/// filter in sorted order before numbering any row.
fn complete<E>(built: &mut WindowRun, cx: &Cx<'_, E>, position: usize) -> Result<(), Fail<E>> {
    let group = built.group_for_row[position];
    let Some(mut members) = built.groups[group].take() else {
        return Ok(());
    };
    let window = built.window.clone();
    // A baseline's tie-break terms compare only eligible rows sharing the
    // latest date; its partition keeps construction order.
    let baseline = matches!(window.kind, WindowKind::BaselineFlag { .. });
    if !baseline && !window.order_by.is_empty() {
        // Members are in construction order, which a tie keeps (VALUE-23).
        super::relations::sort(&mut members, &window.order_by, |&member, term| {
            built.get(cx, member, window.order_by[term].column)
        })
        .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
    }
    if let Some(filter) = &window.filter {
        let mut eligible = Vec::with_capacity(members.len());
        for &member in &members {
            let row = built.start + member;
            let source_row = cx.frame.members(row)[0];
            let truth = super::predicates::row_truth(filter, cx.table, source_row, |column| {
                built.get(cx, member, column)
            })
            .map_err(|error| {
                Box::new(ExecutionError::Predicate {
                    source_row,
                    error: *error,
                })
            })?;
            if truth == crate::expression::Truth::True {
                eligible.push(member);
            }
        }
        members = eligible;
    }
    match window.kind {
        WindowKind::BaselineFlag {
            date,
            reference_date,
            value,
            inclusive,
        } => {
            let mut latest: Option<usize> = None;
            let mut count = 0;
            for &member in &members {
                let candidate = built.get(cx, member, date);
                let reference = built.get(cx, member, reference_date);
                if matches!(candidate, ValueRef::Missing)
                    || matches!(reference, ValueRef::Missing)
                    || matches!(built.get(cx, member, value), ValueRef::Missing)
                {
                    continue;
                }
                match temporal_order(candidate, reference)? {
                    Ordering::Greater => continue,
                    Ordering::Equal if !inclusive => continue,
                    _ => {}
                }
                let order = match latest {
                    None => Ordering::Greater,
                    Some(best) => match temporal_order(candidate, built.get(cx, best, date))? {
                        Ordering::Equal => ordered(built, cx, &window, member, best)?,
                        order => order,
                    },
                };
                if order == Ordering::Greater {
                    latest = Some(member);
                    count = 1;
                } else if order == Ordering::Equal {
                    count += 1;
                }
            }
            if let Some(row) = latest {
                if count > 1 {
                    for &member in &members {
                        built.answers[member] = Answer::AmbiguousBaseline { row, date, count };
                    }
                } else {
                    built.answers[row] = Answer::Flag;
                }
            }
        }
        WindowKind::RowValue { column, offset } => {
            for (index, &member) in members.iter().enumerate() {
                let target = usize::try_from(index as i128 + i128::from(offset)).ok();
                if let Some(&row) = target.and_then(|target| members.get(target)) {
                    built.answers[member] = Answer::Cell { row, column };
                }
            }
        }
        WindowKind::Locf {
            column,
            include_current,
        } => {
            let mut previous = None;
            for &member in &members {
                let present = !matches!(built.get(cx, member, column), ValueRef::Missing);
                let donor = if present && include_current {
                    Some(member)
                } else {
                    previous
                };
                if let Some(row) = donor {
                    built.answers[member] = Answer::Cell { row, column };
                }
                if present {
                    previous = Some(member);
                }
            }
        }
        WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense => {
            let mut rank = 0;
            for (index, &member) in members.iter().enumerate() {
                let new_rank = index == 0
                    || window.kind == WindowKind::RowNumber
                    || ordered(built, cx, &window, members[index - 1], member)? != Ordering::Equal;
                if new_rank {
                    rank = if window.kind == WindowKind::Dense {
                        rank + 1
                    } else {
                        index as i64 + 1
                    };
                }
                built.answers[member] = Answer::Number(rank);
            }
        }
    }
    Ok(())
}

fn answer<E>(
    plan: &crate::dataset::resolved::ResolvedPlan,
    built: &WindowRun,
    cx: &Cx<'_, E>,
    expr: ExprId,
    position: usize,
) -> Result<Value, Fail<E>> {
    Ok(match built.answers[position] {
        Answer::Missing => Value::Missing,
        Answer::Flag => Value::Str("Y".into()),
        Answer::AmbiguousBaseline {
            row: donor,
            date,
            count,
        } => {
            let (window, _, column) = payload(plan, expr);
            let path = plan.expr(expr).path.to_string();
            let column_name = cx
                .schema
                .columns()
                .get(column)
                .map(|column| column.name.clone())
                .unwrap_or_else(|| {
                    path.rsplit_once(".key.")
                        .and_then(|(_, value)| value.split('.').next())
                        .map_or_else(|| path.clone(), |field| format!("key[{field}]"))
                });
            let schema = built.schema.as_ref().unwrap_or(cx.schema);
            return Err(Box::new(ExecutionError::BaselineAmbiguity {
                path,
                column: column_name,
                date: super::eval::owned(built.get(cx, donor, date)),
                match_count: count,
                partition: window
                    .group_by
                    .iter()
                    .map(|&group| {
                        (
                            schema.columns()[group].name.clone(),
                            super::eval::owned(built.get(cx, position, group)),
                        )
                    })
                    .collect(),
            }));
        }
        Answer::Number(value) => Value::Int(value),
        Answer::Cell { row, column } => super::eval::owned(built.get(cx, row, column)),
    })
}

/// Compare declared order terms between two positions. Binding refuses a
/// term whose values do not all compare (VALUE-22); a pair whose types no
/// source decided before the run fails the run here, never a guess.
fn ordered<E>(
    built: &WindowRun,
    cx: &Cx<'_, E>,
    window: &Window,
    left: usize,
    right: usize,
) -> Result<Ordering, Fail<E>> {
    for term in &window.order_by {
        let order = super::relations::compare(
            built.get(cx, left, term.column),
            built.get(cx, right, term.column),
            term,
        )
        .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

/// A baseline's dates in order: binding admits a date and a reference of one
/// temporal type, decided before any row is read (VALUE-2); a pair of
/// another kind, whose types no source decided, fails the run.
fn temporal_order<E>(left: ValueRef<'_>, right: ValueRef<'_>) -> Result<Ordering, Fail<E>> {
    match (left, right) {
        (ValueRef::Date(left), ValueRef::Date(right)) => Ok(left.cmp(&right)),
        (ValueRef::DateTime(left), ValueRef::DateTime(right)) => Ok(left.cmp(&right)),
        _ => Err(Box::new(ExecutionError::SchemaMismatch)),
    }
}
