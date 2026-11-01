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

/// The group of each item `0..count`, as [`partition`] forms the groups,
/// and how many groups there are.
pub(super) fn assign<'v>(
    count: usize,
    width: usize,
    value: impl Fn(usize, usize) -> ValueRef<'v>,
) -> Result<(Vec<usize>, usize), TryReserveError> {
    crate::table_grouping::assign_keys(count, width, |item, position| {
        Key::from(value(item, position))
    })
}

/// The groups of items `0..count`, as [`partition`] forms them, and the
/// group of each item.
pub(super) struct Grouping {
    pub(super) groups: Groups,
    pub(super) group_of: Vec<usize>,
}

impl Grouping {
    /// Every item in one group.
    pub(super) fn whole(count: usize) -> Result<Self, TryReserveError> {
        let mut group_of = Vec::new();
        group_of.try_reserve_exact(count)?;
        group_of.resize(count, 0);
        Ok(Self {
            groups: Groups::whole(count),
            group_of,
        })
    }

    /// Partition items `0..count` as [`partition`] does.
    pub(super) fn partition<'v>(
        count: usize,
        width: usize,
        value: impl Fn(usize, usize) -> ValueRef<'v>,
    ) -> Result<Self, TryReserveError> {
        let (group_of, groups) = assign(count, width, value)?;
        Ok(Self {
            groups: Groups::from_assignment(&group_of, groups)?,
            group_of,
        })
    }

    /// The coarser grouping of the same items by fewer keys: `value(item,
    /// position)` is the item's value at one of the fewer key positions, of
    /// `width`, which every item of one of this grouping's groups shares.
    /// Only each group's first item is read: groups that share their values
    /// merge, in the order of their first items, which is first occurrence,
    /// and members stay in item order, as [`Grouping::partition`] over the
    /// fewer keys would answer.
    pub(super) fn coarsen<'v>(
        &self,
        width: usize,
        value: impl Fn(usize, usize) -> ValueRef<'v>,
    ) -> Result<Self, TryReserveError> {
        let firsts = |group: usize| self.groups.group(group)[0];
        let (merged, groups) =
            crate::table_grouping::assign_keys(self.groups.len(), width, |group, position| {
                Key::from(value(firsts(group), position))
            })?;
        let mut group_of = Vec::new();
        group_of.try_reserve_exact(self.group_of.len())?;
        group_of.extend(self.group_of.iter().map(|&group| merged[group]));
        Ok(Self {
            groups: Groups::from_assignment(&group_of, groups)?,
            group_of,
        })
    }
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

    #[test]
    fn a_grouping_merged_into_fewer_keys_is_the_grouping_by_them() {
        // ROW-4 over the first key alone, derived by hand: the int 1 and
        // the missing value each a group of their own, apart from the float
        // 1.0, in first-occurrence order and members in item order. The
        // second key splits them further, so the first grouping has five
        // groups for the merge to read.
        let keys = [
            (ValueRef::Int(1), ValueRef::Str("A")),
            (ValueRef::Missing, ValueRef::Str("A")),
            (ValueRef::Int(1), ValueRef::Str("B")),
            (
                ValueRef::Float(FiniteFloat::new(1.0).unwrap()),
                ValueRef::Str("A"),
            ),
            (ValueRef::Int(1), ValueRef::Str("A")),
            (ValueRef::Missing, ValueRef::Str("B")),
        ];
        let both = Grouping::partition(keys.len(), 2, |item, position| match position {
            0 => keys[item].0,
            _ => keys[item].1,
        })
        .unwrap();
        assert_eq!(both.groups.len(), 5, "both keys");
        let first = both.coarsen(1, |item, _| keys[item].0).unwrap();
        let expected = (
            vec![vec![0, 2, 4], vec![1, 5], vec![3]],
            vec![0, 1, 0, 2, 0, 1],
        );
        assert_eq!(
            (first.groups.into_vecs(), first.group_of),
            expected,
            "the first key, merged"
        );
    }
}
