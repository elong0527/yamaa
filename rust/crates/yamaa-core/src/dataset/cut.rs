//! The cut of a driver input into partitions (#3697, #3763), read once in
//! record order, so a reader can give it one partition of records at a time
//! and drop each before the next (#3830).
//!
//! Without fields, the partitions are consecutive records, `rows` each, the
//! last holding the rest. With fields, they are key-bounded: the records are
//! grouped by the fields with the equality key groups use
//! (`table_grouping::Key`: missing equals missing, the float zero signs are
//! equal, an integer is not its decimal text), and each partition holds as
//! many whole runs of equal values as fit in `rows` records, a longer run
//! alone. The cut keeps the key of every run that has ended, never a record,
//! so it finds a value that recurs after its run has ended: the input is
//! then not grouped by the fields, and has no cut.
use crate::table::TableAccess;
use crate::table_grouping::{Key, OwnedKey};
use crate::temporal::{Date, DateTime};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::ops::Range;

/// Partitions of `rows` consecutive records of an input of `len` records,
/// the last holding the rest (#3697). An empty input is one empty
/// partition, so admission still checks its schema.
pub(crate) fn consecutive(len: usize, rows: usize) -> Vec<Range<usize>> {
    (0..len.max(1))
        .step_by(rows)
        .map(|start| start..start + rows.min(len.saturating_sub(start)))
        .collect()
}

/// The cut of the records read so far, in order.
#[derive(Debug)]
pub struct PartitionCut {
    /// The fields whose runs no partition splits; none for consecutive
    /// records.
    fields: Vec<usize>,
    rows: usize,
    /// The records read so far.
    read: usize,
    /// The first record of the run being read. Its key is the last
    /// ascending one of `ended`, or, when `held`, the one `key` holds.
    run: Option<usize>,
    held: bool,
    key: Vec<Held>,
    /// The key of every run read so far, the one being read's too.
    ended: Ended,
    /// Where each partition starts, the open one last.
    starts: Vec<usize>,
    /// The records of the open partition's completed runs.
    size: usize,
    /// Whether the records read are grouped by the fields, each field read.
    grouped: bool,
}

impl PartitionCut {
    /// A cut into partitions of `rows` records that keep the runs of
    /// `fields` whole, or of consecutive records without fields.
    pub fn new(fields: Vec<usize>, rows: NonZeroUsize) -> Self {
        let width = fields.len();
        Self {
            fields,
            rows: rows.get(),
            read: 0,
            run: None,
            held: false,
            key: Vec::with_capacity(width),
            ended: Ended {
                ascending: Keys::new(width),
                rest: BTreeSet::new(),
            },
            starts: vec![0],
            size: 0,
            grouped: true,
        }
    }

    /// Read `table`'s records, the next ones of the input, in order. Each
    /// record's fields are compared with the run's key where they are
    /// read, from the fields' storage when the table lends it (#4190),
    /// which lends each row the value its cells do. A record that continues
    /// its run allocates nothing, and a run's key is ended as its run
    /// begins (`Ended::insert`), in the keys' arena.
    pub fn read<T: TableAccess + ?Sized>(&mut self, table: &T) {
        let first = self.read;
        self.read += table.row_count();
        if self.fields.is_empty() || !self.grouped {
            return;
        }
        let fields = self.fields.clone();
        let lent: Option<Vec<_>> = fields.iter().map(|&field| table.column(field)).collect();
        let cell = |row: usize, index: usize| match &lent {
            Some(columns) => Some(columns[index].get(row)),
            None => table.cell(row, fields[index]).ok(),
        };
        let mut key = Vec::with_capacity(fields.len());
        for row in 0..table.row_count() {
            key.clear();
            for index in 0..fields.len() {
                let Some(value) = cell(row, index) else {
                    self.grouped = false;
                    return;
                };
                key.push(Key::from(value));
            }
            // The run's key is the last ascending key, or the one `key`
            // holds once the order broke.
            let same = match self.run {
                None => false,
                Some(_) if self.held => self.key.iter().map(Held::key).eq(key.iter().cloned()),
                Some(_) => self.ended.ascending.last_equals(&key),
            };
            if same {
                continue;
            }
            let order =
                (self.run.is_some() && !self.held).then(|| self.ended.ascending.compare_last(&key));
            let record = first + row;
            self.close(record);
            // The run ends no earlier key, and no later record equal to it
            // reaches the ended keys: the run takes it first.
            let kept = match order {
                Some(Ordering::Less) => {
                    self.ended.ascending.push(key.iter().cloned());
                    Some(true)
                }
                _ => self.ended.insert(&key),
            };
            match kept {
                None => {
                    self.grouped = false;
                    return;
                }
                Some(true) => self.held = false,
                Some(false) => {
                    self.held = true;
                    self.key
                        .resize_with(key.len(), || Held::Other(Key::Missing));
                    for (held, field) in self.key.iter_mut().zip(&key) {
                        held.set(field);
                    }
                }
            }
            self.run = Some(record);
        }
    }

