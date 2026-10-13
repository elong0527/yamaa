//! Attempt-local ordinary string keys; immutable donor coordinates retain order.
use super::*;
use alloc::{collections::BTreeMap, string::String, vec::Vec};

/// Avoid indexing small relations whose scan is already cheap.
pub(super) const MINIMUM_ROWS: usize = 128;

#[derive(Default)]
pub(super) struct StringIndex(BTreeMap<String, Vec<usize>>);

/// Rust's B-tree has at most eleven keys per node and at least six children
/// per non-root internal node. Twelve comparisons per conservatively rounded
/// level also covers the root and unsuccessful searches. No content is hashed.
pub(super) fn search_work(mut entries: usize) -> usize {
    let mut levels = 1;
    while entries > 1 {
        entries = entries.div_ceil(6);
        levels += 1;
    }
    levels * 12
}
impl StringIndex {
    pub(super) fn build<E>(
        table: &dyn TableAccess<Error = E>,
        item: &Intermediate,
        eligible: &[usize],
        column: usize,
        budget: &mut Budget,
    ) -> Result<Self, Box<ExecutionError<E>>> {
        // The first ordinary scan keeps its existing admission and donor read
        // order. cell() admits each string before this index can copy it.
        budget.work(eligible.len(), 1)?;
        let mut index = Self::default();
        for &row in eligible {
            let value = super::cell(table, item, row, column, budget)?;
            let ValueRef::Str(key) = value else { continue };
            budget.work(search_work(index.0.len()), 2)?;
            if let Some(rows) = index.0.get_mut(key) {
                rows.try_reserve(1)
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                rows.push(row);
            } else {
                let mut rows = Vec::new();
                rows.try_reserve_exact(1)
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                rows.push(row);
                index.0.insert(key.into(), rows);
            }
        }
        Ok(index)
    }
    pub(super) fn matching<E>(
        &self,
        key: &str,
        budget: &mut Budget,
    ) -> Result<&[usize], Box<ExecutionError<E>>> {
        budget.work(search_work(self.0.len()), 1)?;
        Ok(self.0.get(key).map_or(&[], Vec::as_slice))
    }
}
