//! A cloneable cancellation handle over a shared atomic flag.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// A cloneable handle over a shared atomic cancellation flag. All clones
/// share one flag; hosts wire interrupts into the token.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create an uncancelled token.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cancel every handle sharing this flag.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Return whether any shared handle has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_share_one_flag_across_clones() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        let shared = token.clone();
        token.cancel();
        assert!(shared.is_cancelled());
        assert!(!CancellationToken::default().is_cancelled());
    }
}
