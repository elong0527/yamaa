//! Streamed runs (#3697): a specification run whose output its consumer
//! reads as a stream of Arrow batches, the consumer's pull driving the run.
//!
//! [`SpecRequest::stream`](crate::SpecRequest::stream) compiles a
//! specification, or reuses the plan its request's [`Plans`](crate::Plans)
//! holder kept, and returns a [`Streamed`] run before reading any study data
//! or calling any project function. A [`Batches`] reader of its output
//! produces it as it is read:
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
//!   (D46), or the release of the reader that started the stream before it
//!   handed out the last batch ends production: no further batch is built or
//!   kept, and the run is cancelled, with no result and no issue, so nothing
//!   of it can be published. The reader's pull, and [`Streamed::run`], fail
//!   with a [`FailureKind::Cancelled`] failure. A reader released after the
//!   last batch, before its end-of-stream pull, leaves the run its result.
//! - **Fail.** A run that fails ends the stream with its issues: the pull
//!   fails with a [`FailureKind::Rejected`] failure, and the run, which
//!   [`Streamed::run`] then gives, holds those issues and no output; its save
//!   writes only its report file (OUTPUT-10). An accepted output's batch that
//!   cannot be built ends the stream with that error, and the run then has
//!   no result, so nothing of it is published.
//!
//! A panic while a C stream pulls ends that stream with an error instead of
//! unwinding into its C caller.
//!
//! Every refusal of a streamed run is a [`Failure`] of reading it: its
//! message begins `cannot read <subject>`, with [`Streamed::subject`] as the
//! subject, as D39's error text of every other read does.
//!
//! A run whose output nobody has started to read runs whole when its result
//! is asked for: [`Streamed::run`] builds it as
//! [`SpecRequest::run`](crate::SpecRequest::run) does, and a reader then
//! reads the output table that run keeps. Once a reader has started the
//! stream, the run's result is known when that stream ends.
//!
//! When the run can call its host's project functions (an engine with
//! [`ProjectFunctions`](crate::ProjectFunctions) runs a specification with a
//! project), only the thread that started it builds it: a pull or a
//! [`Streamed::run`] that would build it on another thread fails with
//! `host_thread` instead of calling the host (the engine boundary design of
//! #2870, section 3.6). Batches of a built run are read on any thread.

use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{fence, AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread::ThreadId;

use arrow_array::ffi_stream::FFI_ArrowArrayStream;
use arrow_array::{RecordBatch, RecordBatchReader};
use arrow_schema::{ArrowError, Schema, SchemaRef};

use crate::api::{Run, Status};
use crate::cancel::CancellationToken;
use crate::engine::Engine;
use crate::error::{Failure, FailureKind, TableError};
use crate::internal::{EngineShared, PublicBatches};
use crate::plan::Plan;
use crate::progress::{Measured, Reporter};
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
    /// The run has ended: it has a result, as its stream ended or it ran
    /// whole, or, when an accepted output's batch could not be built, it has
    /// none and [`Streamed::run`] gives that error.
    Ended,
    /// The run was cancelled and has no result.
    Cancelled,
}

/// The progresses in the order of their codes, which the run's atomic holds.
const PROGRESSES: [StreamProgress; 4] = [
    StreamProgress::Unread,
    StreamProgress::Reading,
    StreamProgress::Ended,
    StreamProgress::Cancelled,
];

/// The code of the progress `state` stands at.
fn progress_code(state: &State) -> u8 {
    let progress = match state {
        State::Unread(_) => StreamProgress::Unread,
        State::Taken(_) | State::Producing { .. } => StreamProgress::Reading,
        State::Ended { .. } | State::Failed(_) => StreamProgress::Ended,
        State::Cancelled => StreamProgress::Cancelled,
    };
    PROGRESSES
        .iter()
        .position(|known| *known == progress)
        .map_or(u8::MAX, |position| position as u8)
}

/// What a run needs to be built.
struct Pending {
    inputs: crate::internal::RunInputs,
    shared: Arc<EngineShared>,
    plan: Plan,
    /// The measurements its compilation started, which its build continues.
    measured: Measured,
    reporter: Arc<Reporter>,
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
        let observed = self.reporter.resume(self.measured.clone());
        let (mut build, batches) = match rows {
            Some(rows) => {
                self.plan
                    .stream_through(&self.shared, &observed, token, rows, self.inputs)
            }
            None => (
                self.plan
                    .run_through(&self.shared, self.inputs, &observed, token),
                None,
            ),
        };
        if build.interrupted() || token.is_cancelled() {
            return None;
        }
        self.measured.measurements().complete(build.accepted());
        // A failed progress callback fails the run, which keeps no output.
        if self.reporter.failed() {
            build.fail_progress(&self.reporter, yamaa_core::application_issue::Step::Build);
            return Some((
                Run {
                    build: build.with_subject(subject),
                },
                None,
            ));
        }
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
    /// An accepted output's batch could not be built: the stream ended with
    /// this failure, and the run has no result, so nothing of it is
    /// published.
    Failed(Failure),
    Cancelled,
}

