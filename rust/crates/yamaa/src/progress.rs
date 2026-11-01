//! The progress port and per-phase timings (#3762): what an operation tells
//! its caller while it runs, and how long each of its phases took.
//!
//! An engine configured with
//! [`EngineOptions::with_progress`](crate::EngineOptions::with_progress)
//! calls its callback with one [`ProgressEvent`] as each phase of a check, a
//! run or a submission starts and as it ends, with its outcome, in the order
//! the phases run. A partitioned run also reports each partition as it
//! enters it, and a streamed run each batch as its reader hands it out.
//! The callback runs on the thread that runs the operation, or, for a batch,
//! that pulls it, and never while the engine holds a lock of its own, such
//! as a [`Plans`](crate::Plans) holder's, so it may start another operation,
//! on the same engine and holder too. While a streamed run builds, its own
//! [`Streamed`](crate::Streamed) answers `busy`, as it does to the run's
//! project functions.
//!
//! A callback that panics or returns an error is contained, as a project
//! function's failure is: the engine calls it no more in that operation,
//! finishes the work in hand, and then fails the result with one issue,
//! `engine_rejected` with the code `host_progress_failure`, at the step of
//! the operation, with `host_error` and `host_message` naming the failure.
//! The result retains the original host exception for a refused save to chain.
//! The step is `build` for a run, `check` for a check, `define` for a
//! submission. A failed run keeps no output and saves nothing, a project
//! run builds no later specification, a submission composes no later
//! section, and either refuses its save, writing nothing.
//!
//! Every [`Check`](crate::Check), [`Run`](crate::Run),
//! [`ProjectRun`](crate::ProjectRun) and [`Submission`](crate::Submission)
//! carries its [`Timings`]: one [`Timing`] for each phase it ran, in the
//! order the phases started, with or without a callback. Timings are
//! wall-clock facts of one attempt, so they live on the result alone: no
//! saved run report, dataset, Dataset-JSON or Define-XML holds them (P10).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use yamaa_application::application_measurements::Measurements;
use yamaa_core::application_issue::Step;
use yamaa_core::application_observer::{Event, Observer, Outcome, Phase};
use yamaa_core::dataset::ExecutionPhase;
use yamaa_core::diagnostic::Diagnostic;

use crate::api::Status;
use crate::host::functions::{FunctionError, HostPayload};
use crate::table::Table;

/// A phase of an operation, as a progress event names it and a result's
/// timings list it. A run passes the phases from `preparation` to `output`
/// in that order, `activation` only when its specification calls a project
/// function; a check is one `preparation` of each specification it checks,
/// and a submission one `composition` of each section it composes.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressPhase {
    /// Reading and admitting the metadata, and compiling the specification.
    Preparation,
    /// Verifying the lock and binding the project functions a specification
    /// calls.
    Activation,
    /// Reading and decoding the inputs.
    Input,
    /// Admitting the inputs within the limits, before any record is derived.
    Admission,
    /// Deriving the rows and columns, partition by partition when the run is
    /// partitioned.
    Derivation,
    /// Checking that the output keys are complete and unique.
    OutputKeys,
    /// Running the dataset checks.
    Verification,
    /// Converting the accepted dataset into its output and run report.
    Output,
    /// Composing one submission section's documents.
    Composition,
}

/// The phases as written, one for each [`ProgressPhase`] in declaration
/// order: the closed vocabulary of every `phase` a progress event or a
/// timings table holds.
pub const PROGRESS_PHASES: [&str; 9] = [
    "preparation",
    "activation",
    "input",
    "admission",
    "derivation",
    "output_keys",
    "verification",
    "output",
    "composition",
];

impl ProgressPhase {
    /// The phase as written: one of [`PROGRESS_PHASES`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preparation => "preparation",
            Self::Activation => "activation",
            Self::Input => "input",
            Self::Admission => "admission",
            Self::Derivation => "derivation",
            Self::OutputKeys => "output_keys",
            Self::Verification => "verification",
            Self::Output => "output",
            Self::Composition => "composition",
        }
    }

    /// The public phase of an observed one. A build's own span, which holds
    /// every phase after preparation, and the marks of its end are no phase
    /// a caller sees.
    fn of(phase: Phase) -> Option<Self> {
        Some(match phase {
            Phase::Preparation => Self::Preparation,
            Phase::Activation => Self::Activation,
            Phase::Input => Self::Input,
            Phase::Dataset(ExecutionPhase::Admission) => Self::Admission,
            Phase::Dataset(ExecutionPhase::Derivation) => Self::Derivation,
            Phase::Dataset(ExecutionPhase::OutputKeys) => Self::OutputKeys,
            Phase::Dataset(ExecutionPhase::Verification) => Self::Verification,
            Phase::Output => Self::Output,
            Phase::Composition => Self::Composition,
            Phase::Build | Phase::Dataset(ExecutionPhase::Finished) | Phase::Finished => {
                return None
            }
        })
    }
}

