//! Stable presentation order over completed rows, after all verification.
use super::*;
use std::cmp::Ordering;

/// Compare borrowed normalized cells without cloning payloads.
pub(super) fn apply<E>(
    terms: &[OrderTerm],
    dataset: &mut Dataset,
) -> Result<(), Box<ExecutionError<E>>> {
    if terms.is_empty() || dataset.rows.len() < 2 {
        return Ok(());
    }

    let mut members = Vec::new();
    members
        .try_reserve_exact(dataset.rows.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    members.extend(0..dataset.rows.len());
    sort(&mut members, |left, right| {
        for term in terms {
            let (a, b) = (
                &dataset.rows[left][term.column],
                &dataset.rows[right][term.column],
            );
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

    for member in members {
        rows.push(std::mem::take(&mut dataset.rows[member]));
    }
    dataset.rows = rows;
    Ok(())
}

/// Fallible merge sorting never retries a failed comparison or returns a partial order.
pub(super) fn sort<E>(
    members: &mut [usize],
    mut compare: impl FnMut(usize, usize) -> Result<Ordering, Box<ExecutionError<E>>>,
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
                        && compare(members[left], members[right])? != Ordering::Greater);
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

        members.copy_from_slice(&scratch);
        width = span;
    }
    Ok(())
}