impl State {
    /// A reader that started the stream was released before its end-of-
    /// stream pull. A stream that handed out its last batch has ended, and
    /// its run keeps its result, unless the progress callback failed, whose
    /// `failure` then ends it (#3762); any other is cancelled.
    fn abandon(&mut self, failure: Option<Failure>) {
        match std::mem::replace(self, Self::Cancelled) {
            Self::Producing { .. } if failure.is_some() => {
                *self = Self::Failed(failure.expect("a failure"));
            }
            Self::Producing { run, batches } if batches.ended() => {
                *self = Self::Ended {
                    run,
                    streamed: true,
                };
            }
            Self::Taken(_) | Self::Producing { .. } => {}
            other => *self = other,
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
    /// What the run's progress events name: the path as given.
    name: String,
    /// The run's progress, which each batch handed out reports (#3762).
    reporter: Arc<Reporter>,
    /// A reader released while another call held the state, which the
    /// holder applies as its hold ends, or the next hold as it starts.
    abandoned: AtomicBool,
    /// Where the run stands, as the state was when last released, read
    /// without the lock.
    progress: AtomicU8,
    state: Mutex<State>,
}

/// The state, held: releasing it applies a reader's release recorded
/// meanwhile, then publishes where the run stands.
struct Held<'a> {
    /// The lock, which only the end of the hold takes.
    state: Option<MutexGuard<'a, State>>,
    shared: &'a Shared,
}

impl Held<'_> {
    /// Publish where the run stands now, before the hold ends.
    fn publish(&self) {
        self.shared.publish(self);
    }
}

impl Deref for Held<'_> {
    type Target = State;

    fn deref(&self) -> &State {
        self.state.as_deref().expect("a hold holds the state")
    }
}

impl DerefMut for Held<'_> {
    fn deref_mut(&mut self) -> &mut State {
        self.state.as_deref_mut().expect("a hold holds the state")
    }
}

impl Drop for Held<'_> {
    /// A reader released while this hold lasted left its release to it
    /// ([`Batches::drop`]): apply it, publish, and release the lock. A
    /// release recorded after that check is applied once the lock is free,
    /// by holding the state again, unless another call holds it, whose own
    /// hold then applies it, so no release is left pending.
    fn drop(&mut self) {
        let mut state = self.state.take();
        while let Some(mut held) = state {
            if self.shared.abandoned.swap(false, Ordering::SeqCst) {
                held.abandon(self.shared.progress_failure());
            }
            self.shared.publish(&held);
            drop(held);
            // Pairs with the fence of `Batches::drop`: either this load sees
            // its record, or its attempt to hold sees the lock released.
            fence(Ordering::SeqCst);
            state = if self.shared.abandoned.load(Ordering::SeqCst) {
                self.shared.guard()
            } else {
                None
            };
        }
    }
}

impl Shared {
    /// The refusal of reading the run for a failure of `kind`.
    fn refusal(&self, kind: FailureKind) -> Failure {
        Failure::of(Action::Read(&self.subject), kind)
    }

    /// The refusal of reading the run that the failed `run`'s issues
    /// explain, which ends its stream.
    fn failure(&self, run: &Run) -> Failure {
        Failure::rejected(Action::Read(&self.subject), run.build.report())
            .with_cause(self.reporter.cause())
    }

    /// The refusal that ends the stream once the progress callback failed
    /// (#3762), or none.
    fn progress_failure(&self) -> Option<Failure> {
        self.reporter.failed().then(|| {
            Failure::from_issues(
                Action::Read(&self.subject),
                &[self
                    .reporter
                    .failure(yamaa_core::application_issue::Step::Build)],
            )
            .with_cause(self.reporter.cause())
        })
    }