    /// End the run being read before record `end`, packing it into the open
    /// partition, or opening a new one when it does not fit.
    fn close(&mut self, end: usize) {
        let Some(start) = self.run.take() else {
            return;
        };
        let len = end - start;
        if self.size > 0 && self.size + len > self.rows {
            self.starts.push(start);
            self.size = 0;
        }
        self.size += len;
    }

    /// The partitions of every record read, in order. None when the records
    /// are not grouped by the fields, or a field could not be read: no cut
    /// keeps their runs whole, so the input is one partition.
    pub fn finish(mut self) -> Option<Vec<Range<usize>>> {
        if self.fields.is_empty() {
            return Some(consecutive(self.read, self.rows));
        }
        if !self.grouped {
            return None;
        }
        self.close(self.read);
        let mut bounds: Vec<Range<usize>> = self
            .starts
            .windows(2)
            .map(|bounds| bounds[0]..bounds[1])
            .collect();
        bounds.push(self.starts[self.starts.len() - 1]..self.read);
        Some(bounds)
    }
}

/// One field of the key of the run being read, held so that the next run's
/// key reuses its text's allocation.
#[derive(Debug)]
enum Held {
    Text(String),
    /// A key that holds no text.
    Other(Key<'static>),
}

impl Held {
    fn key(&self) -> Key<'_> {
        match self {
            Self::Text(text) => Key::Str(text),
            Self::Other(key) => key.clone(),
        }
    }

    fn set(&mut self, key: &Key<'_>) {
        *self = match (std::mem::replace(self, Self::Other(Key::Missing)), key) {
            (Self::Text(mut text), Key::Str(value)) => {
                text.clear();
                text.push_str(value);
                Self::Text(text)
            }
            (_, Key::Str(value)) => Self::Text((*value).to_owned()),
            (_, Key::Missing) => Self::Other(Key::Missing),
            (_, Key::Int(value)) => Self::Other(Key::Int(*value)),
            (_, Key::Float(bits)) => Self::Other(Key::Float(*bits)),
            (_, Key::Bool(value)) => Self::Other(Key::Bool(*value)),
            (_, Key::Date(value)) => Self::Other(Key::Date(*value)),
            (_, Key::DateTime(value)) => Self::Other(Key::DateTime(*value)),
        };
    }
}

/// Keys of one width, field after field, their text in one arena, so that
/// a key costs no allocation of its own. Each field reads back as the
/// [`Key`] it was kept from, with that key's equality and order.
#[derive(Debug)]
struct Keys {
    width: usize,
    text: String,
    fields: Vec<Field>,
}

