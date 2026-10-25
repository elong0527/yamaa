//! The candidate rows of one region: a typed buffer and a completed bitmap
//! per output column, and per row its template, selection identity and the
//! driver records it was built from.
use super::buffer::{Bits, Values};
use crate::table::ValueRef;
use crate::value::Value;

/// The selection identity of a row: the cache key of its relation choices.
/// Key probes and candidates never share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Identity {
    /// A destination-free context with no current row.
    Free,
    /// One driving record while the key grain derives its keys.
    Probe(usize),
    Candidate(usize),
}

/// Hashes identities, which are row numbers the executor assigns: one
/// multiplication per word, where the default hasher's rounds would cost
/// more than the map. Data never chooses an identity.
#[derive(Clone, Copy, Default)]
pub(super) struct IdentityHasher(u64);

impl IdentityHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl std::hash::Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        // Multiplication spreads bits upward only; fold the high bits into
        // the low ones a table indexes by.
        let mut hash = self.0;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^ (hash >> 33)
    }
    fn write(&mut self, bytes: &[u8]) {
        let words = bytes.chunks_exact(8);
        let rest = words.remainder();
        for word in words {
            self.add(u64::from_le_bytes(word.try_into().unwrap()));
        }
        if !rest.is_empty() {
            let mut last = [0; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(last));
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
    fn write_isize(&mut self, value: isize) {
        self.add(value as u64);
    }
}

/// A map keyed by identities.
pub(super) type IdentityMap<K, V> =
    std::collections::HashMap<K, V, std::hash::BuildHasherDefault<IdentityHasher>>;

/// No template: a key-grain candidate or a donor record.
pub(super) const NO_TEMPLATE: u32 = u32::MAX;

#[derive(Debug, Default)]
pub(super) struct Frame {
    columns: Vec<Values>,
    completed: Vec<Bits>,
    templates: Vec<u32>,
    identities: Vec<Identity>,
    offsets: Vec<usize>,
    records: Vec<usize>,
}

impl Frame {
    pub(super) fn new(width: usize) -> Self {
        Self {
            columns: (0..width).map(|_| Values::missing(0)).collect(),
            completed: (0..width).map(|_| Bits::default()).collect(),
            templates: Vec::new(),
            identities: Vec::new(),
            offsets: vec![0],
            records: Vec::new(),
        }
    }

    pub(super) fn rows(&self) -> usize {
        self.templates.len()
    }

    pub(super) fn width(&self) -> usize {
        self.columns.len()
    }

    /// The member records of every row.
    pub(super) fn member_count(&self) -> usize {
        self.records.len()
    }

    /// Room for `rows` more rows holding `records` more member records.
    pub(super) fn try_reserve(
        &mut self,
        rows: usize,
        records: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        for column in &mut self.columns {
            column.try_reserve(rows)?;
        }
        for completed in &mut self.completed {
            completed.try_reserve(rows)?;
        }
        self.templates.try_reserve(rows)?;
        self.identities.try_reserve(rows)?;
        self.offsets.try_reserve(rows)?;
        self.records.try_reserve(records)
    }

    /// Append one row with every column missing and incomplete.
    pub(super) fn push(&mut self, template: u32, identity: Identity, members: &[usize]) -> usize {
        let row = self.rows();
        for column in &mut self.columns {
            column.resize_missing(row + 1);
        }
        for completed in &mut self.completed {
            completed.push(false);
        }
        self.templates.push(template);
        self.identities.push(identity);
        self.records.extend_from_slice(members);
        self.offsets.push(self.records.len());
        row
    }

    /// Drop every row from `rows` on.
    pub(super) fn truncate(&mut self, rows: usize) {
        if rows >= self.rows() {
            return;
        }
        for column in &mut self.columns {
            column.truncate(rows);
        }
        for completed in &mut self.completed {
            completed.truncate(rows);
        }
        self.templates.truncate(rows);
        self.identities.truncate(rows);
        self.offsets.truncate(rows + 1);
        self.records.truncate(self.offsets[rows]);
    }

    /// Keep, from `first` on, only the rows `keep` marks, in order.
    pub(super) fn retain_from(&mut self, first: usize, keep: &[bool]) {
        debug_assert_eq!(keep.len(), self.rows() - first);
        if keep.iter().all(|&kept| kept) {
            return;
        }
        let kept: Vec<usize> = (first..self.rows())
            .filter(|row| keep[row - first])
            .collect();
        for column in &mut self.columns {
            let tail = column.gather(&kept);
            column.truncate(first);
            for index in 0..tail.len() {
                column.push(tail.get(index));
            }
        }
        for completed in &mut self.completed {
            let tail = completed.gather(&kept);
            completed.truncate(first);
            for index in 0..tail.len() {
                completed.push(tail.get(index));
            }
        }
        let templates: Vec<u32> = kept.iter().map(|&row| self.templates[row]).collect();
        let identities: Vec<Identity> = kept.iter().map(|&row| self.identities[row]).collect();
        let members: Vec<Vec<usize>> = kept.iter().map(|&row| self.members(row).to_vec()).collect();
        self.templates.truncate(first);
        self.identities.truncate(first);
        self.offsets.truncate(first + 1);
        self.records.truncate(self.offsets[first]);
        self.templates.extend(templates);
        self.identities.extend(identities);
        for members in members {
            self.records.extend(members);
            self.offsets.push(self.records.len());
        }
    }

    pub(super) fn template(&self, row: usize) -> Option<usize> {
        let template = self.templates[row];
        (template != NO_TEMPLATE).then_some(template as usize)
    }

    pub(super) fn identity(&self, row: usize) -> Identity {
        self.identities[row]
    }

    /// The driver records a row was built from, in record order.
    pub(super) fn members(&self, row: usize) -> &[usize] {
        &self.records[self.offsets[row]..self.offsets[row + 1]]
    }

    pub(super) fn get(&self, row: usize, column: usize) -> ValueRef<'_> {
        self.columns[column].get(row)
    }

    pub(super) fn value(&self, row: usize, column: usize) -> Value {
        self.columns[column].value(row)
    }

    pub(super) fn is_completed(&self, row: usize, column: usize) -> bool {
        self.completed[column].get(row)
    }

    /// Complete one cell with its converted value.
    pub(super) fn complete(&mut self, row: usize, column: usize, value: ValueRef<'_>) {
        self.columns[column].set(row, value);
        self.completed[column].set(row, true);
    }

    /// Complete one cell, moving an owned value in.
    pub(super) fn complete_value(&mut self, row: usize, column: usize, value: Value) {
        self.columns[column].set_value(row, value);
        self.completed[column].set(row, true);
    }

    /// Complete one column on `rows` with the values aligned with them. When
    /// the rows are every row in order and none was complete, the buffer is
    /// taken whole.
    pub(super) fn complete_rows(&mut self, rows: &[usize], column: usize, values: Values) {
        let whole = rows.len() == self.rows()
            && rows.iter().enumerate().all(|(index, &row)| index == row)
            && (0..self.rows()).all(|row| !self.completed[column].get(row));
        if whole {
            self.columns[column] = values;
            self.completed[column] = Bits::new(self.rows(), true);
            return;
        }
        for (index, &row) in rows.iter().enumerate() {
            self.complete(row, column, values.get(index));
        }
    }

    /// Undo a cell's completion; its value reads missing again.
    pub(super) fn reset(&mut self, row: usize, column: usize) {
        self.columns[column].set(row, ValueRef::Missing);
        self.completed[column].set(row, false);
    }

    /// Append rows `rows` of `other`, their member records moved on by
    /// `records`: one partition's candidates joining the whole run's, whose
    /// records count the whole input's (#3830).
    pub(super) fn append(&mut self, other: &Frame, rows: std::ops::Range<usize>, records: usize) {
        for row in rows {
            for (values, from) in self.columns.iter_mut().zip(&other.columns) {
                values.push(from.get(row));
            }
            for (completed, from) in self.completed.iter_mut().zip(&other.completed) {
                completed.push(from.get(row));
            }
            self.templates.push(other.templates[row]);
            self.identities.push(other.identities[row]);
            self.records
                .extend(other.members(row).iter().map(|&record| record + records));
            self.offsets.push(self.records.len());
        }
    }

    /// The rows `order` names, in that order, as columns, consuming the
    /// frame a column at a time (#3870): a column moves whole when `order`
    /// keeps every row in place, and is otherwise gathered and dropped before
    /// the next one is.
    pub(super) fn into_columns(
        self,
        order: &[usize],
    ) -> Result<Vec<Values>, std::collections::TryReserveError> {
        let in_place = order.len() == self.rows()
            && order
                .iter()
                .enumerate()
                .all(|(position, &row)| position == row);
        let Self {
            columns,
            completed,
            templates,
            identities,
            offsets,
            records,
        } = self;
        drop((completed, templates, identities, offsets, records));
        let mut ordered = Vec::new();
        ordered.try_reserve_exact(columns.len())?;
        for column in columns {
            ordered.push(if in_place {
                column
            } else {
                let mut gathered = Values::missing(0);
                gathered.try_reserve(order.len())?;
                for &row in order {
                    gathered.push(column.get(row));
                }
                gathered
            });
        }
        Ok(ordered)
    }
}
