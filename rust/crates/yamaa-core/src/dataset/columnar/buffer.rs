//! Typed column buffers: one per closed value type with a validity mask,
//! text in one byte arena, and a mixed buffer for raw results whose type
//! varies by row. A buffer changes layout, never a value: every read lends
//! the same `ValueRef` a table port would.
use crate::{
    expression::{Column, Lent},
    numeric::Number,
    table::{ColumnRef, ColumnValues, ValueRef},
    temporal::{Date, DateTime},
    value::{FiniteFloat, Value},
};
use std::collections::TryReserveError;
use std::ops::Range;

/// A growable bit vector.
#[derive(Clone, Debug, Default)]
pub(crate) struct Bits {
    words: Vec<u64>,
    len: usize,
}

impl Bits {
    /// Room for `additional` more bits.
    pub(super) fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        let words = self
            .len
            .saturating_add(additional)
            .div_ceil(64)
            .saturating_sub(self.words.len());
        self.words.try_reserve(words)
    }

    pub(super) fn new(len: usize, value: bool) -> Self {
        let fill = if value { u64::MAX } else { 0 };
        let mut bits = Self {
            words: vec![fill; len.div_ceil(64)],
            len,
        };
        bits.clear_tail();
        bits
    }

    /// `len` bits, bit `index` set when `set(index)` holds, asked in order
    /// and built a word at a time.
    fn from_fn(len: usize, mut set: impl FnMut(usize) -> bool) -> Result<Self, TryReserveError> {
        let mut words = Vec::new();
        words.try_reserve_exact(len.div_ceil(64))?;
        let mut start = 0;
        while start < len {
            let end = (start + 64).min(len);
            let mut word = 0u64;
            for index in start..end {
                word |= u64::from(set(index)) << (index - start);
            }
            words.push(word);
            start = end;
        }
        Ok(Self { words, len })
    }

    /// Grow to `len` bits, each new one clear.
    pub(super) fn grow(&mut self, len: usize) {
        if len > self.len {
            self.words.resize(len.div_ceil(64), 0);
            self.len = len;
        }
    }

    /// Whether any bit is set.
    fn any(&self) -> bool {
        self.words.iter().any(|&word| word != 0)
    }

    /// Append bits `range` of `other`, a word at a time.
    pub(super) fn extend_from(&mut self, other: &Bits, range: Range<usize>) {
        let start = self.len;
        self.grow(start + range.len());
        self.copy_from(start, other, range);
    }

    /// The `count` bits from `start`, at most 64, in the low bits of one
    /// word.
    fn word_at(&self, start: usize, count: usize) -> u64 {
        debug_assert!(count <= 64 && start + count <= self.len);
        if count == 0 {
            return 0;
        }
        let (index, shift) = (start / 64, start % 64);
        let mut word = self.words[index] >> shift;
        if shift + count > 64 {
            word |= self.words[index + 1] << (64 - shift);
        }
        word & low_mask(count)
    }

    /// Overwrite the `count` bits from `start`, at most 64, with the low
    /// bits of `word`.
    fn put_word(&mut self, start: usize, count: usize, word: u64) {
        debug_assert!(count <= 64 && start + count <= self.len);
        if count == 0 {
            return;
        }
        let mask = low_mask(count);
        let word = word & mask;
        let (index, shift) = (start / 64, start % 64);
        self.words[index] = (self.words[index] & !(mask << shift)) | (word << shift);
        if shift + count > 64 {
            let written = 64 - shift;
            self.words[index + 1] =
                (self.words[index + 1] & !(mask >> written)) | (word >> written);
        }
    }

    /// Overwrite the bits from `at` with bits `range` of `other`, a word at
    /// a time.
    fn copy_from(&mut self, at: usize, other: &Bits, range: Range<usize>) {
        let mut offset = 0;
        while offset < range.len() {
            let count = (range.len() - offset).min(64);
            let word = other.word_at(range.start + offset, count);
            self.put_word(at + offset, count, word);
            offset += count;
        }
    }

    /// Set every bit of `range` to `value`, a word at a time.
    pub(super) fn fill(&mut self, range: Range<usize>, value: bool) {
        let word = if value { u64::MAX } else { 0 };
        let mut start = range.start;
        while start < range.end {
            let count = (range.end - start).min(64);
            self.put_word(start, count, word);
            start += count;
        }
    }

    /// Whether any bit of `range` is set, read a word at a time.
    pub(super) fn any_in(&self, range: Range<usize>) -> bool {
        let mut start = range.start;
        while start < range.end {
            let count = (range.end - start).min(64);
            if self.word_at(start, count) != 0 {
                return true;
            }
            start += count;
        }
        false
    }

    /// The set bits, ascending.
    fn ones(&self) -> Ones<'_> {
        Ones {
            words: &self.words,
            index: 0,
            word: self.words.first().copied().unwrap_or(0),
        }
    }

    /// The unset bits below the length, ascending, found a word at a time.
    fn zeros(&self) -> Vec<usize> {
        let mut zeros = Vec::new();
        for (index, &word) in self.words.iter().enumerate() {
            let mut unset = !word & low_mask(self.len.saturating_sub(index * 64));
            while unset != 0 {
                zeros.push(index * 64 + unset.trailing_zeros() as usize);
                unset &= unset - 1;
            }
        }
        zeros
    }

    /// The first set bit, found a word at a time.
    fn first_set(&self) -> Option<usize> {
        self.words
            .iter()
            .position(|&word| word != 0)
            .map(|index| index * 64 + self.words[index].trailing_zeros() as usize)
    }

    /// Keep, from `first` on, only the bits at `kept`, ascending rows from
    /// `first` on, moved down in place.
    pub(super) fn retain(&mut self, first: usize, kept: &[usize]) {
        // Each write lands at or below the row it reads, and every later
        // read is above it.
        for (offset, &row) in kept.iter().enumerate() {
            let bit = self.get(row);
            self.set(first + offset, bit);
        }
        self.truncate(first + kept.len());
    }

    fn clear_tail(&mut self) {
        let used = self.len % 64;
        if used != 0 {
            if let Some(last) = self.words.last_mut() {
                *last &= (1u64 << used) - 1;
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.len
    }

    pub(super) fn get(&self, index: usize) -> bool {
        debug_assert!(index < self.len);
        self.words[index / 64] >> (index % 64) & 1 == 1
    }

    pub(super) fn set(&mut self, index: usize, value: bool) {
        debug_assert!(index < self.len);
        let word = &mut self.words[index / 64];
        let mask = 1u64 << (index % 64);
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
    }

    pub(super) fn push(&mut self, value: bool) {
        if self.len % 64 == 0 {
            self.words.push(0);
        }
        self.len += 1;
        if value {
            self.set(self.len - 1, true);
        }
    }

    pub(super) fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.len = len;
            self.words.truncate(len.div_ceil(64));
            self.clear_tail();
        }
    }

    /// Whether every bit is set, read a word at a time.
    fn all(&self) -> bool {
        let mut start = 0;
        while start < self.len {
            let count = (self.len - start).min(64);
            if self.word_at(start, count) != low_mask(count) {
                return false;
            }
            start += count;
        }
        true
    }

    /// Keep the bits at `rows`, in that order.
    pub(super) fn gather(&self, rows: &[usize]) -> Result<Self, TryReserveError> {
        Self::from_fn(rows.len(), |index| self.get(rows[index]))
    }
}

/// Append to `bytes` the text of each span of `from`, in order, and hand
/// `place` each key with the span its text takes in `bytes`. Spans lying end
/// to end are copied as one run, and an empty span joins any run, as it
/// copies nothing; a span equal to the one before it, as a repeated value
/// lends, shares that one's copy.
fn append_runs(
    bytes: &mut String,
    from: &str,
    spans: impl Iterator<Item = (usize, (usize, usize))>,
    mut place: impl FnMut(usize, (usize, usize)),
) {
    // The source bytes of the run not yet copied.
    let mut run = (0, 0);
    // The last span copied, and where its text went.
    let mut last = None;
    for (key, span) in spans {
        if let Some((source, placed)) = last {
            if source == span {
                place(key, placed);
                continue;
            }
        }
        let (start, end) = span;
        if start != end && start != run.1 {
            bytes.push_str(&from[run.0..run.1]);
            run = (start, start);
        }
        let at = bytes.len() + (run.1 - run.0);
        if start != end {
            run.1 = end;
        }
        let placed = (at, at + (end - start));
        last = Some((span, placed));
        place(key, placed);
    }
    bytes.push_str(&from[run.0..run.1]);
}

