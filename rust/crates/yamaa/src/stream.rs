//! Streamed runs (#3697): a specification run whose output its consumer
//! reads as a stream of Arrow batches, the consumer's pull driving the run.
//!
//! [`Engine::spec_stream`](crate::Engine::spec_stream) compiles a
//! specification, or reuses the plan a [`Plans`](crate::Plans) holder kept,
//! and returns a [`Streamed`] run before reading any study data or calling
//! any project function. A [`Batches`] reader of its output produces it as
//! it is read:
//!
//! - its first pull builds the run, through every barrier a whole-table run
//!   has (admission, derivation, output keys, verification and the final
//!   order), so an accepted run's first batch exists only once its whole
//!   output is accepted;
//! - each pull after it builds the next batch of the accepted rows, at most
//!   65,536 records, or on an engine with
//!   [`Partitions`](crate::Partitions) at most their `rows`, never crossing
//!   a batch of the whole-table run's output table. The batches hold that
//!   table's values in its order, under the schema
//!   [`Streamed::schema`] gives before the first pull.
//!
//! The pull is the backpressure: nothing is built ahead of a pull, no batch
//! is kept once handed out, and there is no queue. [`Batches::into_stream`]
//! exports a reader as a standard Arrow C stream whose `get_next` is the
//! pull and whose release before the end is the reader's release.
//!
//! The run polls the token it was given at every checkpoint of its schedule,
//! as a whole-table run does, and before each batch. Its end states are the
//! mid-stream row of #3697's cancel-or-fail matrix:
//!
//! - **Cancel.** A token found cancelled, a project function's interrupt
//!   (D46), or the release of a reader before the end of the stream it
//!   started ends production: no further batch is built or kept, and the run
//!   is cancelled, with no result and no issue, so nothing of it can be
//!   published. The reader's pull, and [`Streamed::run`], fail with
//!   [`Error::Cancelled`].
//! - **Fail.** A run that fails ends the stream with its issues: the pull
//!   fails with [`Error::Rejected`], and the run, which [`Streamed::run`]
//!   then gives, holds those issues and no output; its save writes only its
//!   report file (OUTPUT-10).
//!
//! A run whose output nobody has started to read runs whole when its result
//! is asked for: [`Streamed::run`] builds it as
//! [`Engine::spec_run_reusing`](crate::Engine::spec_run_reusing) does, and a
//! reader then reads the output table that run keeps. Once a reader has
//! started the stream, the run's result is known when that stream ends.
//!
//! When the run can call its host's project functions (an engine with
//! [`ProjectFunctions`](crate::ProjectFunctions) runs a specification with a
//! project), only the thread that started it builds it: a pull or a
//! [`Streamed::run`] that would build it on another thread fails with
//! `host_thread` instead of calling the host (the engine boundary design of
//! #2870, section 3.6). Batches of a built run are read on any thread.

use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread::ThreadId;

use arrow_array::ffi_stream::FFI_ArrowArrayStream;
use arrow_array::{RecordBatch, RecordBatchReader};
use arrow_schema::{ArrowError, Schema, SchemaRef};
use yamaa_adapters::application_measurements::Measurements;

use crate::api::{Failure, Run, Status};
use crate::cancel::CancellationToken;
use crate::engine::Engine;
use crate::error::{Error, TableError};
use crate::internal::{EngineShared, PublicBatches};
use crate::plan::Plan;
use crate::table::Table;
use crate::text::Action;

/// Where a streamed run stands.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamProgress {
    /// Nothing is built and no reader has started its stream.
    Unread,
    /// A reader has started its stream, which has not ended.
    Reading,
    /// The run has a result: its stream ended, or it ran whole.
    Ended,
    /// The run was cancelled and has no result.
    Cancelled,
}

/// What a run needs to be built.
struct Pending {
    shared: Arc<EngineShared>,
    plan: Plan,
    measurements: Arc<Measurements>,
}

impl Pending {
    /// Build the run, named `subject`, polling `token`; with `rows`, as a
    /// stream of batches of at most that many records. None when the run
    /// was cancelled.
    fn build(
        self,
        subject: &str,
        token: &CancellationToken,
        rows: Option<NonZeroUsize>,
    ) -> Option<(Run, Option<PublicBatches>)> {
        if token.is_cancelled() {
            return None;
        }
        let (build, batches) = match rows {
            Some(rows) => self
                .plan
                .stream_through(&self.shared, &self.measurements, token, rows),
            None => (
                self.plan
                    .run_through(&self.shared, None, &self.measurements, token),
                None,
            ),
        };
        if build.interrupted() || token.is_cancelled() {
            return None;
        }
        self.measurements.complete(build.accepted());
        let build = build.with_subject(subject);
        Some((Run { build }, batches))
    }
}

