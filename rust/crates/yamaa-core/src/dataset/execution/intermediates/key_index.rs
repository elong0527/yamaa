//! Attempt-local ordinary string keys; immutable donor coordinates retain order.
use super::*;
use std::collections::BTreeMap;

/// Avoid indexing small relations whose scan is already cheap.
pub(super) const MINIMUM_ROWS: usize = 128;

#[derive(Default)]
pub(super) struct StringIndex(pub(super) BTreeMap<String, Vec<usize>>);

impl StringIndex {
    pub(super) fn build<E>(
        table: &dyn TableAccess<Error = E>,
        item: &Intermediate,
        eligible: &[usize],
        column: usize,
    ) -> Result<Self, Box<ExecutionError<E>>> {
        // The first ordinary scan keeps its existing admission and donor read
        // order. cell() admits each string before this index can copy it.

        let mut index = Self::default();
        for &row in eligible {
            let value = super::cell(table, item, row, column)?;
            let ValueRef::Str(key) = value else { continue };

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
    pub(super) fn matching<E>(&self, key: &str) -> Result<&[usize], Box<ExecutionError<E>>> {
        Ok(self.0.get(key).map_or(&[], Vec::as_slice))
    }
}
