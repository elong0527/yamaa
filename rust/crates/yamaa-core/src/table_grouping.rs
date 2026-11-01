//! Stable, exact partitioning of an immutable normalized table snapshot.

use crate::limits::{LimitExceeded, Limits};
use crate::{
    table::{CellError, ColumnRef, TableAccess, ValueRef},
    temporal::{Date, DateTime},
};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, TryReserveError};
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

/// Hash order is internal: returned groups always follow first source occurrence.
/// Finite float bits suffice for equality after canonicalizing signed zero. Under
/// grouping equality (`From`) types remain distinct; normalized table columns
/// have one declared logical type. Under key equality ([`Key::matching`]) an
/// integral float is the integer it equals.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key<'a> {
    Missing,
    Str(&'a str),
    Int(i64),
    Float(u64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

/// Hash consistent with key equality: temporal values hash their civil
/// fields, as their equality compares only those. Always inlined: a
/// grouping hashes every key of every row, and an out-of-line call writes
/// each key to memory first, which once doubled the output key check's
/// time when more groupings shared the hasher.
impl std::hash::Hash for Key<'_> {
    #[inline(always)]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Missing => (),
            Self::Str(value) => value.hash(state),
            Self::Int(value) => value.hash(state),
            Self::Float(bits) => bits.hash(state),
            Self::Bool(value) => value.hash(state),
            Self::Date(value) => value.fields().hash(state),
            Self::DateTime(value) => value.fields().hash(state),
        }
    }
}

impl<'a> From<ValueRef<'a>> for Key<'a> {
    /// Borrow text and retain logical equality, ignoring temporal precision.
    fn from(value: ValueRef<'a>) -> Self {
        match value {
            ValueRef::Missing => Self::Missing,
            ValueRef::Str(value) => Self::Str(value),
            ValueRef::Int(value) => Self::Int(value),
            ValueRef::Float(value) => Self::Float(if value.get() == 0.0 {
                0
            } else {
                value.get().to_bits()
            }),
            ValueRef::Bool(value) => Self::Bool(value),
            ValueRef::Date(value) => Self::Date(value),
            ValueRef::DateTime(value) => Self::DateTime(value),
        }
    }
}

impl<'a> Key<'a> {
    /// The row hash's state, for a map keyed by keys: aHash under the fixed
    /// seeds, since no output depends on the hasher.
    pub(crate) fn hasher() -> ahash::RandomState {
        ahash::RandomState::with_seeds(SEEDS[0], SEEDS[1], SEEDS[2], SEEDS[3])
    }

    /// `value` under key equality (VALUE-19, LOOKUP-9), as key matching and
    /// an intermediate's `unique` check compare it: numbers by value, so an
    /// integral float within the `int` range is the `Int` it equals, and
    /// every other value as grouping equality has it, a missing value its
    /// own key. Key matching never offers a missing key; `unique` counts two
    /// missing values as one combination (LOOKUP-17).
    pub(crate) fn matching(value: ValueRef<'a>) -> Self {
        if let ValueRef::Float(number) = value {
            let number = number.get();
            if number == number.trunc()
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&number)
            {
                return Self::Int(number as i64);
            }
        }
        Self::from(value)
    }
}