/// What a progress event marks.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressKind {
    /// A phase starts.
    Start,
    /// A phase ends, with its outcome.
    End,
    /// A partitioned run enters one partition of its driver input, in its
    /// derivation.
    Partition,
    /// A streamed run's reader hands out one batch of its output, after its
    /// output phase.
    Batch,
}

/// The kinds as written, one for each [`ProgressKind`] in declaration
/// order: the closed vocabulary of a progress event's `kind`.
pub const PROGRESS_KINDS: [&str; 4] = ["start", "end", "partition", "batch"];

impl ProgressKind {
    /// The kind as written: one of [`PROGRESS_KINDS`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Partition => "partition",
            Self::Batch => "batch",
        }
    }
}

/// One event a progress callback receives.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressEvent {
    /// What the work is named by: a project run member's listed name, or
    /// the path the project file lists for one whose listing could not be
    /// read; a submission's section; otherwise the specification's path as
    /// the call was given it.
    pub name: String,
    pub phase: ProgressPhase,
    pub kind: ProgressKind,
    /// An end's outcome, `succeeded` or `failed`; none for any other kind.
    pub outcome: Option<Status>,
    /// The partition's or the batch's number, from 1.
    pub number: Option<usize>,
    /// The driver records the partition holds, or the rows of the batch.
    pub rows: Option<usize>,
}

/// The callback an engine's operations report their progress to. An error it
/// returns, such as the exception a host's callback raised, fails the
/// operation's result, as a panic in it does.
pub type ProgressCallback = Arc<dyn Fn(&ProgressEvent) -> Result<(), FunctionError> + Send + Sync>;

/// One phase a result ran, a row of its [`Timings`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    /// What the work is named by, as [`ProgressEvent::name`] names it.
    pub name: String,
    pub phase: ProgressPhase,
    /// When the phase started, from the start of the operation.
    pub started: Duration,
    /// How long the phase took; none for a phase that never ended, as when
    /// the engine contained a panic in it.
    pub elapsed: Option<Duration>,
    /// `succeeded` or `failed`; none for a phase that never ended.
    pub outcome: Option<Status>,
}

/// The columns of a timings table, in order.
pub const TIMING_COLUMNS: [&str; 5] = ["name", "phase", "started_ms", "elapsed_ms", "outcome"];

/// How one run divided its driver input (#3794), as its measured
/// observations name it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Division {
    /// What the run is named by, as [`ProgressEvent::name`] names it.
    pub name: String,
    /// `whole_table`, `key_bounded` or `consecutive`.
    pub strategy: &'static str,
    /// How many partitions the run derived, 1 for the whole table.
    pub count: usize,
}

/// The phases a result ran, in the order they started: a project run's
/// specifications in run order, a project check's in listed order and a
/// submission's sections in section order. Beside them, how each run among
/// them divided its driver input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timings {
    entries: Vec<Timing>,
    divisions: Vec<Division>,
}

/// Milliseconds, with the nanoseconds a duration holds.
fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

impl Timings {
    /// Every phase, in the order it started.
    pub fn entries(&self) -> &[Timing] {
        &self.entries
    }

    /// How each run divided its driver input, in run order: none for a
    /// check, a submission or a run that never reached its derivation.
    pub fn divisions(&self) -> &[Division] {
        &self.divisions
    }

    /// The timings table with the [`TIMING_COLUMNS`], one row per phase:
    /// `started_ms` and `elapsed_ms` are milliseconds, and `elapsed_ms` and
    /// `outcome` are missing for a phase that never ended.
    pub fn table(&self) -> Table {
        use yamaa_adapters::public_table::{PublicColumn, PublicTable};

        let text = |cell: fn(&Timing) -> Option<&str>| {
            PublicColumn::Str(self.entries.iter().map(cell).collect())
        };
        let number = |cell: fn(&Timing) -> Option<f64>| {
            PublicColumn::Float(self.entries.iter().map(cell).collect())
        };
        let columns: [PublicColumn; TIMING_COLUMNS.len()] = [
            text(|entry| Some(&entry.name)),
            text(|entry| Some(entry.phase.as_str())),
            number(|entry| Some(milliseconds(entry.started))),
            number(|entry| entry.elapsed.map(milliseconds)),
            text(|entry| entry.outcome.map(Status::as_str)),
        ];
        let columns = TIMING_COLUMNS
            .into_iter()
            .zip(columns)
            .map(|(name, column)| (name.to_owned(), column))
            .collect();
        Table::from_public(PublicTable::new(columns).expect("the timing columns form one table"))
    }

