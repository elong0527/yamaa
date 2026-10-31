//! Stable, exact partitioning of an immutable normalized table snapshot.

use crate::limits::{LimitExceeded, Limits};
use crate::{
    table::{CellError, TableAccess, ValueRef},
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
/// fields, as their equality compares only those.
impl std::hash::Hash for Key<'_> {
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

impl OwnedKey {
    /// The key this owns.
    pub(crate) fn key(&self) -> Key<'_> {
        match self {
            Self::Missing => Key::Missing,
            Self::Str(value) => Key::Str(value),
            Self::Int(value) => Key::Int(*value),
            Self::Float(bits) => Key::Float(*bits),
            Self::Bool(value) => Key::Bool(*value),
            Self::Date(value) => Key::Date(*value),
            Self::DateTime(value) => Key::DateTime(*value),
        }
    }
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

/// Partition items `0..count`; `key(item, position)` is the item's key at a
/// key position, for `width` positions. Groups follow first occurrence and
/// members item order. Each item is hashed once and compared with its
/// group's first item, so no key is copied. The buffers sized by the item
/// count are reserved first.
///
/// The hash only finds an item's candidate groups; equality decides, and the
/// groups follow first occurrence, so no output depends on the hasher. It is
/// aHash under fixed seeds rather than `std`'s SipHash: no key comes from an
/// adversary the engine must resist, and grouping is on every run's path.
pub(crate) fn partition_keys<'v>(
    count: usize,
    width: usize,
    key: impl Fn(usize, usize) -> Key<'v>,
) -> Result<Groups, TryReserveError> {
    let mut first: HashMap<u64, usize, BuildHasherDefault<Prehashed>> = HashMap::default();
    first.try_reserve(count)?;
    let state = ahash::RandomState::with_seeds(SEEDS[0], SEEDS[1], SEEDS[2], SEEDS[3]);
    // Groups sharing a hash, chained by group.
    let mut chain: Vec<usize> = Vec::new();
    let mut representative: Vec<usize> = Vec::new();
    let mut group_of = Vec::new();
    group_of.try_reserve_exact(count)?;
    let mut members = Vec::new();
    members.try_reserve_exact(count)?;
    let equal = |left: usize, right: usize| {
        (0..width).all(|position| key(left, position) == key(right, position))
    };
    for item in 0..count {
        let mut hasher = state.build_hasher();
        for position in 0..width {
            key(item, position).hash(&mut hasher);
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
