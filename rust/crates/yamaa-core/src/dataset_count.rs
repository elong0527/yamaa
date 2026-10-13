//! Exact authored count bounds, independent of machine integer width.
use alloc::string::String;
use core::cmp::Ordering;

/// Admission policy for each canonical decimal count bound.
pub const MAX_COUNT_BOUND_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountBoundError {
    Invalid,
    Limit,
}

/// A bounded canonical integer whose mathematical value need not fit i64.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowCountBound(String);

impl RowCountBound {
    pub fn new(text: &str) -> Result<Self, CountBoundError> {
        if text.len() > MAX_COUNT_BOUND_BYTES {
            return Err(CountBoundError::Limit);
        }
        if !crate::schema::canonical_integer(text) {
            return Err(CountBoundError::Invalid);
        }
        Ok(Self(text.into()))
    }

    pub fn decimal(&self) -> &str {
        &self.0
    }

    /// Compare the bound to a count without narrowing or allocating a bigint.
    pub fn compare_count(&self, mut count: usize) -> Ordering {
        let mut digits = [0; core::mem::size_of::<usize>() * 3];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (count % 10) as u8;
            count /= 10;
            if count == 0 {
                break;
            }
        }
        if self.0.starts_with('-') {
            Ordering::Less
        } else {
            magnitude(self.0.as_bytes(), &digits[start..])
        }
    }
}

fn magnitude(a: &[u8], b: &[u8]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

impl Ord for RowCountBound {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.0.strip_prefix('-'), other.0.strip_prefix('-')) {
            (Some(a), Some(b)) => magnitude(b.as_bytes(), a.as_bytes()),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => magnitude(self.0.as_bytes(), other.0.as_bytes()),
        }
    }
}

impl PartialOrd for RowCountBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Inclusive bounds and row predicates over one completed output relation.
#[derive(Clone, Debug, PartialEq)]
pub struct RowCount {
    pub min: Option<RowCountBound>,
    pub max: Option<RowCountBound>,
    pub min_fraction: Option<f64>,
    pub max_fraction: Option<f64>,
    /// None denotes the one whole-artifact group, even for an empty output.
    pub group_by: Option<alloc::vec::Vec<usize>>,
    pub filter: Option<crate::bound_expression::BoundPredicate>,
    pub when: Option<crate::bound_expression::BoundPredicate>,
}
impl RowCount {
    pub(crate) fn validate(&self, width: usize) -> Result<(), super::PlanError> {
        use super::PlanError;
        if [self.min_fraction, self.max_fraction]
            .into_iter()
            .flatten()
            .any(|n| !n.is_finite() || !(0.0..=1.0).contains(&n))
            || (self.min.is_none()
                && self.max.is_none()
                && self.min_fraction.is_none()
                && self.max_fraction.is_none())
            || self
                .min
                .as_ref()
                .zip(self.max.as_ref())
                .is_some_and(|(a, b)| a > b)
            || self
                .min_fraction
                .zip(self.max_fraction)
                .is_some_and(|(a, b)| a > b)
        {
            return Err(PlanError::InvalidBounds);
        }
        if let Some(groups) = &self.group_by {
            if groups.is_empty()
                || groups
                    .iter()
                    .enumerate()
                    .any(|(i, &c)| c >= width || groups[..i].contains(&c))
            {
                return Err(PlanError::InvalidColumns);
            }
        }
        for predicate in self.filter.iter().chain(&self.when) {
            predicate
                .validate(0, &alloc::vec![true;width], true)
                .map_err(PlanError::Predicate)?;
        }
        Ok(())
    }
    pub fn accepts(&self, count: usize, total: usize) -> bool {
        let fraction = if total == 0 {
            0.0
        } else {
            count as f64 / total as f64
        };
        self.min
            .as_ref()
            .is_none_or(|min| min.compare_count(count) != Ordering::Greater)
            && self
                .max
                .as_ref()
                .is_none_or(|max| max.compare_count(count) != Ordering::Less)
            && self.min_fraction.is_none_or(|min| fraction >= min)
            && self.max_fraction.is_none_or(|max| fraction <= max)
    }
    pub fn bound_work(&self) -> usize {
        self.min
            .iter()
            .chain(&self.max)
            .map(|n| n.decimal().len())
            .sum::<usize>()
            .max(1)
    }
}