/// The bytes `spans` cover when each begins where the one before it ends,
/// as a buffer filled in row order holds them.
fn end_to_end(spans: &[(usize, usize)]) -> Option<(usize, usize)> {
    let (&(first, _), &(_, last)) = (spans.first()?, spans.last()?);
    spans
        .windows(2)
        .all(|pair| pair[0].1 == pair[1].0)
        .then_some((first, last))
}

/// The set bits of a bitmap, ascending, found a word at a time.
struct Ones<'b> {
    words: &'b [u64],
    index: usize,
    word: u64,
}

impl Iterator for Ones<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.word == 0 {
            self.index += 1;
            self.word = *self.words.get(self.index)?;
        }
        let bit = self.word.trailing_zeros() as usize;
        self.word &= self.word - 1;
        Some(self.index * 64 + bit)
    }
}

/// The low `count` bits of a word, `count` at most 64.
fn low_mask(count: usize) -> u64 {
    if count >= 64 {
        u64::MAX
    } else {
        (1u64 << count) - 1
    }
}

/// Text in one byte arena; each row lends a slice of it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Strings {
    spans: Vec<(usize, usize)>,
    bytes: String,
}

impl Strings {
    /// Room for `additional` more rows; their text grows the arena as it
    /// arrives.
    fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.spans.try_reserve(additional)
    }

    fn get(&self, index: usize) -> &str {
        let (start, end) = self.spans[index];
        &self.bytes[start..end]
    }

    /// One row's text as bytes, without the character boundary checks
    /// that lending it as text makes.
    fn bytes_of(&self, index: usize) -> &[u8] {
        let (start, end) = self.spans[index];
        &self.bytes.as_bytes()[start..end]
    }

    fn push(&mut self, value: &str) {
        let start = self.bytes.len();
        self.bytes.push_str(value);
        self.spans.push((start, self.bytes.len()));
    }

    fn push_empty(&mut self) {
        let end = self.bytes.len();
        self.spans.push((end, end));
    }

    /// The rows `rows` of `self`, their text copied once into an arena
    /// reserved to its size.
    fn gather(&self, rows: &[usize]) -> Result<Self, TryReserveError> {
        let mut gathered = Self::default();
        gathered.spans.try_reserve_exact(rows.len())?;
        let bytes = rows.iter().map(|&row| self.get(row).len()).sum();
        gathered.bytes.try_reserve_exact(bytes)?;
        for &row in rows {
            gathered.push(self.get(row));
        }
        Ok(gathered)
    }

    /// Append rows `range` of `other`, their text in one copy per run of
    /// rows lying end to end in its arena.
    fn extend_from(&mut self, other: &Strings, range: Range<usize>) {
        let spans = &other.spans[range];
        if let Some((first, last)) = end_to_end(spans) {
            let base = self.bytes.len();
            self.bytes.push_str(&other.bytes[first..last]);
            self.spans.extend(
                spans
                    .iter()
                    .map(|&(start, end)| (start - first + base, end - first + base)),
            );
            return;
        }
        self.spans.reserve(spans.len());
        let Self { spans: to, bytes } = self;
        append_runs(
            bytes,
            &other.bytes,
            spans.iter().copied().enumerate(),
            |_, span| {
                to.push(span);
            },
        );
    }

    /// Overwrite one row; the replaced bytes stay in the arena.
    fn set(&mut self, index: usize, value: &str) {
        let start = self.bytes.len();
        self.bytes.push_str(value);
        self.spans[index] = (start, self.bytes.len());
    }

    /// Overwrite the rows from `start` with the rows of `from` that `valid`
    /// marks, each reading as `set` of it leaves it, and every other row
    /// untouched: their text is appended in row order, one copy per run of
    /// rows lying end to end in `from`'s arena.
    fn set_from(&mut self, start: usize, from: &Strings, valid: &Bits) {
        if let (true, Some((first, last))) = (valid.all(), end_to_end(&from.spans)) {
            let base = self.bytes.len();
            self.bytes.push_str(&from.bytes[first..last]);
            let to = &mut self.spans[start..start + from.len()];
            for (span, &(begin, end)) in to.iter_mut().zip(&from.spans) {
                *span = (begin - first + base, end - first + base);
            }
            return;
        }
        let Self { spans, bytes } = self;
        let rows = valid.ones().map(|row| (row, from.spans[row]));
        append_runs(bytes, &from.bytes, rows, |row, span| {
            spans[start + row] = span;
        });
    }

    /// Grow to `len` rows, each new one empty.
    fn resize_empty(&mut self, len: usize) {
        let end = self.bytes.len();
        self.spans.resize(len, (end, end));
    }

    fn len(&self) -> usize {
        self.spans.len()
    }

    fn truncate(&mut self, len: usize) {
        self.spans.truncate(len);
    }
}

/// One value per row. A typed buffer holds every present value in one closed
/// type; the first value of another type turns the buffer mixed.
#[derive(Clone, Debug)]
pub(crate) enum Values {
    /// Every row is missing.
    Missing(usize),
    Int(Vec<i64>, Bits),
    Float(Vec<FiniteFloat>, Bits),
    Bool(Vec<bool>, Bits),
    Date(Vec<Date>, Bits),
    DateTime(Vec<DateTime>, Bits),
    Str(Strings, Bits),
    Mixed(Vec<Value>),
}

impl Default for Values {
    fn default() -> Self {
        Self::Missing(0)
    }
}

/// A buffer lends a column program one operand's values (#4199).
impl Column for Values {
    fn get(&self, row: usize) -> ValueRef<'_> {
        Values::get(self, row)
    }

    /// Its typed storage, every row in order (#4218).
    fn lend(&self) -> Option<Lent<'_>> {
        self.column().map(|view| Lent {
            view,
            rows: None,
            offset: 0,
        })
    }
}

impl Values {
    pub(crate) fn missing(len: usize) -> Self {
        Self::Missing(len)
    }

