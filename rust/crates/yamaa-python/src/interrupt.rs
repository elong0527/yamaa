//! Per-call pending signals and original callback interrupts on the calling
//! interpreter thread. Nested native calls restore the enclosing call's state.
use pyo3::prelude::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex, PoisonError},
};
use yamaa::CancellationToken;

type State = Arc<Mutex<Option<PyErr>>>;
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

/// Install the call's probe, run the engine, then raise the original interrupt
/// in preference to cancellation's payload-free facade error.
pub(super) fn run<T>(
    _py: Python<'_>,
    run: impl FnOnce(&CancellationToken) -> PyResult<T>,
) -> PyResult<T> {
    let state = Arc::new(Mutex::new(None));
    let _scope = Scope {
        previous: ACTIVE.with(|active| active.replace(Some(Arc::clone(&state)))),
        _thread: std::marker::PhantomData,
    };
    let polled = Arc::clone(&state);
    let token = CancellationToken::with_probe(move || {
        if polled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
        {
            return true;
        }
        Python::attach(|py| match py.check_signals() {
            Ok(()) => false,
            Err(error) => {
                keep(&polled, error);
                true
            }
        })
    });
    let result = run(&token);
    // Also deliver a signal first observed as the engine returns.
    let _ = token.is_cancelled();
    let error = state.lock().unwrap_or_else(PoisonError::into_inner).take();
    match error {
        Some(error) => Err(error),
        None => result,
    }
}
