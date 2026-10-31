//! Partitions derived at once (#4193): the calling thread decodes each
//! partition in order and hands it to one of a bounded set of scoped
//! threads, which runs it as the serial run does
//! ([`ResolvedPlan::derive_serially`]); the calling thread then takes the
//! runs back in partition order.
//!
//! A partition's run depends on the partitions before it only through the
//! mark it stops at: the earliest mark an earlier partition failed at. A
//! thread runs each partition under the earliest failure known when the
//! partition was handed out, which is never earlier than the serial run's
//! limit. Taking the runs back in order, the calling thread knows that
//! limit: it keeps a run that is the serial run's ([`Part::holds_under`]),
//! and runs again on its own a partition whose run went past it. A run that
//! failed comes back without its failure, which may hold a host's error
//! that never leaves the thread that raised it, and the calling thread runs
//! that partition again to find it. Only a run with a failure reruns
//! anything.
//!
//! The calling thread alone reports progress and polls the cancellation
//! probe, which a host answers only on its own thread. It reports each
//! partition when it takes the partition's run back, in the serial run's
//! order, and none the serial run would not have decoded; and once the
//! probe answers true it stops every thread's run, which polls a flag at
//! the checkpoints the serial run polls the probe at. Only the first
//! partition's run can fail at its first mark, before it reads a record,
//! after which the serial run decodes no other partition: so no other
//! partition is decoded until that run has entered its second mark.
//!
//! A partition that cannot move to another thread
//! ([`TableAccess::into_shared`]) runs on the calling thread when its turn
//! comes.

use super::{plain, Derived, Fail, Failure, Part, Placement, ResolvedPlan, Run, Shared, Work};
use crate::dataset::execution::{ExecutionError, ExecutionEvent};
use crate::function_invocation::NoFunctions;
use crate::table::TableAccess;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the calling thread waits for a run before it polls the
/// cancellation probe again.
const POLL: Duration = Duration::from_millis(10);

/// A decoded partition as a thread owns it.
type Moved<'t, E> = Box<dyn TableAccess<Error = E> + Send + Sync + 't>;

/// A decoded partition handed to a thread.
struct Job<'t, E> {
    index: usize,
    table: Moved<'t, E>,
    placement: Placement,
    /// Raised once the first partition's run enters its second mark.
    opened: Option<Arc<AtomicBool>>,
}

/// A partition's run as a thread hands it back: without its failure, and
/// whether it failed; none when it was cancelled.
type Sent = Option<(Box<Part<Infallible>>, bool)>;

/// A partition's run handed back, with the mark it stopped at.
struct Ran {
    index: usize,
    limit: usize,
    part: Sent,
}

/// A partition the calling thread holds until it takes it in turn.
enum Slot<T> {
    /// A thread's run of it, with the mark it stopped at.
    Ran(usize, Sent),
    /// Decoded, but it cannot move to another thread.
    Held(T),
}

impl<E> Part<E> {
    /// This run without its failure, and whether it failed.
    fn sent(self) -> (Box<Part<Infallible>>, bool) {
        let failed = self.failure.is_some();
        let part = Box::new(Part {
            records: self.records,
            frame: self.frame,
            templates: self.templates,
            checked: self.checked,
            marks: self.marks,
            counts: self.counts,
            failure: None,
        });
        (part, failed)
    }
}

impl Part<Infallible> {
    /// A run handed back that did not fail, as the run it is part of holds
    /// it.
    fn received<E>(self: Box<Self>) -> Part<E> {
        Part {
            records: self.records,
            frame: self.frame,
            templates: self.templates,
            checked: self.checked,
            marks: self.marks,
            counts: self.counts,
            failure: None,
        }
    }
}

/// The partition at which the calling thread stopped handing out
/// partitions, which the serial run reaches only after every one before it.
#[derive(Clone, Copy)]
enum Halt {
    /// It did not decode, which ends the run as a cancelled one.
    Undecoded(usize),
    /// Its schema is not the driver input's.
    Schema(usize),
}

/// The calling thread's side of a concurrent derivation.
struct Coordinator<'c, 't, T, E> {
    work: Work<'c, T>,
    jobs: mpsc::Sender<Job<'t, E>>,
    ran: mpsc::Receiver<Ran>,
    threads: usize,
    /// Each partition handed out and not yet taken in turn.
    slots: Vec<Option<Slot<T>>>,
    /// The runs taken in turn, in partition order.
    parts: Vec<Part<E>>,
    /// The first failure among them.
    failure: Failure,
    /// The partitions handed out, and those a thread still runs.
    handed: usize,
    running: usize,
    opened: Arc<AtomicBool>,
    halt: Option<Halt>,
}

