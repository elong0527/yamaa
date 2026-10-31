//! Windows over completed rows: one run per window and unit, partitioned
//! once, each partition completed when its first row is read.
use super::buffer::Values;
use super::frame::{Frame, IdentityMap};
use super::group::Grouping;
use super::{Cx, Fail, Run};
use crate::dataset::execution::{ExecutionError, Resource};
use crate::dataset::resolved::{ExprId, ExprKind, Op, WindowId};
use crate::dataset::{Window, WindowKind};
use crate::expression::Truth;
use crate::limits::Limits;
use crate::table::{TableAccess, TableSchema, ValueRef};
use crate::value::Value;
use std::cmp::Ordering;
use std::ops::Range;
use std::sync::Arc;

/// The window runs of the unit being evaluated.
#[derive(Default)]
pub(super) struct Runs {
    runs: IdentityMap<WindowId, WindowRun>,
    /// Output aggregates' indexes and filter answers, kept for the unit as
    /// window runs are: the frame values they read do not change in one.
    pub(super) outputs: super::aggregates::Outputs,
    /// The groupings of frame rows the windows and output aggregates share,
    /// one per key list and range of rows: the unit's, and in a window stage
    /// or the column phase, every later unit's whose key columns have not
    /// changed since.
    groupings: Vec<Shared>,
}

/// One grouping of frame rows `rows` by the frame columns `columns`.
struct Shared {
    columns: Vec<usize>,
    rows: Range<usize>,
    grouping: Arc<Grouping>,
}

impl Runs {
    /// The state of a new unit of the window stage or the column phase: no
    /// window run or output answer yet, and the groupings of the units
    /// before it, whose frame rows are the same and whose key columns no
    /// unit has written since.
    pub(super) fn step(&mut self) {
        let groupings = std::mem::take(&mut self.groupings);
        *self = Self {
            groupings,
            ..Self::default()
        };
    }

    /// Frame column `column` was written: no grouping by it stays.
    pub(super) fn written(&mut self, column: usize) {
        self.groupings
            .retain(|shared| !shared.columns.contains(&column));
    }

