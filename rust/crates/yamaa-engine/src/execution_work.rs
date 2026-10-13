//! One bounded logical-work owner for serial entered dataset executions.
//! Other execution resources keep their separately admitted per-node scopes.
use core::cell::Cell;

/// A whole-attempt ceiling, independent of node count or projected output width.
/// This owner is not cloneable and cannot be shared across concurrent threads.
pub struct WorkBudget {
    limit: usize,
    used: Cell<usize>,
}
impl WorkBudget {
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            used: Cell::new(0),
        }
    }
    pub fn limit(&self) -> usize {
        self.limit
    }
    /// Actual admitted work after each return; an interrupted run reserves the
    /// remaining ceiling so catching an unwind cannot grant that work twice.
    pub fn used(&self) -> usize {
        self.used.get()
    }
    pub(crate) fn enter(&self) -> Reservation<'_> {
        let prior = self.used.get();
        self.used.set(self.limit);
        Reservation { owner: self, prior }
    }
}
pub(crate) struct Reservation<'a> {
    owner: &'a WorkBudget,
    prior: usize,
}
impl Reservation<'_> {
    pub(crate) fn prior(&self) -> usize {
        self.prior
    }
    pub(crate) fn limit(&self) -> usize {
        self.owner.limit
    }
    pub(crate) fn complete(self, used: usize) {
        assert!(used >= self.prior && used <= self.owner.limit);
        self.owner.used.set(used);
    }
}
