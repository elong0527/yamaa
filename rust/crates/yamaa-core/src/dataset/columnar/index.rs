//! The join operator's indexes (#2842): a donor's records grouped by their
//! key, built once and probed per row. A bucket keeps its records in stored
//! order, so every later step of a match (range, filter, selection) sees the
//! records the scan would reach, in its order.
//!
//! A key index groups by key equality (VALUE-19), as
//! [`Key::matching`] encodes it: numbers by value, so an integral float
//! shares the integer's bucket and both zeros share one; temporal values by
//! their civil fields, whatever their precision; text by its bytes. A missing
//! key is in no bucket. A grouping index groups by grouping equality instead
//! (`Key::from`), where a missing value is a key of its own and `int` and
//! `float` never meet.
use super::{Fail, Table};
use crate::dataset::resolved::ExprId;
use crate::table::{TableAccess, ValueRef};
use crate::table_grouping::Key;
use crate::value::{comparable_types, ValueType};
use std::collections::HashMap;

/// The records of a donor grouped by the values of some of its columns.
#[derive(Default)]
pub(super) struct Index {
    columns: Vec<usize>,
    buckets: HashMap<Vec<u8>, Bucket>,
    /// Reused to encode a probe without allocating.
    probe: std::cell::RefCell<Vec<u8>>,
}

/// The records of one key, in stored order; most keys have one.
enum Bucket {
    One(usize),
    Many(Vec<usize>),
}

impl Bucket {
    fn push(&mut self, row: usize) {
        match self {
            Self::One(first) => *self = Self::Many(vec![*first, row]),
            Self::Many(rows) => rows.push(row),
        }
    }

    fn rows(&self) -> &[usize] {
        match self {
            Self::One(row) => std::slice::from_ref(row),
            Self::Many(rows) => rows,
        }
    }
}

impl Index {
    /// An empty index with room for `rows` keys.
    fn new<E>(columns: &[usize], rows: usize) -> Result<Self, Fail<E>> {
        let mut buckets = HashMap::new();
        super::reserved(buckets.try_reserve(rows))?;
        Ok(Self {
            columns: columns.to_vec(),
            buckets,
            probe: std::cell::RefCell::default(),
        })
    }

    fn insert(&mut self, key: Vec<u8>, row: usize) {
        match self.buckets.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut bucket) => bucket.get_mut().push(row),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(Bucket::One(row));
            }
        }
    }

    fn bucket(&self, key: &[u8]) -> &[usize] {
        self.buckets.get(key).map_or(&[], Bucket::rows)
    }
}

/// How a key component compares.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Equality {
    /// Key equality (VALUE-19): a missing key matches nothing.
    Key,
    /// Grouping equality: a missing value matches a missing value.
    Grouping,
}

