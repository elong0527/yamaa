//! The partitions of a held file read by partition (#3792): each one's
//! pages admitted, its stored columns decoded and its table built once, and
//! given to the scan's visit in order on the calling thread (#3830), one at
//! a time on the calling thread or up to `threads` at once, each on a
//! thread of its own (#4220).
//!
//! A partition's checks read the held file and its layout alone, and its
//! first record's position among the file's comes from the layout, so no
//! partition's checks depend on another's. What is serial is the order of
//! the findings, which the whole read sets ([`super::scan`]), and the
//! visit, which reads the records in order. So each thread builds the
//! partitions handed to it ([`partition`]), with what the scan's caller
//! makes of each table on its own ([`Prepare`]), and the calling thread
//! takes them back in order and folds them as the serial scan does
//! ([`Order::take`]), visiting each table in turn. It hands out a partition
//! only while fewer than `threads` are handed out and not yet taken, so no
//! more than `threads` partitions are decoded at once.
//!
//! A thread may build a partition the serial scan would not have: one past
//! a partition whose stored columns do not decode, or past one whose pages
//! do not admit, which ends the scan. The calling thread drops what such a
//! partition gives, so the result is the serial scan's. A thread that
//! panics hands its panic back, and the calling thread resumes it in that
//! partition's turn.

use super::{reader, table, table_error, Error, ADMIT, BUILD};
use crate::column_table::ColumnTable;
use std::num::NonZeroUsize;
use std::sync::{mpsc, Mutex};

/// What the scan's caller makes of each table built, on the thread that
/// built it, before the calling thread visits it: a step that reads that
/// table alone.
pub(super) type Prepare<'p, T> = &'p (dyn Fn(ColumnTable) -> T + Sync);

/// What the whole read finds of one partition, stage by stage: its pages,
/// then its stored columns, then its table.
enum Partition<T> {
    /// Its pages do not admit.
    Refused(reader::Error),
    /// Its pages admit and its stored columns do not decode.
    Undecoded(reader::Error),
    /// Its table prepared, or its first finding with the finding's
    /// position; `None` when the read builds no table of it.
    Built(Option<Result<T, (reader::Position, Error)>>),
}

/// How far the read checks a partition past its pages.
#[derive(Clone, Copy)]
struct Steps {
    /// Decode its stored columns: the layout admits, and no partition
    /// before it is known not to decode.
    decode: bool,
    /// Build its table: the checks of the whole file the read makes once
    /// every partition's stored columns are decoded admit it.
    build: bool,
}

/// Partition `index` of `runs`, whose first record is record `before` of
/// the file, checked as far as `steps` asks, its table prepared.
fn partition<T>(
    runs: &reader::Runs,
    (index, before): (usize, usize),
    steps: Steps,
    prepare: Prepare<'_, T>,
) -> Partition<T> {
    // A partition decodes from the bodies its pages' admission kept.
    let bodies = match runs.admit(index) {
        Ok(bodies) => bodies,
        Err(error) => return Partition::Refused(error),
    };
    if !steps.decode {
        return Partition::Built(None);
    }
    let raw = match runs.raw(index, &bodies) {
        Ok(raw) => raw,
        Err(error) => return Partition::Undecoded(error),
    };
    drop(bodies);
    if !steps.build {
        return Partition::Built(None);
    }
    Partition::Built(Some(built(runs, raw, (index, before)).map(prepare)))
}

/// Partition `index`'s table from its stored columns `raw`, or its first
/// finding with the finding's position.
fn built(
    runs: &reader::Runs,
    raw: Vec<super::columns::Column>,
    (index, before): (usize, usize),
) -> Result<ColumnTable, (reader::Position, Error)> {
    use yamaa_core::table::TableAccess;
    let decoded = runs
        .decoded(raw, index, before)
        .map_err(|(position, error)| (position, Error::from(error)))?;
    let rows = table::admit(&decoded).map_err(|error| (ADMIT, table_error(error)))?;
    let table = table::build(decoded, rows).map_err(|error| (BUILD, table_error(error)))?;
    debug_assert_eq!(table.row_count(), runs.records(index));
    Ok(table)
}

