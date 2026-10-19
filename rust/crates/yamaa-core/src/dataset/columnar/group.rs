//! Partition rows by key values, groups in first-occurrence order and
//! members in row order (REQ-0037/0038/0065). Missing values compare equal;
//! text is exact; temporal precision and float zero signs do not affect
//! equality, as `table_grouping::Key` defines it. Each row is hashed once
//! and compared with its group's first row, so no key is copied.
use crate::table::ValueRef;
use crate::table_grouping::Key;
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::HashMap;
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
/// value at a key position, for `width` positions.
pub(super) fn partition<'v>(
    count: usize,
    width: usize,
    value: impl Fn(usize, usize) -> ValueRef<'v>,
) -> Groups {
    let mut first: HashMap<u64, usize> = HashMap::new();
    // Groups sharing a hash, chained by group.
    let mut chain: Vec<usize> = Vec::new();
    let mut representative: Vec<usize> = Vec::new();
    let mut group_of = Vec::with_capacity(count);
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
    let mut offsets = vec![0usize; groups + 1];
    for &group in &group_of {
        offsets[group + 1] += 1;
    }
    for group in 0..groups {
        offsets[group + 1] += offsets[group];
    }
    let mut cursor = offsets.clone();
    let mut members = vec![0usize; count];
    for (item, &group) in group_of.iter().enumerate() {
        members[cursor[group]] = item;
        cursor[group] += 1;
    }
    Groups { offsets, members }
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
        let groups = partition(values.len(), 1, |item, _| values[item]);
        let groups: Vec<Vec<usize>> = groups.into_vecs();
        assert_eq!(groups, vec![vec![0, 3], vec![1, 4], vec![2, 5], vec![6]]);
    }
}
