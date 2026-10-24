//! The cut of a driver input into partitions (#3697, #3763), read once in
//! record order, so a reader can give it one partition of records at a time
//! and drop each before the next (#3830).
//!
//! Without fields, the partitions are consecutive records, `rows` each, the
//! last holding the rest. With fields, they are key-bounded: the records are
//! grouped by the fields with the equality key groups use
//! (`table_grouping::Key`: missing equals missing, the float zero signs are
//! equal, an integer is not its decimal text), and each partition holds as
//! many whole runs of equal values as fit in `rows` records, a longer run
//! alone. The cut keeps the key of every run that has ended, never a record,
//! so it finds a value that recurs after its run has ended: the input is
//! then not grouped by the fields, and has no cut.
use crate::table::TableAccess;
use crate::table_grouping::{Key, OwnedKey};
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::ops::Range;

/// Partitions of `rows` consecutive records of an input of `len` records,
/// the last holding the rest (#3697). An empty input is one empty
/// partition, so admission still checks its schema.
pub(crate) fn consecutive(len: usize, rows: usize) -> Vec<Range<usize>> {
    (0..len.max(1))
        .step_by(rows)
        .map(|start| start..start + rows.min(len.saturating_sub(start)))
        .collect()
}

/// The cut of the records read so far, in order.
#[derive(Debug)]
pub struct PartitionCut {
    /// The fields whose runs no partition splits; none for consecutive
    /// records.
    fields: Vec<usize>,
    rows: usize,
    /// The records read so far.
    read: usize,
    /// The run being read: its key, and its first record.
    run: Option<(Vec<OwnedKey>, usize)>,
    /// The key of every run that has ended.
    ended: BTreeSet<Vec<OwnedKey>>,
    /// Where each partition starts, the open one last.
    starts: Vec<usize>,
    /// The records of the open partition's completed runs.
    size: usize,
    /// Whether the records read are grouped by the fields, each field read.
    grouped: bool,
}

impl PartitionCut {
    /// A cut into partitions of `rows` records that keep the runs of
    /// `fields` whole, or of consecutive records without fields.
    pub fn new(fields: Vec<usize>, rows: NonZeroUsize) -> Self {
        Self {
            fields,
            rows: rows.get(),
            read: 0,
            run: None,
            ended: BTreeSet::new(),
            starts: vec![0],
            size: 0,
            grouped: true,
        }
    }

    /// Read `table`'s records, the next ones of the input, in order.
    pub fn read<T: TableAccess + ?Sized>(&mut self, table: &T) {
        let first = self.read;
        self.read += table.row_count();
        if self.fields.is_empty() || !self.grouped {
            return;
        }
        for row in 0..table.row_count() {
            let mut key = Vec::with_capacity(self.fields.len());
            for &field in &self.fields {
                match table.cell(row, field) {
                    Ok(value) => key.push(Key::from(value)),
                    Err(_) => {
                        self.grouped = false;
                        return;
                    }
                }
            }
            let same = self.run.as_ref().is_some_and(|(current, _)| {
                current.iter().map(OwnedKey::key).eq(key.iter().cloned())
            });
            if same {
                continue;
            }
            let record = first + row;
            self.close(record);
            let key: Vec<OwnedKey> = key.into_iter().map(OwnedKey::from).collect();
            if self.ended.contains(&key) {
                self.grouped = false;
                return;
            }
            self.run = Some((key, record));
        }
    }

    /// End the run being read before record `end`, packing it into the open
    /// partition, or opening a new one when it does not fit.
    fn close(&mut self, end: usize) {
        let Some((key, start)) = self.run.take() else {
            return;
        };
        let len = end - start;
        if self.size > 0 && self.size + len > self.rows {
            self.starts.push(start);
            self.size = 0;
        }
        self.size += len;
        self.ended.insert(key);
    }

    /// The partitions of every record read, in order. None when the records
    /// are not grouped by the fields, or a field could not be read: no cut
    /// keeps their runs whole, so the input is one partition.
    pub fn finish(mut self) -> Option<Vec<Range<usize>>> {
        if self.fields.is_empty() {
            return Some(consecutive(self.read, self.rows));
        }
        if !self.grouped {
            return None;
        }
        self.close(self.read);
        let mut bounds: Vec<Range<usize>> = self
            .starts
            .windows(2)
            .map(|bounds| bounds[0]..bounds[1])
            .collect();
        bounds.push(self.starts[self.starts.len() - 1]..self.read);
        Some(bounds)
    }
}
