//! Handler counts by site slot. Every site of the resolved table is
//! registered at zero before any row is read and reported in table order,
//! donor sites included, each counted per firing.
use crate::dataset::resolved::{HandlerSiteId, ResolvedPlan};
use crate::numeric_lifecycle::{HandlerCount, HandlerCountOverflow, HandlerKind};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct Handlers {
    counts: Vec<u64>,
    /// Sites by path and kind, for firings whose node lists no site.
    index: HashMap<(Arc<str>, u8), HandlerSiteId>,
    /// Sites the table lacks, registered when they first fire. The
    /// registration walk is checked against the table on every bind, so
    /// none is expected; one would join the report after the table.
    dynamic: Vec<HandlerCount>,
}

/// The handler state before a batch unit, restored if the unit replays.
#[derive(Clone, Debug)]
pub(super) struct HandlerMark {
    counts: Vec<u64>,
    dynamic: Vec<HandlerCount>,
}

fn kind_order(kind: HandlerKind) -> u8 {
    match kind {
        HandlerKind::Absent => 0,
        HandlerKind::Missing => 1,
        HandlerKind::Unmapped => 2,
        HandlerKind::Invalid => 3,
        HandlerKind::Unconvertible => 4,
        HandlerKind::MultipleMatches => 5,
        HandlerKind::NoMatch => 6,
        HandlerKind::Partial => 7,
        HandlerKind::CrossedBounds => 8,
    }
}

impl Handlers {
    pub(super) fn new(plan: &ResolvedPlan) -> Self {
        let index = plan
            .handlers()
            .iter()
            .enumerate()
            .map(|(slot, site)| {
                (
                    (site.path.clone(), kind_order(site.kind)),
                    HandlerSiteId::try_from(slot).expect("a table slot"),
                )
            })
            .collect();
        Self {
            counts: vec![0; plan.handlers().len()],
            index,
            dynamic: Vec::new(),
        }
    }

    /// Count `firings` firings of a table site.
    pub(super) fn fire(
        &mut self,
        plan: &ResolvedPlan,
        site: HandlerSiteId,
        firings: u64,
    ) -> Result<(), HandlerCountOverflow> {
        let count = &mut self.counts[site.index()];
        *count = count.checked_add(firings).ok_or_else(|| {
            let entry = plan.handler(site);
            HandlerCountOverflow {
                spec_path: entry.path.to_string(),
                handler: entry.kind,
            }
        })?;
        Ok(())
    }

    /// Count `firings` firings at a path, at least one: its table site, or
    /// a site registered now.
    pub(super) fn fire_path(
        &mut self,
        plan: &ResolvedPlan,
        path: &str,
        kind: HandlerKind,
        firings: u64,
    ) -> Result<(), HandlerCountOverflow> {
        let key: (Arc<str>, u8) = (path.into(), kind_order(kind));
        if let Some(&site) = self.index.get(&key) {
            return self.fire(plan, site, firings);
        }
        let position = match self
            .dynamic
            .iter()
            .position(|entry| entry.spec_path == path && entry.handler == kind)
        {
            Some(position) => position,
            None => {
                self.dynamic.push(HandlerCount {
                    spec_path: path.into(),
                    handler: kind,
                    count: 0,
                });
                self.dynamic.len() - 1
            }
        };
        let entry = &mut self.dynamic[position];
        entry.count = entry
            .count
            .checked_add(firings)
            .ok_or_else(|| HandlerCountOverflow {
                spec_path: path.into(),
                handler: kind,
            })?;
        Ok(())
    }

    pub(super) fn mark(&self) -> HandlerMark {
        HandlerMark {
            counts: self.counts.clone(),
            dynamic: self.dynamic.clone(),
        }
    }

    pub(super) fn restore(&mut self, mark: HandlerMark) {
        self.counts = mark.counts;
        self.dynamic = mark.dynamic;
    }

    /// Whether the counts are those `mark` holds.
    pub(super) fn holds(&self, mark: &HandlerMark) -> bool {
        self.counts == mark.counts && self.dynamic == mark.dynamic
    }

    /// The report: every table site in table order, then any site the table lacks.
    pub(super) fn snapshot(&self, plan: &ResolvedPlan) -> Vec<HandlerCount> {
        plan.handlers()
            .iter()
            .zip(&self.counts)
            .map(|(site, &count)| HandlerCount {
                spec_path: site.path.to_string(),
                handler: site.kind,
                count,
            })
            .chain(self.dynamic.iter().cloned())
            .collect()
    }
}

impl HandlerMark {
    /// Add the firings `other` counts, one partition's run's to another's
    /// (#3830): each table site's count, then each site the table lacks, in
    /// the order the runs first fired it. Counts of firings stay far below
    /// the integer's range, which no input of the row limit reaches.
    pub(super) fn add(&mut self, other: &HandlerMark) {
        for (count, added) in self.counts.iter_mut().zip(&other.counts) {
            *count = count.saturating_add(*added);
        }
        for entry in &other.dynamic {
            match self
                .dynamic
                .iter_mut()
                .find(|held| held.spec_path == entry.spec_path && held.handler == entry.handler)
            {
                Some(held) => held.count = held.count.saturating_add(entry.count),
                None => self.dynamic.push(entry.clone()),
            }
        }
    }
}
