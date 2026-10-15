//! One actual charge owner for serial entered dataset executions.
use crate::dataset::Limits;
use core::cell::Cell;
use {crate::predicate, crate::regex};

/// Actual admitted graph execution counters. Shape tracks retained successful outputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub runtime: predicate::Usage,
    pub output_rows: usize,
    pub output_cells: usize,
    pub output_text_bytes: usize,
    pub identity_cells: usize,
    pub identity_text_bytes: usize,
}
impl Usage {
    fn ceiling(limits: Limits) -> Self {
        Self {
            runtime: predicate::Usage {
                work: limits.work_cells,
                resolutions: limits.work_cells,
                text_bytes: limits.scalar_text_bytes,
                like_work: limits.work_cells,
                regex: regex::MatchUsage {
                    subject_bytes: limits.scalar_text_bytes,
                    work: limits.work_cells,
                    state_cells: limits.work_cells,
                },
            },
            output_rows: limits.output_rows,
            output_cells: limits.output_cells,
            output_text_bytes: limits.output_text_bytes,
            identity_cells: limits.identity_cells,
            identity_text_bytes: limits.identity_text_bytes,
        }
    }
}

/// One whole-attempt owner; interrupted executions reserve all remaining capacity.
/// The work-only owner is the base accounting; pooled owners add the native
/// graph's measured execution charges and retained geometry.
pub struct WorkBudget {
    limit: usize,
    used: Cell<usize>,
    limits: Option<Limits>,
    usage: Cell<Usage>,
    known: Cell<bool>,
}
impl WorkBudget {
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            used: Cell::new(0),
            limits: None,
            known: Cell::new(true),
            usage: Cell::new(Usage {
                runtime: predicate::Usage {
                    work: 0,
                    resolutions: 0,
                    text_bytes: 0,
                    like_work: 0,
                    regex: regex::MatchUsage {
                        subject_bytes: 0,
                        work: 0,
                        state_cells: 0,
                    },
                },
                output_rows: 0,
                output_cells: 0,
                output_text_bytes: 0,
                identity_cells: 0,
                identity_text_bytes: 0,
            }),
        }
    }
    /// Pool the native graph's measured execution charges and retained geometry.
    /// Local node policies may tighten these bounds without resetting usage.
    pub fn pooled(limits: Limits) -> Self {
        Self {
            limits: Some(limits),
            ..Self::new(limits.work_cells)
        }
    }
    pub fn limit(&self) -> usize {
        self.limit
    }
    pub fn used(&self) -> usize {
        self.used.get()
    }
    pub fn usage(&self) -> Option<Usage> {
        self.limits
            .filter(|_| self.known.get())
            .map(|_| self.usage.get())
    }
    pub(crate) fn unknown_prefix(&self) -> bool {
        self.limits.is_some() && !self.known.get()
    }
    /// Shape admission receives the actual remaining output pool. Temporary
    /// grouping storage is run-local; only one node executes at a time.
    pub(crate) fn node_limits(&self) -> Option<Limits> {
        self.limits.map(|limits| Limits {
            output_rows: limits.output_rows - self.usage.get().output_rows,
            output_cells: limits.output_cells - self.usage.get().output_cells,
            ..limits
        })
    }
    pub(crate) fn enter(&self) -> Reservation<'_> {
        let prior = self.used.get();
        let usage = self.usage();
        self.used.set(self.limit);
        if let Some(limits) = self.limits {
            self.usage.set(Usage::ceiling(limits));
            self.known.set(false);
        }
        Reservation {
            owner: self,
            prior,
            usage,
        }
    }
}
pub(crate) struct Reservation<'a> {
    owner: &'a WorkBudget,
    prior: usize,
    usage: Option<Usage>,
}
impl Reservation<'_> {
    pub(crate) fn prior(&self) -> usize {
        self.prior
    }
    pub(crate) fn limit(&self) -> usize {
        self.owner.limit
    }
    pub(crate) fn usage(&self) -> Option<Usage> {
        self.usage
    }
    pub(crate) fn complete(self, used: usize, usage: Usage, shape: Option<(usize, usize)>) {
        assert!(used >= self.prior && used <= self.owner.limit);
        if let Some(prior) = self.usage {
            let mut usage = Usage {
                output_rows: prior.output_rows,
                output_cells: prior.output_cells,
                ..usage
            };
            if let Some((rows, cells)) = shape {
                usage.output_rows = prior.output_rows.checked_add(rows).expect("admitted rows");
                usage.output_cells = prior
                    .output_cells
                    .checked_add(cells)
                    .expect("admitted cells");
            }
            let limits = self.owner.limits.expect("pooled limits");
            assert!(usage.output_rows <= limits.output_rows);
            assert!(usage.output_cells <= limits.output_cells);
            assert!(
                usage.output_text_bytes >= prior.output_text_bytes
                    && usage.output_text_bytes <= limits.output_text_bytes
            );
            assert!(
                usage.identity_cells >= prior.identity_cells
                    && usage.identity_cells <= limits.identity_cells
            );
            assert!(
                usage.identity_text_bytes >= prior.identity_text_bytes
                    && usage.identity_text_bytes <= limits.identity_text_bytes
            );
            self.owner.usage.set(usage);
            self.owner.known.set(true);
        }
        self.owner.used.set(used);
    }
}
