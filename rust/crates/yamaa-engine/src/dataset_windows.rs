//! Partitioned window results over completed, typed output columns.
use super::*;
use core::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    RowNumber,
    Competition,
    Dense,
    RowValue { column: usize, offset: i64 },
    PreviousNonMissing { column: usize },
    Locf { column: usize },
    BaselineFlag { date: usize, reference_date: usize },
}

/// Null placement is independent of direction; construction order breaks remaining ties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderTerm {
    pub column: usize,
    pub descending: bool,
    pub nulls_first: bool,
}

/// Window dependencies bind only completed output columns, never qualified source reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub kind: WindowKind,
    pub group_by: Vec<usize>,
    pub order_by: Vec<OrderTerm>,
    pub filter: Option<BoundPredicate>,
}
impl Window {
    /// Reject incomplete dependencies and enforce operation-specific ordering before source access.
    pub(super) fn validate(
        &self,
        available: &[bool],
        output: &TableSchema,
    ) -> Result<(), PlanError> {
        let baseline = matches!(self.kind, WindowKind::BaselineFlag { .. });
        if self.order_by.is_empty() != baseline
            || self.group_by.iter().enumerate().any(|(i, column)| {
                !available.get(*column).copied().unwrap_or(false)
                    || self.group_by[..i].contains(column)
            })
            || self
                .order_by
                .iter()
                .any(|term| !available.get(term.column).copied().unwrap_or(false))
        {
            return Err(PlanError::InvalidWindow);
        }
        if let WindowKind::RowValue { offset: 0, .. } = self.kind {
            return Err(PlanError::InvalidWindow);
        }
        let source = match self.kind {
            WindowKind::RowValue { column, .. }
            | WindowKind::PreviousNonMissing { column }
            | WindowKind::Locf { column } => Some(column),
            _ => None,
        };
        if source.is_some_and(|column| !available.get(column).copied().unwrap_or(false)) {
            return Err(PlanError::InvalidWindow);
        }
        if let WindowKind::BaselineFlag {
            date,
            reference_date,
        } = self.kind
        {
            if !available.get(date).copied().unwrap_or(false)
                || !available.get(reference_date).copied().unwrap_or(false)
            {
                return Err(PlanError::InvalidWindow);
            }
            let kind = output.columns()[date].kind;
            if !matches!(
                kind,
                yamaa_core::value::ColumnType::Date | yamaa_core::value::ColumnType::DateTime
            ) || output.columns()[reference_date].kind != kind
            {
                return Err(PlanError::InvalidWindow);
            }
        }
        if let Some(filter) = &self.filter {
            filter
                .validate(0, available, true)
                .map_err(PlanError::Filter)?;
        }
        Ok(())
    }
}