    /// Room for `additional` more rows: a missing buffer holds only its
    /// length.
    pub(super) fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        match self {
            Self::Missing(_) => Ok(()),
            Self::Int(values, valid) => {
                values.try_reserve(additional)?;
                valid.try_reserve(additional)
            }
            Self::Float(values, valid) => {
                values.try_reserve(additional)?;
                valid.try_reserve(additional)
            }
            Self::Bool(values, valid) => {
                values.try_reserve(additional)?;
                valid.try_reserve(additional)
            }
            Self::Date(values, valid) => {
                values.try_reserve(additional)?;
                valid.try_reserve(additional)
            }
            Self::DateTime(values, valid) => {
                values.try_reserve(additional)?;
                valid.try_reserve(additional)
            }
            Self::Str(values, valid) => {
                values.try_reserve(additional)?;
                valid.try_reserve(additional)
            }
            Self::Mixed(values) => values.try_reserve(additional),
        }
    }

    /// The same value on every row; text is held once, and every row lends
    /// it.
    pub(super) fn repeat(value: ValueRef<'_>, len: usize) -> Self {
        if len == 0 {
            return Self::Missing(0);
        }
        let valid = || Bits::new(len, true);
        match value {
            ValueRef::Missing => Self::Missing(len),
            ValueRef::Int(value) => Self::Int(vec![value; len], valid()),
            ValueRef::Float(value) => Self::Float(vec![value; len], valid()),
            ValueRef::Bool(value) => Self::Bool(vec![value; len], valid()),
            ValueRef::Date(value) => Self::Date(vec![value; len], valid()),
            ValueRef::DateTime(value) => Self::DateTime(vec![value; len], valid()),
            ValueRef::Str(value) => Self::Str(
                Strings {
                    spans: vec![(0, value.len()); len],
                    bytes: value.to_owned(),
                },
                valid(),
            ),
        }
    }

    /// The values of `column` at `rows`, positions in it, in that order,
    /// each the `ValueRef` the column lends; `None` when a row is outside
    /// the column. The buffer is sized to `rows` before it is filled, and is
    /// missing when no row holds a value, as one pushed a row at a time is.
    pub(super) fn from_column(
        column: ColumnRef<'_>,
        rows: &[usize],
    ) -> Option<Result<Self, TryReserveError>> {
        if rows.iter().any(|&row| row >= column.len()) {
            return None;
        }
        Some(Self::gather_column(column, rows))
    }

    /// One number per row, as a buffer pushed them a row at a time holds
    /// them: typed while every present number has one type, mixed when
    /// both do, and missing when none is present. A column program's result
    /// (#4199).
    pub(super) fn from_numbers(numbers: &[Number]) -> Result<Self, TryReserveError> {
        let present = |int: bool| {
            numbers.iter().any(|number| match number {
                Number::Int(_) => int,
                Number::Float(_) => !int,
                Number::Missing => false,
            })
        };
        let valid = || Bits::from_fn(numbers.len(), |row| numbers[row] != Number::Missing);
        Ok(match (present(true), present(false)) {
            (false, false) => Self::Missing(numbers.len()),
            (true, false) => {
                let mut values = Vec::new();
                values.try_reserve_exact(numbers.len())?;
                values.extend(numbers.iter().map(|number| match number {
                    Number::Int(value) => *value,
                    _ => 0,
                }));
                Self::Int(values, valid()?)
            }
            (false, true) => {
                let zero = FiniteFloat::new(0.0).expect("zero is finite");
                let mut values = Vec::new();
                values.try_reserve_exact(numbers.len())?;
                values.extend(numbers.iter().map(|number| match number {
                    Number::Float(value) => *value,
                    _ => zero,
                }));
                Self::Float(values, valid()?)
            }
            (true, true) => {
                let mut values = Vec::new();
                values.try_reserve_exact(numbers.len())?;
                values.extend(numbers.iter().map(|&number| Value::from(number)));
                Self::Mixed(values)
            }
        })
    }

    fn gather_column(column: ColumnRef<'_>, rows: &[usize]) -> Result<Self, TryReserveError> {
        fn fill<T: Copy>(
            rows: &[usize],
            placeholder: T,
            read: impl Fn(usize) -> Option<T>,
        ) -> Result<(Vec<T>, Bits), TryReserveError> {
            let mut values = Vec::new();
            values.try_reserve_exact(rows.len())?;
            let mut last = placeholder;
            let valid = Bits::from_fn(rows.len(), |index| match read(rows[index]) {
                Some(value) => {
                    last = value;
                    values.push(value);
                    true
                }
                None => {
                    values.push(last);
                    false
                }
            })?;
            Ok((values, valid))
        }
        use crate::value::ColumnType;
        let values = match column.values().kind() {
            ColumnType::Int => {
                let (values, valid) = fill(rows, 0, |row| match column.get(row) {
                    ValueRef::Int(value) => Some(value),
                    _ => None,
                })?;
                Self::Int(values, valid)
            }
            ColumnType::Float => {
                let zero = FiniteFloat::new(0.0).expect("zero is finite");
                let (values, valid) = fill(rows, zero, |row| match column.get(row) {
                    ValueRef::Float(value) => Some(value),
                    _ => None,
                })?;
                Self::Float(values, valid)
            }
            ColumnType::Date => {
                let date = |row| match column.get(row) {
                    ValueRef::Date(value) => Some(value),
                    _ => None,
                };
                let Some(first) = rows.iter().find_map(|&row| date(row)) else {
                    return Ok(Self::Missing(rows.len()));
                };
                let (values, valid) = fill(rows, first, date)?;
                Self::Date(values, valid)
            }
            ColumnType::DateTime => {
                let datetime = |row| match column.get(row) {
                    ValueRef::DateTime(value) => Some(value),
                    _ => None,
                };
                let Some(first) = rows.iter().find_map(|&row| datetime(row)) else {
                    return Ok(Self::Missing(rows.len()));
                };
                let (values, valid) = fill(rows, first, datetime)?;
                Self::DateTime(values, valid)
            }
            ColumnType::Str => {
                let mut strings = Strings::default();
                strings.spans.try_reserve_exact(rows.len())?;
                let bytes = rows.iter().map(|&row| column.text_len(row)).sum();
                strings.bytes.try_reserve_exact(bytes)?;
                let valid = Bits::from_fn(rows.len(), |index| match column.get(rows[index]) {
                    ValueRef::Str(value) => {
                        strings.push(value);
                        true
                    }
                    _ => {
                        strings.push_empty();
                        false
                    }
                })?;
                Self::Str(strings, valid)
            }
        };
        Ok(values.or_missing())
    }

    /// The same buffer, missing when it is typed and no row holds a value,
    /// as a buffer pushed a row at a time stays.
    fn or_missing(self) -> Self {
        let present = match &self {
            Self::Int(_, valid)
            | Self::Float(_, valid)
            | Self::Bool(_, valid)
            | Self::Date(_, valid)
            | Self::DateTime(_, valid)
            | Self::Str(_, valid) => valid.any(),
            Self::Missing(_) | Self::Mixed(_) => return self,
        };
        if present {
            self
        } else {
            Self::Missing(self.len())
        }
    }

    /// The buffer's layout, validity, payloads and every row's text, with
    /// text in an arena of its own row order, where each row's text lies
    /// once: what a test compares when only the arena may differ.
    #[cfg(test)]
    pub(super) fn canonical(&self) -> String {
        match self {
            Self::Str(strings, valid) => {
                let rows: Vec<usize> = (0..strings.len()).collect();
                format!(
                    "{:?}",
                    Self::Str(strings.gather(&rows).unwrap(), valid.clone())
                )
            }
            other => format!("{other:?}"),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Missing(len) => *len,
            Self::Int(values, _) => values.len(),
            Self::Float(values, _) => values.len(),
            Self::Bool(values, _) => values.len(),
            Self::Date(values, _) => values.len(),
            Self::DateTime(values, _) => values.len(),
            Self::Str(values, _) => values.len(),
            Self::Mixed(values) => values.len(),
        }
    }

    /// Lend one row's value.
    pub(crate) fn get(&self, index: usize) -> ValueRef<'_> {
        match self {
            Self::Missing(len) => {
                debug_assert!(index < *len);
                ValueRef::Missing
            }
            Self::Int(values, valid) => {
                if valid.get(index) {
                    ValueRef::Int(values[index])
                } else {
                    ValueRef::Missing
                }
            }
            Self::Float(values, valid) => {
                if valid.get(index) {
                    ValueRef::Float(values[index])
                } else {
                    ValueRef::Missing
                }
            }
            Self::Bool(values, valid) => {
                if valid.get(index) {
                    ValueRef::Bool(values[index])
                } else {
                    ValueRef::Missing
                }
            }
            Self::Date(values, valid) => {
                if valid.get(index) {
                    ValueRef::Date(values[index])
                } else {
                    ValueRef::Missing
                }
            }
            Self::DateTime(values, valid) => {
                if valid.get(index) {
                    ValueRef::DateTime(values[index])
                } else {
                    ValueRef::Missing
                }
            }
            Self::Str(values, valid) => {
                if valid.get(index) {
                    ValueRef::Str(values.get(index))
                } else {
                    ValueRef::Missing
                }
            }
            Self::Mixed(values) => ValueRef::from(&values[index]),
        }
    }

    /// The rows that hold no value, ascending: read from a typed buffer's
    /// bitmap a word at a time (#4211).
    pub(super) fn missing_rows(&self) -> Vec<usize> {
        match self {
            Self::Missing(len) => (0..*len).collect(),
            Self::Int(_, valid)
            | Self::Float(_, valid)
            | Self::Bool(_, valid)
            | Self::Date(_, valid)
            | Self::DateTime(_, valid)
            | Self::Str(_, valid) => valid.zeros(),
            Self::Mixed(values) => (0..values.len())
                .filter(|&row| matches!(values[row], Value::Missing))
                .collect(),
        }
    }

    /// Lend the whole buffer as a column view, its storage as it is
    /// (#4201); `None` for a buffer of no single column type: every row
    /// missing, booleans, or mixed values.
    pub(crate) fn column(&self) -> Option<ColumnRef<'_>> {
        let (values, valid) = match self {
            Self::Int(values, valid) => (ColumnValues::Int(values), valid),
            Self::Float(values, valid) => (ColumnValues::FiniteFloat(values), valid),
            Self::Date(values, valid) => (ColumnValues::MaskedDate(values), valid),
            Self::DateTime(values, valid) => (ColumnValues::MaskedDateTime(values), valid),
            Self::Str(values, valid) => (
                ColumnValues::TextSpans {
                    spans: &values.spans,
                    bytes: &values.bytes,
                },
                valid,
            ),
            Self::Missing(_) | Self::Bool(..) | Self::Mixed(_) => return None,
        };
        ColumnRef::lend(values, Some(&valid.words))
    }

    /// An owned copy of one row's value.
    pub(crate) fn value(&self, index: usize) -> Value {
        own(self.get(index))
    }

    /// Append one value, keeping the buffer typed while its present values
    /// share one type.
    pub(crate) fn push(&mut self, value: ValueRef<'_>) {
        match (&mut *self, value) {
            (Self::Missing(len), ValueRef::Missing) => *len += 1,
            (Self::Missing(len), _) => {
                let missing = *len;
                *self = Self::typed_like(value, missing);
                self.push(value);
            }
            (Self::Int(values, valid), ValueRef::Int(value)) => {
                values.push(value);
                valid.push(true);
            }
            (Self::Float(values, valid), ValueRef::Float(value)) => {
                values.push(value);
                valid.push(true);
            }
            (Self::Bool(values, valid), ValueRef::Bool(value)) => {
                values.push(value);
                valid.push(true);
            }
            (Self::Date(values, valid), ValueRef::Date(value)) => {
                values.push(value);
                valid.push(true);
            }
            (Self::DateTime(values, valid), ValueRef::DateTime(value)) => {
                values.push(value);
                valid.push(true);
            }
            (Self::Str(values, valid), ValueRef::Str(value)) => {
                values.push(value);
                valid.push(true);
            }
            (Self::Mixed(values), value) => values.push(own(value)),
            (_, ValueRef::Missing) => self.push_missing(),
            (_, value) => {
                self.make_mixed();
                self.push(value);
            }
        }
    }

    /// Append one owned value without copying text twice.
    pub(super) fn push_value(&mut self, value: Value) {
        match (&mut *self, value) {
            (Self::Mixed(values), value) => values.push(value),
            (_, value) => self.push(ValueRef::from(&value)),
        }
    }

    fn push_missing(&mut self) {
        match self {
            Self::Missing(len) => *len += 1,
            Self::Int(values, valid) => {
                values.push(0);
                valid.push(false);
            }
            Self::Float(values, valid) => {
                values.push(FiniteFloat::new(0.0).expect("zero is finite"));
                valid.push(false);
            }
            Self::Bool(values, valid) => {
                values.push(false);
                valid.push(false);
            }
            // A temporal placeholder copies an earlier row; an empty typed
            // buffer is the same as an empty missing one.
            Self::Date(values, valid) => match values.last().copied() {
                Some(placeholder) => {
                    values.push(placeholder);
                    valid.push(false);
                }
                None => *self = Self::Missing(1),
            },
            Self::DateTime(values, valid) => match values.last().copied() {
                Some(placeholder) => {
                    values.push(placeholder);
                    valid.push(false);
                }
                None => *self = Self::Missing(1),
            },
            Self::Str(values, valid) => {
                values.push_empty();
                valid.push(false);
            }
            Self::Mixed(values) => values.push(Value::Missing),
        }
    }

    /// A typed buffer of the value's type holding `missing` missing rows;
    /// placeholders behind unset validity bits copy the value.
    fn typed_like(value: ValueRef<'_>, missing: usize) -> Self {
        let valid = Bits::new(missing, false);
        match value {
            ValueRef::Missing => Self::Missing(missing),
            ValueRef::Int(value) => Self::Int(vec![value; missing], valid),
            ValueRef::Float(value) => Self::Float(vec![value; missing], valid),
            ValueRef::Bool(value) => Self::Bool(vec![value; missing], valid),
            ValueRef::Date(value) => Self::Date(vec![value; missing], valid),
            ValueRef::DateTime(value) => Self::DateTime(vec![value; missing], valid),
            ValueRef::Str(_) => {
                let mut strings = Strings::default();
                strings.resize_empty(missing);
                Self::Str(strings, valid)
            }
        }
    }

    fn make_mixed(&mut self) {
        if matches!(self, Self::Mixed(_)) {
            return;
        }
        let values = (0..self.len()).map(|index| self.value(index)).collect();
        *self = Self::Mixed(values);
    }

    /// Overwrite one row, keeping the buffer typed when the value fits.
    pub(super) fn set(&mut self, index: usize, value: ValueRef<'_>) {
        match (&mut *self, value) {
            (Self::Missing(_), ValueRef::Missing) => (),
            (Self::Missing(len), _) => {
                let len = *len;
                *self = Self::typed_like(value, len);
                self.set(index, value);
            }
            (Self::Int(values, valid), ValueRef::Int(value)) => {
                values[index] = value;
                valid.set(index, true);
            }
            (Self::Float(values, valid), ValueRef::Float(value)) => {
                values[index] = value;
                valid.set(index, true);
            }
            (Self::Bool(values, valid), ValueRef::Bool(value)) => {
                values[index] = value;
                valid.set(index, true);
            }
            (Self::Date(values, valid), ValueRef::Date(value)) => {
                values[index] = value;
                valid.set(index, true);
            }
            (Self::DateTime(values, valid), ValueRef::DateTime(value)) => {
                values[index] = value;
                valid.set(index, true);
            }
            (Self::Str(values, valid), ValueRef::Str(value)) => {
                values.set(index, value);
                valid.set(index, true);
            }
            (Self::Mixed(values), value) => values[index] = own(value),
            (
                Self::Int(_, valid)
                | Self::Float(_, valid)
                | Self::Bool(_, valid)
                | Self::Date(_, valid)
                | Self::DateTime(_, valid)
                | Self::Str(_, valid),
                ValueRef::Missing,
            ) => valid.set(index, false),
            (_, value) => {
                self.make_mixed();
                self.set(index, value);
            }
        }
    }

    /// Overwrite the rows from `start` with `values`, leaving the buffer as
    /// `set` of each row in order does: its layout, validity, payloads and
    /// placeholders, and each row's text, though the arena may hold a
    /// repeated text once. Payloads of one type are copied under their
    /// validity a word at a time, and text in runs. Any other pairing of
    /// layouts, which widens the buffer, sets one row at a time.
    pub(super) fn set_range(&mut self, start: usize, values: &Values) {
        debug_assert!(start + values.len() <= self.len());
        if let Self::Missing(len) = *self {
            // `set` turns a missing buffer typed at the first present value;
            // setting a missing row of a missing buffer changes nothing.
            match values.first_present() {
                Some(value) => *self = Self::typed_like(value, len),
                None => return,
            }
        }
        let rows = start..start + values.len();
        match (&mut *self, values) {
            (Self::Int(to, valid), Self::Int(from, from_valid)) => {
                put_valid(to, valid, start, from, from_valid);
            }
            (Self::Float(to, valid), Self::Float(from, from_valid)) => {
                put_valid(to, valid, start, from, from_valid);
            }
            (Self::Bool(to, valid), Self::Bool(from, from_valid)) => {
                put_valid(to, valid, start, from, from_valid);
            }
            (Self::Date(to, valid), Self::Date(from, from_valid)) => {
                put_valid(to, valid, start, from, from_valid);
            }
            (Self::DateTime(to, valid), Self::DateTime(from, from_valid)) => {
                put_valid(to, valid, start, from, from_valid);
            }
            (Self::Str(to, valid), Self::Str(from, from_valid)) => {
                to.set_from(start, from, from_valid);
                valid.copy_from(start, from_valid, 0..from.len());
            }
            (
                Self::Int(_, valid)
                | Self::Float(_, valid)
                | Self::Bool(_, valid)
                | Self::Date(_, valid)
                | Self::DateTime(_, valid)
                | Self::Str(_, valid),
                Self::Missing(_),
            ) => valid.fill(rows, false),
            _ => {
                for (index, row) in rows.enumerate() {
                    self.set(row, values.get(index));
                }
            }
        }
    }

    /// The first value present, in row order.
    fn first_present(&self) -> Option<ValueRef<'_>> {
        match self {
            Self::Missing(_) => None,
            Self::Int(_, valid)
            | Self::Float(_, valid)
            | Self::Bool(_, valid)
            | Self::Date(_, valid)
            | Self::DateTime(_, valid)
            | Self::Str(_, valid) => valid.first_set().map(|row| self.get(row)),
            Self::Mixed(values) => values
                .iter()
                .map(ValueRef::from)
                .find(|value| *value != ValueRef::Missing),
        }
    }

    /// Grow to `len` rows with missing values, by count: the rows and
    /// placeholders `push_missing` would leave, one at a time.
    pub(super) fn resize_missing(&mut self, len: usize) {
        if len < self.len() {
            self.truncate(len);
        }
        if len == self.len() {
            return;
        }
        match self {
            Self::Missing(rows) => *rows = len,
            Self::Int(values, valid) => {
                values.resize(len, 0);
                valid.grow(len);
            }
            Self::Float(values, valid) => {
                values.resize(len, FiniteFloat::new(0.0).expect("zero is finite"));
                valid.grow(len);
            }
            Self::Bool(values, valid) => {
                values.resize(len, false);
                valid.grow(len);
            }
            // A temporal placeholder copies the last row; an empty typed
            // buffer is the same as an empty missing one.
            Self::Date(values, valid) => match values.last().copied() {
                Some(placeholder) => {
                    values.resize(len, placeholder);
                    valid.grow(len);
                }
                None => *self = Self::Missing(len),
            },
            Self::DateTime(values, valid) => match values.last().copied() {
                Some(placeholder) => {
                    values.resize(len, placeholder);
                    valid.grow(len);
                }
                None => *self = Self::Missing(len),
            },
            Self::Str(values, valid) => {
                values.resize_empty(len);
                valid.grow(len);
            }
            Self::Mixed(values) => values.resize(len, Value::Missing),
        }
    }

    /// Keep, from `first` on, only the rows at `kept`, ascending rows from
    /// `first` on, moved down in place. The buffer is left as appending the
    /// kept rows to its first `first` rows leaves it.
    pub(super) fn retain(&mut self, first: usize, kept: &[usize]) {
        fn compact<T: Copy>(values: &mut Vec<T>, first: usize, kept: &[usize]) {
            for (offset, &row) in kept.iter().enumerate() {
                values[first + offset] = values[row];
            }
            values.truncate(first + kept.len());
        }
        let len = first + kept.len();
        match self {
            Self::Missing(rows) => *rows = len,
            Self::Int(values, valid) => {
                compact(values, first, kept);
                valid.retain(first, kept);
            }
            Self::Float(values, valid) => {
                compact(values, first, kept);
                valid.retain(first, kept);
            }
            Self::Bool(values, valid) => {
                compact(values, first, kept);
                valid.retain(first, kept);
            }
            Self::Date(values, valid) => {
                compact(values, first, kept);
                valid.retain(first, kept);
            }
            Self::DateTime(values, valid) => {
                compact(values, first, kept);
                valid.retain(first, kept);
            }
            Self::Str(values, valid) => {
                compact(&mut values.spans, first, kept);
                valid.retain(first, kept);
            }
            Self::Mixed(values) => {
                for (offset, &row) in kept.iter().enumerate() {
                    values.swap(first + offset, row);
                }
                values.truncate(len);
            }
        }
        // An emptied temporal buffer is a missing one, and stays missing
        // while the rows appended to it hold no value.
        if let (0, Self::Date(_, valid) | Self::DateTime(_, valid)) = (first, &*self) {
            if len > 0 && !valid.any() {
                *self = Self::Missing(len);
            }
        }
    }

    /// Whether every present value has the column type `kind`.
    pub(super) fn has_type(&self, kind: crate::value::ColumnType) -> bool {
        use crate::value::ColumnType as K;
        matches!(
            (self, kind),
            (Self::Missing(_), _)
                | (Self::Int(..), K::Int)
                | (Self::Float(..), K::Float)
                | (Self::Date(..), K::Date)
                | (Self::DateTime(..), K::DateTime)
                | (Self::Str(..), K::Str)
        )
    }

    pub(super) fn truncate(&mut self, len: usize) {
        match self {
            Self::Missing(rows) => *rows = (*rows).min(len),
            Self::Int(values, valid) => {
                values.truncate(len);
                valid.truncate(len);
            }
            Self::Float(values, valid) => {
                values.truncate(len);
                valid.truncate(len);
            }
            Self::Bool(values, valid) => {
                values.truncate(len);
                valid.truncate(len);
            }
            Self::Date(values, valid) => {
                values.truncate(len);
                valid.truncate(len);
            }
            Self::DateTime(values, valid) => {
                values.truncate(len);
                valid.truncate(len);
            }
            Self::Str(values, valid) => {
                values.truncate(len);
                valid.truncate(len);
            }
            Self::Mixed(values) => values.truncate(len),
        }
    }

    /// The values at `rows`, in that order, in a buffer sized to them: a
    /// typed buffer gathers its payloads in one pass, and stays typed while
    /// any gathered row holds a value.
    pub(crate) fn gather(&self, rows: &[usize]) -> Result<Self, TryReserveError> {
        fn pick<T: Copy>(values: &[T], rows: &[usize]) -> Result<Vec<T>, TryReserveError> {
            let mut picked = Vec::new();
            picked.try_reserve_exact(rows.len())?;
            picked.extend(rows.iter().map(|&row| values[row]));
            Ok(picked)
        }
        let gathered = match self {
            Self::Missing(_) => Self::Missing(rows.len()),
            Self::Int(values, valid) => Self::Int(pick(values, rows)?, valid.gather(rows)?),
            Self::Float(values, valid) => Self::Float(pick(values, rows)?, valid.gather(rows)?),
            Self::Bool(values, valid) => Self::Bool(pick(values, rows)?, valid.gather(rows)?),
            Self::Date(values, valid) => Self::Date(pick(values, rows)?, valid.gather(rows)?),
            Self::DateTime(values, valid) => {
                Self::DateTime(pick(values, rows)?, valid.gather(rows)?)
            }
            Self::Str(values, valid) => Self::Str(values.gather(rows)?, valid.gather(rows)?),
            // A mixed buffer's rows may share one type, as the buffer they
            // came from did before it turned mixed.
            Self::Mixed(_) => {
                let mut gathered = Self::Missing(0);
                gathered.try_reserve(rows.len())?;
                for &row in rows {
                    gathered.push(self.get(row));
                }
                gathered
            }
        };
        Ok(gathered.or_missing())
    }

    /// Append rows `range` of `other`: payloads copied in bulk when both
    /// buffers hold one type, or into an empty buffer; a row at a time
    /// otherwise, as `push` keeps the buffer typed.
    pub(super) fn extend_from(&mut self, other: &Values, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        match (&mut *self, other) {
            (Self::Missing(len), Self::Missing(_)) => *len += range.len(),
            (_, Self::Missing(_)) => {
                let len = self.len() + range.len();
                self.resize_missing(len);
            }
            (Self::Int(values, valid), Self::Int(from, from_valid)) => {
                values.extend_from_slice(&from[range.clone()]);
                valid.extend_from(from_valid, range);
            }
            (Self::Float(values, valid), Self::Float(from, from_valid)) => {
                values.extend_from_slice(&from[range.clone()]);
                valid.extend_from(from_valid, range);
            }
            (Self::Bool(values, valid), Self::Bool(from, from_valid)) => {
                values.extend_from_slice(&from[range.clone()]);
                valid.extend_from(from_valid, range);
            }
            (Self::Date(values, valid), Self::Date(from, from_valid)) => {
                values.extend_from_slice(&from[range.clone()]);
                valid.extend_from(from_valid, range);
            }
            (Self::DateTime(values, valid), Self::DateTime(from, from_valid)) => {
                values.extend_from_slice(&from[range.clone()]);
                valid.extend_from(from_valid, range);
            }
            (Self::Str(values, valid), Self::Str(from, from_valid)) => {
                values.extend_from(from, range.clone());
                valid.extend_from(from_valid, range);
            }
            (
                Self::Missing(0),
                Self::Int(..)
                | Self::Float(..)
                | Self::Bool(..)
                | Self::Date(..)
                | Self::DateTime(..)
                | Self::Str(..),
            ) => {
                *self = other.empty_like();
                self.extend_from(other, range);
                *self = std::mem::take(self).or_missing();
            }
            _ => {
                for row in range {
                    self.push(other.get(row));
                }
            }
        }
    }

    /// For each of `rows`, whether its value equals the previous row's as
    /// grouping keys compare them (`Key::from`): a missing value equals a
    /// missing one, and values of one type compare by value, temporal ones
    /// by their civil fields. The first row repeats none. Each layout is
    /// compared in one pass, with no value lent.
    pub(super) fn repeats(&self, rows: &[usize]) -> Result<Vec<bool>, TryReserveError> {
        self.repeats_by(rows.len(), |index| rows[index])
    }

    /// [`Values::repeats`] over the rows of a range, in order.
    pub(super) fn repeats_in(&self, rows: Range<usize>) -> Result<Vec<bool>, TryReserveError> {
        self.repeats_by(rows.len(), |index| rows.start + index)
    }

    /// [`Values::repeats`] over `count` rows, `row(index)` the row at each
    /// index.
    fn repeats_by(
        &self,
        count: usize,
        row: impl Fn(usize) -> usize,
    ) -> Result<Vec<bool>, TryReserveError> {
        fn typed<T>(
            values: &[T],
            valid: &Bits,
            pairs: impl Iterator<Item = (usize, usize)>,
            repeats: &mut Vec<bool>,
            equal: impl Fn(&T, &T) -> bool,
        ) {
            repeats.extend(pairs.map(|(previous, row)| {
                match (valid.get(previous), valid.get(row)) {
                    (true, true) => equal(&values[previous], &values[row]),
                    (present, other) => present == other,
                }
            }));
        }
        let mut repeats = Vec::new();
        repeats.try_reserve_exact(count)?;
        if count == 0 {
            return Ok(repeats);
        }
        repeats.push(false);
        let pairs = (1..count).map(|index| (row(index - 1), row(index)));
        match self {
            Self::Missing(_) => repeats.resize(count, true),
            Self::Int(values, valid) => typed(values, valid, pairs, &mut repeats, |a, b| a == b),
            Self::Float(values, valid) => typed(values, valid, pairs, &mut repeats, |a, b| a == b),
            Self::Bool(values, valid) => typed(values, valid, pairs, &mut repeats, |a, b| a == b),
            Self::Date(values, valid) => typed(values, valid, pairs, &mut repeats, |a, b| a == b),
            Self::DateTime(values, valid) => {
                typed(values, valid, pairs, &mut repeats, |a, b| a == b);
            }
            Self::Str(strings, valid) => {
                repeats.extend(pairs.map(|(previous, row)| {
                    match (valid.get(previous), valid.get(row)) {
                        (true, true) => {
                            same_bytes(strings.bytes_of(previous), strings.bytes_of(row))
                        }
                        (present, other) => present == other,
                    }
                }));
            }
            Self::Mixed(values) => repeats.extend(pairs.map(|(previous, row)| {
                ValueRef::from(&values[previous]) == ValueRef::from(&values[row])
            })),
        }
        Ok(repeats)
    }

    /// An empty buffer of the same layout.
    fn empty_like(&self) -> Self {
        match self {
            Self::Missing(_) => Self::Missing(0),
            Self::Int(..) => Self::Int(Vec::new(), Bits::default()),
            Self::Float(..) => Self::Float(Vec::new(), Bits::default()),
            Self::Bool(..) => Self::Bool(Vec::new(), Bits::default()),
            Self::Date(..) => Self::Date(Vec::new(), Bits::default()),
            Self::DateTime(..) => Self::DateTime(Vec::new(), Bits::default()),
            Self::Str(..) => Self::Str(Strings::default(), Bits::default()),
            Self::Mixed(_) => Self::Mixed(Vec::new()),
        }
    }
}

