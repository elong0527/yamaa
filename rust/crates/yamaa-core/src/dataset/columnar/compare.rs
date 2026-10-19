//! The test-only executor selector, compiled in by the `executor-comparison`
//! feature. The `YAMAA_EXECUTOR` environment variable selects `compare`, the
//! attempt comparison of the equivalence gate (Q7 of #2840), or `resolved`,
//! the executor over the resolved plan alone, which the executor probe times.
//! Unset, today's executor runs.
//!
//! Today's executor runs first with the real ports and returns its attempt
//! to the caller; every host call and observer event is recorded. Both
//! executors then run again over the same tables with a port that replays
//! the recorded results, so host code runs once, and with opaque failures
//! replaced by tokens naming where they arose. The two replayed attempts
//! must be equal: the result, handler counts in order, retained check
//! records and template rows; and the callbacks and observer events must
//! follow today's. A difference, or a plan that does not resolve, aborts
//! the process with the report on the standard error stream.
use super::super::execution::{ExecutionAttempt, ExecutionEvent};
use crate::dataset::DatasetPlan;
use crate::function_invocation::{Argument, FunctionPort, HostError};
use crate::function_signature::{InvocationPlan, ProjectInvocationPlan};
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::value::Value;

/// The executor this process runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selected {
    Rows,
    Compare,
    Resolved,
}

/// The executor `YAMAA_EXECUTOR` selects for this process.
pub(crate) fn selected() -> Selected {
    static SELECTED: std::sync::OnceLock<Selected> = std::sync::OnceLock::new();
    *SELECTED.get_or_init(|| match std::env::var("YAMAA_EXECUTOR").as_deref() {
        Ok("compare") => Selected::Compare,
        Ok("resolved") => Selected::Resolved,
        _ => Selected::Rows,
    })
}

/// A sized view of a table port.
struct Borrowed<'t, T: ?Sized>(&'t T);
impl<T: TableAccess + ?Sized> TableAccess for Borrowed<'_, T> {
    type Error = T::Error;
    fn schema(&self) -> &TableSchema {
        self.0.schema()
    }
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<T::Error>> {
        self.0.cell(row, column)
    }
}

/// Run the executor over the resolved plan alone.
pub(crate) fn resolved<T: TableAccess + ?Sized>(
    plan: &DatasetPlan,
    table: &T,
    secondary: &[&dyn TableAccess<Error = T::Error>],
    functions: &mut dyn FunctionPort<Error = T::Error>,
    observer: Option<&mut dyn FnMut(ExecutionEvent)>,
) -> ExecutionAttempt<T::Error> {
    let resolved = match plan.resolve() {
        Ok(resolved) => resolved,
        Err(error) => fail(&format!("the bound plan does not resolve: {error}")),
    };
    resolved.execute(&Borrowed(table), secondary, functions, observer)
}

/// An opaque failure, named by where it arose. Its fields are read only
/// through the attempts' renderings.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum Token {
    /// The host call at this position failed.
    Host(usize),
    /// A table port failed to lend this cell.
    Cell { row: usize, column: usize },
}

/// One recorded host call and whether it succeeded.
struct Call {
    slot: usize,
    arguments: Vec<(String, Value)>,
    outcome: Result<Value, bool>,
}

fn arguments(arguments: &[Argument<'_>]) -> Vec<(String, Value)> {
    arguments
        .iter()
        .map(|argument| (argument.name.to_owned(), super::buffer::own(argument.value)))
        .collect()
}

/// The real port, recording every call.
struct Recorder<'p, E> {
    inner: &'p mut dyn FunctionPort<Error = E>,
    calls: Vec<Call>,
}
impl<E> FunctionPort for Recorder<'_, E> {
    type Error = E;
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        self.inner.signature(slot)
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        self.inner.project_signature(slot)
    }
    fn call(&mut self, slot: usize, supplied: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        let result = self.inner.call(slot, supplied);
        self.calls.push(Call {
            slot,
            arguments: arguments(supplied),
            outcome: match &result {
                Ok(value) => Ok(value.clone()),
                Err(HostError::Raised(_)) => Err(true),
                Err(HostError::InvalidResult(_)) => Err(false),
            },
        });
        result
    }
}

/// The recorded calls, replayed in order; the first call that differs is noted.
struct Replay<'r, E> {
    port: &'r dyn FunctionPort<Error = E>,
    calls: &'r [Call],
    position: usize,
    mismatch: Option<String>,
}
impl<E> FunctionPort for Replay<'_, E> {
    type Error = Token;
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        self.port.signature(slot)
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        self.port.project_signature(slot)
    }
    fn call(&mut self, slot: usize, supplied: &[Argument<'_>]) -> Result<Value, HostError<Token>> {
        let supplied = arguments(supplied);
        let position = self.position;
        self.position += 1;
        let Some(call) = self.calls.get(position) else {
            self.mismatch.get_or_insert_with(|| {
                format!("an extra call {position}: slot {slot} {supplied:?}")
            });
            return Err(HostError::Raised(Token::Host(position)));
        };
        if call.slot != slot || call.arguments != supplied {
            self.mismatch.get_or_insert_with(|| {
                format!(
                    "call {position}: recorded slot {} {:?}, replayed slot {slot} {supplied:?}",
                    call.slot, call.arguments
                )
            });
        }
        match &call.outcome {
            Ok(value) => Ok(value.clone()),
            Err(true) => Err(HostError::Raised(Token::Host(position))),
            Err(false) => Err(HostError::InvalidResult(Token::Host(position))),
        }
    }
}

