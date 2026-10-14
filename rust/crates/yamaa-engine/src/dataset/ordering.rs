//! Stable presentation order over completed rows, after all verification.
use super::*;
use core::cmp::Ordering;

/// Charge every comparison before borrowing normalized cells; never clone payloads.
pub(super) fn apply<E>(
    terms: &[OrderTerm],
    dataset: &mut Dataset,
    budget: &mut Budget,
) -> Result<(), Box<ExecutionError<E>>> {
    if terms.is_empty() || dataset.rows.len() < 2 {
        return Ok(());
    }
    budget.work(dataset.rows.len(), 1)?;
    let mut members = Vec::new();
    members
        .try_reserve_exact(dataset.rows.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    members.extend(0..dataset.rows.len());
    sort(&mut members, budget, |left, right, budget| {
        for term in terms {
            budget.work(1, 2)?;
            let (a, b) = (
                &dataset.rows[left][term.column],
                &dataset.rows[right][term.column],
            );
            for value in [a, b] {
                if let Value::Str(text) = value {
                    budget.scalar_text(text.len())?;
                }
            }
            let order = windows::compare(ValueRef::from(a), ValueRef::from(b), term);
            if order != Ordering::Equal {
                return Ok(order);
            }
        }
        Ok(left.cmp(&right))
    })?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(members.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    budget.work(members.len(), 1)?;
    for member in members {
        rows.push(core::mem::take(&mut dataset.rows[member]));
    }
    dataset.rows = rows;
    Ok(())
}

/// Fallible merge sorting never retries a failed comparison or returns a partial order.
pub(super) fn sort<E>(
    members: &mut [usize],
    budget: &mut Budget,
    mut compare: impl FnMut(usize, usize, &mut Budget) -> Result<Ordering, Box<ExecutionError<E>>>,
) -> Result<(), Box<ExecutionError<E>>> {
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(members.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    scratch.resize(members.len(), 0);
    let mut width = 1usize;
    while width < members.len() {
        let span = width.saturating_mul(2);
        for start in (0..members.len()).step_by(span) {
            let middle = start.saturating_add(width).min(members.len());
            let end = start.saturating_add(span).min(members.len());
            let (mut left, mut right) = (start, middle);
            for slot in &mut scratch[start..end] {
                let take_left = right == end
                    || (left < middle
                        && compare(members[left], members[right], budget)? != Ordering::Greater);
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
        width = span;
    }
    Ok(())
}
