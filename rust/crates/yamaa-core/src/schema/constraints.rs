//! Descriptor constraints run only after the owning type has matched.
//! Findings borrow their value/descriptor context at the caller, rather than
//! multiplying copies of large permitted lists for every invalid document value.

use super::{Descriptor, DocumentNode};
use crate::regex::{MatchBudget, MatchError, MatchLimits, MatchUsage};
use alloc::vec::Vec;

/// Constraint order is values, pattern, minimum length, then exact size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintViolation {
    ValueNotPermitted,
    PatternMismatch,
    MinimumLength,
    InvalidSize,
}

/// Logical validation work and regex matching retain independent request ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintError {
    Work { limit: usize },
    Regex(MatchError),
}

/// Successful charge prefixes survive failed checks; no check refunds them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstraintUsage {
    pub work: usize,
    pub regex: MatchUsage,
}

/// Reuse one scope for every constraint visited during a document validation attempt.
pub struct ConstraintBudget {
    work_limit: usize,
    work: usize,
    regex: MatchBudget,
}
impl ConstraintBudget {
    /// Construct a fresh scope with explicit scalar-comparison and matching ceilings.
    pub fn new(work_limit: usize, regex: MatchLimits) -> Self {
        Self {
            work_limit,
            work: 0,
            regex: MatchBudget::new(regex),
        }
    }
    /// Report cumulative work independently from the regex engine's logical counters.
    pub fn used(&self) -> ConstraintUsage {
        ConstraintUsage {
            work: self.work,
            regex: self.regex.used(),
        }
    }
    /// Reserve work before scanning a string or comparing a permitted choice.
    fn charge(&mut self, amount: usize) -> Result<(), ConstraintError> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|n| *n <= self.work_limit)
            .ok_or(ConstraintError::Work {
                limit: self.work_limit,
            })?;
        Ok(())
    }
}

impl Descriptor {
    /// Check all declared constraints in order, without substituting for type validation.
    /// Resource refusals stop the attempt and never become a mismatch finding.
    pub fn check_constraints(
        &self,
        value: &DocumentNode,
        regex_limits: MatchLimits,
        budget: &mut ConstraintBudget,
    ) -> Result<Vec<ConstraintViolation>, ConstraintError> {
        budget.charge(1)?;
        let mut findings = Vec::new();
        if let Some(permitted) = self.permitted() {
            let mut found = false;
            for choice in permitted {
                budget.charge(1)?;
                if let DocumentNode::Text(text) = value {
                    // Conservative byte-comparison accounting; never rely on a
                    // mismatch occurring early to make the operation affordable.
                    budget.charge(choice.len())?;
                    budget.charge(text.len())?;
                    if text == choice {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                findings.push(ConstraintViolation::ValueNotPermitted);
            }
        }
        if let (Some((_, pattern)), DocumentNode::Text(text)) = (self.pattern(), value) {
            if pattern
                .full_match_with_budget(text, regex_limits, &mut budget.regex)
                .map_err(ConstraintError::Regex)?
                .is_none()
            {
                findings.push(ConstraintViolation::PatternMismatch);
            }
        }
        if self.minimum().is_some() || self.size().is_some() {
            if let DocumentNode::Text(text) = value {
                budget.charge(text.len())?;
            }
            let length = value.length();
            if self.minimum().is_some() && !length.is_some_and(|n| self.length_meets_minimum(n)) {
                findings.push(ConstraintViolation::MinimumLength);
            }
            if self.size().is_some() && !length.is_some_and(|n| self.length_matches_size(n)) {
                findings.push(ConstraintViolation::InvalidSize);
            }
        }
        Ok(findings)
    }
}