enum State {
    /// Nothing built, no reader started.
    Unread(Box<Pending>),
    /// A reader started the stream; its first pull builds the run.
    Taken(Box<Pending>),
    /// The run was accepted; its batches are being read.
    Producing {
        run: Run,
        batches: PublicBatches,
    },
    /// The run has a result: its stream ended when `streamed`, or it ran
    /// whole.
    Ended {
        run: Run,
        streamed: bool,
    },
    Cancelled,
}

impl State {
    /// A reader that started the stream was released before its end.
    fn abandon(&mut self) {
        if matches!(self, Self::Taken(_) | Self::Producing { .. }) {
            *self = Self::Cancelled;
        }
    }
}

struct Shared {
    /// What the run's refusals name: the dataset name once read, or the
    /// path as given.
    subject: String,
    schema: Option<SchemaRef>,
    token: CancellationToken,
    /// The thread that started the run.
    owner: ThreadId,
    /// Whether building the run can call the host.
    host: bool,
    rows: NonZeroUsize,
    /// A reader released while another call held the state.
    abandoned: AtomicBool,
    state: Mutex<State>,
}

impl Shared {
    /// The state, unless another call holds it.
    fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return Err(Error::Stream { code: "busy" }),
        };
        if self.abandoned.swap(false, Ordering::SeqCst) {
            state.abandon();
        }
        Ok(state)
    }

    /// Refuse to build a run that can call the host off the thread that
    /// started it.
    fn on_owner(&self) -> Result<(), Error> {
        if self.host && std::thread::current().id() != self.owner {
            return Err(Error::Stream {
                code: "host_thread",
            });
        }
        Ok(())
    }
}

/// The error that ends the stream of the failed `run`: its issues.
fn failure(run: &Run) -> Error {
    Error::Rejected(run.build.report().clone())
}

/// One specification run whose output its consumer reads as a stream; see
/// the [module](self) documentation. It is `Send` and `Sync`, and every
/// method refuses with the `busy` code of [`Error::Stream`] while another
/// call, such as a project function called by its own build, uses it.
pub struct Streamed {
    shared: Arc<Shared>,
}

impl Streamed {
    /// A streamed run of `plan` through `engine`, polling `token`. A refused
    /// compilation's run has already ended: it holds its refusal and reads
    /// nothing.
    pub(crate) fn new(
        engine: &Engine,
        plan: Plan,
        specification: &str,
        measurements: Arc<Measurements>,
        token: &CancellationToken,
    ) -> Self {
        let shared = engine.shared_owned();
        let subject = plan.name().unwrap_or(specification).to_owned();
        let schema = plan.schema();
        let host = plan.calls_host(&shared);
        let rows = shared
            .partitioning
            .map_or(NonZeroUsize::MAX, |partitioning| partitioning.rows);
        let state = if plan.report().is_empty() {
            State::Unread(Box::new(Pending {
                shared,
                plan,
                measurements,
            }))
        } else {
            let build = plan.run_through(&shared, None, &measurements, token);
            measurements.complete(false);
            let build = build.with_subject(&subject);
            State::Ended {
                run: Run { build },
                streamed: false,
            }
        };
        Self {
            shared: Arc::new(Shared {
                subject,
                schema,
                token: token.clone(),
                owner: std::thread::current().id(),
                host,
                rows,
                abandoned: AtomicBool::new(false),
                state: Mutex::new(state),
            }),
        }
    }

    /// The Arrow schema of every batch of the output, the whole-table run's
    /// output table's, known before the run: one nullable field per output
    /// column, `Utf8`, `Int64`, `Float64`, `Date32` or `Timestamp(Second)`.
    /// None for a refused compilation, which has no output.
    pub fn schema(&self) -> Option<&SchemaRef> {
        self.shared.schema.as_ref()
    }

    /// What the run's printed text and refusals name: the dataset name once
    /// its compilation read it, or else the path as given.
    pub fn subject(&self) -> &str {
        &self.shared.subject
    }

    /// Where the run stands.
    pub fn progress(&self) -> Result<StreamProgress, Error> {
        Ok(match &*self.shared.lock()? {
            State::Unread(_) => StreamProgress::Unread,
            State::Taken(_) | State::Producing { .. } => StreamProgress::Reading,
            State::Ended { .. } => StreamProgress::Ended,
            State::Cancelled => StreamProgress::Cancelled,
        })
    }