/// A [`Key`] that owns its text, with the same equality and order: what a
/// reader keeps of a key once the records it was read from are dropped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OwnedKey {
    Missing,
    Str(Box<str>),
    Int(i64),
    Float(u64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl From<Key<'_>> for OwnedKey {
    fn from(key: Key<'_>) -> Self {
        match key {
            Key::Missing => Self::Missing,
            Key::Str(value) => Self::Str(value.into()),
            Key::Int(value) => Self::Int(value),
            Key::Float(bits) => Self::Float(bits),
            Key::Bool(value) => Self::Bool(value),
            Key::Date(value) => Self::Date(value),
            Key::DateTime(value) => Self::DateTime(value),
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum GroupingError<E> {
    #[error("table grouping empty columns")]
    EmptyColumns,
    #[error("table grouping invalid column: position={position:?}, column={column:?}")]
    InvalidColumn { position: usize, column: usize },
    #[error("table grouping duplicate column: position={position:?}, column={column:?}")]
    DuplicateColumn { position: usize, column: usize },
    #[error("table grouping boundary: {0}")]
    Limit(LimitExceeded),
    #[error("table grouping allocation")]
    Allocation,
    #[error("table grouping cell: row={row:?}, column={column:?}, error={error:?}")]
    Cell {
        row: usize,
        column: usize,
        error: CellError<E>,
    },
}

/// Partition all rows by bound columns (ROW-4). Missing values
/// compare equal; text is exact; temporal precision and float zero signs do not
/// affect equality. Each member list is strictly increasing for ordered folds.
/// Admission validates the complete column list and table geometry before any
/// cell access. No source row is sorted and no host is called; the hash of
/// [`partition_keys`] only finds a row's candidate groups.
pub fn partition<T: TableAccess + ?Sized>(
    table: &T,
    columns: &[usize],
) -> Result<Vec<Vec<usize>>, GroupingError<T::Error>> {
    partition_by(table, columns, Key::from)
}

/// Partition all rows as [`partition`] does, comparing each value under key
/// equality ([`Key::matching`]): an intermediate's `unique` check groups the
/// combinations key matching would meet, and missing values still compare
/// equal (LOOKUP-17).
pub(crate) fn partition_matching<T: TableAccess + ?Sized>(
    table: &T,
    columns: &[usize],
) -> Result<Vec<Vec<usize>>, GroupingError<T::Error>> {
    partition_by(table, columns, Key::matching)
}

/// The one partition both equalities share: each row's key is `key_of` of
/// each of its cells.
fn partition_by<'t, T: TableAccess + ?Sized>(
    table: &'t T,
    columns: &[usize],
    key_of: fn(ValueRef<'t>) -> Key<'t>,
) -> Result<Vec<Vec<usize>>, GroupingError<T::Error>> {
    if columns.is_empty() {
        return Err(GroupingError::EmptyColumns);
    }
    for (position, &column) in columns.iter().enumerate() {
        if column >= table.schema().columns().len() {
            return Err(GroupingError::InvalidColumn { position, column });
        }
        if columns[..position].contains(&column) {
            return Err(GroupingError::DuplicateColumn { position, column });
        }
    }
    let rows = table.row_count();
    Limits::table(rows, table.schema().columns().len()).map_err(GroupingError::Limit)?;
    // Each cell is read once, in row order, into one flat list of keys, so
    // the first unreadable cell fails the partition before any grouping.
    let width = columns.len();
    let mut keys = Vec::new();
    keys.try_reserve_exact(rows.saturating_mul(width))
        .map_err(|_| GroupingError::Allocation)?;
    for row in 0..rows {
        for &column in columns {
            keys.push(key_of(
                table
                    .cell(row, column)
                    .map_err(|error| GroupingError::Cell { row, column, error })?,
            ));
        }
    }
    let groups = partition_keys(rows, width, |row, position| {
        keys[row * width + position].clone()
    })
    .map_err(|_| GroupingError::Allocation)?;
    Ok(groups.into_vecs())
}

/// The fixed seeds of the row hash: any constants serve, as no output
/// depends on them.
const SEEDS: [u64; 4] = [
    0x243f_6a88_85a3_08d3,
    0x1319_8a2e_0370_7344,
    0xa409_3822_299f_31d0,
    0x082e_fa98_ec4e_6c89,
];

/// The hasher of the map from a row hash to its first group: the key is
/// already a well-mixed hash, so it passes through.
#[derive(Default)]
struct Prehashed(u64);

impl Hasher for Prehashed {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("the map's key is a row hash")
    }

    fn write_u64(&mut self, hash: u64) {
        self.0 = hash;
    }
}

/// Groups as offsets into one member list.
pub(crate) struct Groups {
    offsets: Vec<usize>,
    members: Vec<usize>,
}

impl Groups {
    pub(crate) fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub(crate) fn group(&self, group: usize) -> &[usize] {
        &self.members[self.offsets[group]..self.offsets[group + 1]]
    }

    /// The groups of items `0..group_of.len()`, `group_of[item]` the group of
    /// each item among `groups` groups: members in item order.
    pub(crate) fn from_assignment(
        group_of: &[usize],
        groups: usize,
    ) -> Result<Self, TryReserveError> {
        let mut offsets = Vec::new();
        offsets.try_reserve_exact(groups + 1)?;
        // Groups that are runs of consecutive items, numbered in item order,
        // as items in key order form them, hold the items in order: each
        // group starts where the number moves on.
        if group_of.first().is_none_or(|&group| group == 0)
            && group_of.last().map_or(0, |&group| group + 1) == groups
            && group_of
                .windows(2)
                .all(|pair| pair[1] == pair[0] || pair[1] == pair[0] + 1)
        {
            let mut members = Vec::new();
            members.try_reserve_exact(group_of.len())?;
            members.extend(0..group_of.len());
            offsets.extend(
                (0..group_of.len())
                    .filter(|&item| item == 0 || group_of[item] != group_of[item - 1]),
            );
            offsets.push(group_of.len());
            return Ok(Self { offsets, members });
        }
        // Otherwise each group's items are counted, then placed in item
        // order; a run of consecutive items of one group, as groups merged
        // from runs hold them, is counted and placed at once.
        offsets.resize(groups + 1, 0usize);
        for (group, items) in group_runs(group_of) {
            offsets[group + 1] += items.len();
        }
        for group in 0..groups {
            offsets[group + 1] += offsets[group];
        }
        let mut cursor = Vec::new();
        cursor.try_reserve_exact(groups + 1)?;
        cursor.extend_from_slice(&offsets);
        let mut members = Vec::new();
        members.try_reserve_exact(group_of.len())?;
        members.resize(group_of.len(), 0usize);
        for (group, items) in group_runs(group_of) {
            let at = cursor[group];
            cursor[group] += items.len();
            for (member, item) in members[at..at + items.len()].iter_mut().zip(items) {
                *member = item;
            }
        }
        Ok(Self { offsets, members })
    }

    /// One group of every item `0..count`, empty when there is no item.
    pub(crate) fn whole(count: usize) -> Self {
        Self {
            offsets: vec![0, count],
            members: (0..count).collect(),
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &[usize]> + '_ {
        (0..self.len()).map(|group| self.group(group))
    }

    /// The groups as owned member lists.
    pub(crate) fn into_vecs(self) -> Vec<Vec<usize>> {
        (0..self.len())
            .map(|group| self.group(group).to_vec())
            .collect()
    }
}

/// Each run of consecutive items of one group, in item order: its group and
/// its items.
fn group_runs(group_of: &[usize]) -> impl Iterator<Item = (usize, std::ops::Range<usize>)> + '_ {
    let mut start = 0;
    std::iter::from_fn(move || {
        let group = *group_of.get(start)?;
        let end = group_of[start..]
            .iter()
            .position(|&other| other != group)
            .map_or(group_of.len(), |length| start + length);
        let run = (group, start..end);
        start = end;
        Some(run)
    })
}

/// Partition items `0..count`; `key(item, position)` is the item's key at a
/// key position, for `width` positions. Groups follow first occurrence and
/// members item order. No key is copied. The buffers sized by the item count
/// are reserved first.
///
/// Items whose keys never decrease, as records sorted by their keys arrive,
/// group without hashing ([`assign_keys`]); any others hash. Either way the
/// groups are the same, so no output depends on the path or on the hasher.
pub(crate) fn partition_keys<'v>(
    count: usize,
    width: usize,
    key: impl Fn(usize, usize) -> Key<'v>,
) -> Result<Groups, TryReserveError> {
    let (group_of, groups) = assign_keys(count, width, key)?;
    Groups::from_assignment(&group_of, groups)
}

/// The group of each item `0..count` that [`partition_keys`] forms, and how
/// many groups there are.
///
/// Each item's key is first compared with the one before, position by
/// position, in [`Key`]'s order, which agrees with its equality: two keys
/// compare equal exactly when they are equal. While no key is less than the
/// one before, a key greater than the one before is greater than every
/// earlier key, so each run of equal keys is a group of its own, the runs
/// follow first occurrence, and no hash table is built. At the first key
/// less than the one before, the groups found so far, each at its first
/// item, seed a hash table and the remaining items hash: the groups a hash
/// of every item finds.
pub(crate) fn assign_keys<'v>(
    count: usize,
    width: usize,
    key: impl Fn(usize, usize) -> Key<'v>,
) -> Result<(Vec<usize>, usize), TryReserveError> {
    let mut group_of = Vec::new();
    group_of.try_reserve_exact(count)?;
    let runs = ascending(count, width, &key, &mut group_of)?;
    let start = group_of.len();
    if start == count {
        return Ok((group_of, runs));
    }
    let mut hashed = Hashed::new(count, width, &key)?;
    // Each run takes its own number again, at its first item: no two runs
    // are equal.
    for item in 0..start {
        if item == 0 || group_of[item] != group_of[item - 1] {
            hashed.group(item);
        }
    }
    for item in start..count {
        group_of.push(hashed.group(item));
    }
    Ok((group_of, hashed.representative.len()))
}

