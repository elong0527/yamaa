//! Per-call pending signals and original callback interrupts on the calling
//! interpreter thread. Nested native calls restore the enclosing call's state.
use pyo3::prelude::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex, PoisonError},
};

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
pub(super) fn cancelled(state: &State) -> bool {
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
    let error = state.lock().unwrap_or_else(PoisonError::into_inner).take();
    match error {
        Some(error) => Err(error),
        None => result,
    }
}
