//! Single-threaded R object ownership for the objects the engine keeps.
//!
//! R API objects are neither `Send` nor `Sync`: they may only be created,
//! used and dropped on the R thread. Engine operations for this host run on
//! the calling R thread in normal use, but the engine keeps a bound function
//! or a condition for as long as the result that holds it lives, and drops
//! it on the thread that releases that result.
//!
//! `ThreadBound` therefore records the creating thread. Every access goes
//! through [`ThreadBound::get`], which answers `None` on another thread, so
//! that the caller reports a `host_thread` failure instead of invoking the R
//! API off-thread. Dropping off-thread would be equally undefined (unprotect
//! and friends are thread-bound), so a mismatched drop leaks the value
//! instead of freeing it.

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
    pub(crate) fn get(&self) -> Option<&T> {
        (std::thread::current().id() == self.owner).then_some(&*self.value)
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

// SAFETY: same contract. Shared references may cross threads inside the
// engine's handles, but the contents are only dereferenced via the checked
// accessor.
unsafe impl<T> Sync for ThreadBound<T> {}