    /// A reader of the output, which reads nothing until its first pull.
    ///
    /// That pull starts the run's stream when no reader has, and builds the
    /// run; once the run ran whole, it reads the output table the run keeps,
    /// with a cursor of its own. Otherwise it fails: with the run's issues
    /// for a failed run, [`Error::Cancelled`] for a cancelled one, and the
    /// `stream_taken` code of [`Error::Stream`] when another reader started
    /// the stream. A reader that would build a run that can call its host
    /// off the thread that started it fails with `host_thread` and starts
    /// nothing.
    pub fn reader(&self) -> Batches {
        Batches {
            shared: Arc::clone(&self.shared),
            mode: Mode::Unstarted,
        }
    }

    /// The run's result: its output (missing when it failed, or when its
    /// output was read as a stream), issues, run report, summary and save.
    ///
    /// A run that nobody has started to read runs whole now, as
    /// [`Engine::spec_run_reusing`](crate::Engine::spec_run_reusing) runs
    /// it, and keeps its output table. While a reader's stream has not
    /// ended, this fails with the `stream_open` code of [`Error::Stream`];
    /// a cancelled run fails with [`Error::Cancelled`]; and a run that can
    /// call its host is built only on the thread that started it, failing
    /// with `host_thread` on another.
    pub fn run(&self) -> Result<RunGuard<'_>, Error> {
        let mut state = self.shared.lock()?;
        if matches!(&*state, State::Unread(_)) {
            self.shared.on_owner()?;
            let State::Unread(pending) = std::mem::replace(&mut *state, State::Cancelled) else {
                unreachable!("an unread run")
            };
            if let Some((run, _)) = pending.build(&self.shared.subject, &self.shared.token, None) {
                *state = State::Ended {
                    run,
                    streamed: false,
                };
            }
        }
        match &*state {
            State::Ended { .. } => Ok(RunGuard { state }),
            State::Cancelled => Err(Error::Cancelled),
            State::Unread(_) | State::Taken(_) | State::Producing { .. } => Err(Error::Stream {
                code: "stream_open",
            }),
        }
    }

    /// The output read whole: the table the run keeps once it ran whole,
    /// which it does now when nobody has started to read its output. A
    /// failed run refuses with its issues ([`Error::Rejected`]), a run whose
    /// output was read as a stream with the `stream_taken` code of
    /// [`Error::Stream`], and otherwise this refuses as [`Streamed::run`]
    /// does.
    pub fn table(&self) -> Result<Table, Error> {
        let run = self.run()?;
        match run.output() {
            Some(table) => Ok(table.clone()),
            None if run.status() == Status::Failed => Err(failure(&run)),
            None => Err(Error::Stream {
                code: "stream_taken",
            }),
        }
    }
}

/// The result of a [`Streamed`] run, borrowed: it dereferences to the
/// [`Run`]. While it lives, every other call on the streamed run refuses
/// with the `busy` code of [`Error::Stream`].
pub struct RunGuard<'a> {
    state: MutexGuard<'a, State>,
}

impl Deref for RunGuard<'_> {
    type Target = Run;

    fn deref(&self) -> &Run {
        match &*self.state {
            State::Ended { run, .. } => run,
            _ => unreachable!("a guard holds an ended run"),
        }
    }
}

impl DerefMut for RunGuard<'_> {
    fn deref_mut(&mut self) -> &mut Run {
        match &mut *self.state {
            State::Ended { run, .. } => run,
            _ => unreachable!("a guard holds an ended run"),
        }
    }
}

enum Mode {
    /// Nothing pulled.
    Unstarted,
    /// This reader started the run's stream.
    Producing,
    /// This reader reads the table a whole run kept.
    Table(std::vec::IntoIter<RecordBatch>),
    Done,
}

/// A reader of a [`Streamed`] run's output, from [`Streamed::reader`]: an
/// iterator whose every pull builds what it hands out, and which ends with
/// `None`, or with one error and then `None`. A refusal that starts
/// nothing, `host_thread` or `busy`, leaves the reader as it was.
///
/// Dropping a reader before the end of the stream it started cancels the
/// run.
pub struct Batches {
    shared: Arc<Shared>,
    mode: Mode,
}

impl Batches {
    /// The schema of every batch, [`Streamed::schema`], or an empty schema
    /// for a refused compilation.
    pub fn schema(&self) -> SchemaRef {
        self.shared
            .schema
            .clone()
            .unwrap_or_else(|| Arc::new(Schema::empty()))
    }

    /// This reader as a standard Arrow C stream: `get_schema` gives
    /// [`Batches::schema`], each `get_next` is one pull, and releasing the
    /// stream drops the reader. A pull's error ends the stream with an
    /// error whose message is D39's text of reading the run, such as
    /// `cannot read TEST: operation cancelled`, or the run's first issue.
    pub fn into_stream(self) -> FFI_ArrowArrayStream {
        FFI_ArrowArrayStream::new(Box::new(Exported(self)))
    }