/// The whole read's order over the partitions, taken in turn.
struct Order {
    /// The first finding among the tables built, by its position, then by
    /// its partition.
    first: Option<(reader::Position, Error)>,
    /// What the first partition whose stored columns do not decode finds.
    undecoded: Option<reader::Error>,
}

impl Order {
    /// Take the next partition, whose first record is record `before` of
    /// the file, visiting its table while no finding has come before it.
    /// `Err` when its pages do not admit, which ends the read.
    fn take<T>(
        &mut self,
        partition: Partition<T>,
        before: usize,
        visit: &mut dyn FnMut(T, usize),
    ) -> Result<(), Error> {
        match partition {
            // The whole read checks every page before anything later, so the
            // first page that does not admit is its finding, wherever it is.
            Partition::Refused(error) => return Err(error.into()),
            // It decodes every partition's columns before any later check,
            // so the first partition that does not decode is its finding,
            // and nothing past it is decoded.
            _ if self.undecoded.is_some() => {}
            Partition::Undecoded(error) => self.undecoded = Some(error),
            Partition::Built(None) => {}
            Partition::Built(Some(Ok(table))) => {
                if self.first.is_none() {
                    visit(table, before);
                }
            }
            Partition::Built(Some(Err((position, error)))) => {
                if self.first.as_ref().is_none_or(|(at, _)| position < *at) {
                    self.first = Some((position, error));
                }
            }
        }
        Ok(())
    }

    /// The read's result once every partition is taken: what the whole read
    /// checks once every page is admitted, the first partition that did not
    /// decode, the checks of the whole file, then the first finding.
    fn finish(self, runs: &reader::Runs) -> Result<(), Error> {
        runs.layout()?;
        if let Some(error) = self.undecoded {
            return Err(error.into());
        }
        runs.deferred()?;
        self.first.map_or(Ok(()), |(_, error)| Err(error))
    }
}

/// Check every partition of `runs` as the whole read does, on up to
/// `threads` threads, and give each table, prepared, to `visit` in order on
/// the calling thread while no finding has come before it.
pub(super) fn scan<T: Send>(
    runs: &reader::Runs,
    threads: NonZeroUsize,
    (prepare, visit): super::ScanSteps<'_, T>,
) -> Result<(), Error> {
    let admitted = runs.layout().is_ok();
    // Each partition's first record among the file's, once the layout
    // admits its records.
    let mut at = 0usize;
    let starts: Vec<usize> = (0..runs.len())
        .map(|index| {
            let start = at;
            if admitted {
                at += runs.records(index);
            }
            start
        })
        .collect();
    let steps = Steps {
        decode: admitted,
        build: runs.deferred().is_ok(),
    };
    let mut order = Order {
        first: None,
        undecoded: None,
    };
    let threads = threads.get().min(runs.len());
    if threads <= 1 {
        for (index, &before) in starts.iter().enumerate() {
            let steps = Steps {
                decode: steps.decode && order.undecoded.is_none(),
                ..steps
            };
            let partition = partition(runs, (index, before), steps, prepare);
            order.take(partition, before, visit)?;
        }
    } else {
        let work = Work {
            runs,
            starts: &starts,
            steps,
            prepare,
        };
        threaded(work, threads, &mut order, visit)?;
    }
    order.finish(runs)
}

/// What every thread reads to build the partitions handed to it.
struct Work<'w, T> {
    runs: &'w reader::Runs,
    starts: &'w [usize],
    steps: Steps,
    prepare: Prepare<'w, T>,
}

impl<T> Clone for Work<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Work<'_, T> {}

/// A partition a thread hands back: its index, and what it found, or the
/// thread's panic.
type Handed<T> = (usize, std::thread::Result<Partition<T>>);

