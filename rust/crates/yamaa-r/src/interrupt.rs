//! The interrupt of one native call (#3030 D46). Its token's probe asks R
//! whether an interrupt is pending, as a long computation does with
//! `R_CheckUserInterrupt`, and keeps the condition R then signals; an
//! interrupt condition a project function raised is kept the same way.
//! Either cancels the call, which replies with the kept condition for
//! `.reply()` to signal again, and publishes nothing.
//!
//! R's API is used only on the thread that made the call. `R_tryCatch`
//! catches the condition with an exiting handler, so control returns to
//! Rust normally. The jump R takes to that handler leaves only the frame of
//! `check`, which owns nothing to drop. Besides interrupts it catches
//! errors, such as an elapsed time limit that `setTimeLimit()` set, which R
//! checks at the same point: such an error, too, stops the call and is kept
//! to be raised again.

use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use extendr_api::prelude::*;
use yamaa::CancellationToken;

use crate::thread_bound::ThreadBound;

// R's C API for catching a condition; a `SEXP` is declared as `void *`.
unsafe extern "C" {
    fn R_tryCatch(
        body: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        body_data: *mut c_void,
        classes: *mut c_void,
        handler: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void,
        handler_data: *mut c_void,
        finally: Option<unsafe extern "C" fn(*mut c_void)>,
        finally_data: *mut c_void,
    ) -> *mut c_void;
    fn R_CheckUserInterrupt();
    fn Rf_protect(value: *mut c_void) -> *mut c_void;
    fn Rf_unprotect(count: c_int);
}

/// How often the probe asks R at most. Each ask evaluates R's `tryCatch`,
/// and the engine may poll before every row that calls an R function; a
/// call's first poll always asks.
const INTERVAL: Duration = Duration::from_millis(20);

/// `R_CheckUserInterrupt` under `R_tryCatch`, returning `nil`, R's `NULL`,
/// when it returns.
unsafe extern "C" fn check(nil: *mut c_void) -> *mut c_void {
    // SAFETY: R_tryCatch calls this on the R thread. A pending interrupt or
    // an error makes R jump to the handler R_tryCatch established, past this
    // frame, which owns nothing to drop.
    unsafe { R_CheckUserInterrupt() };
    nil
}

/// The handler: the condition itself.
unsafe extern "C" fn caught(condition: *mut c_void, _: *mut c_void) -> *mut c_void {
    condition
}

/// The condition R signals when asked now, an interrupt or an error, if any.
fn pending() -> Option<Robj> {
    let nil = Robj::from(());
    let classes = Strings::from_values(["interrupt", "error"]).into_robj();
    // SAFETY: on the R thread. `nil` and `classes` stay protected by their
    // owners throughout, and the result, `nil` or the condition, is protected
    // before anything else can allocate.
    let condition = unsafe {
        let result = R_tryCatch(
            check,
            nil.get().cast(),
            classes.get().cast(),
            caught,
            std::ptr::null_mut(),
            None,
            std::ptr::null_mut(),
        );
        Rf_protect(result);
        let condition = Robj::from_sexp(result.cast());
        Rf_unprotect(1);
        condition
    };
    (!condition.is_null()).then_some(condition)
}

/// One call's interrupt: the token its engine polls and the condition kept
/// for the reply.
#[derive(Clone)]
pub(crate) struct Interrupt {
    token: CancellationToken,
    kept: Arc<Mutex<Option<ThreadBound<Robj>>>>,
}

impl Interrupt {
    pub(crate) fn new() -> Self {
        let kept: Arc<Mutex<Option<ThreadBound<Robj>>>> = Arc::default();
        let held = Arc::clone(&kept);
        let owner = std::thread::current().id();
        let asked = Mutex::new(None::<Instant>);
        let token = CancellationToken::with_probe(move || {
            if std::thread::current().id() != owner {
                return false;
            }
            {
                let mut last = asked.lock().unwrap_or_else(PoisonError::into_inner);
                if last.is_some_and(|last| last.elapsed() < INTERVAL) {
                    return false;
                }
                *last = Some(Instant::now());
            }
            match pending() {
                Some(condition) => {
                    keep(&held, condition);
                    true
                }
                None => false,
            }
        });
        Self { token, kept }
    }

    /// The token the call's engine polls.
    pub(crate) fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Whether the call is cancelled, asking R once more whatever the
    /// interval. A call that would return a value asks as it returns, so an
    /// interrupt the probe's interval passed over still stops the call, rather
    /// than reaching R at some later point of the caller's code.
    pub(crate) fn pending_now(&self) -> bool {
        if self.token.is_cancelled() {
            return true;
        }
        match pending() {
            Some(condition) => {
                self.keep(condition);
                self.token.cancel();
                true
            }
            None => false,
        }
    }

    /// Keep `condition`, an interrupt a project function raised, unless the
    /// call already keeps one.
    pub(crate) fn keep(&self, condition: Robj) {
        keep(&self.kept, condition);
    }

    /// The condition a cancelled call replies with: the kept one, or else an
    /// interrupt condition as R makes one.
    pub(crate) fn condition(&self) -> Robj {
        let kept = self
            .kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        kept.and_then(|condition| condition.get().ok().cloned())
            .unwrap_or_else(|| {
                let mut condition = List::new(0).into_robj();
                // A fresh empty list takes any class.
                let _ = condition.set_class(["interrupt", "condition"]);
                condition
            })
    }
}

fn keep(kept: &Mutex<Option<ThreadBound<Robj>>>, condition: Robj) {
    kept.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(|| ThreadBound::new(condition));
}
