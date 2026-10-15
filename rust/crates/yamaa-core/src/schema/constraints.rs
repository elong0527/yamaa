//! Descriptor constraints run only after the owning type has matched.
//! Findings borrow their value/descriptor context at the caller, rather than
//! multiplying copies of large permitted lists for every invalid document value.

use super::{Descriptor, DocumentNode};
use crate::regex::MatchError;

/// Constraint order is values, pattern, minimum length, then exact size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintViolation {
    ValueNotPermitted,
    PatternMismatch,
    MinimumLength,
    InvalidSize,
}

/// Logical validation work and regex matching retain independent request ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConstraintError {
    #[error("schema constraints regex: {0:?}")]
    Regex(MatchError),
}

impl Descriptor {
    /// Check all declared constraints in order, without substituting for type validation.
    /// Resource refusals stop the attempt and never become a mismatch finding.
    pub fn check_constraints(
        &self,
        value: &DocumentNode,
    ) -> Result<Vec<ConstraintViolation>, ConstraintError> {
        let mut findings = Vec::new();
        if let Some(permitted) = self.permitted() {
            let mut found = false;
            for choice in permitted {
                if let DocumentNode::Text(text) = value {
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
                .full_match(text)
                .map_err(ConstraintError::Regex)?
                .is_none()
            {
                findings.push(ConstraintViolation::PatternMismatch);
            }
        }
        if self.minimum().is_some() || self.size().is_some() {
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