    /// The grouping of frame rows `rows` by the frame columns `columns`
    /// under grouping equality, items numbered from `rows.start`: built on
    /// first use, while the rows and key columns do not change. A grouping
    /// of the same rows by more keys, these among them, is merged into it
    /// rather than reading every row again.
    pub(super) fn grouping(
        &mut self,
        frame: &Frame,
        columns: &[usize],
        rows: Range<usize>,
    ) -> Result<Arc<Grouping>, std::collections::TryReserveError> {
        if let Some(shared) = self
            .groupings
            .iter()
            .find(|shared| shared.rows == rows && shared.columns == columns)
        {
            return Ok(Arc::clone(&shared.grouping));
        }
        let start = rows.start;
        let finer = self.groupings.iter().find(|shared| {
            shared.rows == rows && columns.iter().all(|column| shared.columns.contains(column))
        });
        let keys: Vec<&Values> = columns.iter().map(|&column| frame.column(column)).collect();
        let built = if columns.is_empty() {
            Grouping::whole(rows.len())
        } else if let Some(finer) = finer {
            finer
                .grouping
                .coarsen(keys.len(), |item, key| keys[key].get(start + item))
        } else {
            Grouping::partition(rows.len(), keys.len(), |item, key| {
                keys[key].get(start + item)
            })
        };
        let grouping = Arc::new(built?);
        self.groupings.push(Shared {
            columns: columns.to_vec(),
            rows,
            grouping: Arc::clone(&grouping),
        });
        Ok(grouping)
    }
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
    grouping: Arc<Grouping>,
    /// Whether each group's partition is complete.
    completed: Vec<bool>,
    /// The filter's truth at each position, from its column program; none
    /// when there is no filter or a position evaluates through the row
    /// evaluator.
    truths: Option<Vec<Truth>>,
    /// Whether every two values of each order term compare across the whole
    /// scope, so no partition's sort can fail and none checks again.
    admitted: bool,
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
    let grouping = if window.group_by.is_empty() {
        Arc::new(super::reserved(Grouping::whole(len))?)
    } else {
        let width = schema.as_ref().unwrap_or(cx.schema).columns().len();
        super::group::admit(&window.group_by, width, len, None)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
        if window
            .group_by
            .iter()
            .all(|&column| column < cx.frame.width())
        {
            // Frame columns only: the grouping the unit's other windows and
            // output aggregates over the same keys share.
            run.windows
                .grouping(cx.frame, &window.group_by, start..start + len)
                .map_err(|_| {
                    Box::new(ExecutionError::OutputGrouping(
                        crate::table_grouping::GroupingError::Allocation,
                    ))
                })?
        } else {
            let read = |position: usize, column: usize| {
                if column < cx.frame.width() {
                    cx.frame.get(start + position, column)
                } else {
                    ValueRef::from(&collected[position][column - cx.frame.width()])
                }
            };
            let grouping = Grouping::partition(len, window.group_by.len(), |position, key| {
                read(position, window.group_by[key])
            })
            .map_err(|_| {
                Box::new(ExecutionError::OutputGrouping(
                    crate::table_grouping::GroupingError::Allocation,
                ))
            })?;
            Arc::new(grouping)
        }
    };
    let completed = super::filled(grouping.groups.len(), false)?;
    let mut built = WindowRun {
        window: window.clone(),
        start,
        width,
        collected,
        schema,
        grouping,
        completed,
        truths: None,
        admitted: false,
        answers: super::filled(len, Answer::Missing)?,
    };
    if !matches!(window.kind, WindowKind::BaselineFlag { .. }) && !window.order_by.is_empty() {
        // A typed frame column holds present values of one type, which
        // compare unless they are truth values (VALUE-19).
        let typed = window.order_by.iter().all(|term| {
            term.column < width
                && matches!(
                    cx.frame.column(term.column),
                    Values::Missing(_)
                        | Values::Int(..)
                        | Values::Float(..)
                        | Values::Date(..)
                        | Values::DateTime(..)
                        | Values::Str(..)
                )
        });
        let positions: Vec<usize> = if typed {
            Vec::new()
        } else {
            (0..len).collect()
        };
        let terms = window.order_by.len();
        built.admitted = typed
            || super::relations::comparable(&positions, terms, |&position, term| {
                built.get(cx, position, window.order_by[term].column)
            })
            .is_ok();
    }
    if let Some(filter) = &window.filter {
        built.truths = filter_truths(filter, &built, cx);
    }
    Ok(built)
}

/// The window filter's truth at every position of the scope, from its
/// column program (#4199), or none when a position would fail: then each
/// member evaluates through the row evaluator, which reports the failure.
fn filter_truths<E>(
    filter: &crate::bound_expression::BoundPredicate,
    built: &WindowRun,
    cx: &Cx<'_, E>,
) -> Option<Vec<Truth>> {
    let len = built.answers.len();
    if len < 2 {
        return None;
    }
    let rows: Vec<usize> = (built.start..built.start + len).collect();
    let records: Vec<usize> = rows
        .iter()
        .map(|&row| cx.frame.members(row).first().copied())
        .collect::<Option<_>>()?;
    super::predicates::record_truths(filter, cx.table, &records, |column| {
        if column < built.width {
            super::predicates::Operand::Frame(cx.frame.column(column), &rows)
        } else {
            let mut values = Values::missing(0);
            for position in 0..len {
                values.push(built.get(cx, position, column));
            }
            super::predicates::Operand::Values(values)
        }
    })
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
            match built.answers[position] {
                // A cell is lent, never owned on its way into the buffer.
                Answer::Cell { row, column } => values.push(built.get(cx, row, column)),
                _ => values.push_value(answer(plan, &built, cx, expr, position)?),
            }
        }
        Ok(values)
    })();
    run.windows.runs.insert(id, built);
    result
}

