//! Partition rows by key values, groups in first-occurrence order and
//! members in row order (ROW-4). Missing values compare equal;
//! text is exact; temporal precision and float zero signs do not affect
//! equality, as `table_grouping::Key` defines it. Each row is hashed once
//! and compared with its group's first row, so no key is copied. A
//! partitioned run (#3697) groups each partition, then merges the
//! partitions' groups: the key groups are its combine state.
use crate::table::ValueRef;
use crate::table_grouping::Key;
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{HashMap, TryReserveError};
use std::hash::{Hash, Hasher};

/// Groups as offsets into one member list.
pub(super) struct Groups {
    offsets: Vec<usize>,
    members: Vec<usize>,
}

impl Groups {
    pub(super) fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub(super) fn group(&self, group: usize) -> &[usize] {
        &self.members[self.offsets[group]..self.offsets[group + 1]]
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &[usize]> + '_ {
        (0..self.len()).map(|group| self.group(group))
    }

    /// The groups as owned member lists.
    pub(super) fn into_vecs(self) -> Vec<Vec<usize>> {
        (0..self.len())
            .map(|group| self.group(group).to_vec())
            .collect()
    }
}

/// The admission checks of `table_grouping::partition`, before any value is
/// read: a nonempty list of distinct, bound columns over an admitted table.
pub(super) fn admit(
    columns: &[usize],
    width: usize,
    rows: usize,
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
    crate::limits::Limits::table(rows, width).map_err(GroupingError::Limit)
}

/// Partition items `0..count`; `value(item, position)` is the item's key
/// value at a key position, for `width` positions. The buffers sized by the
/// item count are reserved first.
pub(super) fn partition<'v>(
    count: usize,
    width: usize,
    value: impl Fn(usize, usize) -> ValueRef<'v>,
) -> Result<Groups, TryReserveError> {
    let mut first: HashMap<u64, usize> = HashMap::new();
    // Groups sharing a hash, chained by group.
    let mut chain: Vec<usize> = Vec::new();
    let mut representative: Vec<usize> = Vec::new();
    let mut group_of = Vec::new();
    group_of.try_reserve_exact(count)?;
    let mut members = Vec::new();
    members.try_reserve_exact(count)?;
    let equal = |left: usize, right: usize| {
        (0..width)
            .all(|position| Key::from(value(left, position)) == Key::from(value(right, position)))
    };
    for item in 0..count {
        let mut hasher = DefaultHasher::new();
        for position in 0..width {
            Key::from(value(item, position)).hash(&mut hasher);
        }
        let hash = hasher.finish();
        let group = match first.entry(hash) {
            Entry::Vacant(entry) => {
                let group = representative.len();
                entry.insert(group);
                representative.push(item);
                chain.push(usize::MAX);
                group
            }
            Entry::Occupied(entry) => {
                let mut group = *entry.get();
                loop {
                    if equal(representative[group], item) {
                        break group;
                    }
                    if chain[group] == usize::MAX {
                        let created = representative.len();
                        representative.push(item);
                        chain.push(usize::MAX);
                        chain[group] = created;
                        break created;
                    }
                    group = chain[group];
                }
            }
        };
        group_of.push(group);
    }
    let groups = representative.len();
    let mut offsets = Vec::new();
    offsets.try_reserve_exact(groups + 1)?;
    offsets.resize(groups + 1, 0usize);
    for &group in &group_of {
        offsets[group + 1] += 1;
    }
    for group in 0..groups {
        offsets[group + 1] += offsets[group];
    }
    let mut cursor = offsets.clone();
    members.resize(count, 0usize);
    for (item, &group) in group_of.iter().enumerate() {
        members[cursor[group]] = item;
        cursor[group] += 1;
    }
    Ok(Groups { offsets, members })
}