impl Index {
    /// Index `rows` on `columns` under key equality, or answer none when a
    /// cell's type cannot compare with its column's declared type in
    /// `kinds`: the scan owns that shape's refusal.
    pub(super) fn keys<'t, E>(
        columns: &[usize],
        kinds: &[Option<ValueType>],
        rows: impl ExactSizeIterator<Item = usize>,
        mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    ) -> Result<Option<Self>, Fail<E>> {
        let mut index = Self::new(columns, rows.len())?;
        'rows: for row in rows {
            let mut key = Vec::new();
            for (position, &column) in columns.iter().enumerate() {
                let value = read(row, column)?;
                if matches!(value, ValueRef::Missing) {
                    continue 'rows;
                }
                let fits = value
                    .value_type()
                    .zip(kinds.get(position).copied().flatten())
                    .is_none_or(|(actual, kind)| comparable_types(actual, kind));
                if !fits {
                    return Ok(None);
                }
                encode(&mut key, value, Equality::Key);
            }
            index.insert(key, row);
        }
        Ok(Some(index))
    }

    /// Index `rows` on `columns` by their exact text, or answer none when a
    /// cell holds a value of another type, whose equality the scan owns.
    pub(super) fn texts<'t, E>(
        columns: &[usize],
        rows: impl ExactSizeIterator<Item = usize>,
        mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    ) -> Result<Option<Self>, Fail<E>> {
        let mut index = Self::new(columns, rows.len())?;
        for row in rows {
            let mut key = Vec::new();
            for &column in columns {
                let value = read(row, column)?;
                if !matches!(value, ValueRef::Str(_) | ValueRef::Missing) {
                    return Ok(None);
                }
                encode(&mut key, value, Equality::Grouping);
            }
            index.insert(key, row);
        }
        Ok(Some(index))
    }

    /// Index `rows` on `columns` under grouping equality.
    pub(super) fn groups<'t, E>(
        columns: &[usize],
        rows: impl ExactSizeIterator<Item = usize>,
        mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    ) -> Result<Self, Fail<E>> {
        let mut index = Self::new(columns, rows.len())?;
        for row in rows {
            let mut key = Vec::new();
            for &column in columns {
                encode(&mut key, read(row, column)?, Equality::Grouping);
            }
            index.insert(key, row);
        }
        Ok(index)
    }

    /// The records whose values equal `values`, one per indexed column in
    /// order, in stored order.
    pub(super) fn matching<'v>(
        &self,
        values: impl IntoIterator<Item = ValueRef<'v>>,
        equality: Equality,
    ) -> &[usize] {
        let mut key = self.probe.borrow_mut();
        key.clear();
        for value in values {
            encode(&mut key, value, equality);
        }
        self.bucket(&key)
    }

    /// The records whose key pairs equal `keys`, in stored order: each pair
    /// is a donor column and the value it must equal.
    pub(super) fn matching_pairs(&self, keys: &[(usize, ValueRef<'_>)]) -> &[usize] {
        let mut key = self.probe.borrow_mut();
        key.clear();
        for column in &self.columns {
            let Some((_, value)) = keys.iter().find(|(bound, _)| bound == column) else {
                return &[];
            };
            encode(&mut key, *value, Equality::Key);
        }
        self.bucket(&key)
    }
}

/// Indexes and filter answers over source ports, shared for the attempt by
/// every read that matches the same columns of the same port, in any region.
/// A port is known by its address: it outlives the attempt.
#[derive(Default)]
pub(super) struct Ports {
    keys: HashMap<(usize, Vec<usize>), Option<Index>>,
    texts: HashMap<(usize, Vec<usize>), Option<Index>>,
    /// Per reading node and port, whether its filter keeps each record.
    filters: HashMap<(ExprId, usize), Vec<Option<bool>>>,
}

/// The address of a source port; none for frame rows, which change.
pub(super) fn port<E>(table: Table<'_, E>) -> Option<usize> {
    match table {
        Table::Source(port) => Some(std::ptr::from_ref(port).cast::<()>() as usize),
        Table::Rows { .. } | Table::Input { .. } => None,
    }
}

impl Ports {
    /// The key index over `columns` of a port, built on first use.
    pub(super) fn keys<'t, E>(
        &mut self,
        table: Table<'t, E>,
        columns: &[usize],
        read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    ) -> Result<Option<&Index>, Fail<E>> {
        let Some(port) = port(table) else {
            return Ok(None);
        };
        let key = (port, columns.to_vec());
        if !self.keys.contains_key(&key) {
            let kinds: Vec<Option<ValueType>> = columns
                .iter()
                .map(|&column| {
                    table
                        .schema()
                        .columns()
                        .get(column)
                        .map(|field| field.kind.into())
                })
                .collect();
            let built = Index::keys(columns, &kinds, 0..table.row_count(), read)?;
            self.keys.insert(key.clone(), built);
        }
        Ok(self.keys.get(&key).and_then(Option::as_ref))
    }

    /// The text index over `columns` of a port, built on first use.
    pub(super) fn texts<'t, E>(
        &mut self,
        table: Table<'t, E>,
        columns: &[usize],
        read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    ) -> Result<Option<&Index>, Fail<E>> {
        let Some(port) = port(table) else {
            return Ok(None);
        };
        let key = (port, columns.to_vec());
        if !self.texts.contains_key(&key) {
            let built = Index::texts(columns, 0..table.row_count(), read)?;
            self.texts.insert(key.clone(), built);
        }
        Ok(self.texts.get(&key).and_then(Option::as_ref))
    }

    /// Whether the filter of the reading node `expr` keeps record `row` of a
    /// port: evaluated once per record, since it reads only the record.
    pub(super) fn kept<E>(
        &mut self,
        expr: ExprId,
        table: Table<'_, E>,
        row: usize,
        evaluate: impl FnOnce() -> Result<bool, Fail<E>>,
    ) -> Result<bool, Fail<E>> {
        let Some(port) = port(table) else {
            return evaluate();
        };
        let answers = match self.filters.entry((expr, port)) {
            std::collections::hash_map::Entry::Occupied(answers) => answers.into_mut(),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(super::filled(table.row_count(), None)?)
            }
        };
        if let Some(Some(kept)) = answers.get(row) {
            return Ok(*kept);
        }
        let kept = evaluate()?;
        if let Some(slot) = self
            .filters
            .get_mut(&(expr, port))
            .and_then(|answers| answers.get_mut(row))
        {
            *slot = Some(kept);
        }
        Ok(kept)
    }
}