    /// The table as the command line prints it: a line of the
    /// [`TIMING_COLUMNS`], then one line per phase, each cell separated by a
    /// tab, milliseconds with three decimals and a missing cell empty.
    pub fn lines(&self) -> Vec<String> {
        let shown = |duration: Option<Duration>| {
            duration.map_or_else(String::new, |duration| {
                format!("{:.3}", milliseconds(duration))
            })
        };
        std::iter::once(TIMING_COLUMNS.join("\t"))
            .chain(self.entries.iter().map(|entry| {
                [
                    entry.name.clone(),
                    entry.phase.as_str().to_owned(),
                    shown(Some(entry.started)),
                    shown(entry.elapsed),
                    entry.outcome.map_or("", Status::as_str).to_owned(),
                ]
                .join("\t")
            }))
            .collect()
    }

    /// The timings of every part, in order.
    pub(crate) fn joined(parts: impl IntoIterator<Item = Timings>) -> Self {
        let mut joined = Self::default();
        for part in parts {
            joined.entries.extend(part.entries);
            joined.divisions.extend(part.divisions);
        }
        joined
    }
}

/// The status a phase ended with.
fn status(outcome: Outcome) -> Status {
    match outcome {
        Outcome::Succeeded => Status::Succeeded,
        Outcome::Failed => Status::Failed,
    }
}

/// What one member's timings are read from: its name and the measurements
/// its work took, which own the clock (#3686).
#[derive(Clone)]
pub(crate) struct Measured {
    name: Arc<str>,
    measurements: Arc<Measurements>,
}

impl Measured {
    /// The measurements of work named `name` that no operation started,
    /// counting from now.
    pub(crate) fn unobserved(name: &str) -> Self {
        Self {
            name: Arc::from(name),
            measurements: Arc::new(Measurements::new()),
        }
    }

    pub(crate) fn measurements(&self) -> &Arc<Measurements> {
        &self.measurements
    }

    /// The phases the work ran, in the order they started.
    pub(crate) fn timings(&self) -> Timings {
        Timings {
            entries: self
                .measurements
                .stages()
                .into_iter()
                .filter_map(|stage| {
                    Some(Timing {
                        name: self.name.to_string(),
                        phase: ProgressPhase::of(stage.phase)?,
                        started: stage.entered,
                        elapsed: stage
                            .completed
                            .map(|completed| completed.saturating_sub(stage.entered)),
                        outcome: stage.outcome.map(status),
                    })
                })
                .collect(),
            divisions: self
                .measurements
                .division()
                .map(|(strategy, count)| Division {
                    name: self.name.to_string(),
                    strategy,
                    count,
                })
                .into_iter()
                .collect(),
        }
    }
}

/// One operation's progress: the engine's callback, the moment the operation
/// started, from which its timings count, and whether the callback failed.
pub(crate) struct Reporter {
    callback: Option<ProgressCallback>,
    origin: Instant,
    failure: Mutex<Option<CallbackFailure>>,
}

#[derive(Clone)]
struct CallbackFailure {
    class: String,
    message: String,
    payload: Option<HostPayload>,
}

impl From<FunctionError> for CallbackFailure {
    fn from(error: FunctionError) -> Self {
        let (class, message) = match &error {
            FunctionError::Exception { class, message, .. } => (class.clone(), message.clone()),
            _ => (error.code().to_owned(), error.message()),
        };
        let payload = match error {
            FunctionError::Exception { payload, .. }
            | FunctionError::Representation { payload, .. }
            | FunctionError::Interrupted { payload }
            | FunctionError::SdkBoundary { payload } => payload,
            _ => None,
        };
        Self {
            class,
            message,
            payload,
        }
    }
}

impl Reporter {
    /// The progress of an operation starting now, reported to `callback`.
    pub(crate) fn new(callback: Option<ProgressCallback>) -> Self {
        Self {
            callback,
            origin: Instant::now(),
            failure: Mutex::new(None),
        }
    }

