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

    /// Append `count` rows with every column missing and incomplete, each
    /// of `template`: `row(index, records)` appends the row's member records
    /// to `records` and answers its identity. The columns grow once, not
    /// once per row (#4190).
    pub(super) fn extend(
        &mut self,
        template: u32,
        count: usize,
        mut row: impl FnMut(usize, &mut Vec<usize>) -> Identity,
    ) {
        for index in 0..count {
            let identity = row(index, &mut self.records);
            self.templates.push(template);
            self.identities.push(identity);
            self.offsets.push(self.records.len());
        }
        let rows = self.rows();
        for column in &mut self.columns {
            column.resize_missing(rows);
        }
        for completed in &mut self.completed {
            completed.grow(rows);
        }
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
    pub(super) fn retain_from(
        &mut self,
        first: usize,
        keep: &[bool],
    ) -> Result<(), std::collections::TryReserveError> {
        debug_assert_eq!(keep.len(), self.rows() - first);
        if keep.iter().all(|&kept| kept) {
            return Ok(());
        }
        let mut kept = Vec::new();
        kept.try_reserve_exact(keep.iter().filter(|&&kept| kept).count())?;
        kept.extend((first..self.rows()).filter(|row| keep[row - first]));
        // Every column, bitmap and per-row list is compacted in place: each
        // kept row moves down to its new position, which no later kept row
        // reads.
        for column in &mut self.columns {
            column.retain(first, &kept);
        }
        for completed in &mut self.completed {
            completed.retain(first, &kept);
        }
        let mut end = self.offsets[first];
        for (offset, &row) in kept.iter().enumerate() {
            let position = first + offset;
            self.templates[position] = self.templates[row];
            self.identities[position] = self.identities[row];
            // Read before the write: while no row is dropped, the offset
            // written next is the one read next, and is unchanged.
            let members = self.offsets[row]..self.offsets[row + 1];
            self.records.copy_within(members.clone(), end);
            end += members.len();
            self.offsets[position + 1] = end;
        }
        let rows = first + kept.len();
        self.templates.truncate(rows);
        self.identities.truncate(rows);
        self.offsets.truncate(rows + 1);
        self.records.truncate(end);
        Ok(())
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

    /// Every row's value of one column.
    pub(super) fn column(&self, column: usize) -> &Values {
        &self.columns[column]
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

    /// Complete one column on `rows` with the values aligned with them.
    /// When the rows are one ascending run and none of them was complete,
    /// the values are written into that run in bulk (#4208): the buffer is
    /// taken whole when the run is every row, and is otherwise left as
    /// completing each cell in order leaves it. Other rows complete one cell
    /// at a time.
    pub(super) fn complete_rows(&mut self, rows: &[usize], column: usize, values: Values) {
        debug_assert_eq!(rows.len(), values.len());
        let start = rows.first().copied().unwrap_or(0);
        let run = start..start + rows.len();
        let bulk = rows
            .iter()
            .enumerate()
            .all(|(index, &row)| row == start + index)
            && !self.completed[column].any_in(run.clone());
        if !bulk {
            for (index, &row) in rows.iter().enumerate() {
                self.complete(row, column, values.get(index));
            }
        } else if run == (0..self.rows()) {
            self.columns[column] = values;
            self.completed[column] = Bits::new(self.rows(), true);
        } else {
            self.columns[column].set_range(start, &values);
            self.completed[column].fill(run, true);
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
    /// Each column is copied in bulk (#4190).
    pub(super) fn append(&mut self, other: &Frame, rows: std::ops::Range<usize>, records: usize) {
        for (values, from) in self.columns.iter_mut().zip(&other.columns) {
            values.extend_from(from, rows.clone());
        }
        for (completed, from) in self.completed.iter_mut().zip(&other.completed) {
            completed.extend_from(from, rows.clone());
        }
        self.templates
            .extend_from_slice(&other.templates[rows.clone()]);
        self.identities
            .extend_from_slice(&other.identities[rows.clone()]);
        let base = self.records.len();
        let first = other.offsets[rows.start];
        let members = &other.records[first..other.offsets[rows.end]];
        self.records
            .extend(members.iter().map(|&record| record + records));
        self.offsets.extend(
            other.offsets[rows.start + 1..rows.end + 1]
                .iter()
                .map(|&offset| offset - first + base),
        );
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
                column.gather(order)?
            });
        }
        Ok(ordered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of `rows` rows of template 0, row `n` built from records
    /// `n` and `n + 1000`.
    fn frame(width: usize, rows: usize) -> Frame {
        let mut frame = Frame::new(width);
        frame.extend(0, rows, |row, records| {
            records.extend([row, row + 1000]);
            Identity::Candidate(row)
        });
        frame
    }

    /// Everything a frame holds, each column's text in an arena of its own
    /// row order.
    fn canonical(frame: &Frame) -> String {
        let columns: Vec<String> = frame.columns.iter().map(Values::canonical).collect();
        format!(
            "{columns:?} {:?} {:?} {:?} {:?} {:?}",
            frame.completed, frame.templates, frame.identities, frame.offsets, frame.records
        )
    }

    /// A buffer of `values` pushed one row at a time.
    fn pushed(values: impl IntoIterator<Item = Value>) -> Values {
        let mut buffer = Values::missing(0);
        for value in values {
            buffer.push_value(value);
        }
        buffer
    }

    // The seam is the frame's bulk completion (#4208): completing a run of
    // rows must leave the frame, buffers and completion bits, as completing
    // each cell in order does; other rows go a cell at a time.
    #[test]
    fn a_completed_run_leaves_the_frame_as_completing_each_cell_in_order() {
        let ints = |rows: std::ops::Range<usize>| {
            pushed(rows.map(|row| match row % 5 {
                0 => Value::Missing,
                _ => Value::Int(row as i64),
            }))
        };
        // Each case completes its steps in order on one frame of 140 rows.
        type Step = (Vec<usize>, Values);
        let cases: Vec<(&str, Vec<Step>)> = vec![
            (
                "a template's run across bitmap words, then the run before it",
                vec![
                    ((60..130).collect(), ints(60..130)),
                    ((0..60).collect(), ints(0..60)),
                ],
            ),
            (
                "text after ints widens the column to mixed",
                vec![
                    ((0..70).collect(), ints(0..70)),
                    (
                        (70..140).collect(),
                        pushed((70..140).map(|_| Value::Str("t".into()))),
                    ),
                ],
            ),
            (
                "scattered rows, then a run over a completed cell",
                vec![
                    (vec![5, 3, 90], ints(0..3)),
                    ((80..100).collect(), ints(80..100)),
                ],
            ),
        ];
        for (label, steps) in cases {
            let mut bulk = frame(1, 140);
            let mut cells = frame(1, 140);
            for (rows, values) in steps {
                for (index, &row) in rows.iter().enumerate() {
                    cells.complete(row, 0, values.get(index));
                }
                bulk.complete_rows(&rows, 0, values);
            }
            assert_eq!(canonical(&bulk), canonical(&cells), "{label}");
        }
    }

    // Filtering must keep the kept rows in order, each with its template,
    // identity, member records, values and completion, as a frame that
    // pushed only those rows holds them.
    #[test]
    fn a_filtered_frame_holds_its_kept_rows_in_order() {
        let mut filtered = Frame::new(2);
        let mut expected = Frame::new(2);
        let keep: Vec<bool> = (0..130).map(|row| row % 3 != 1 && row != 64).collect();
        let add = |frame: &mut Frame, row: usize| {
            let members: Vec<usize> = (0..row % 4).map(|member| row * 10 + member).collect();
            let at = frame.push((row % 2) as u32, Identity::Candidate(row), &members);
            let value = match row % 7 {
                0 => Value::Missing,
                1 => Value::Str(format!("r{row}")),
                _ => Value::Int(row as i64),
            };
            frame.complete(at, 0, ValueRef::from(&value));
            if row % 5 != 0 {
                frame.complete(at, 1, ValueRef::Int(row as i64));
            }
        };
        for row in 0..133 {
            add(&mut filtered, row);
            if row < 3 || keep[row - 3] {
                add(&mut expected, row);
            }
        }
        filtered.retain_from(3, &keep).unwrap();
        let rows = |frame: &Frame| {
            (0..frame.rows())
                .map(|row| {
                    (
                        frame.template(row),
                        frame.identity(row),
                        frame.members(row).to_vec(),
                        [frame.value(row, 0), frame.value(row, 1)],
                        [frame.is_completed(row, 0), frame.is_completed(row, 1)],
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&filtered),
            rows(&expected),
            "rows 3..133, every third and row 67 dropped"
        );
        assert_eq!(
            (0..2)
                .map(|column| std::mem::discriminant(filtered.column(column)))
                .collect::<Vec<_>>(),
            (0..2)
                .map(|column| std::mem::discriminant(expected.column(column)))
                .collect::<Vec<_>>(),
            "the column layouts"
        );
        assert_eq!(
            filtered.member_count(),
            expected.member_count(),
            "the member records"
        );
    }

    // Appending a partition's rows must read as pushing them, with their
    // records moved on.
    #[test]
    fn appended_rows_read_as_the_rows_they_copy() {
        let mut part = frame(1, 70);
        part.complete_rows(
            &(0..70).collect::<Vec<_>>(),
            0,
            pushed((0..70).map(|row| Value::Int(row as i64))),
        );
        let mut whole = frame(1, 3);
        whole.append(&part, 2..68, 500);
        for (row, from) in (3..69).zip(2..68) {
            assert_eq!(
                (
                    whole.members(row).to_vec(),
                    whole.value(row, 0),
                    whole.is_completed(row, 0)
                ),
                (vec![from + 500, from + 1500], Value::Int(from as i64), true),
                "appended row {row}"
            );
        }
        assert_eq!(
            whole.members(2),
            [2, 1002],
            "the rows before the appended ones"
        );
    }
}
