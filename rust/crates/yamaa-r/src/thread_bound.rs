//! Single-threaded R object ownership for facade handles.
//!
//! R API objects are neither `Send` nor `Sync`: they may only be created,
//! used and dropped on the R thread. Engine operations for this host run on
//! the calling R thread in normal use, but the facade types that carry these
//! objects (`FunctionHandle`, `HostPayload`) are `Send`, so a concurrent
//! attempt from another thread is well-typed. A `Mutex` only serializes such
//! access; it does not move execution to the R thread.
//!
//! `ThreadBound` therefore records the creating thread. Every access goes
//! through [`ThreadBound::get`], which reports a mismatch as a typed
//! `host_thread` error instead of invoking the R API off-thread. Dropping
//! off-thread would be equally undefined (unprotect and friends are
//! thread-bound), so a mismatched drop leaks the value instead of freeing it.

use yamaa::FunctionError;

pub(crate) struct ThreadBound<T> {
    value: std::mem::ManuallyDrop<T>,
    owner: std::thread::ThreadId,
}

impl<T> ThreadBound<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value: std::mem::ManuallyDrop::new(value),
            owner: std::thread::current().id(),
        }
    }

    /// Borrow the wrapped value when called on the creating thread.
    pub(crate) fn get(&self) -> Result<&T, FunctionError> {
        if std::thread::current().id() == self.owner {
            Ok(&self.value)
        } else {
            Err(FunctionError::Internal {
                code: "host_thread",
            })
        }
    }
}

impl<T> Drop for ThreadBound<T> {
    fn drop(&mut self) {
        if std::thread::current().id() != self.owner {
            return;
        }
        // SAFETY: owner thread, runs at most once per value, and the value
        // is never touched afterwards.
        unsafe { std::mem::ManuallyDrop::drop(&mut self.value) };
    }
}

// SAFETY: the wrapped value is only dereferenced on its creating thread
// (checked by `get`) and only freed there (checked by `drop`); cross-thread
// shares observe the handle without touching the R object.
unsafe impl<T> Send for ThreadBound<T> {}

// SAFETY: same contract. Shared references may cross threads inside facade
// handles, but the contents are only dereferenced via the checked accessor.
unsafe impl<T> Sync for ThreadBound<T> {}