/// One kept field: a [`Key`] whose text is a span of the arena.
#[derive(Clone, Copy, Debug)]
enum Field {
    Missing,
    Str(usize, usize),
    Int(i64),
    Float(u64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl Keys {
    fn new(width: usize) -> Self {
        Self {
            width,
            text: String::new(),
            fields: Vec::new(),
        }
    }

    /// The number of keys.
    fn len(&self) -> usize {
        self.fields.len() / self.width
    }

    /// Field `field` of every key end to end, as the key it was kept from.
    fn field(&self, field: usize) -> Key<'_> {
        match self.fields[field] {
            Field::Missing => Key::Missing,
            Field::Str(start, end) => Key::Str(&self.text[start..end]),
            Field::Int(value) => Key::Int(value),
            Field::Float(bits) => Key::Float(bits),
            Field::Bool(value) => Key::Bool(value),
            Field::Date(value) => Key::Date(value),
            Field::DateTime(value) => Key::DateTime(value),
        }
    }

    /// Key `index`, field by field.
    fn key(&self, index: usize) -> impl Iterator<Item = Key<'_>> {
        (index * self.width..(index + 1) * self.width).map(|field| self.field(field))
    }

    /// Whether the last key, which there must be, is `key`. Text compares
    /// as the bytes it is.
    fn last_equals(&self, key: &[Key<'_>]) -> bool {
        let first = self.fields.len() - self.width;
        key.iter()
            .enumerate()
            .all(|(index, field)| match (self.fields[first + index], field) {
                (Field::Str(start, end), Key::Str(value)) => {
                    self.text.as_bytes()[start..end] == *value.as_bytes()
                }
                _ => self.field(first + index) == *field,
            })
    }

    /// The last key against `key`, in the keys' order; `Less` when there is
    /// no key.
    fn compare_last(&self, key: &[Key<'_>]) -> Ordering {
        match self.len() {
            0 => Ordering::Less,
            keys => self.compare(keys - 1, key.iter().cloned()),
        }
    }

    /// Key `index` against `key`, in the keys' order.
    fn compare<'a>(&self, index: usize, key: impl IntoIterator<Item = Key<'a>>) -> Ordering {
        let mut kept = self.key(index);
        for other in key {
            let Some(field) = kept.next() else {
                return Ordering::Less;
            };
            match field.cmp(&other) {
                Ordering::Equal => {}
                order => return order,
            }
        }
        match kept.next() {
            Some(_) => Ordering::Greater,
            None => Ordering::Equal,
        }
    }

    fn push<'a>(&mut self, key: impl IntoIterator<Item = Key<'a>>) {
        for field in key {
            self.fields.push(match field {
                Key::Missing => Field::Missing,
                Key::Str(value) => {
                    let start = self.text.len();
                    self.text.push_str(value);
                    Field::Str(start, self.text.len())
                }
                Key::Int(value) => Field::Int(value),
                Key::Float(bits) => Field::Float(bits),
                Key::Bool(value) => Field::Bool(value),
                Key::Date(value) => Field::Date(value),
                Key::DateTime(value) => Field::DateTime(value),
            });
        }
    }
}

/// The keys of the runs read so far, in the order the runs began. While
/// each is greater than the one before, as in an input sorted by the
/// fields, they are kept in that order, and a key greater than the last is
/// new without a search. Once a key is not, it and every later one go into
/// a set.
#[derive(Debug)]
struct Ended {
    ascending: Keys,
    /// The keys of the runs that began after the order broke.
    rest: BTreeSet<Vec<OwnedKey>>,
}

