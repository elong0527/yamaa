//! Filtered positional numbering over completed, typed output columns.
use super::*;
use core::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberingKind {
    RowNumber,
    Competition,
    Dense,
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
pub struct Numbering {
    pub kind: NumberingKind,
    pub group_by: Vec<usize>,
    pub order_by: Vec<OrderTerm>,
    pub filter: Option<BoundPredicate>,
}
impl Numbering {
    /// Reject incomplete dependencies and empty ordering before any source access.
    pub(super) fn validate(&self, available: &[bool]) -> Result<(), PlanError> {
        if self.order_by.is_empty()
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
    window: &Numbering,
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

/// Run-local partition membership and results; no state survives plan reuse.
pub(super) struct Run {
    groups: Vec<Option<Vec<usize>>>,
    group_for_row: Vec<usize>,
    numbers: Vec<Option<i64>>,
}
impl Run {
    /// Partition once without evaluating later partitions' filters or converting results.
    pub(super) fn new<E>(
        window: &Numbering,
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
        let mut numbers = Vec::new();
        numbers
            .try_reserve_exact(rows)
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        numbers.resize(rows, None);
        Ok(Self {
            groups: groups.into_iter().map(Some).collect(),
            group_for_row,
            numbers,
        })
    }

    /// Complete this row's partition once, before the caller converts the current result.
    pub(super) fn value<T: TableAccess + ?Sized>(
        &mut self,
        row: usize,
        window: &Numbering,
        candidates: &[Candidate],
        table: &T,
        budget: &mut Budget,
    ) -> Result<Value, Box<ExecutionError<T::Error>>> {
        if let Some(mut members) = self.groups[self.group_for_row[row]].take() {
            sort(&mut members, window, candidates, budget)?;
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
            let mut rank = 0;
            for (position, &member) in members.iter().enumerate() {
                let new_rank = position == 0
                    || window.kind == NumberingKind::RowNumber
                    || ordered(
                        window,
                        &candidates[members[position - 1]],
                        &candidates[member],
                        budget,
                    )? != Ordering::Equal;
                if new_rank {
                    rank = if window.kind == NumberingKind::Dense {
                        rank + 1
                    } else {
                        position as i64 + 1
                    };
                }
                self.numbers[member] = Some(rank);
            }
        }
        Ok(self.numbers[row].map_or(Value::Missing, Value::Int))
    }
}

/// Fallible merge sorting stops at the first limit without an inconsistent comparator.
fn sort<E>(
    members: &mut [usize],
    window: &Numbering,
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