/// Complete the partition of one position, sorting it and evaluating every
/// filter in sorted order before numbering any row.
fn complete<E>(built: &mut WindowRun, cx: &Cx<'_, E>, position: usize) -> Result<(), Fail<E>> {
    let group = built.grouping.group_of[position];
    if std::mem::replace(&mut built.completed[group], true) {
        return Ok(());
    }
    let mut members = built.grouping.groups.group(group).to_vec();
    // The answers are written apart from the run, which the window and the
    // values it reads are borrowed from.
    let mut answers = std::mem::take(&mut built.answers);
    let result = partition(built, cx, &mut members, &mut answers);
    built.answers = answers;
    result
}

/// Number one partition, `members` in construction order: sort it, then
/// evaluate every filter in sorted order before numbering any row.
fn partition<E>(
    built: &WindowRun,
    cx: &Cx<'_, E>,
    members: &mut Vec<usize>,
    answers: &mut [Answer],
) -> Result<(), Fail<E>> {
    let window = &built.window;
    // A baseline's tie-break terms compare only eligible rows sharing the
    // latest date; its partition keeps construction order.
    let baseline = matches!(window.kind, WindowKind::BaselineFlag { .. });
    if !baseline && !window.order_by.is_empty() {
        // Members are in construction order, which a tie keeps (VALUE-23).
        if built.admitted {
            // Every pair compares; a frame column's buffer is read directly.
            let lanes: Vec<_> = window
                .order_by
                .iter()
                .map(|term| {
                    (
                        term,
                        (term.column < built.width).then(|| cx.frame.column(term.column)),
                    )
                })
                .collect();
            let order = |left: usize, right: usize| {
                for (term, values) in &lanes {
                    let pair = match values {
                        Some(values) => (
                            values.get(built.start + left),
                            values.get(built.start + right),
                        ),
                        None => (
                            built.get(cx, left, term.column),
                            built.get(cx, right, term.column),
                        ),
                    };
                    match super::relations::compare(pair.0, pair.1, term) {
                        Ok(Ordering::Equal) => {}
                        Ok(order) => return order,
                        Err(_) => return Ordering::Equal,
                    }
                }
                Ordering::Equal
            };
            // A partition already in order, as rows built in order often
            // are, is left as the stable sort would leave it.
            if members
                .windows(2)
                .any(|pair| order(pair[0], pair[1]) == Ordering::Greater)
            {
                members.sort_by(|&left, &right| order(left, right));
            }
        } else {
            super::relations::sort(members, &window.order_by, |&member, term| {
                built.get(cx, member, window.order_by[term].column)
            })
            .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
        }
    }
    if let Some(filter) = &window.filter {
        let mut eligible = Vec::with_capacity(members.len());
        for &member in members.iter() {
            let truth = match &built.truths {
                Some(truths) => truths[member],
                None => {
                    let row = built.start + member;
                    let source_row = cx.frame.members(row)[0];
                    super::predicates::row_truth(filter, cx.table, source_row, |column| {
                        built.get(cx, member, column)
                    })
                    .map_err(|error| {
                        Box::new(ExecutionError::Predicate {
                            source_row,
                            error: *error,
                        })
                    })?
                }
            };
            if truth == Truth::True {
                eligible.push(member);
            }
        }
        *members = eligible;
    }
    let members = &members[..];
    match window.kind {
        WindowKind::BaselineFlag {
            date,
            reference_date,
            value,
            inclusive,
        } => {
            let mut latest: Option<usize> = None;
            let mut count = 0;
            for &member in members {
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
                        Ordering::Equal => ordered(built, cx, window, member, best)?,
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
                    for &member in members {
                        answers[member] = Answer::AmbiguousBaseline { row, date, count };
                    }
                } else {
                    answers[row] = Answer::Flag;
                }
            }
        }
        WindowKind::RowValue { column, offset } => {
            for (index, &member) in members.iter().enumerate() {
                let target = usize::try_from(index as i128 + i128::from(offset)).ok();
                if let Some(&row) = target.and_then(|target| members.get(target)) {
                    answers[member] = Answer::Cell { row, column };
                }
            }
        }
        WindowKind::Locf {
            column,
            include_current,
        } => {
            let mut previous = None;
            for &member in members {
                let present = !matches!(built.get(cx, member, column), ValueRef::Missing);
                let donor = if present && include_current {
                    Some(member)
                } else {
                    previous
                };
                if let Some(row) = donor {
                    answers[member] = Answer::Cell { row, column };
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
                    || ordered(built, cx, window, members[index - 1], member)? != Ordering::Equal;
                if new_rank {
                    rank = if window.kind == WindowKind::Dense {
                        rank + 1
                    } else {
                        index as i64 + 1
                    };
                }
                answers[member] = Answer::Number(rank);
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

#[cfg(test)]
mod tests {
    // The groupings the windows and output aggregates of a window stage or
    // of the column phase share (`Runs::grouping`), a seam no specification
    // reaches: which grouping a later unit reuses, merges from one by more
    // keys, or builds again once a key column is written. The groups follow
    // ROW-4, derived by hand: first occurrence, members in row order.
    use super::*;
    use crate::dataset::columnar::frame::Identity;

    fn groups(grouping: &Grouping) -> Vec<Vec<usize>> {
        grouping.groups.iter().map(<[usize]>::to_vec).collect()
    }

    #[test]
    fn a_grouping_is_shared_until_a_key_column_is_written() {
        let cells = [("S1", "A"), ("S1", "B"), ("S2", "A"), ("S1", "A")];
        let mut frame = Frame::new(3);
        for (row, (subject, parameter)) in cells.iter().enumerate() {
            frame.push(0, Identity::Candidate(row), &[row]);
            frame.complete(row, 0, ValueRef::Str(subject));
            frame.complete(row, 1, ValueRef::Str(parameter));
        }
        let mut runs = Runs::default();
        let both = runs.grouping(&frame, &[0, 1], 0..4).unwrap();
        assert_eq!(groups(&both), [vec![0, 3], vec![1], vec![2]], "both keys");
        let again = runs.grouping(&frame, &[0, 1], 0..4).unwrap();
        assert!(Arc::ptr_eq(&both, &again), "the same keys share one");
        let subject = runs.grouping(&frame, &[0], 0..4).unwrap();
        assert_eq!(
            (groups(&subject), subject.group_of.clone()),
            (vec![vec![0, 1, 3], vec![2]], vec![0, 0, 1, 0]),
            "the first key, merged from both"
        );
        let tail = runs.grouping(&frame, &[0], 1..4).unwrap();
        assert_eq!(groups(&tail), [vec![0, 2], vec![1]], "rows 1 to 3");
        // Another column written leaves the groupings; a key column written
        // drops every grouping by it.
        frame.complete(0, 2, ValueRef::Int(1));
        runs.written(2);
        let kept = runs.grouping(&frame, &[0], 0..4).unwrap();
        assert!(Arc::ptr_eq(&subject, &kept), "column 2 is no key");
        frame.reset(3, 0);
        frame.complete(3, 0, ValueRef::Str("S2"));
        runs.written(0);
        let subject = runs.grouping(&frame, &[0], 0..4).unwrap();
        assert_eq!(
            groups(&subject),
            [vec![0, 1], vec![2, 3]],
            "the first key, built again"
        );
        let both = runs.grouping(&frame, &[0, 1], 0..4).unwrap();
        assert_eq!(
            groups(&both),
            [vec![0], vec![1], vec![2, 3]],
            "both keys, built again"
        );
    }
}