/// The combine step of key groups across partitions (#3697): merge the
/// groups of consecutive partitions, each formed by [`partition`] over its
/// own items and naming them by their position in the whole input, into the
/// groups of the whole input. Equal keys, as `table_grouping::Key` compares
/// them, join one group across partition boundaries; a group keeps the place
/// of its first occurrence, and its members stay in item order because later
/// partitions hold later items. `value(item, position)` reads an item's key
/// value at a key position, for `width` positions.
pub(super) fn merge<'v>(
    parts: &[Vec<Vec<usize>>],
    width: usize,
    value: impl Fn(usize, usize) -> ValueRef<'v>,
) -> Result<Groups, TryReserveError> {
    let mut first: HashMap<u64, usize> = HashMap::new();
    let mut chain: Vec<usize> = Vec::new();
    let mut merged: Vec<Vec<usize>> = Vec::new();
    let equal = |left: usize, right: usize| {
        (0..width)
            .all(|position| Key::from(value(left, position)) == Key::from(value(right, position)))
    };
    for groups in parts {
        for members in groups {
            let representative = members[0];
            let mut hasher = DefaultHasher::new();
            for position in 0..width {
                Key::from(value(representative, position)).hash(&mut hasher);
            }
            let group = match first.entry(hasher.finish()) {
                Entry::Vacant(entry) => {
                    entry.insert(merged.len());
                    chain.push(usize::MAX);
                    merged.try_reserve(1)?;
                    merged.push(Vec::new());
                    merged.len() - 1
                }
                Entry::Occupied(entry) => {
                    let mut group = *entry.get();
                    loop {
                        if equal(merged[group][0], representative) {
                            break group;
                        }
                        if chain[group] == usize::MAX {
                            chain.push(usize::MAX);
                            merged.try_reserve(1)?;
                            merged.push(Vec::new());
                            chain[group] = merged.len() - 1;
                            break merged.len() - 1;
                        }
                        group = chain[group];
                    }
                }
            };
            merged[group].try_reserve(members.len())?;
            merged[group].extend_from_slice(members);
        }
    }
    let mut offsets = Vec::new();
    offsets.try_reserve_exact(merged.len() + 1)?;
    offsets.push(0);
    let mut members = Vec::new();
    members.try_reserve_exact(merged.iter().map(Vec::len).sum())?;
    for group in merged {
        members.extend(group);
        offsets.push(members.len());
    }
    Ok(Groups { offsets, members })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::FiniteFloat;

    /// Groups formed over each partition and merged equal the groups of
    /// the whole input, for every way of dividing it: a group that spans
    /// partitions keeps its first occurrence's place and every member in
    /// order, and equality is `Key`'s (missing equal to missing, float zero
    /// signs equal, text exact) across partition boundaries too.
    #[test]
    fn merged_partition_groups_equal_the_groups_of_the_whole_input() {
        let values = [
            ValueRef::Int(2),
            ValueRef::Missing,
            ValueRef::Float(FiniteFloat::new(-0.0).unwrap()),
            ValueRef::Int(2),
            ValueRef::Missing,
            ValueRef::Float(FiniteFloat::new(0.0).unwrap()),
            ValueRef::Str("2"),
            ValueRef::Int(2),
        ];
        // Authored: first occurrences 0, 1, 2 and 6, members in item order.
        let whole = vec![vec![0, 3, 7], vec![1, 4], vec![2, 5], vec![6]];
        for bounds in [
            vec![0, 8],
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            vec![0, 3, 3, 8],
            vec![0, 5, 8],
            vec![0, 0, 2, 7, 8, 8],
        ] {
            let parts: Vec<Vec<Vec<usize>>> = bounds
                .windows(2)
                .map(|range| {
                    let (start, end) = (range[0], range[1]);
                    partition(end - start, 1, |item, _| values[start + item])
                        .unwrap()
                        .into_vecs()
                        .into_iter()
                        .map(|members| members.into_iter().map(|item| start + item).collect())
                        .collect()
                })
                .collect();
            let merged = merge(&parts, 1, |item, _| values[item]).unwrap();
            assert_eq!(merged.into_vecs(), whole, "partition bounds {bounds:?}");
        }
    }

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
