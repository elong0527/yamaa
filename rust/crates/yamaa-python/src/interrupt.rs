//! Per-call pending signals and original callback interrupts on the calling
//! interpreter thread. Nested native calls restore the enclosing call's state.
use pyo3::prelude::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex, PoisonError},
};
use yamaa::CancellationToken;

/// The interrupt one call kept, if any.
pub(super) type State = Arc<Mutex<Option<PyErr>>>;

thread_local! {
    static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct Scope {
    previous: Option<State>,
    // A scope must restore the thread on which it was entered.
    _thread: std::marker::PhantomData<Rc<()>>,
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.previous.take()));
    }
}

fn keep(state: &State, error: PyErr) {
    let mut kept = state.lock().unwrap_or_else(PoisonError::into_inner);
    if kept.is_none() {
        *kept = Some(error);
    }
}

/// Keep a callback's original BaseException before the cancelled attempt is
/// dropped. Ordinary exceptions still belong to the engine's diagnostics.
pub(super) fn remember(py: Python<'_>, error: &PyErr) {
    let state = ACTIVE.with(|active| active.borrow().clone());
    if let Some(state) = state {
        keep(&state, error.clone_ref(py));
    }
}

/// Whether the call `state` belongs to is cancelled: an interrupt it kept,
/// or a signal pending now, which it keeps. The engine asks this probe at
/// its cancellation points and once more as the call returns.
fn cancelled(state: &State) -> bool {
    if state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some()
    {
        return true;
    }
    Python::attach(|py| match py.check_signals() {
        Ok(()) => false,
        Err(error) => {
            keep(state, error);
            true
        }
    })
}

/// Run a call with its interrupt state, then raise the original interrupt
/// in preference to whatever the call answered.
pub(super) fn run<T>(_py: Python<'_>, run: impl FnOnce(&State) -> PyResult<T>) -> PyResult<T> {
    let state: State = Arc::new(Mutex::new(None));
    let _scope = Scope {
        previous: ACTIVE.with(|active| active.replace(Some(Arc::clone(&state)))),
        _thread: std::marker::PhantomData,
    };
    let result = run(&state);
    // Streamed accessors pass no token of their own. Also deliver a signal
    // first observed as any native call returns.
    let _ = cancelled(&state);
    let error = state.lock().unwrap_or_else(PoisonError::into_inner).take();
    match error {
        Some(error) => Err(error),
        None => result,
    }
}

/// The token of the call that keeps `state`. On the thread that made the
/// call its probe asks [`cancelled`]; on any other thread it never touches
/// the interpreter.
pub(super) fn token(state: &State) -> CancellationToken {
    let owner = std::thread::current().id();
    let state = Arc::clone(state);
    CancellationToken::with_probe(move || std::thread::current().id() == owner && cancelled(&state))
}

/// The token of a streamed run (#3697), which outlives the call that started
/// it. On the thread that started the run, its probe polls the native call
/// that is running, which keeps the interrupt and raises it again; with no
/// native call running, as when another library pulls the run's Arrow
/// stream, it checks the pending signals, whose interrupt ends the stream as
/// cancelled. On any other thread it never touches the interpreter.
pub(super) fn stream_token() -> CancellationToken {
    let owner = std::thread::current().id();
    CancellationToken::with_probe(move || {
        if std::thread::current().id() != owner {
            return false;
        }
        match ACTIVE.with(|active| active.borrow().clone()) {
            Some(state) => cancelled(&state),
            None => Python::attach(|py| py.check_signals().is_err()),
        }
    })
}