/// Whether two byte strings are equal. Text up to 16 bytes, as most key
/// text is, compares in at most two overlapping reads of each, not through
/// a call to `memcmp`.
fn same_bytes(left: &[u8], right: &[u8]) -> bool {
    let len = left.len();
    if len != right.len() {
        return false;
    }
    let word = |bytes: &[u8], at: usize| {
        u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"))
    };
    let half = |bytes: &[u8], at: usize| {
        u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
    };
    match len {
        0 => true,
        1..=3 => {
            left[0] == right[0]
                && left[len / 2] == right[len / 2]
                && left[len - 1] == right[len - 1]
        }
        4..=7 => half(left, 0) == half(right, 0) && half(left, len - 4) == half(right, len - 4),
        8..=16 => word(left, 0) == word(right, 0) && word(left, len - 8) == word(right, len - 8),
        _ => left == right,
    }
}

/// Overwrite `to` from `start` with the values of `from` that `from_valid`
/// marks, and the validity of those rows with `from_valid`, a word at a
/// time. A row `from` holds missing keeps its placeholder, as `set` does.
fn put_valid<T: Copy>(to: &mut [T], valid: &mut Bits, start: usize, from: &[T], from_valid: &Bits) {
    let mut offset = 0;
    while offset < from.len() {
        let count = (from.len() - offset).min(64);
        let word = from_valid.word_at(offset, count);
        let at = start + offset;
        if word == low_mask(count) {
            to[at..at + count].copy_from_slice(&from[offset..offset + count]);
        } else {
            let mut rest = word;
            while rest != 0 {
                let bit = rest.trailing_zeros() as usize;
                to[at + bit] = from[offset + bit];
                rest &= rest - 1;
            }
        }
        offset += count;
    }
    valid.copy_from(start, from_valid, 0..from.len());
}

