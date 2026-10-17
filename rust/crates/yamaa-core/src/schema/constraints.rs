//! Descriptor constraints run only after the owning type has matched.
//! Findings borrow their value/descriptor context at the caller, rather than
//! multiplying copies of large permitted lists for every invalid document value.

use super::{Descriptor, DocumentNode};

/// Constraint order is values, pattern, minimum length, then exact size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintViolation {
    ValueNotPermitted,
    PatternMismatch,
    MinimumLength,
    InvalidSize,
}

impl Descriptor {
    /// Check all declared constraints in order, without substituting for type validation.
    pub fn check_constraints(&self, value: &DocumentNode) -> Vec<ConstraintViolation> {
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
            if !pattern.is_full_match(text) {
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
        findings
    }
}