/// Push onto `group_of` the run of equal keys of each item before the first
/// whose key is less than the one before it, and answer how many runs there
/// are.
fn ascending<'v>(
    count: usize,
    width: usize,
    key: &impl Fn(usize, usize) -> Key<'v>,
    group_of: &mut Vec<usize>,
) -> Result<usize, TryReserveError> {
    if count == 0 {
        return Ok(0);
    }
    let mut previous = Vec::new();
    previous.try_reserve_exact(width)?;
    previous.extend((0..width).map(|position| key(0, position)));
    group_of.push(0);
    let mut runs = 1;
    for item in 1..count {
        let mut order = std::cmp::Ordering::Equal;
        for (position, kept) in previous.iter_mut().enumerate() {
            let current = key(item, position);
            if order.is_eq() {
                order = current.cmp(kept);
                if order.is_lt() {
                    return Ok(runs);
                }
            }
            *kept = current;
        }
        if order.is_gt() {
            runs += 1;
        }
        group_of.push(runs - 1);
    }
    Ok(runs)
}

/// The group of each of `rows` of `columns`, numbered from the first row,
/// and how many groups there are, when the rows' keys never decrease: the
/// groups [`assign_keys`] forms, read a column at a time, so each column's
/// storage is read in one pass, each row compared with the one before in
/// [`Key`]'s order only while the columns before were equal (#4211). None at
/// the first row whose key is less than the one before it. Every row must
/// lie within every column.
pub(crate) fn ascending_columns(
    columns: &[ColumnRef<'_>],
    rows: std::ops::Range<usize>,
) -> Result<Option<(Vec<usize>, usize)>, TryReserveError> {
    let count = rows.len();
    if count == 0 {
        return Ok(Some((Vec::new(), 0)));
    }
    // Whether each row's key equals the one before it in the columns read
    // so far; the first row begins a group.
    let mut equal = Vec::new();
    equal.try_reserve_exact(count)?;
    equal.resize(count, true);
    equal[0] = false;
    for column in columns {
        let mut previous = Key::from(column.get(rows.start));
        for (index, row) in rows.clone().enumerate().skip(1) {
            let current = Key::from(column.get(row));
            if equal[index] {
                match current.cmp(&previous) {
                    std::cmp::Ordering::Less => return Ok(None),
                    std::cmp::Ordering::Greater => equal[index] = false,
                    std::cmp::Ordering::Equal => {}
                }
            }
            previous = current;
        }
    }
    let mut group_of = Vec::new();
    group_of.try_reserve_exact(count)?;
    let mut groups = 0;
    for same in equal {
        groups += usize::from(!same);
        group_of.push(groups - 1);
    }
    Ok(Some((group_of, groups)))
}

/// The hash grouping of items: each item is hashed once and compared with
/// the first item of each group that shares its hash.
///
/// The hash only finds an item's candidate groups; equality decides, and
/// the groups follow first occurrence, so no output depends on the hasher.
/// It is aHash under fixed seeds rather than `std`'s SipHash: no key comes
/// from an adversary the engine must resist, and grouping is on every
/// run's path.
struct Hashed<'k, K> {
    /// The first group of each hash, reserved for every item.
    first: HashMap<u64, usize, BuildHasherDefault<Prehashed>>,
    state: ahash::RandomState,
    /// Groups sharing a hash, chained by group.
    chain: Vec<usize>,
    /// The first item of each group.
    representative: Vec<usize>,
    width: usize,
    /// Borrowed, so a key is the closure's own call, which inlines.
    key: &'k K,
}