    /// The observer of the work named `name`, with measurements of its own
    /// that count from the start of the operation.
    pub(crate) fn observe<'a>(&'a self, name: &str) -> Observed<'a> {
        Observed {
            reporter: self,
            measured: Measured {
                name: Arc::from(name),
                measurements: Arc::new(Measurements::starting_at(self.origin)),
            },
        }
    }

    /// The observer of work `measured` already holds, such as a streamed
    /// run's build after its compilation, or a project run member that
    /// compiled on another thread and runs on this one (#4193).
    pub(crate) fn resume(&self, measured: Measured) -> Observed<'_> {
        Observed {
            reporter: self,
            measured,
        }
    }

    /// A reporter for work on another thread (#4193): it counts from the
    /// start of this one's operation, and keeps, in order, the events this
    /// one's callback would hear, which the calling thread then hands to
    /// [`Reporter::replay`]. Without a callback it keeps none.
    pub(crate) fn deferred(&self) -> (Reporter, Heard) {
        let heard = Heard::default();
        let kept = Arc::clone(&heard.0);
        let callback = self.callback.as_ref().map(|_| {
            Arc::new(move |event: &ProgressEvent| {
                kept.lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(event.clone());
                Ok(())
            }) as ProgressCallback
        });
        let reporter = Reporter {
            callback,
            origin: self.origin,
            failure: Mutex::new(None),
        };
        (reporter, heard)
    }

    /// Report the events another thread's [`Reporter::deferred`] kept, in
    /// order, as this reporter reports its own: none once its callback has
    /// failed.
    pub(crate) fn replay(&self, events: Vec<ProgressEvent>) {
        for event in events {
            self.send(move || event);
        }
    }

    /// Whether the callback panicked or returned an error.
    pub(crate) fn failed(&self) -> bool {
        self.failure
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }

    /// The issue that fails the result of an operation whose callback
    /// failed, at the operation's `step`.
    pub(crate) fn failure(&self, step: Step) -> Diagnostic {
        let failure = self
            .failure
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let failure = failure.as_ref().expect("a failed callback has its failure");
        yamaa_core::application_issue::progress_failure(step, &failure.class, &failure.message)
    }

    pub(crate) fn cause(&self) -> Option<HostPayload> {
        self.failure
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .and_then(|failure| failure.payload.clone())
    }

    /// Report one event, unless there is no callback or it has failed; a
    /// panic in it is contained.
    fn send(&self, event: impl FnOnce() -> ProgressEvent) {
        let Some(callback) = &self.callback else {
            return;
        };
        if self.failed() {
            return;
        }
        let event = event();
        let delivered = catch_unwind(AssertUnwindSafe(|| callback(&event)));
        let failure = match delivered {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error.into(),
            Err(panic) => CallbackFailure {
                class: "panic".into(),
                message: panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                    .unwrap_or_else(|| "the progress callback panicked".into()),
                payload: None,
            },
        };
        let mut held = self
            .failure
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if held.is_none() {
            *held = Some(failure);
        }
    }

    /// Report the batch `number` of `rows` rows that the reader of a
    /// streamed run named `name` hands out.
    #[cfg(feature = "arrow")]
    pub(crate) fn batch(&self, name: &str, number: usize, rows: usize) {
        self.send(|| ProgressEvent {
            name: name.to_owned(),
            phase: ProgressPhase::Output,
            kind: ProgressKind::Batch,
            outcome: None,
            number: Some(number),
            rows: Some(rows),
        });
    }
}

/// The events a [`Reporter::deferred`] kept.
#[derive(Default)]
pub(crate) struct Heard(Arc<Mutex<Vec<ProgressEvent>>>);

impl Heard {
    /// The events kept so far, in order, leaving none.
    pub(crate) fn take(&self) -> Vec<ProgressEvent> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|error| error.into_inner()))
    }
}

/// The observer of one member's work: its measurements take every event,
/// and the operation's callback hears of each public phase.
pub(crate) struct Observed<'a> {
    reporter: &'a Reporter,
    measured: Measured,
}

impl Observed<'_> {
    /// What the member's timings are read from.
    pub(crate) fn measured(&self) -> &Measured {
        &self.measured
    }

    /// The member's measurements.
    pub(crate) fn measurements(&self) -> &Arc<Measurements> {
        &self.measured.measurements
    }

    /// Run `work` as `phase`, which fails when `failed` says its result does.
    pub(crate) fn phase<T>(
        &self,
        phase: Phase,
        work: impl FnOnce() -> T,
        failed: impl FnOnce(&T) -> bool,
    ) -> T {
        self.observe(Event::Phase { phase });
        let done = work();
        let outcome = if failed(&done) {
            Outcome::Failed
        } else {
            Outcome::Succeeded
        };
        self.observe(Event::Exit { phase, outcome });
        done
    }
}