    /// The state, unless another call holds it.
    fn lock(&self) -> Result<Held<'_>, Failure> {
        let mut state = match self.try_hold() {
            Some(state) => state,
            None => return Err(self.refusal(FailureKind::Stream { code: "busy" })),
        };
        if self.abandoned.swap(false, Ordering::SeqCst) {
            state.abandon(self.progress_failure());
        }
        Ok(state)
    }

    /// The state, unless another call holds it.
    fn try_hold(&self) -> Option<Held<'_>> {
        Some(Held {
            state: Some(self.guard()?),
            shared: self,
        })
    }

    /// The lock of the state, unless another call holds it; a panic that
    /// left it poisoned left it consistent, so it is taken all the same.
    fn guard(&self) -> Option<MutexGuard<'_, State>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// Publish where the run stands as `state`, which is held, stands.
    fn publish(&self, state: &State) {
        self.progress.store(progress_code(state), Ordering::SeqCst);
    }

    /// Refuse to build a run that can call the host off the thread that
    /// started it.
    fn on_owner(&self) -> Result<(), Failure> {
        if self.host && std::thread::current().id() != self.owner {
            return Err(self.refusal(FailureKind::Stream {
                code: "host_thread",
            }));
        }
        Ok(())
    }
}

/// One specification run whose output its consumer reads as a stream; see
/// the [module](self) documentation. It is `Send` and `Sync`, and every
/// method refuses with the `busy` code of [`FailureKind::Stream`] while
/// another call, such as a project function called by its own build, uses
/// it.
pub struct Streamed {
    shared: Arc<Shared>,
}

impl Streamed {
    /// A streamed run of `plan` through `engine`, polling `token`, whose
    /// compilation `measured` measured and which reports to `reporter`. A
    /// refused compilation's run has already ended: it holds its refusal and
    /// reads nothing.
    pub(crate) fn new(
        engine: &Engine,
        plan: Plan,
        specification: &str,
        (measured, reporter): (Measured, Arc<Reporter>),
        token: &CancellationToken,
        frames: std::collections::BTreeMap<String, crate::InputFrame>,
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
                inputs: crate::internal::RunInputs {
                    held: None,
                    frames,
                    threads: None,
                },
                shared,
                plan,
                measured,
                reporter: Arc::clone(&reporter),
            }))
        } else {
            let mut build = plan.run_through(
                &shared,
                Default::default(),
                &reporter.resume(measured.clone()),
                token,
            );
            measured.measurements().complete(false);
            if reporter.failed() {
                build.fail_progress(&reporter, yamaa_core::application_issue::Step::Build);
            }
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
                name: specification.to_owned(),
                reporter,
                abandoned: AtomicBool::new(false),
                progress: AtomicU8::new(progress_code(&state)),
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

    /// Where the run stands, read without waiting for, or getting in the way
    /// of, a call that is building it or a batch of it.
    pub fn progress(&self) -> StreamProgress {
        PROGRESSES[usize::from(self.shared.progress.load(Ordering::SeqCst))]
    }

    /// A reader of the output, which reads nothing until its first pull.
    ///
    /// That pull starts the run's stream when no reader has, and builds the
    /// run; once the run ran whole, it reads the output table the run keeps,
    /// with a cursor of its own. Otherwise it fails: with the run's issues
    /// for a failed run, as [`FailureKind::Cancelled`] for a cancelled one,
    /// and with the `stream_taken` code of [`FailureKind::Stream`] when
    /// another reader started the stream. A reader that would build a run
    /// that can call its host off the thread that started it fails with
    /// `host_thread` and starts nothing.
    pub fn reader(&self) -> Batches {
        Batches {
            shared: Arc::clone(&self.shared),
            mode: Mode::Unstarted,
            handed: 0,
        }
    }

    /// The run's result: its output (missing when it failed, or when its
    /// output was read as a stream), issues, run report, summary and save.
    ///
    /// A run that nobody has started to read runs whole now, as
    /// [`SpecRequest::run`](crate::SpecRequest::run) runs it, and keeps its
    /// output table. While a reader's stream has not ended, this fails with
    /// the `stream_open` code of [`FailureKind::Stream`]; a cancelled run
    /// fails as [`FailureKind::Cancelled`], and one whose accepted output's
    /// batch could not be built with that failure; and a run that can call
    /// its host is built only on the thread that started it, failing with
    /// `host_thread` on another. A cancelled run stays cancelled, so its
    /// refusal needs no lock: it answers as cancelled even while another call
    /// holds the run, where any other refusal would be `busy`.
    pub fn run(&self) -> Result<RunGuard<'_>, Failure> {
        if self.progress() == StreamProgress::Cancelled {
            return Err(self.shared.refusal(FailureKind::Cancelled));
        }
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
            State::Failed(error) => Err(error.clone()),
            State::Cancelled => Err(self.shared.refusal(FailureKind::Cancelled)),
            State::Unread(_) | State::Taken(_) | State::Producing { .. } => {
                Err(self.shared.refusal(FailureKind::Stream {
                    code: "stream_open",
                }))
            }
        }
    }

    /// The output read whole: the table the run keeps once it ran whole,
    /// which it does now when nobody has started to read its output. A
    /// failed run refuses with its issues ([`FailureKind::Rejected`]), a run
    /// whose output was read as a stream with the `stream_taken` code of
    /// [`FailureKind::Stream`], and otherwise this refuses as
    /// [`Streamed::run`] does.
    pub fn table(&self) -> Result<Table, Failure> {
        let run = self.run()?;
        match run.output() {
            Some(table) => Ok(table.clone()),
            None if run.status() == Status::Failed => Err(self.shared.failure(&run)),
            None => Err(self.shared.refusal(FailureKind::Stream {
                code: "stream_taken",
            })),
        }
    }
}