/// A table whose opaque failures become tokens.
struct Port<'t, T: ?Sized>(&'t T);
impl<T: TableAccess + ?Sized> TableAccess for Port<'_, T> {
    type Error = Token;
    fn schema(&self) -> &TableSchema {
        self.0.schema()
    }
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Token>> {
        self.0.cell(row, column).map_err(|error| match error {
            CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
            CellError::Access(_) => CellError::Access(Token::Cell { row, column }),
        })
    }
}

/// Run both executors and require equal attempts; return today's.
pub(crate) fn compare<T: TableAccess + ?Sized>(
    plan: &DatasetPlan,
    table: &T,
    secondary: &[&dyn TableAccess<Error = T::Error>],
    functions: &mut dyn FunctionPort<Error = T::Error>,
    mut observer: Option<&mut dyn FnMut(ExecutionEvent)>,
) -> ExecutionAttempt<T::Error> {
    let resolved = match plan.resolve() {
        Ok(resolved) => resolved,
        Err(error) => fail(&format!("the bound plan does not resolve: {error}")),
    };
    let mut recorder = Recorder {
        inner: functions,
        calls: Vec::new(),
    };
    let mut events = Vec::new();
    let today = {
        let mut forward = |event| {
            events.push(event);
            if let Some(observer) = observer.as_mut() {
                observer(event);
            }
        };
        plan.execute_rows(table, secondary, &mut recorder, Some(&mut forward))
    };
    let Recorder { inner, calls } = recorder;
    let primary = Port(table);
    let ports: Vec<Port<'_, dyn TableAccess<Error = T::Error>>> =
        secondary.iter().map(|&table| Port(table)).collect();
    let tables: Vec<&dyn TableAccess<Error = Token>> = ports
        .iter()
        .map(|port| port as &dyn TableAccess<Error = Token>)
        .collect();
    let mut replayed = Replay {
        port: &*inner,
        calls: &calls,
        position: 0,
        mismatch: None,
    };
    let mut replayed_events = Vec::new();
    let reference = plan.execute_rows(
        &primary,
        &tables,
        &mut replayed,
        Some(&mut |event| replayed_events.push(event)),
    );
    let mut candidate_port = Replay {
        port: &*inner,
        calls: &calls,
        position: 0,
        mismatch: None,
    };
    let mut candidate_events = Vec::new();
    let candidate = resolved.execute(
        &primary,
        &tables,
        &mut candidate_port,
        Some(&mut |event| candidate_events.push(event)),
    );
    let left = format!("{reference:#?}");
    let right = format!("{candidate:#?}");
    let calls_differ = |replay: &Replay<'_, T::Error>| {
        replay.mismatch.clone().or_else(|| {
            (replay.position != calls.len()).then(|| {
                format!(
                    "{} calls recorded, {} replayed",
                    calls.len(),
                    replay.position
                )
            })
        })
    };
    let reference_calls = calls_differ(&replayed);
    let candidate_calls = calls_differ(&candidate_port);
    if reference_calls.is_some() || replayed_events != events {
        fail(&format!(
            "today's executor did not replay its own run: {reference_calls:?}"
        ));
    }
    if left != right || candidate_events != events || candidate_calls.is_some() {
        fail(&format!(
            "the attempts differ\n{}\n--- today\n{left}\n--- resolved\n{right}\n--- events today\n{events:?}\n--- events resolved\n{candidate_events:?}\n--- calls\n{candidate_calls:?}",
            first_difference(&left, &right)
        ));
    }
    today
}

/// Report a difference and stop the process: the hosts catch panics, and an
/// abort cannot be mistaken for an ordinary failure of the run. The report
/// is written to the standard error stream directly, past any test harness
/// capture that the abort would discard.
fn fail(message: &str) -> ! {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "executor comparison: {message}");
    std::process::abort()
}

/// The first differing line of two renderings.
fn first_difference(left: &str, right: &str) -> String {
    for (index, (a, b)) in left.lines().zip(right.lines()).enumerate() {
        if a != b {
            return format!("first difference at line {index}:\n  today:    {a}\n  resolved: {b}");
        }
    }
    format!(
        "one rendering is a prefix of the other ({} and {} lines)",
        left.lines().count(),
        right.lines().count()
    )
}
