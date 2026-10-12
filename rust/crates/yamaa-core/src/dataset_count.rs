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
