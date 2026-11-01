//! A cloneable cancellation handle over a shared atomic flag, with the
//! probe a host installs to ask its interpreter whether an interrupt is
//! pending (#3030 D46).

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// A cloneable handle over a shared atomic cancellation flag. All clones
/// share one flag, and one probe when the token has one.
///
/// An operation polls its token, on its calling thread, at fixed points:
/// when it starts, before each member of a project run or check and after
/// each member it builds, before each submission section, at every
/// checkpoint of a dataset's schedule and before each batch of
/// project-function calls, and once in a save, after every file is staged
/// and before the first commit (D74). A cancelled operation fails with a
/// [`FailureKind::Cancelled`](crate::FailureKind::Cancelled) failure, with no
/// result and no issue, and a cancelled save writes nothing.
///
/// A host makes its token with [`CancellationToken::with_probe`], whose
/// probe asks the interpreter whether an interrupt is pending, keeps that
/// interrupt and answers true; the host raises it again once the call has
/// returned.
#[non_exhaustive]
#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    probe: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

/// Why an internal step stopped: its token was found cancelled, or a project
/// function raised the host's interrupt (D46). The operation that ran the
/// step turns it into its own cancelled [`Failure`](crate::Failure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cancelled;

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("cancelled", &self.cancelled.load(Ordering::SeqCst))
            .field("probe", &self.probe.is_some())
            .finish()
    }
}

impl CancellationToken {
    /// Create an uncancelled token.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            probe: None,
        }
    }

    /// Create an uncancelled token that also asks `probe` whether to cancel
    /// each time it is polled. Once `probe` answers true, every handle
    /// sharing the token is cancelled and `probe` is not asked again.
    pub fn with_probe(probe: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            probe: Some(Arc::new(probe)),
            ..Self::new()
        }
    }

    /// Cancel every handle sharing this flag.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Return whether any shared handle has been cancelled, first asking the
    /// probe, if the token has one, while it has not been.
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::SeqCst) {
            return true;
        }
        match &self.probe {
            Some(probe) if probe() => {
                self.cancel();
                true
            }
            _ => false,
        }
    }

    /// A token for work on another thread (#4193): cancelled once this
    /// token's flag is, or once it or a clone of it is cancelled, which
    /// leaves this token as it is. It never asks this token's probe, which
    /// a host answers on the calling thread alone; the calling thread polls
    /// this token, and so raises the flag the follower reads.
    pub(crate) fn follower(&self) -> Self {
        let leader = Arc::clone(&self.cancelled);
        Self::with_probe(move || leader.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn tokens_share_one_flag_across_clones() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        let shared = token.clone();
        token.cancel();
        assert!(shared.is_cancelled());
        assert!(!CancellationToken::default().is_cancelled());
    }

    /// The probe is asked at each poll until it answers true; from then on
    /// every clone is cancelled and the probe is not asked again.
    #[test]
    fn a_probe_cancels_every_clone_once_it_answers_true() {
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let token =
            CancellationToken::with_probe(move || counted.fetch_add(1, Ordering::SeqCst) >= 2);
        let shared = token.clone();
        assert!(!token.is_cancelled());
        assert!(!shared.is_cancelled());
        assert!(token.is_cancelled());
        assert!(shared.is_cancelled());
        assert!(token.is_cancelled());
        assert_eq!(asked.load(Ordering::SeqCst), 3);
        assert_eq!(
            format!("{token:?}"),
            "CancellationToken { cancelled: true, probe: true }"
        );
        // A token cancelled directly never asks its probe.
        let never = CancellationToken::with_probe(|| panic!("asked"));
        never.cancel();
        assert!(never.is_cancelled());
    }

    /// A follower reads its leader's flag and never asks the leader's probe;
    /// cancelling a follower, or a clone of it, leaves the leader as it is.
    #[test]
    fn a_follower_reads_its_leader_s_flag_and_never_its_probe() {
        let leader = CancellationToken::with_probe(|| panic!("a follower asked the probe"));
        let follower = leader.follower();
        assert!(!follower.is_cancelled(), "an uncancelled leader");
        let stopped = leader.follower();
        stopped.clone().cancel();
        assert!(
            stopped.is_cancelled(),
            "a follower cancelled through a clone"
        );
        assert!(!follower.is_cancelled(), "a sibling follower");
        assert!(
            !leader.cancelled.load(Ordering::SeqCst),
            "the leader of a cancelled follower"
        );
        leader.cancel();
        assert!(
            follower.is_cancelled(),
            "the follower of a cancelled leader"
        );
    }
}
