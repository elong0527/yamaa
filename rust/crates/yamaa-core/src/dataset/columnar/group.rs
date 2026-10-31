//! Partition frame rows by key values, as `table_grouping::partition_keys`
//! does: groups in first-occurrence order and members in row order (ROW-4).
//! Missing values compare equal; text is exact; temporal precision and
//! float zero signs do not affect equality, as `table_grouping::Key`
//! defines it. A partitioned run (#3830) groups each partition on its own:
//! its partitions never split a group (`ResolvedPlan::cut_fields`).
use crate::table::ValueRef;
pub(super) use crate::table_grouping::Groups;
use crate::table_grouping::Key;
use std::collections::TryReserveError;

/// The admission checks of `table_grouping::partition`, before any value is
/// read: a nonempty list of distinct, bound columns over an admitted table,
/// whole or, for an output a run admits a partition at a time, each of its
/// partitions of at most `partition` rows (D264).
pub(super) fn admit(
    columns: &[usize],
    width: usize,
    rows: usize,
    partition: Option<std::num::NonZeroUsize>,
) -> Result<(), crate::table_grouping::GroupingError<std::convert::Infallible>> {
    use crate::table_grouping::GroupingError;
    if columns.is_empty() {
        return Err(GroupingError::EmptyColumns);
    }
    for (position, &column) in columns.iter().enumerate() {
        if column >= width {
            return Err(GroupingError::InvalidColumn { position, column });
        }
        if columns[..position].contains(&column) {
            return Err(GroupingError::DuplicateColumn { position, column });
        }
    }
    crate::limits::Limits::partitioned(rows, partition, width).map_err(GroupingError::Limit)
}

/// Partition items `0..count`; `value(item, position)` is the item's key
/// value at a key position, for `width` positions, under grouping equality.
pub(super) fn partition<'v>(
    count: usize,
    width: usize,
    value: impl Fn(usize, usize) -> ValueRef<'v>,
) -> Result<Groups, TryReserveError> {
    crate::table_grouping::partition_keys(count, width, |item, position| {
        Key::from(value(item, position))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::FiniteFloat;

    #[test]
    fn groups_follow_first_occurrence_and_key_equality() {
        let values = [
            ValueRef::Int(2),
            ValueRef::Missing,
            ValueRef::Float(FiniteFloat::new(-0.0).unwrap()),
            ValueRef::Int(2),
            ValueRef::Missing,
            ValueRef::Float(FiniteFloat::new(0.0).unwrap()),
            ValueRef::Str("2"),
        ];
        let groups = partition(values.len(), 1, |item, _| values[item]).unwrap();
        let groups: Vec<Vec<usize>> = groups.into_vecs();
        assert_eq!(groups, vec![vec![0, 3], vec![1, 4], vec![2, 5], vec![6]]);
    }
}