/// Whether a probe's key pairs can be answered from a key index: each match
/// value is present and compares with its donor column's declared type, so
/// no record can refuse the pair (LOOKUP-9). Otherwise the scan answers it.
pub(super) fn exact(keys: &[(usize, ValueRef<'_>)], kinds: &[Option<ValueType>]) -> bool {
    keys.iter()
        .zip(kinds)
        .all(|(&(_, value), &kind)| exact_pair(value, kind))
}

/// Whether one key pair can be answered from a key index, as [`exact`].
pub(super) fn exact_pair(value: ValueRef<'_>, kind: Option<ValueType>) -> bool {
    match (value.value_type(), kind) {
        (Some(actual), Some(kind)) => comparable_types(actual, kind),
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Append `value`'s bytes under `equality`: the [`Key`] it is, as key
/// matching or grouping compares it.
fn encode(key: &mut Vec<u8>, value: ValueRef<'_>, equality: Equality) {
    let value = match equality {
        Equality::Key => Key::matching(value),
        Equality::Grouping => Key::from(value),
    };
    match value {
        Key::Missing => key.push(0),
        Key::Str(value) => {
            key.push(1);
            key.extend_from_slice(&(value.len() as u64).to_be_bytes());
            key.extend_from_slice(value.as_bytes());
        }
        Key::Int(value) => {
            key.push(2);
            key.extend_from_slice(&value.to_be_bytes());
        }
        Key::Float(bits) => {
            key.push(3);
            key.extend_from_slice(&bits.to_be_bytes());
        }
        Key::Bool(value) => {
            key.push(4);
            key.push(u8::from(value));
        }
        Key::Date(value) => {
            let (year, month, day) = value.fields();
            key.push(5);
            key.extend_from_slice(&year.to_be_bytes());
            key.push(month);
            key.push(day);
        }
        Key::DateTime(value) => {
            let (year, month, day, hour, minute, second) = value.fields();
            key.push(6);
            key.extend_from_slice(&year.to_be_bytes());
            key.push(month);
            key.push(day);
            key.push(hour);
            key.push(minute);
            key.push(second);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal::{Date, DateTime, Precision};
    use crate::value::Value;
    use std::convert::Infallible;

    fn read<'c>(
        cells: &'c [Value],
    ) -> impl FnMut(usize, usize) -> Result<ValueRef<'c>, Fail<Infallible>> + 'c {
        |row, _| Ok(ValueRef::from(&cells[row]))
    }

    /// One column of `cells` indexed under key equality with a declared type.
    fn keyed(cells: &[Value], kind: ValueType) -> Option<Index> {
        Index::keys(&[0], &[Some(kind)], 0..cells.len(), read(cells)).unwrap()
    }

    fn rows(index: &Index, value: &Value) -> Vec<usize> {
        index.matching_pairs(&[(0, ValueRef::from(value))]).to_vec()
    }

    #[test]
    fn key_equality_meets_numbers_by_value_and_never_missing() {
        let cells = [
            Value::Int(2),
            Value::float(2.0),
            Value::float(2.5),
            Value::Int(0),
            Value::float(-0.0),
            Value::Missing,
            Value::Int(i64::MAX),
            Value::float(9_223_372_036_854_775_808.0),
        ];
        let index = keyed(&cells, ValueType::Float).unwrap();
        assert_eq!(rows(&index, &Value::Int(2)), [0, 1]);
        assert_eq!(rows(&index, &Value::float(2.0)), [0, 1]);
        assert_eq!(rows(&index, &Value::float(2.5)), [2]);
        assert_eq!(rows(&index, &Value::float(0.0)), [3, 4]);
        // The largest integer and 2^63 compare apart, as `int` and `float` do.
        assert_eq!(rows(&index, &Value::Int(i64::MAX)), [6]);
        assert_eq!(
            rows(&index, &Value::float(9_223_372_036_854_775_808.0)),
            [7]
        );
        assert!(rows(&index, &Value::Missing).is_empty());
    }

    #[test]
    fn key_equality_meets_temporal_values_whatever_their_precision() {
        let day = Date::new(2024, 3, 1, Precision::Day).unwrap();
        let month = Date::new(2024, 3, 1, Precision::Month).unwrap();
        let cells = [Value::Date(day), Value::Date(month)];
        let index = keyed(&cells, ValueType::Date).unwrap();
        assert_eq!(rows(&index, &Value::Date(day)), [0, 1]);
        let second = DateTime::new(day, 8, 30, 0, Precision::Second).unwrap();
        let minute = second.with_precision(Precision::Minute);
        let cells = [Value::DateTime(minute), Value::DateTime(second)];
        let index = keyed(&cells, ValueType::DateTime).unwrap();
        assert_eq!(rows(&index, &Value::DateTime(second)), [0, 1]);
    }

    #[test]
    fn a_cell_that_cannot_compare_with_its_column_leaves_the_scan_its_refusal() {
        // Text in an integer column, and booleans, which key equality never
        // indexes: the scan answers, and refuses where the match would.
        assert!(keyed(&[Value::Int(1), Value::Str("a".into())], ValueType::Int).is_none());
        assert!(keyed(&[Value::Bool(true)], ValueType::Bool).is_none());
        assert!(!exact(&[(0, ValueRef::Str("a"))], &[Some(ValueType::Int)]));
        assert!(!exact(&[(0, ValueRef::Missing)], &[Some(ValueType::Int)]));
        assert!(exact(&[(0, ValueRef::Int(1))], &[Some(ValueType::Float)]));
    }

    #[test]
    fn grouping_equality_keeps_missing_and_parts_int_from_float() {
        let cells = [
            Value::Int(1),
            Value::float(1.0),
            Value::Missing,
            Value::Int(1),
            Value::Missing,
        ];
        let index = Index::groups(&[0], 0..cells.len(), read(&cells)).unwrap();
        let group = |value: &Value| {
            index
                .matching([ValueRef::from(value)], Equality::Grouping)
                .to_vec()
        };
        assert_eq!(group(&Value::Int(1)), [0, 3]);
        assert_eq!(group(&Value::float(1.0)), [1]);
        assert_eq!(group(&Value::Missing), [2, 4]);
    }

    #[test]
    fn a_text_index_admits_only_text_and_missing() {
        let cells = [
            Value::Str("A".into()),
            Value::Missing,
            Value::Str("A".into()),
        ];
        let index = Index::texts(&[0], 0..cells.len(), read(&cells))
            .unwrap()
            .unwrap();
        let found = |value: ValueRef<'_>| index.matching([value], Equality::Grouping).to_vec();
        assert_eq!(found(ValueRef::Str("A")), [0, 2]);
        assert_eq!(found(ValueRef::Missing), [1]);
        assert!(found(ValueRef::Str("a")).is_empty());
        let mixed = [Value::Str("A".into()), Value::Int(1)];
        assert!(Index::texts(&[0], 0..mixed.len(), read(&mixed))
            .unwrap()
            .is_none());
    }
}
