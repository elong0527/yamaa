//! Single-threaded R object ownership for facade handles.
//!
//! R API objects are neither `Send` nor `Sync`: they may only be created,
//! used and dropped on the R thread. Engine operations for this host run
//! exclusively on the calling R thread, exactly as the pre-facade host
//! crates did, so transferring ownership through `Send` facade handles never
//! observes concurrent access. The facade never inspects payload contents;
//! hosts clone and consume them on the R thread only.
pub(crate) struct ThreadBound<T>(pub(crate) T);

impl<T> ThreadBound<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(value)
    }
}

// SAFETY: upheld by the host contract above. The wrapped value is only
// touched on the R thread, which is the only thread driving this host.
unsafe impl<T> Send for ThreadBound<T> {}

// SAFETY: same contract. Shared references may cross threads inside facade
// handles, but the contents are only dereferenced on the R thread.
unsafe impl<T> Sync for ThreadBound<T> {}