/// The result of a [`Streamed`] run, borrowed: it dereferences to the
/// [`Run`]. While it lives, every other call on the streamed run refuses
/// with the `busy` code of [`FailureKind::Stream`].
pub struct RunGuard<'a> {
    state: Held<'a>,
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
/// Dropping the reader that started the stream before it handed out the
/// last batch cancels the run.
pub struct Batches {
    shared: Arc<Shared>,
    mode: Mode,
    /// The batches this reader's stream has handed out.
    handed: usize,
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
        self.into_stream_with_lifetime(())
    }

    /// Retain an external owner until the foreign consumer releases its reader.
    /// Host bindings use this for GC-visible callbacks and original causes.
    pub(crate) fn into_stream_with_lifetime(
        self,
        lifetime: impl Send + 'static,
    ) -> FFI_ArrowArrayStream {
        FFI_ArrowArrayStream::new(Box::new(Exported {
            batches: self,
            _lifetime: lifetime,
        }))
    }

    /// One pull that starts the stream, or continues the one this reader
    /// started.
    fn pull(&mut self) -> Option<Result<RecordBatch, Failure>> {
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
                    // Readers see the stream started while its run builds.
                    state.publish();
                    self.mode = Mode::Producing;
                }
                State::Ended {
                    run,
                    streamed: false,
                } => {
                    let Some(table) = run.output() else {
                        self.mode = Mode::Done;
                        return Some(Err(shared.failure(run)));
                    };
                    // The reader owns its cursor over the kept table's batches.
                    let owned: Vec<RecordBatch> = table.batches().to_vec();
                    let mut batches = owned.into_iter();
                    let first = batches.next();
                    self.mode = Mode::Table(batches);
                    return first.map(Ok);
                }
                State::Cancelled => {
                    self.mode = Mode::Done;
                    return Some(Err(shared.refusal(FailureKind::Cancelled)));
                }
                State::Failed(failure) => {
                    self.mode = Mode::Done;
                    return Some(Err(failure.clone()));
                }
                State::Taken(_) | State::Producing { .. } | State::Ended { .. } => {
                    self.mode = Mode::Done;
                    return Some(Err(shared.refusal(FailureKind::Stream {
                        code: "stream_taken",
                    })));
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
                    return Some(Err(shared.refusal(FailureKind::Cancelled)));
                }
                Some((run, Some(batches))) => *state = State::Producing { run, batches },
                Some((run, None)) => {
                    self.mode = Mode::Done;
                    // An accepted output whose batches cannot be built has no
                    // result to save.
                    if run.build.accepted() {
                        let failure = shared.refusal(FailureKind::Internal {
                            code: "stream_batches",
                        });
                        *state = State::Failed(failure.clone());
                        return Some(Err(failure));
                    }
                    let failure = shared.failure(&run);
                    *state = State::Ended {
                        run,
                        streamed: true,
                    };
                    return Some(Err(failure));
                }
            }
        }
        let State::Producing { run, mut batches } =
            std::mem::replace(&mut *state, State::Cancelled)
        else {
            // Only this reader's own release cancels the stream it started.
            self.mode = Mode::Done;
            return Some(Err(shared.refusal(FailureKind::Cancelled)));
        };
        // A progress callback that failed at an earlier batch ends the stream,
        // and the run, whose output was never all handed out, has no result.
        if let Some(failure) = shared.progress_failure() {
            self.mode = Mode::Done;
            drop(run);
            *state = State::Failed(failure.clone());
            return Some(Err(failure));
        }
        // Before each batch, the token; the state stays cancelled.
        if !batches.ended() && shared.token.is_cancelled() {
            self.mode = Mode::Done;
            return Some(Err(shared.refusal(FailureKind::Cancelled)));
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
            // A batch that cannot be built ends the stream, and the run,
            // whose output was never all handed out, has no result to save.
            Some(Err(error)) => {
                self.mode = Mode::Done;
                drop(run);
                let failure =
                    shared.refusal(FailureKind::Table(Box::new(TableError::from_public(error))));
                *state = State::Failed(failure.clone());
                Some(Err(failure))
            }
        }
    }
}