/// Rows whose values come from several sub-selections, reassembled in the
/// order of the whole selection. Rows no part covers are missing.
pub(super) struct Assembly {
    slots: Vec<(u32, u32)>,
    parts: Vec<Values>,
}

impl Assembly {
    pub(super) fn new(len: usize) -> Self {
        Self {
            slots: vec![(u32::MAX, 0); len],
            parts: Vec::new(),
        }
    }

    /// The part `values` answers the selection positions `positions`.
    pub(super) fn part(&mut self, positions: &[usize], values: Values) {
        let part = self.parts.len() as u32;
        for (index, &position) in positions.iter().enumerate() {
            self.slots[position] = (part, index as u32);
        }
        self.parts.push(values);
    }

    pub(super) fn finish(self) -> Values {
        if self.parts.len() == 1
            && self
                .slots
                .iter()
                .enumerate()
                .all(|(position, &(part, index))| part == 0 && index as usize == position)
        {
            return self.parts.into_iter().next().expect("one part");
        }
        let mut values = Values::Missing(0);
        for &(part, index) in &self.slots {
            if part == u32::MAX {
                values.push(ValueRef::Missing);
            } else {
                values.push(self.parts[part as usize].get(index as usize));
            }
        }
        values
    }
}

/// Copy a lent value, keeping temporal precision.
pub(super) fn own(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Missing,
        ValueRef::Str(value) => Value::Str(value.into()),
        ValueRef::Int(value) => Value::Int(value),
        ValueRef::Float(value) => Value::Float(value),
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(value) => Value::Date(value),
        ValueRef::DateTime(value) => Value::DateTime(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The seam is a join's repeat check (#4210) and a window grouping's runs
    // (#4209): each row compared with the previous one as grouping keys
    // compare them (`Key::from`), so a row taken as a repeat matches the
    // records the previous row matched, and is in its group. A range of rows
    // repeats as the same rows listed do.
    #[test]
    fn repeats_compare_each_row_with_the_previous_as_grouping_keys_do() {
        use crate::temporal::{Date, Precision};
        let pushed = |values: &[ValueRef<'_>]| {
            let mut buffer = Values::missing(0);
            for &value in values {
                buffer.push(value);
            }
            buffer
        };
        let float = |value: f64| ValueRef::Float(FiniteFloat::new(value).unwrap());
        let day = Date::new(2024, 3, 1, Precision::Day).unwrap();
        let month = Date::new(2024, 3, 1, Precision::Month).unwrap();
        let cases: Vec<(&str, Values, Vec<usize>, Vec<bool>)> = vec![
            (
                "integers, a missing value repeating a missing one",
                pushed(&[
                    ValueRef::Int(1),
                    ValueRef::Int(1),
                    ValueRef::Missing,
                    ValueRef::Missing,
                    ValueRef::Int(2),
                ]),
                (0..5).collect(),
                vec![false, true, false, true, false],
            ),
            (
                "both zeros are one key",
                pushed(&[float(0.0), float(-0.0), float(1.5)]),
                (0..3).collect(),
                vec![false, true, false],
            ),
            (
                "dates by their civil fields, whatever their precision",
                pushed(&[ValueRef::Date(day), ValueRef::Date(month)]),
                vec![0, 1],
                vec![false, true],
            ),
            (
                "text by its bytes",
                pushed(&[
                    ValueRef::Str("A"),
                    ValueRef::Str("A"),
                    ValueRef::Str("a"),
                    ValueRef::Missing,
                    ValueRef::Str(""),
                ]),
                (0..5).collect(),
                vec![false, true, false, false, false],
            ),
            (
                "text of every length by all of its bytes",
                pushed(&[
                    ValueRef::Str("abc"),
                    ValueRef::Str("abc"),
                    ValueRef::Str("abd"),
                    ValueRef::Str("PERF-01"),
                    ValueRef::Str("PERF-01"),
                    ValueRef::Str("PERF-02"),
                    ValueRef::Str("PERF-01-000001"),
                    ValueRef::Str("PERF-01-000001"),
                    ValueRef::Str("PERF-02-000001"),
                    ValueRef::Str("a long subject identifier"),
                    ValueRef::Str("a long subject identifier"),
                    ValueRef::Str("a long subject identifiez"),
                    ValueRef::Str("a long subject identifie"),
                ]),
                (0..13).collect(),
                vec![
                    false, true, false, false, true, false, false, true, false, false, true, false,
                    false,
                ],
            ),
            (
                "an int and a float of equal value are two keys",
                pushed(&[ValueRef::Int(2), float(2.0), float(2.0)]),
                (0..3).collect(),
                vec![false, false, true],
            ),
            (
                "rows in selection order",
                pushed(&[ValueRef::Int(5), ValueRef::Int(6), ValueRef::Int(5)]),
                vec![0, 2, 1],
                vec![false, true, false],
            ),
            (
                "every row missing",
                Values::missing(3),
                (0..3).collect(),
                vec![false, true, true],
            ),
        ];
        for (label, values, rows, expected) in cases {
            assert_eq!(values.repeats(&rows).unwrap(), expected, "{label}");
            if rows.windows(2).all(|pair| pair[1] == pair[0] + 1) {
                let range = rows[0]..rows[0] + rows.len();
                assert_eq!(
                    values.repeats_in(range).unwrap(),
                    expected,
                    "{label}, a range"
                );
            }
        }
    }

    #[test]
    fn a_buffer_stays_typed_until_a_second_type_arrives() {
        let mut values = Values::missing(1);
        values.push(ValueRef::Int(3));
        values.push(ValueRef::Missing);
        assert!(matches!(values, Values::Int(..)));
        assert_eq!(values.get(0), ValueRef::Missing);
        assert_eq!(values.get(1), ValueRef::Int(3));
        values.push(ValueRef::Str("a"));
        assert!(matches!(values, Values::Mixed(_)));
        assert_eq!(values.get(3), ValueRef::Str("a"));
        assert_eq!(values.len(), 4);
    }

    #[test]
    fn text_rows_can_be_overwritten_in_place() {
        let mut values = Values::missing(3);
        values.set(1, ValueRef::Str("first"));
        values.set(1, ValueRef::Str("second"));
        values.set(2, ValueRef::Str(""));
        assert_eq!(values.get(0), ValueRef::Missing);
        assert_eq!(values.get(1), ValueRef::Str("second"));
        assert_eq!(values.get(2), ValueRef::Str(""));
        values.set(1, ValueRef::Missing);
        assert_eq!(values.get(1), ValueRef::Missing);
    }

    #[test]
    fn an_assembly_reorders_parts_by_selection_position() {
        let mut assembly = Assembly::new(4);
        assembly.part(&[3, 0], Values::repeat(ValueRef::Int(1), 2));
        assembly.part(&[1], Values::repeat(ValueRef::Str("x"), 1));
        let values = assembly.finish();
        assert_eq!(values.get(0), ValueRef::Int(1));
        assert_eq!(values.get(1), ValueRef::Str("x"));
        assert_eq!(values.get(2), ValueRef::Missing);
        assert_eq!(values.get(3), ValueRef::Int(1));
    }

    /// Every row's value of a buffer.
    fn read(values: &Values) -> Vec<ValueRef<'_>> {
        (0..values.len()).map(|row| values.get(row)).collect()
    }

    // The seam is the output key check's missing rows (#4211), read from a
    // buffer's bitmap a word at a time: they must be the rows that read
    // missing, across word boundaries and up to the buffer's length.
    #[test]
    fn missing_rows_are_the_rows_that_read_missing() {
        let mut ints = Values::missing(0);
        for row in 0..130 {
            ints.push(if [0, 63, 64, 129].contains(&row) {
                ValueRef::Missing
            } else {
                ValueRef::Int(row as i64)
            });
        }
        let cases = [
            ("ints over three words", ints, vec![0, 63, 64, 129]),
            ("every row missing", Values::missing(3), vec![0, 1, 2]),
            (
                "mixed values",
                Values::Mixed(vec![Value::Int(1), Value::Missing, Value::Str("a".into())]),
                vec![1],
            ),
        ];
        for (label, values, expected) in cases {
            assert_eq!(values.missing_rows(), expected, "{label}");
        }
    }

    // The seam is the bulk paths of a crate-private buffer (#4190): each
    // must read back what pushing the same values a row at a time reads.
    #[test]
    fn a_lent_column_gathers_what_each_row_reads_and_stays_missing_without_a_value() {
        let words = [0b1101u64];
        let ints = ColumnRef::new(ColumnValues::Int(&[5, 0, 7, 9]), Some(&words)).unwrap();
        let gathered = Values::from_column(ints, &[3, 1, 0, 1]).unwrap().unwrap();
        assert_eq!(
            read(&gathered),
            [
                ValueRef::Int(9),
                ValueRef::Missing,
                ValueRef::Int(5),
                ValueRef::Missing
            ],
            "ints at rows 3, 1, 0, 1, row 1 missing by the bitmap"
        );
        let only_missing = Values::from_column(ints, &[1, 1]).unwrap().unwrap();
        assert!(
            matches!(only_missing, Values::Missing(2)),
            "rows without a value gather a missing buffer, as pushing them does"
        );
        assert!(
            Values::from_column(ints, &[4]).is_none(),
            "a row outside the column is left to the cell read"
        );
        let text = ColumnRef::new(
            ColumnValues::Str {
                offsets: &[0, 1, 1, 3],
                bytes: "a\u{e9}",
            },
            None,
        )
        .unwrap();
        let gathered = Values::from_column(text, &[2, 0, 1]).unwrap().unwrap();
        assert_eq!(
            read(&gathered),
            [
                ValueRef::Str("\u{e9}"),
                ValueRef::Str("a"),
                ValueRef::Str("")
            ],
            "text rows 2, 0, 1, row 1 empty and present"
        );
        let day = Date::new(2024, 2, 29, crate::temporal::Precision::Day).unwrap();
        let days = [None, Some(day), None];
        let dates = ColumnRef::new(ColumnValues::Date(&days), None).unwrap();
        let gathered = Values::from_column(dates, &[0, 1, 2]).unwrap().unwrap();
        assert_eq!(
            read(&gathered),
            [ValueRef::Missing, ValueRef::Date(day), ValueRef::Missing],
            "dates with missing rows before and after the present one"
        );
    }

    #[test]
    fn gathered_extended_and_repeated_buffers_read_the_values_pushed_a_row_at_a_time() {
        let mut ints = Values::missing(0);
        for value in [
            ValueRef::Missing,
            ValueRef::Int(1),
            ValueRef::Int(2),
            ValueRef::Missing,
        ] {
            ints.push(value);
        }
        let gathered = ints.gather(&[2, 0, 1]).unwrap();
        assert_eq!(
            read(&gathered),
            [ValueRef::Int(2), ValueRef::Missing, ValueRef::Int(1)],
            "ints gathered at rows 2, 0, 1"
        );
        assert!(matches!(ints.gather(&[0, 3]).unwrap(), Values::Missing(2)));
        let mut extended = Values::missing(0);
        extended.extend_from(&ints, 1..4);
        extended.extend_from(&ints, 0..2);
        assert_eq!(
            read(&extended),
            [
                ValueRef::Int(1),
                ValueRef::Int(2),
                ValueRef::Missing,
                ValueRef::Missing,
                ValueRef::Int(1)
            ],
            "ints 1..4 then 0..2 appended to an empty buffer"
        );
        let mut text = Values::missing(0);
        for value in ["ab", "c", "de"] {
            text.push(ValueRef::Str(value));
        }
        // An overwritten row's text leaves the arena out of row order.
        text.set(0, ValueRef::Str("xyz"));
        let mut extended = Values::repeat(ValueRef::Str("q"), 2);
        extended.extend_from(&text, 1..3);
        extended.extend_from(&text, 0..2);
        assert_eq!(
            read(&extended),
            ["q", "q", "c", "de", "xyz", "c"].map(ValueRef::Str),
            "text appended from rows in and out of arena order"
        );
        let mut mixed = Values::repeat(ValueRef::Missing, 1);
        mixed.extend_from(&text, 1..2);
        assert_eq!(read(&mixed), [ValueRef::Missing, ValueRef::Str("c")]);
        assert!(matches!(
            Values::repeat(ValueRef::Int(1), 0),
            Values::Missing(0)
        ));
    }

    /// A buffer of `values` pushed one row at a time.
    fn pushed(values: &[ValueRef<'_>]) -> Values {
        let mut buffer = Values::missing(0);
        for &value in values {
            buffer.push(value);
        }
        buffer
    }

    /// `len` values, every `gap`-th missing, the rest `value(row)`.
    fn every<'v>(
        len: usize,
        gap: usize,
        value: impl Fn(usize) -> ValueRef<'v>,
    ) -> Vec<ValueRef<'v>> {
        (0..len)
            .map(|row| {
                if row % gap == 0 {
                    ValueRef::Missing
                } else {
                    value(row)
                }
            })
            .collect()
    }

    fn day(day: u8) -> Date {
        Date::new(2024, 1, day, crate::temporal::Precision::Day).unwrap()
    }

    fn float(value: f64) -> FiniteFloat {
        FiniteFloat::new(value).unwrap()
    }

    // The seam is the bulk completion of a crate-private buffer (#4208):
    // writing a run of rows must leave the buffer, its layout, payloads,
    // placeholders, validity and each row's text, exactly as `set` of each
    // row of the run in order leaves it.
    #[test]
    fn a_range_set_leaves_the_buffer_as_setting_each_row_in_order() {
        let names = ["a", "bc", "", "def"];
        let mut out_of_order = pushed(&["x", "yz", "w"].map(ValueRef::Str));
        out_of_order.set(0, ValueRef::Str("moved"));
        let mut emptied = pushed(&[ValueRef::Int(4), ValueRef::Int(5)]);
        emptied.set(0, ValueRef::Missing);
        emptied.set(1, ValueRef::Missing);
        let half_ints = || {
            let mut buffer = pushed(&every(60, 5, |row| ValueRef::Int(row as i64)));
            buffer.resize_missing(140);
            buffer
        };
        let cases: Vec<(&str, Values, usize, Values)> = vec![
            (
                "ints with missing rows into a missing column, across two bitmap words",
                Values::missing(140),
                60,
                pushed(&every(70, 7, |row| ValueRef::Int(row as i64))),
            ),
            (
                "ints into an int column whose earlier rows are complete",
                half_ints(),
                60,
                pushed(&every(80, 3, |row| ValueRef::Int(-(row as i64)))),
            ),
            (
                "a run that starts with missing rows takes the type of its first value",
                Values::missing(10),
                2,
                pushed(&[
                    ValueRef::Missing,
                    ValueRef::Missing,
                    ValueRef::Float(float(1.5)),
                ]),
            ),
            (
                "floats into an int column widen it to mixed",
                half_ints(),
                60,
                pushed(&[ValueRef::Float(float(0.5)), ValueRef::Missing]),
            ),
            (
                "mixed values into a missing column",
                Values::missing(6),
                1,
                pushed(&[ValueRef::Int(1), ValueRef::Str("s"), ValueRef::Missing]),
            ),
            (
                "only missing values leave a missing column missing",
                Values::missing(6),
                1,
                Values::missing(4),
            ),
            (
                "an int buffer holding no value leaves a missing column missing",
                Values::missing(6),
                3,
                emptied,
            ),
            (
                "missing values clear the validity of a typed run",
                pushed(&[ValueRef::Int(1); 70]),
                3,
                Values::missing(65),
            ),
            (
                "text with missing rows into a missing column, across two bitmap words",
                Values::missing(150),
                50,
                pushed(&every(90, 4, |row| ValueRef::Str(names[row % 4]))),
            ),
            (
                "text present on every row into a text column, across two bitmap words",
                pushed(&["p"; 140].map(ValueRef::Str)),
                10,
                pushed(&every(100, 1000, |row| ValueRef::Str(names[row % 4]))[1..]),
            ),
            (
                "a repeated text into a text column",
                pushed(&["p", "q", "r"].map(ValueRef::Str)),
                1,
                Values::repeat(ValueRef::Str("same"), 2),
            ),
            (
                "text out of arena order into a text column",
                pushed(&["p", "q", "r", "s", "t"].map(ValueRef::Str)),
                1,
                out_of_order,
            ),
            (
                "dates into a date column with missing rows",
                pushed(&every(66, 2, |row| {
                    ValueRef::Date(day(1 + (row % 28) as u8))
                })),
                64,
                pushed(&[ValueRef::Missing, ValueRef::Date(day(9))]),
            ),
            (
                "booleans into a missing column",
                Values::missing(5),
                0,
                pushed(&[
                    ValueRef::Bool(true),
                    ValueRef::Missing,
                    ValueRef::Bool(false),
                ]),
            ),
            (
                "text into a mixed column",
                pushed(&[ValueRef::Int(1), ValueRef::Str("a"), ValueRef::Missing]),
                1,
                pushed(&[ValueRef::Str("b"), ValueRef::Str("c")]),
            ),
        ];
        for (label, initial, start, values) in cases {
            let mut bulk = initial.clone();
            bulk.set_range(start, &values);
            let mut cells = initial;
            for row in 0..values.len() {
                cells.set(start + row, values.get(row));
            }
            assert_eq!(bulk.canonical(), cells.canonical(), "{label}");
        }
    }

    // Growing by count must leave every layout as pushing one missing row
    // at a time does.
    #[test]
    fn growing_by_count_leaves_the_buffer_as_pushing_missing_rows() {
        let mut emptied_dates = pushed(&[ValueRef::Date(day(3))]);
        emptied_dates.truncate(0);
        let cases: Vec<(&str, Values)> = vec![
            ("missing", Values::missing(2)),
            ("ints", pushed(&[ValueRef::Int(1)])),
            ("floats", pushed(&[ValueRef::Float(float(2.0))])),
            ("booleans", pushed(&[ValueRef::Bool(true)])),
            (
                "dates",
                pushed(&[ValueRef::Date(day(3)), ValueRef::Missing]),
            ),
            ("an emptied date buffer", emptied_dates),
            ("text", pushed(&[ValueRef::Str("ab")])),
            ("mixed", pushed(&[ValueRef::Int(1), ValueRef::Str("a")])),
        ];
        for (label, initial) in cases {
            for len in [initial.len(), 70, 129] {
                let mut grown = initial.clone();
                grown.resize_missing(len);
                let mut pushed = initial.clone();
                while pushed.len() < len {
                    pushed.push(ValueRef::Missing);
                }
                assert_eq!(
                    format!("{grown:?}"),
                    format!("{pushed:?}"),
                    "{label} to {len} rows"
                );
            }
        }
    }

    // Filtering in place must leave the layout and values that appending
    // the kept rows to the first rows leaves.
    #[test]
    fn a_retained_buffer_reads_as_its_kept_rows_appended_to_its_head() {
        let ints = pushed(&every(140, 3, |row| ValueRef::Int(row as i64)));
        let dates = pushed(&[ValueRef::Missing, ValueRef::Date(day(2)), ValueRef::Missing]);
        let mixed = pushed(&[ValueRef::Int(1), ValueRef::Str("a"), ValueRef::Int(2)]);
        let text = pushed(&every(80, 5, |row| ValueRef::Str(["a", "bc"][row % 2])));
        let cases: Vec<(&str, Values, usize, Vec<usize>)> = vec![
            (
                "ints, dropping rows across bitmap words",
                ints.clone(),
                10,
                (10..140).filter(|row| row % 4 != 1).collect(),
            ),
            ("ints, keeping no row", ints, 64, vec![]),
            (
                "dates from the first row, keeping only missing rows",
                dates.clone(),
                0,
                vec![0, 2],
            ),
            (
                "dates from the first row, keeping a value",
                dates,
                0,
                vec![1, 2],
            ),
            (
                "mixed rows sharing one type stay mixed",
                mixed,
                0,
                vec![0, 2],
            ),
            (
                "text",
                text,
                3,
                (3..80).filter(|row| row % 3 == 0).collect(),
            ),
            ("missing", Values::missing(9), 2, vec![4, 8]),
        ];
        for (label, initial, first, kept) in cases {
            let mut retained = initial.clone();
            retained.retain(first, &kept);
            let mut appended = initial.clone();
            appended.truncate(first);
            for &row in &kept {
                appended.push(initial.get(row));
            }
            assert_eq!(
                (std::mem::discriminant(&retained), read(&retained)),
                (std::mem::discriminant(&appended), read(&appended)),
                "{label}"
            );
        }
    }

    // Word-at-a-time bit runs must read and write what one bit at a time
    // does, at every alignment.
    #[test]
    fn bit_runs_move_what_single_bits_do_at_every_alignment() {
        let source = Bits::from_fn(200, |index| index % 3 == 0 || index % 7 == 0).unwrap();
        for (at, start, len) in [
            (0, 0, 200),
            (3, 61, 70),
            (63, 1, 64),
            (64, 5, 65),
            (100, 130, 1),
            (7, 0, 0),
        ] {
            let mut words = Bits::new(250, true);
            words.copy_from(at, &source, start..start + len);
            let mut bits = Bits::new(250, true);
            for offset in 0..len {
                bits.set(at + offset, source.get(start + offset));
            }
            let label = format!("{len} bits from {start} to {at}");
            assert_eq!(words.words, bits.words, "{label}");
            assert_eq!(
                source.any_in(start..start + len),
                (start..start + len).any(|index| source.get(index)),
                "{label}"
            );
            let mut filled = Bits::new(250, false);
            filled.fill(at..at + len, true);
            assert_eq!(
                (0..250).map(|index| filled.get(index)).collect::<Vec<_>>(),
                (0..250)
                    .map(|index| (at..at + len).contains(&index))
                    .collect::<Vec<_>>(),
                "{label} filled"
            );
        }
        let mut extended = Bits::new(3, true);
        extended.extend_from(&source, 60..130);
        assert_eq!(
            (0..73).map(|index| extended.get(index)).collect::<Vec<_>>(),
            (0..3)
                .map(|_| true)
                .chain((60..130).map(|index| source.get(index)))
                .collect::<Vec<_>>(),
            "70 bits appended after 3"
        );
        assert_eq!(Bits::new(130, false).first_set(), None);
        let mut late = Bits::new(130, false);
        late.set(129, true);
        assert_eq!(late.first_set(), Some(129));
    }

    #[test]
    fn bits_grow_and_shrink() {
        let mut bits = Bits::new(70, true);
        assert!(bits.all());
        bits.set(65, false);
        assert!(!bits.get(65));
        bits.truncate(65);
        assert!(bits.all());
        bits.push(false);
        assert!(!bits.get(65));
        assert_eq!(bits.gather(&[0, 65]).unwrap().len(), 2);
    }
}