impl Ended {
    /// Keep `key`, the key of a run that begins: whether it ascends, or
    /// `None` when it is a key already kept, whose run has ended, since the
    /// run being read takes its own records first. A key
    /// greater than the last ascending one, before the order broke, needs
    /// no search.
    fn insert(&mut self, key: &[Key<'_>]) -> Option<bool> {
        if self.rest.is_empty() && self.ascending.compare_last(key).is_lt() {
            self.ascending.push(key.iter().cloned());
            return Some(true);
        }
        if self.contains(key) {
            return None;
        }
        self.rest
            .insert(key.iter().cloned().map(OwnedKey::from).collect());
        Some(false)
    }

    fn contains(&self, key: &[Key<'_>]) -> bool {
        let keys = self.ascending.len();
        if keys == 0 {
            return self.contains_rest(key);
        }
        if self.rest.is_empty()
            && self
                .ascending
                .compare(keys - 1, key.iter().cloned())
                .is_lt()
        {
            return false;
        }
        let (mut low, mut high) = (0, keys);
        while low < high {
            let middle = low + (high - low) / 2;
            if self.ascending.compare(middle, key.iter().cloned()).is_lt() {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        (low < keys && self.ascending.compare(low, key.iter().cloned()).is_eq())
            || self.contains_rest(key)
    }

    fn contains_rest(&self, key: &[Key<'_>]) -> bool {
        !self.rest.is_empty()
            && self
                .rest
                .contains(&key.iter().cloned().map(OwnedKey::from).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::PartitionCut;
    use crate::table::{CellError, Column, TableAccess, TableSchema, ValueRef};
    use crate::value::{ColumnType, Value};
    use std::convert::Infallible;
    use std::num::NonZeroUsize;
    use std::ops::Range;

    /// Records of two fields, K and J. The cut reads their cells and never
    /// their declared types, so a field may hold a value of any type.
    struct Records {
        schema: TableSchema,
        rows: Vec<[Value; 2]>,
    }
    impl Records {
        fn new(rows: Vec<[Value; 2]>) -> Self {
            let column = |name: &str, kind| Column {
                name: name.into(),
                kind,
            };
            Self {
                schema: TableSchema::new(vec![
                    column("K", ColumnType::Str),
                    column("J", ColumnType::Int),
                ])
                .unwrap(),
                rows,
            }
        }
    }
    impl TableAccess for Records {
        type Error = Infallible;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            self.rows.len()
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
            self.rows
                .get(row)
                .and_then(|values| values.get(column))
                .map(ValueRef::from)
                .ok_or(CellError::OutOfBounds { row, column })
        }
    }
    /// One partition of the records `range`.
    fn whole(range: Range<usize>) -> Vec<Range<usize>> {
        vec![range]
    }
    fn keyed(keys: &[&str]) -> Records {
        Records::new(
            keys.iter()
                .map(|&key| [Value::Str(key.into()), Value::Int(0)])
                .collect(),
        )
    }

    /// The cut of a held input (`ResolvedPlan::execute_divided`), a seam no
    /// reader reaches: partitions hold whole runs of equal keys, as many
    /// as fit in the partition size, and a run longer than it alone. The
    /// runs of a, a, b, c, c, c, d are 0..2, 2..3, 3..6 and 6..7, so a size
    /// of 3 packs a with b, then c, then d, and a size of 4 packs a with b,
    /// then c with d.
    #[test]
    fn key_bounded_partitions_hold_whole_runs_as_many_as_fit() {
        let records = keyed(&["a", "a", "b", "c", "c", "c", "d"]);
        let cases: [(usize, Vec<Range<usize>>); 5] = [
            (1, vec![0..2, 2..3, 3..6, 6..7]),
            (2, vec![0..2, 2..3, 3..6, 6..7]),
            (3, vec![0..3, 3..6, 6..7]),
            (4, vec![0..3, 3..7]),
            (7, whole(0..7)),
        ];
        for (rows, expected) in cases {
            assert_eq!(
                key_bounded(&records, &[0], NonZeroUsize::new(rows).unwrap()),
                Some(expected),
                "partitions of at most {rows} records"
            );
        }
        assert_eq!(
            key_bounded(&keyed(&[]), &[0], NonZeroUsize::MIN),
            Some(whole(0..0)),
            "an empty input is one empty partition"
        );
    }

    /// The cut's runs, a seam no reader reaches: they are
    /// those of the key tuple, under `Key`'s equality, so missing equals
    /// missing, the float zero signs are equal, and an integer is not its
    /// decimal text. A value that recurs after its run has ended leaves the
    /// input ungrouped, which keeps no partition.
    #[test]
    fn key_bounded_partitions_follow_runs_of_the_whole_key_tuple() {
        let text = |value: &str| Value::Str(value.into());
        let cases = [
            (
                "a value of K recurs under another J",
                vec![
                    [text("a"), Value::Int(1)],
                    [text("b"), Value::Int(1)],
                    [text("a"), Value::Int(2)],
                ],
                vec![0, 1],
                Some(vec![0..1, 1..2, 2..3]),
            ),
            (
                "the same records by K alone",
                vec![
                    [text("a"), Value::Int(1)],
                    [text("b"), Value::Int(1)],
                    [text("a"), Value::Int(2)],
                ],
                vec![0],
                None,
            ),
            (
                "missing values are one run",
                vec![
                    [Value::Missing, Value::Int(1)],
                    [Value::Missing, Value::Int(2)],
                    [text("x"), Value::Int(3)],
                ],
                vec![0],
                Some(vec![0..2, 2..3]),
            ),
            (
                "a missing value recurs",
                vec![
                    [Value::Missing, Value::Int(1)],
                    [text("x"), Value::Int(2)],
                    [Value::Missing, Value::Int(3)],
                ],
                vec![0],
                None,
            ),
            (
                "an integer and its text are two keys",
                vec![[text("1"), Value::Int(1)], [Value::Int(1), Value::Int(1)]],
                vec![0],
                Some(vec![0..1, 1..2]),
            ),
            (
                "the float zero signs are one key",
                vec![
                    [Value::float(-0.0), Value::Int(1)],
                    [Value::float(0.0), Value::Int(2)],
                ],
                vec![0],
                Some(whole(0..2)),
            ),
            (
                "runs that descend without recurring stay grouped",
                ["b", "a", "a", "c"]
                    .map(|key| [text(key), Value::Int(1)])
                    .to_vec(),
                vec![0],
                Some(vec![0..1, 1..3, 3..4]),
            ),
            (
                "an ascending run's value recurs after the order breaks",
                ["a", "b", "c", "a"]
                    .map(|key| [text(key), Value::Int(1)])
                    .to_vec(),
                vec![0],
                None,
            ),
            (
                "a value recurs among the runs after the order breaks",
                ["c", "a", "b", "a"]
                    .map(|key| [text(key), Value::Int(1)])
                    .to_vec(),
                vec![0],
                None,
            ),
        ];
        for (label, rows, fields, expected) in cases {
            assert_eq!(
                key_bounded(&Records::new(rows), &fields, NonZeroUsize::MIN),
                expected,
                "{label}"
            );
        }
    }

    /// The cut reads its input a table at a time, as a scan gives it
    /// (#3830), a seam no reader reaches with chosen tables: a run goes on
    /// across the tables, before the order of the runs breaks and after,
    /// and a value recurs across them. The runs of a, a | a, b are 0..3 and
    /// 3..4; of b | b, a | a, c, 0..2, 2..4 and 4..5; and a | b | a
    /// recurs.
    #[test]
    fn a_run_goes_on_across_the_tables_it_is_read_from() {
        type Case = (
            &'static [&'static [&'static str]],
            Option<Vec<Range<usize>>>,
        );
        let cases: [Case; 3] = [
            (&[&["a", "a"], &["a", "b"]], Some(vec![0..3, 3..4])),
            (
                &[&["b"], &["b", "a"], &["a", "c"]],
                Some(vec![0..2, 2..4, 4..5]),
            ),
            (&[&["a"], &["b"], &["a"]], None),
        ];
        for (tables, expected) in cases {
            let mut cut = PartitionCut::new(vec![0], NonZeroUsize::MIN);
            for keys in tables {
                cut.read(&keyed(keys));
            }
            assert_eq!(cut.finish(), expected, "tables {tables:?}");
        }
    }

    /// The cut of `table`'s records into partitions of at most `rows`
    /// records that keep the runs of `fields` whole.
    fn key_bounded<T: TableAccess>(
        table: &T,
        fields: &[usize],
        rows: NonZeroUsize,
    ) -> Option<Vec<Range<usize>>> {
        let mut cut = PartitionCut::new(fields.to_vec(), rows);
        cut.read(table);
        cut.finish()
    }
}