impl Iterator for Batches {
    type Item = Result<RecordBatch, Failure>;

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
            Mode::Unstarted | Mode::Producing => {
                let next = self.pull();
                // The batch is reported once the run's state is released.
                if let (Some(Ok(batch)), Mode::Producing) = (&next, &self.mode) {
                    self.handed += 1;
                    let shared = &self.shared;
                    shared
                        .reporter
                        .batch(&shared.name, self.handed, batch.num_rows());
                }
                next
            }
        }
    }
}

impl Drop for Batches {
    /// Releasing the reader that started the stream before its last batch
    /// cancels the run, and after it leaves the run its result; a reader that
    /// started nothing changes nothing.
    fn drop(&mut self) {
        if !matches!(self.mode, Mode::Producing) {
            return;
        }
        // Recorded first, then applied as the hold ends: by this hold, or,
        // when another call holds the state, by that call's (`Held::drop`).
        self.shared.abandoned.store(true, Ordering::SeqCst);
        fence(Ordering::SeqCst);
        drop(self.shared.try_hold());
    }
}

/// A reader exported as an Arrow C stream.
struct Exported<T> {
    batches: Batches,
    _lifetime: T,
}

impl<T> Iterator for Exported<T> {
    type Item = Result<RecordBatch, ArrowError>;

    /// One pull from the Arrow crate's `extern "C"` `get_next`. A panic
    /// while pulling ends the stream with the `stream_panic` code of
    /// [`FailureKind::Internal`] instead of unwinding into the C caller
    /// (#2870, section 3.4); the run, which the pull left without a result,
    /// is then cancelled.
    fn next(&mut self) -> Option<Self::Item> {
        let shared = Arc::clone(&self.batches.shared);
        let next = match catch_unwind(AssertUnwindSafe(|| self.batches.next())) {
            Ok(next) => next?,
            Err(_) => {
                self.batches.mode = Mode::Done;
                Err(shared.refusal(FailureKind::Internal {
                    code: "stream_panic",
                }))
            }
        };
        Some(next.map_err(|failure| ArrowError::ExternalError(Box::new(failure))))
    }
}

impl<T> RecordBatchReader for Exported<T> {
    fn schema(&self) -> SchemaRef {
        self.batches.schema()
    }
}

// A streamed run and its reader show no state in a `{:?}` (#5080): reading
// it would lock the run another thread may be building.
impl std::fmt::Debug for Streamed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Streamed").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Batches {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Batches")
            .field("handed", &self.handed)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for RunGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RunGuard").field(&**self).finish()
    }
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use crate::{EngineOptions, HostLanguage, MemoryResolver, Partitions};

    /// Three records, so one-record partitions hand them out as three
    /// batches.
    const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n";

    /// The seam no public call reaches: a reader released while another
    /// call holds the state, which every public call holds only between
    /// its own checks. The release is left to that call, whose hold ends
    /// by applying it: the stream, released before its last batch, is
    /// cancelled, and stands cancelled once the hold ends (the module's
    /// "Cancel" end state), not reading.
    #[test]
    fn a_reader_released_while_another_call_holds_the_run_cancels_it_as_the_hold_ends() {
        let resolver = Arc::new(MemoryResolver::new());
        resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPEC.as_bytes()));
        resolver.insert(
            "/spec.yaml",
            "input.csv",
            "input:1",
            Arc::from(&b"ID\n1\n2\n3\n"[..]),
        );
        let engine = Engine::new(
            EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
                .with_resolver(resolver)
                .with_partitions(Partitions::new(NonZeroUsize::MIN)),
        );
        let streamed = engine.spec("spec.yaml").stream().unwrap();
        let mut reader = streamed.reader();
        let first = reader.next().unwrap().unwrap().num_rows();
        let held = streamed.shared.lock().unwrap();
        drop(reader);
        let recorded = streamed.progress();
        drop(held);
        let state = streamed.shared.state.lock().unwrap();
        assert_eq!(
            (
                first,
                recorded,
                streamed.progress(),
                matches!(*state, State::Cancelled),
                streamed.shared.abandoned.load(Ordering::SeqCst),
            ),
            (
                1,
                StreamProgress::Reading,
                StreamProgress::Cancelled,
                true,
                false
            ),
            "a reader released during another call's hold"
        );
    }
}