struct Rows<'a> {
    candidates: &'a [Candidate],
    schema: &'a TableSchema,
}
impl TableAccess for Rows<'_> {
    type Error = Infallible;
    /// Borrow the declared schema; uncompleted columns are never bound by an admitted window.
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    /// Window scope is the complete constructed output relation.
    fn row_count(&self) -> usize {
        self.candidates.len()
    }
    /// Lend completed cells without cloning text or source access.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.candidates
            .get(row)
            .and_then(|row| row.values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

/// Compare values within one normalized column, retaining exact integer and temporal order.
fn compare(left: &Value, right: &Value, term: &OrderTerm) -> Ordering {
    match (left, right) {
        (Value::Missing, Value::Missing) => Ordering::Equal,
        (Value::Missing, _) => {
            if term.nulls_first {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (_, Value::Missing) => {
            if term.nulls_first {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        _ => {
            let order = match (left, right) {
                (Value::Str(a), Value::Str(b)) => a.cmp(b),
                (Value::Int(a), Value::Int(b)) => a.cmp(b),
                (Value::Float(a), Value::Float(b)) => a
                    .get()
                    .partial_cmp(&b.get())
                    .expect("finite normalized values"),
                (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
                (Value::Date(a), Value::Date(b)) => a.cmp(b),
                (Value::DateTime(a), Value::DateTime(b)) => a.cmp(b),
                _ => unreachable!("completed columns have one admitted logical type"),
            };
            if term.descending {
                order.reverse()
            } else {
                order
            }
        }
    }
}

/// Compare declared terms, charging each logical visit and the text compared before use.
fn ordered<E>(
    window: &Window,
    left: &Candidate,
    right: &Candidate,
    budget: &mut Budget,
) -> Result<Ordering, Box<ExecutionError<E>>> {
    for term in &window.order_by {
        budget.work(1, 2)?;
        let (left, right) = (&left.values[term.column], &right.values[term.column]);
        for value in [left, right] {
            if let Value::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
        }
        let order = compare(left, right, term);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

/// Cache only tiny results or borrowed donor coordinates, never cloned source payloads.
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

/// Borrow only admitted provenance for a partition-scoped runtime condition.
pub(super) struct Context<'a> {
    pub assignment: &'a Assignment,
    pub plan: &'a DatasetPlan,
}

/// Run-local partition membership and results; no state survives plan reuse.
pub(super) struct Run {
    groups: Vec<Option<Vec<usize>>>,
    group_for_row: Vec<usize>,
    answers: Vec<Answer>,
}
impl Run {
    /// Partition once without evaluating later partitions' filters or converting results.
    pub(super) fn new<E>(
        window: &Window,
        candidates: &[Candidate],
        plan: &DatasetPlan,
        limits: Limits,
        budget: &mut Budget,
    ) -> Result<Self, Box<ExecutionError<E>>> {
        let rows = candidates.len();
        if i64::try_from(rows).is_err() {
            return Err(Box::new(ExecutionError::Capacity));
        }
        budget.work(rows, window.group_by.len().max(1))?;
        let groups = if window.group_by.is_empty() {
            vec![(0..rows).collect()]
        } else {
            partition(
                &Rows {
                    candidates,
                    schema: &plan.output,
                },
                &window.group_by,
                limits.output_rows,
                limits.key_cells,
            )
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?
        };
        budget.work(rows, 2)?;
        let mut group_for_row = Vec::new();
        group_for_row
            .try_reserve_exact(rows)
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        group_for_row.resize(rows, 0);
        for (group, members) in groups.iter().enumerate() {
            for &row in members {
                group_for_row[row] = group;
            }
        }
        let mut answers = Vec::new();
        answers
            .try_reserve_exact(rows)
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        answers.resize(rows, Answer::Missing);
        Ok(Self {
            groups: groups.into_iter().map(Some).collect(),
            group_for_row,
            answers,
        })
    }

    /// Complete this row's partition once, before the caller converts the current result.
    pub(super) fn value<T: TableAccess + ?Sized>(
        &mut self,
        row: usize,
        window: &Window,
        candidates: &[Candidate],
        table: &T,
        budget: &mut Budget,
        context: &Context<'_>,
    ) -> Result<Value, Box<ExecutionError<T::Error>>> {
        if let Some(mut members) = self.groups[self.group_for_row[row]].take() {
            if !window.order_by.is_empty() {
                sort(&mut members, window, candidates, budget)?;
            }
            if let Some(filter) = &window.filter {
                // Evaluate every predicate in sorted partition order before numbering any row.
                budget.work(members.len(), 1)?;
                let mut eligible = Vec::new();
                eligible
                    .try_reserve_exact(members.len())
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                for &member in &members {
                    let source_row = candidates[member].members[0];
                    let truth = filter
                        .evaluate(
                            table,
                            source_row,
                            &candidates[member].values,
                            budget.predicate(),
                        )
                        .map_err(|error| match error.kind {
                            yamaa_core::predicate::ErrorKind::Limit(limit) => {
                                Box::new(predicate_limit(limit))
                            }
                            _ => Box::new(ExecutionError::Predicate { source_row, error }),
                        })?;
                    if truth == yamaa_core::predicate::Truth::True {
                        eligible.push(member);
                    }
                }
                members = eligible;
            }
            budget.work(members.len(), 1)?;
            match window.kind {
                WindowKind::BaselineFlag {
                    date,
                    reference_date,
                } => {
                    budget.work(members.len(), 3)?;
                    let mut latest: Option<usize> = None;
                    let mut count = 0;
                    for &member in &members {
                        let (candidate, reference) = (
                            &candidates[member].values[date],
                            &candidates[member].values[reference_date],
                        );
                        if matches!(candidate, Value::Missing)
                            || matches!(reference, Value::Missing)
                            || temporal_order(candidate, reference) == Ordering::Greater
                        {
                            continue;
                        }
                        let order = latest.map_or(Ordering::Greater, |row| {
                            temporal_order(candidate, &candidates[row].values[date])
                        });
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
                                self.answers[member] =
                                    Answer::AmbiguousBaseline { row, date, count };
                            }
                        } else {
                            self.answers[row] = Answer::Flag;
                        }
                    }
                }
                WindowKind::RowValue { column, offset } => {
                    for (position, &member) in members.iter().enumerate() {
                        let target = usize::try_from(position as i128 + i128::from(offset)).ok();
                        if let Some(&row) = target.and_then(|target| members.get(target)) {
                            self.answers[member] = Answer::Cell { row, column };
                        }
                    }
                }
                WindowKind::PreviousNonMissing { column } | WindowKind::Locf { column } => {
                    let mut previous = None;
                    for &member in &members {
                        let present = !matches!(candidates[member].values[column], Value::Missing);
                        let donor = if present && matches!(window.kind, WindowKind::Locf { .. }) {
                            Some(member)
                        } else {
                            previous
                        };
                        if let Some(row) = donor {
                            self.answers[member] = Answer::Cell { row, column };
                        }
                        if present {
                            previous = Some(member);
                        }
                    }
                }
                WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense => {
                    let mut rank = 0;
                    for (position, &member) in members.iter().enumerate() {
                        let new_rank = position == 0
                            || window.kind == WindowKind::RowNumber
                            || ordered(
                                window,
                                &candidates[members[position - 1]],
                                &candidates[member],
                                budget,
                            )? != Ordering::Equal;
                        if new_rank {
                            rank = if window.kind == WindowKind::Dense {
                                rank + 1
                            } else {
                                position as i64 + 1
                            };
                        }
                        self.answers[member] = Answer::Number(rank);
                    }
                }
            }
        }
        Ok(match self.answers[row] {
            Answer::Missing => Value::Missing,
            Answer::Flag => {
                budget.scalar_text(1)?;
                Value::Str("Y".into())
            }
            Answer::AmbiguousBaseline {
                row: donor,
                date,
                count,
            } => {
                budget.identity(
                    window
                        .group_by
                        .iter()
                        .map(|&column| &candidates[row].values[column]),
                )?;
                return Err(Box::new(ExecutionError::BaselineAmbiguity {
                    path: context.assignment.path.clone(),
                    column: context.plan.output.columns()[context.assignment.column]
                        .name
                        .clone(),
                    date: candidates[donor].values[date].clone(),
                    match_count: count,
                    partition: window
                        .group_by
                        .iter()
                        .map(|&column| {
                            (
                                context.plan.output.columns()[column].name.clone(),
                                candidates[row].values[column].clone(),
                            )
                        })
                        .collect(),
                }));
            }

            Answer::Number(value) => Value::Int(value),
            Answer::Cell { row, column } => {
                budget.work(1, 1)?;
                let value = &candidates[row].values[column];
                if let Value::Str(text) = value {
                    budget.scalar_text(text.len())?;
                }
                value.clone()
            }
        })
    }
}

/// Fallible merge sorting stops at the first limit without an inconsistent comparator.
fn sort<E>(
    members: &mut [usize],
    window: &Window,
    candidates: &[Candidate],
    budget: &mut Budget,
) -> Result<(), Box<ExecutionError<E>>> {
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(members.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    scratch.resize(members.len(), 0);
    let mut width = 1;
    while width < members.len() {
        for start in (0..members.len()).step_by(width * 2) {
            let middle = (start + width).min(members.len());
            let end = (middle + width).min(members.len());
            let (mut left, mut right) = (start, middle);
            for slot in &mut scratch[start..end] {
                let take_left = right == end
                    || (left < middle
                        && ordered(
                            window,
                            &candidates[members[left]],
                            &candidates[members[right]],
                            budget,
                        )?
                        .then_with(|| members[left].cmp(&members[right]))
                            != Ordering::Greater);
                *slot = if take_left {
                    let row = members[left];
                    left += 1;
                    row
                } else {
                    let row = members[right];
                    right += 1;
                    row
                };
            }
        }
        budget.work(members.len(), 1)?;
        members.copy_from_slice(&scratch);
        width *= 2;
    }
    Ok(())
}

/// Baseline admission requires matching temporal types, whose complete fields determine order.
fn temporal_order(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Date(left), Value::Date(right)) => left.cmp(right),
        (Value::DateTime(left), Value::DateTime(right)) => left.cmp(right),
        _ => unreachable!("admitted matching temporal baseline columns"),
    }
}
