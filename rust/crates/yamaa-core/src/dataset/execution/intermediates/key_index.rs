//! Attempt-local index over the full key tuple of a plain intermediate.
//!
//! The donor index is built once per intermediate over every key column,
//! for any number of key columns and any scalar type. Each key component
//! is encoded canonically (see `encode`), so values that compare equal
//! under `compare_present_refs` share one bucket: integer and float
//! spellings of the same number encode identically, as do both float
//! zero signs, and temporal precision is ignored exactly as temporal
//! equality ignores it. Immutable donor coordinates retain stored
//! (donor read) order inside each bucket, and the filter narrows a bucket
//! only after its keys match (LOOKUP-25).
use super::*;
use std::collections::BTreeMap;

/// Avoid indexing small relations whose scan is already cheap.
pub(super) const MINIMUM_ROWS: usize = 128;

/// One encoded component discriminates the scalar families that key
/// comparison keeps distinct; numeric values share one family because
/// int/float keys compare across the two spellings.
fn encode(key: &mut Vec<u8>, value: ValueRef<'_>) {
    match value {
        ValueRef::Missing => key.push(0),
        ValueRef::Str(value) => {
            key.push(1);
            key.extend_from_slice(&(value.len() as u64).to_be_bytes());
            key.extend_from_slice(value.as_bytes());
        }
        ValueRef::Int(value) => encode_int(key, value),
        ValueRef::Float(value) => {
            let value = value.get();
            // A float that is exactly an in-range integer is the same key
            // as that integer (this also unifies both zero signs at 0);
            // every other finite float keeps its exact bits.
            if value == value.trunc()
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
            {
                encode_int(key, value as i64);
            } else {
                key.push(3);
                key.extend_from_slice(&value.to_bits().to_be_bytes());
            }
        }
        ValueRef::Bool(value) => {
            key.push(4);
            key.push(u8::from(value));
        }
        ValueRef::Date(value) => {
            let (year, month, day) = value.fields();
            key.push(5);
            key.extend_from_slice(&year.to_be_bytes());
            key.push(month);
            key.push(day);
        }
        ValueRef::DateTime(value) => {
            let (year, month, day, hour, minute, second) = value.fields();
            key.push(6);
            key.extend_from_slice(&year.to_be_bytes());
            key.push(month);
            key.push(day);
            key.push(hour);
            key.push(minute);
            key.push(second);
        }
    }
}

fn encode_int(key: &mut Vec<u8>, value: i64) {
    key.push(2);
    key.extend_from_slice(&value.to_be_bytes());
}

#[derive(Default)]
pub(super) struct KeyIndex {
    columns: Vec<usize>,
    buckets: BTreeMap<Vec<u8>, Vec<usize>>,
}

impl KeyIndex {
    /// Build the index over the given donor rows, or return `None` when a
    /// donor cell's type does not fit its declared column type: such a
    /// relation keeps the scan path, whose per-cell comparison owns the
    /// incompatible-type diagnostic for that shape.
    pub(super) fn build<E>(
        table: &dyn TableAccess<Error = E>,
        item: &Intermediate,
        rows: &[usize],
    ) -> Result<Option<Self>, Box<ExecutionError<E>>> {
        // The first ordinary scan keeps its existing admission and donor read
        // order. cell() admits each value before this index can copy it.
        let mut columns = Vec::new();
        columns
            .try_reserve_exact(item.keys.len())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        columns.extend(item.keys.iter().map(|key| key.source_column));

        let mut index = Self {
            columns,
            buckets: BTreeMap::new(),
        };
        for &row in rows {
            let mut key = Vec::new();
            let mut missing = false;
            for &column in &index.columns {
                let value = super::cell(table, item, row, column)?;
                if matches!(value, ValueRef::Missing) {
                    // A missing donor key never matches a present lookup key.
                    missing = true;
                    break;
                }
                if let (Some(kind), Some(actual)) = (
                    table
                        .schema()
                        .columns()
                        .get(column)
                        .map(|column| column.kind),
                    value.value_type(),
                ) {
                    if !crate::value::comparable_types(actual, kind.into()) {
                        return Ok(None);
                    }
                }
                encode(&mut key, value);
            }
            if missing {
                continue;
            }
            if let Some(rows) = index.buckets.get_mut(&key) {
                rows.try_reserve(1)
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                rows.push(row);
            } else {
                let mut rows = Vec::new();
                rows.try_reserve_exact(1)
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                rows.push(row);
                index.buckets.insert(key, rows);
            }
        }
        Ok(Some(index))
    }

    pub(super) fn matching<E>(
        &self,
        keys: &[(usize, Value)],
    ) -> Result<&[usize], Box<ExecutionError<E>>> {
        let mut key = Vec::new();
        for &column in &self.columns {
            let Some((_, value)) = keys.iter().find(|(bound, _)| *bound == column) else {
                return Ok(&[]);
            };
            encode(&mut key, ValueRef::from(value));
        }
        Ok(self.buckets.get(&key).map_or(&[], Vec::as_slice))
    }

    #[cfg(test)]
    pub(super) fn bucket_count(&self) -> usize {
        self.buckets.len()
    }
}
