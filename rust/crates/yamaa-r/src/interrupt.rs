//! Catch pending R interrupts inside R's own try/catch boundary. The probe
//! never lets R's nonlocal jump cross an engine frame with owned state.
use extendr_api::prelude::*;
use std::{
    cell::RefCell,
    ffi::c_void,
    rc::Rc,
    time::{Duration, Instant},
};
use yamaa::CancellationToken;

// R's SEXP is an opaque pointer. Use that pointer's ABI directly, without
// exposing extendr-ffi's non-exhaustive SEXPREC definition in an extern block.
type Sexp = *mut c_void;

struct State {
    condition: RefCell<Option<Robj>>,
    checked: RefCell<Option<Instant>>,
    classes: Strings,
    token: CancellationToken,
}
thread_local! {
    static ACTIVE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

pub(super) struct Scope {
    state: Rc<State>,
    previous: Option<Rc<State>>,
}
impl Scope {
    pub(super) fn new() -> Self {
        let state = Rc::new(State {
            condition: RefCell::new(None),
            checked: RefCell::new(None),
            classes: Strings::from_values(["interrupt", "error"]),
            token: CancellationToken::with_probe(|| poll(false)),
        });
        Self {
            previous: ACTIVE.with(|active| active.replace(Some(Rc::clone(&state)))),
            state,
        }
    }

    /// Force one final interpreter check even within the probe's throttle.
    pub(super) fn take(&self) -> Option<Robj> {
        let _ = poll_state(&self.state, true);
        self.state.condition.borrow_mut().take()
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.previous.take()));
    }
}

pub(super) fn token() -> CancellationToken {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or_else(CancellationToken::new, |state| state.token.clone())
    })
}

/// Keep the original condition before cancellation drops a callback's attempt.
pub(super) fn remember(condition: Robj) {
    let state = ACTIVE.with(|active| active.borrow().clone());
    if let Some(state) = state {
        let mut kept = state.condition.borrow_mut();
        if kept.is_none() {
            *kept = Some(condition);
        }
    }
}

fn poll(force: bool) -> bool {
    let state = ACTIVE.with(|active| active.borrow().clone());
    state.is_some_and(|state| poll_state(&state, force))
}

fn poll_state(state: &State, force: bool) -> bool {
    if state.condition.borrow().is_some() {
        return true;
    }
    if !force
        && state
            .checked
            .borrow()
            .is_some_and(|time| time.elapsed() < Duration::from_millis(20))
    {
        return false;
    }
    *state.checked.borrow_mut() = Some(Instant::now());
    // SAFETY: this runs on the calling R thread. `classes` stays protected.
    // The body owns no Rust values; R_tryCatch catches its interrupt/error
    // jump before returning through the engine's Rust frames. Its handler
    // returns the original condition, protected immediately without any
    // intervening R allocation. No RefCell borrow is held during the call.
    let condition = unsafe {
        let result = R_tryCatch(
            Some(check),
            std::ptr::null_mut(),
            state.classes.get().cast(),
            Some(raised),
            std::ptr::null_mut(),
            None,
            std::ptr::null_mut(),
        );
        Robj::from_sexp(result.cast())
    };
    if condition.is_null() {
        false
    } else {
        *state.condition.borrow_mut() = Some(condition);
        true
    }
}

unsafe extern "C" fn check(_: *mut c_void) -> Sexp {
    // SAFETY: called only by R_tryCatch on the R thread. There are no Rust
    // destructors in this frame for an interrupt's nonlocal jump to skip.
    unsafe {
        R_CheckUserInterrupt();
        R_NilValue
    }
}
unsafe extern "C" fn raised(condition: Sexp, _: *mut c_void) -> Sexp {
    condition
}

extern "C" {
    static R_NilValue: Sexp;
    fn R_CheckUserInterrupt();
    fn R_tryCatch(
        body: Option<unsafe extern "C" fn(*mut c_void) -> Sexp>,
        data: *mut c_void,
        classes: Sexp,
        handler: Option<unsafe extern "C" fn(Sexp, *mut c_void) -> Sexp>,
        handler_data: *mut c_void,
        finally: Option<unsafe extern "C" fn(*mut c_void)>,
        finally_data: *mut c_void,
    ) -> Sexp;
}