    /// One pull that starts the stream, or continues the one this reader
    /// started.
    fn pull(&mut self) -> Option<Result<RecordBatch, Error>> {
        let shared = Arc::clone(&self.shared);
        let mut state = match shared.lock() {
            Ok(state) => state,
            Err(error) => return Some(Err(error)),
        };
        if matches!(self.mode, Mode::Unstarted) {
            match &*state {
                State::Unread(_) => {
                    if let Err(error) = shared.on_owner() {
                        return Some(Err(error));
                    }
                    let State::Unread(pending) = std::mem::replace(&mut *state, State::Cancelled)
                    else {
                        unreachable!("an unread run")
                    };
                    *state = State::Taken(pending);
                    self.mode = Mode::Producing;
                }
                State::Ended {
                    run,
                    streamed: false,
                } => {
                    let Some(table) = run.output() else {
                        self.mode = Mode::Done;
                        return Some(Err(failure(run)));
                    };
                    let mut batches = table.batches().to_vec().into_iter();
                    let first = batches.next();
                    self.mode = Mode::Table(batches);
                    return first.map(Ok);
                }
                State::Cancelled => {
                    self.mode = Mode::Done;
                    return Some(Err(Error::Cancelled));
                }
                State::Taken(_) | State::Producing { .. } | State::Ended { .. } => {
                    self.mode = Mode::Done;
                    return Some(Err(Error::Stream {
                        code: "stream_taken",
                    }));
                }
            }
        }
        if matches!(&*state, State::Taken(_)) {
            if let Err(error) = shared.on_owner() {
                return Some(Err(error));
            }
            let State::Taken(pending) = std::mem::replace(&mut *state, State::Cancelled) else {
                unreachable!("a taken run")
            };
            match pending.build(&shared.subject, &shared.token, Some(shared.rows)) {
                // The state stays cancelled.
                None => {
                    self.mode = Mode::Done;
                    return Some(Err(Error::Cancelled));
                }
                Some((run, Some(batches))) => *state = State::Producing { run, batches },
                Some((run, None)) => {
                    self.mode = Mode::Done;
                    let error = if run.build.accepted() {
                        Error::Internal {
                            code: "stream_batches",
                        }
                    } else {
                        failure(&run)
                    };
                    *state = State::Ended {
                        run,
                        streamed: true,
                    };
                    return Some(Err(error));
                }
            }
        }
        let State::Producing { run, mut batches } =
            std::mem::replace(&mut *state, State::Cancelled)
        else {
            // Only this reader's own release cancels the stream it started.
            self.mode = Mode::Done;
            return Some(Err(Error::Cancelled));
        };
        // Before each batch, the token; the state stays cancelled.
        if !batches.ended() && shared.token.is_cancelled() {
            self.mode = Mode::Done;
            return Some(Err(Error::Cancelled));
        }
        match batches.next() {
            Some(Ok(batch)) => {
                *state = State::Producing { run, batches };
                Some(Ok(batch))
            }
            None => {
                self.mode = Mode::Done;
                *state = State::Ended {
                    run,
                    streamed: true,
                };
                None
            }
            Some(Err(error)) => {
                self.mode = Mode::Done;
                *state = State::Ended {
                    run,
                    streamed: true,
                };
                Some(Err(Error::Table(TableError::from_ipc(error))))
            }
        }
    }
}

impl Iterator for Batches {
    type Item = Result<RecordBatch, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.mode {
            Mode::Done => None,
            Mode::Table(batches) => {
                let next = batches.next();
                if next.is_none() {
                    self.mode = Mode::Done;
                }
                next.map(Ok)
            }
            Mode::Unstarted | Mode::Producing => self.pull(),
        }
    }
}

impl Drop for Batches {
    /// Releasing the reader that started the stream before its end cancels
    /// the run; a reader that started nothing changes nothing.
    fn drop(&mut self) {
        if !matches!(self.mode, Mode::Producing) {
            return;
        }
        match self.shared.state.try_lock() {
            Ok(mut state) => state.abandon(),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().abandon(),
            // Another call holds the state: the next one cancels the run.
            Err(TryLockError::WouldBlock) => self.shared.abandoned.store(true, Ordering::SeqCst),
        }
    }
}

/// A reader exported as an Arrow C stream.
struct Exported(Batches);

impl Iterator for Exported {
    type Item = Result<RecordBatch, ArrowError>;

    fn next(&mut self) -> Option<Self::Item> {
        let shared = Arc::clone(&self.0.shared);
        let next = self.0.next()?;
        Some(next.map_err(|error| {
            ArrowError::ExternalError(Box::new(Failure::from_error(
                Action::Read(&shared.subject),
                &error,
            )))
        }))
    }
}

impl RecordBatchReader for Exported {
    fn schema(&self) -> SchemaRef {
        self.0.schema()
    }
}