impl<'k, 'v, K: Fn(usize, usize) -> Key<'v>> Hashed<'k, K> {
    fn new(count: usize, width: usize, key: &'k K) -> Result<Self, TryReserveError> {
        let mut first: HashMap<u64, usize, BuildHasherDefault<Prehashed>> = HashMap::default();
        first.try_reserve(count)?;
        Ok(Self {
            first,
            state: ahash::RandomState::with_seeds(SEEDS[0], SEEDS[1], SEEDS[2], SEEDS[3]),
            chain: Vec::new(),
            representative: Vec::new(),
            width,
            key,
        })
    }

    /// The group of `item`: a new one when no earlier item equals it.
    fn group(&mut self, item: usize) -> usize {
        let (key, width) = (self.key, self.width);
        let mut hasher = self.state.build_hasher();
        for position in 0..width {
            key(item, position).hash(&mut hasher);
        }
        let hash = hasher.finish();
        let equal = |left: usize, right: usize| {
            (0..width).all(|position| key(left, position) == key(right, position))
        };
        match self.first.entry(hash) {
            Entry::Vacant(entry) => {
                let group = self.representative.len();
                entry.insert(group);
                self.representative.push(item);
                self.chain.push(usize::MAX);
                group
            }
            Entry::Occupied(entry) => {
                let mut group = *entry.get();
                loop {
                    if equal(self.representative[group], item) {
                        break group;
                    }
                    if self.chain[group] == usize::MAX {
                        let created = self.representative.len();
                        self.representative.push(item);
                        self.chain.push(usize::MAX);
                        self.chain[group] = created;
                        break created;
                    }
                    group = self.chain[group];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // The seam is the crate-private grouping core (#4211): items in key
    // order group as runs without hashing, items out of order hash, and
    // either way the groups are ROW-4's, written here by hand: groups in
    // first-occurrence order, members in item order (#4209: counted and
    // placed a run at a time), a missing value a key of its own, the float
    // zero signs and a date's precision ignored.
    use super::*;
    use crate::table::ColumnValues;
    use crate::temporal::Precision;
    use crate::value::FiniteFloat;

    /// The groups of one key per item, by both paths of [`assign_keys`].
    fn groups(keys: &[ValueRef<'_>]) -> (Vec<usize>, usize) {
        assign_keys(keys.len(), 1, |item, _| Key::from(keys[item])).unwrap()
    }

    #[test]
    fn items_group_as_runs_in_key_order_and_by_first_occurrence_out_of_it() {
        let zero = |value: f64| ValueRef::Float(FiniteFloat::new(value).unwrap());
        let day = |precision| ValueRef::Date(Date::new(2024, 2, 29, precision).unwrap());
        let cases = [
            (
                "in key order, missing first",
                vec![
                    ValueRef::Missing,
                    ValueRef::Missing,
                    ValueRef::Int(1),
                    ValueRef::Int(1),
                    ValueRef::Int(2),
                ],
                (vec![0, 0, 1, 1, 2], 3),
            ),
            (
                "in key order, equal under grouping equality",
                vec![zero(-0.0), zero(0.0), zero(1.5)],
                (vec![0, 0, 1], 2),
            ),
            (
                "a date's precision ignored",
                vec![day(Precision::Year), day(Precision::Day)],
                (vec![0, 0], 1),
            ),
            (
                "in key order, then a decrease",
                vec![
                    ValueRef::Int(1),
                    ValueRef::Int(2),
                    ValueRef::Int(2),
                    ValueRef::Int(3),
                    ValueRef::Int(1),
                    ValueRef::Int(3),
                    ValueRef::Int(4),
                ],
                (vec![0, 1, 1, 2, 0, 2, 3], 4),
            ),
        ];
        for (label, keys, expected) in cases {
            assert_eq!(groups(&keys), expected, "{label}");
        }
    }

    #[test]
    fn group_members_follow_item_order_whether_groups_are_runs_or_interleave() {
        // A label, each item's group, how many groups, and their members.
        type Case = (&'static str, &'static [usize], usize, Vec<Vec<usize>>);
        let cases: [Case; 4] = [
            (
                "runs numbered in item order",
                &[0, 0, 1, 2, 2],
                3,
                vec![vec![0, 1], vec![2], vec![3, 4]],
            ),
            (
                "runs of groups that recur",
                &[0, 0, 1, 1, 0, 0, 2],
                3,
                vec![vec![0, 1, 4, 5], vec![2, 3], vec![6]],
            ),
            (
                "single items that alternate",
                &[0, 1, 0, 1],
                2,
                vec![vec![0, 2], vec![1, 3]],
            ),
            ("no item", &[], 0, vec![]),
        ];
        for (label, group_of, groups, expected) in cases {
            assert_eq!(
                Groups::from_assignment(group_of, groups)
                    .unwrap()
                    .into_vecs(),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn columns_in_key_order_group_as_runs_and_out_of_order_give_way() {
        // Rows (1, a), (1, a), (1, b), (2, missing), (2, missing); then the
        // second column decreasing while the first stays equal.
        let words = [0b00111u64];
        let ids = ColumnRef::new(ColumnValues::Int(&[1, 1, 1, 2, 2]), None).unwrap();
        let text = ColumnRef::new(
            ColumnValues::Str {
                offsets: &[0, 1, 2, 3, 3, 3],
                bytes: "aab",
            },
            Some(&words),
        )
        .unwrap();
        assert_eq!(
            ascending_columns(&[ids, text], 0..5).unwrap(),
            Some((vec![0, 0, 1, 2, 2], 3)),
            "in key order"
        );
        assert_eq!(
            ascending_columns(&[ids, text], 1..4).unwrap(),
            Some((vec![0, 1, 2], 3)),
            "rows 1 to 3, numbered from the first"
        );
        let reversed = ColumnRef::new(
            ColumnValues::Str {
                offsets: &[0, 1, 2, 3, 3, 3],
                bytes: "baa",
            },
            Some(&words),
        )
        .unwrap();
        assert_eq!(
            ascending_columns(&[ids, reversed], 0..5).unwrap(),
            None,
            "b before a under an equal first column"
        );
    }
}