impl Observer for Observed<'_> {
    fn observe(&self, event: Event) {
        self.measured.measurements.observe(event);
        let (phase, kind, outcome, number, rows) = match event {
            Event::Phase { phase } => (phase, ProgressKind::Start, None, None, None),
            Event::Exit { phase, outcome } => {
                (phase, ProgressKind::End, Some(status(outcome)), None, None)
            }
            Event::Partition { number, records } => (
                Phase::Dataset(ExecutionPhase::Derivation),
                ProgressKind::Partition,
                None,
                Some(number),
                Some(records),
            ),
            Event::FunctionCall { .. }
            | Event::FunctionReturn { .. }
            | Event::Refusal { .. }
            | Event::Divided { .. } => return,
        };
        let Some(phase) = ProgressPhase::of(phase) else {
            return;
        };
        self.reporter.send(|| ProgressEvent {
            name: self.measured.name.to_string(),
            phase,
            kind,
            outcome,
            number,
            rows,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test of the vocabulary seam the facade constants own: each phase and
    /// kind is written as its constant lists it, in declaration order.
    #[test]
    fn phases_and_kinds_are_written_as_their_constants_list_them() {
        let phases = [
            ProgressPhase::Preparation,
            ProgressPhase::Activation,
            ProgressPhase::Input,
            ProgressPhase::Admission,
            ProgressPhase::Derivation,
            ProgressPhase::OutputKeys,
            ProgressPhase::Verification,
            ProgressPhase::Output,
            ProgressPhase::Composition,
        ];
        for phase in phases {
            // Exhaustive: a new phase fails to compile here until it is
            // listed above, and so in PROGRESS_PHASES.
            match phase {
                ProgressPhase::Preparation
                | ProgressPhase::Activation
                | ProgressPhase::Input
                | ProgressPhase::Admission
                | ProgressPhase::Derivation
                | ProgressPhase::OutputKeys
                | ProgressPhase::Verification
                | ProgressPhase::Output
                | ProgressPhase::Composition => {}
            }
        }
        assert_eq!(phases.map(ProgressPhase::as_str), PROGRESS_PHASES);
        let kinds = [
            ProgressKind::Start,
            ProgressKind::End,
            ProgressKind::Partition,
            ProgressKind::Batch,
        ];
        for kind in kinds {
            match kind {
                ProgressKind::Start
                | ProgressKind::End
                | ProgressKind::Partition
                | ProgressKind::Batch => {}
            }
        }
        assert_eq!(kinds.map(ProgressKind::as_str), PROGRESS_KINDS);
    }

    /// The seam between the observer port and the callback: a build's own
    /// span and its end marks reach the measurements alone, and a callback
    /// that fails hears nothing more.
    #[test]
    fn a_failed_callback_hears_nothing_more_and_a_build_span_is_no_phase() {
        let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
        let kept = Arc::clone(&heard);
        let reporter = Reporter::new(Some(Arc::new(move |event: &ProgressEvent| {
            kept.lock().unwrap().push((event.phase, event.kind));
            match event.kind {
                ProgressKind::End => Err(FunctionError::Protocol),
                _ => Ok(()),
            }
        })));
        let observed = reporter.observe("spec.yaml");
        for event in [
            Event::Phase {
                phase: Phase::Build,
            },
            Event::Phase {
                phase: Phase::Input,
            },
            Event::Exit {
                phase: Phase::Input,
                outcome: Outcome::Succeeded,
            },
            Event::Phase {
                phase: Phase::Output,
            },
            Event::Phase {
                phase: Phase::Finished,
            },
        ] {
            observed.observe(event);
        }
        assert_eq!(
            *heard.lock().unwrap(),
            [
                (ProgressPhase::Input, ProgressKind::Start),
                (ProgressPhase::Input, ProgressKind::End),
            ],
            "the events a callback hears before it fails"
        );
        assert!(reporter.failed(), "an error fails the callback");
        assert_eq!(
            observed
                .measured()
                .timings()
                .entries()
                .iter()
                .map(|timing| (timing.phase, timing.outcome))
                .collect::<Vec<_>>(),
            [
                (ProgressPhase::Input, Some(Status::Succeeded)),
                (ProgressPhase::Output, None),
            ],
            "the build span is measured but is no phase of the timings"
        );
    }
}