/// Build the partitions of `work` on `threads` scoped threads, and take
/// them back in order into `order`.
fn threaded<T: Send>(
    work: Work<'_, T>,
    threads: usize,
    order: &mut Order,
    visit: &mut dyn FnMut(T, usize),
) -> Result<(), Error> {
    let (jobs, queue) = mpsc::channel::<usize>();
    let queue = Mutex::new(queue);
    let (done, handed) = mpsc::channel::<Handed<T>>();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let (queue, done) = (&queue, done.clone());
            scope.spawn(move || {
                while let Some(index) = queue.lock().ok().and_then(|queue| queue.recv().ok()) {
                    let place = (index, work.starts[index]);
                    let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        partition(work.runs, place, work.steps, work.prepare)
                    }));
                    if done.send((index, found)).is_err() {
                        return;
                    }
                }
            });
        }
        drop(done);
        take_in_turn((jobs, handed), (threads, work.starts), order, visit)
    })
}

/// The calling thread's side: hand out the first `threads` partitions, then
/// one more for each taken in turn, and take each back in order. Returning
/// drops `jobs`, which ends every thread once its partition is built.
fn take_in_turn<T>(
    (jobs, handed): (mpsc::Sender<usize>, mpsc::Receiver<Handed<T>>),
    (threads, starts): (usize, &[usize]),
    order: &mut Order,
    visit: &mut dyn FnMut(T, usize),
) -> Result<(), Error> {
    let count = starts.len();
    // The queue outlives this side, so each partition handed out reaches it.
    let hand_out = |index: usize| {
        if index < count {
            jobs.send(index).expect("the threads wait for partitions");
        }
    };
    (0..threads).for_each(&hand_out);
    let mut held: Vec<Option<std::thread::Result<Partition<T>>>> =
        (0..count).map(|_| None).collect();
    for (next, &before) in starts.iter().enumerate() {
        while held[next].is_none() {
            let (index, found) = handed
                .recv()
                .expect("every partition handed out is handed back");
            held[index] = Some(found);
        }
        // A thread's panic resumes in its partition's turn, as the serial
        // scan would have panicked there.
        let partition = held[next]
            .take()
            .expect("a partition handed back")
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        order.take(partition, before, visit)?;
        hand_out(next + threads);
    }
    Ok(())
}

/// A panic on a scan's thread (#4220): the seam is the threads' hand-back,
/// which no read reaches, since a partition whose decode panics is a defect.
#[cfg(test)]
mod tests {
    use crate::column_table::ColumnTable;
    use arrow_array::{Int64Array, RecordBatch};
    use arrow_schema::{DataType, Field, Schema};
    use parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use yamaa_core::table::{TableAccess, ValueRef};

    /// `ID` 1 to 15 in five row groups of three.
    fn fifteen() -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, true)]));
        let ids = Arc::new(Int64Array::from_iter_values(1..=15));
        let batch = RecordBatch::try_new(schema.clone(), vec![ids]).unwrap();
        let properties = WriterProperties::builder()
            .set_max_row_group_size(3)
            .build();
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        bytes
    }

    /// Partitions of three records on four threads, the third of which
    /// panics on its thread: the scan panics on the calling thread with that
    /// panic, in the third partition's turn, once the first two, whose first
    /// records are 0 and 3, are visited, as the scan on one thread does.
    #[test]
    fn a_partition_that_panics_on_its_thread_panics_the_scan_in_its_turn() {
        let bytes = fifteen();
        let prepare = |table: ColumnTable| {
            if table.cell(0, 0).unwrap() == ValueRef::Int(7) {
                panic!("the partition from ID 7");
            }
            table
        };
        for threads in [1, 4] {
            let mut visited = Vec::new();
            let partitions = (
                NonZeroUsize::new(3).unwrap(),
                NonZeroUsize::new(threads).unwrap(),
            );
            let scan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let visit = &mut |_: ColumnTable, first| visited.push(first);
                super::super::scan(&bytes, false, partitions, (&prepare, visit)).map(|_| ())
            }));
            let panic = scan.expect_err("the scan panics");
            assert_eq!(
                (panic.downcast_ref::<&str>(), visited),
                (Some(&"the partition from ID 7"), vec![0, 3]),
                "{threads} threads"
            );
        }
    }
}