impl<T, E> Coordinator<'_, '_, T, E> {
    /// The mark the next partition's run stops at, as far as the runs taken
    /// in turn tell.
    fn limit(&self) -> usize {
        self.failure.map_or(usize::MAX, |(_, mark)| mark)
    }

    /// Whether a partition after the first may be decoded: once the first
    /// partition's run can no longer fail at its first mark.
    fn open(&self) -> bool {
        self.handed != 1
            || self.opened.load(Ordering::Acquire)
            || matches!(self.slots[0], Some(Slot::Ran(..)))
            || !self.parts.is_empty()
    }

    /// Keep partition `index`'s run, the serial run's.
    fn keep(&mut self, index: usize, part: Part<E>) {
        if let Some(mark) = part.failed_at() {
            self.failure = Some((index, mark));
        }
        self.parts.push(part);
    }
}

impl ResolvedPlan {
    /// Run the partitions of `work` on up to `threads` threads at once
    /// (#4193), each reading the other inputs as `secondary` shares them,
    /// with the serial run's parts, failure and events.
    pub(super) fn derive_concurrently<T: TableAccess>(
        &self,
        outer: &mut Run<'_, '_, T::Error>,
        work: Work<'_, T>,
        (secondary, threads): (&Shared<'_, T::Error>, usize),
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Derived<T::Error>, Fail<T::Error>> {
        let stop = AtomicBool::new(false);
        let (jobs, queue) = mpsc::channel();
        let queue = Mutex::new(queue);
        let (done, ran) = mpsc::channel();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    let done = done.clone();
                    let (queue, stop) = (&queue, &stop);
                    scope.spawn(move || self.derive_handed_out(queue, &done, secondary, stop))
                })
                .collect();
            drop(done);
            let partitions = work.ranges.len();
            let coordinator = Coordinator {
                work,
                jobs,
                ran,
                threads,
                slots: (0..partitions).map(|_| None).collect(),
                parts: Vec::with_capacity(partitions),
                failure: None,
                handed: 0,
                running: 0,
                opened: Arc::new(AtomicBool::new(false)),
                halt: None,
            };
            let ended = || workers.iter().any(|worker| worker.is_finished());
            let derived = self.coordinate(outer, coordinator, &ended, observer);
            // A run that ended early stops the partitions still running.
            stop.store(true, Ordering::Relaxed);
            for worker in workers {
                if let Err(panic) = worker.join() {
                    std::panic::resume_unwind(panic);
                }
            }
            derived
        })
    }

    /// A thread's side: run each partition handed out until none is left.
    fn derive_handed_out<E>(
        &self,
        queue: &Mutex<mpsc::Receiver<Job<'_, E>>>,
        done: &mpsc::Sender<Ran>,
        secondary: &Shared<'_, E>,
        stop: &AtomicBool,
    ) {
        let secondary = plain(secondary);
        let stopped = || stop.load(Ordering::Relaxed);
        loop {
            let job = queue.lock().ok().and_then(|queue| queue.recv().ok());
            let Some(Job {
                index,
                table,
                placement,
                opened,
            }) = job
            else {
                return;
            };
            let mut functions = NoFunctions::new();
            let part = self.run_partition(
                (&*table, &secondary),
                &mut functions,
                &stopped,
                (placement, opened),
            );
            drop(table);
            let ran = Ran {
                index,
                limit: placement.limit,
                part: part.map(Part::sent),
            };
            if done.send(ran).is_err() {
                return;
            }
        }
    }

    /// Hand out the partitions in order, at most one more than there are
    /// threads at once, and take them in turn, until every partition's run
    /// is the serial run's. `ended` answers whether a thread has ended,
    /// which only a panic ends early.
    fn coordinate<'t, T: TableAccess + 't>(
        &self,
        outer: &mut Run<'_, '_, T::Error>,
        mut c: Coordinator<'_, 't, T, T::Error>,
        ended: &dyn Fn() -> bool,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Derived<T::Error>, Fail<T::Error>> {
        let partitions = c.work.ranges.len();
        loop {
            if outer.stop_if_cancelled().is_err() {
                return Ok(None);
            }
            while let Some(slot) = c.slots.get_mut(c.parts.len()).and_then(Option::take) {
                let index = c.parts.len();
                match self.take(outer, &mut c, (index, slot), observer)? {
                    Some(part) => c.keep(index, part),
                    None => return Ok(None),
                }
            }
            if c.parts.len() == partitions {
                return Ok(Some((c.parts, c.failure)));
            }
            if let Some(halt) = c.halt {
                let (Halt::Undecoded(index) | Halt::Schema(index)) = halt;
                if index == c.parts.len() {
                    observer(ExecutionEvent::Partition {
                        number: index + 1,
                        records: c.work.ranges[index].len(),
                    });
                    return match halt {
                        Halt::Undecoded(_) => Ok(None),
                        Halt::Schema(_) => Err(Box::new(ExecutionError::SchemaMismatch)),
                    };
                }
            }
            if c.halt.is_none() && c.handed < partitions && c.running <= c.threads && c.open() {
                self.hand_out(outer, &mut c);
                continue;
            }
            match c.ran.recv_timeout(POLL) {
                Ok(Ran { index, limit, part }) => {
                    c.running -= 1;
                    c.slots[index] = Some(Slot::Ran(limit, part));
                }
                Err(RecvTimeoutError::Timeout) if !ended() => {}
                // A thread panicked: joining it raises the panic.
                Err(_) => return Ok(None),
            }
        }
    }

    /// Take partition `index` in turn, as the serial run reaches it: report
    /// it unless an earlier failure at the first mark passes it, and keep
    /// its run when that is the serial run's, or else run it here. None
    /// when the run was cancelled or the partition did not decode again.
    fn take<T: TableAccess>(
        &self,
        outer: &mut Run<'_, '_, T::Error>,
        c: &mut Coordinator<'_, '_, T, T::Error>,
        (index, slot): (usize, Slot<T>),
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Option<Part<T::Error>>, Fail<T::Error>> {
        let range = c.work.ranges[index].clone();
        let limit = c.limit();
        if limit == 0 {
            return Ok(Some(self.passed(outer, range.start)));
        }
        observer(ExecutionEvent::Partition {
            number: index + 1,
            records: range.len(),
        });
        match slot {
            Slot::Ran(_, None) => Ok(None),
            Slot::Ran(used, Some((part, false))) if part.holds_under(used, limit) => {
                Ok(Some(part.received()))
            }
            // A failure, which its thread keeps, is found again here, and so
            // is a run that went past the serial run's limit.
            Slot::Ran(..) => self.decode_and_run(
                outer,
                &mut *c.work.decode,
                (c.work.schema, c.work.ranges),
                (index, limit),
            ),
            Slot::Held(table) => Ok(self.run_here(outer, &table, (range.start, index == 0, limit))),
        }
    }

    /// Decode the next partition on the calling thread and hand it to a
    /// thread, to run under the earliest failure known, or hold it when it
    /// cannot move.
    fn hand_out<'t, T: TableAccess + 't>(
        &self,
        outer: &Run<'_, '_, T::Error>,
        c: &mut Coordinator<'_, 't, T, T::Error>,
    ) {
        let index = c.handed;
        c.handed += 1;
        let range = c.work.ranges[index].clone();
        let limit = c.limit();
        if limit == 0 {
            let passed = Box::new(self.passed(outer, range.start));
            c.slots[index] = Some(Slot::Ran(0, Some((passed, false))));
            return;
        }
        let Some(table) = (c.work.decode)(index, range.clone()) else {
            c.halt = Some(Halt::Undecoded(index));
            return;
        };
        if table.schema() != c.work.schema {
            c.halt = Some(Halt::Schema(index));
            return;
        }
        let table = match table.into_shared() {
            Ok(table) => table,
            Err(table) => {
                c.slots[index] = Some(Slot::Held(table));
                return;
            }
        };
        let job = Job {
            index,
            table,
            placement: Placement {
                records: range.start,
                first: index == 0,
                limit,
                output_partition: outer.output_partition,
            },
            opened: (index == 0).then(|| Arc::clone(&c.opened)),
        };
        // Every thread runs until the calling thread returns.
        if c.jobs.send(job).is_ok() {
            c.running += 1;
        }
    }
}
