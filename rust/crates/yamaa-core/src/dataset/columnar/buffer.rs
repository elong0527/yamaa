//! Typed column buffers: one per closed value type with a validity mask,
//! text in one byte arena, and a mixed buffer for raw results whose type
//! varies by row. A buffer changes layout, never a value: every read lends
//! the same `ValueRef` a table port would.
use crate::{
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

    /// Append bits `range` of `other`.
    pub(super) fn extend_from(&mut self, other: &Bits, range: Range<usize>) {
        let start = self.len;
        self.len += range.len();
        self.words.resize(self.len.div_ceil(64), 0);
        for (offset, index) in range.enumerate() {
            if other.get(index) {
                self.set(start + offset, true);
            }
        }
    }

    fn clear_tail(&mut self) {
        let used = self.len % 64;
        if used != 0 {
            if let Some(last) = self.words.last_mut() {
                *last &= (1u64 << used) - 1;
            }
        }
    }

    pub(super) fn len(&self) -> usize {
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

    /// Whether every bit is set.
    #[cfg(test)]
    fn all(&self) -> bool {
        (0..self.len).all(|index| self.get(index))
    }

    /// Keep the bits at `rows`, in that order.
    pub(super) fn gather(&self, rows: &[usize]) -> Result<Self, TryReserveError> {
        Self::from_fn(rows.len(), |index| self.get(rows[index]))
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

    /// Append rows `range` of `other`: one copy of the arena when their
    /// text lies in it end to end, as a buffer filled in row order holds it.
    fn extend_from(&mut self, other: &Strings, range: Range<usize>) {
        let spans = &other.spans[range.clone()];
        let contiguous = spans.windows(2).all(|pair| pair[0].1 == pair[1].0);
        if let (true, Some(&(start, _)), Some(&(_, end))) =
            (contiguous, spans.first(), spans.last())
        {
            let base = self.bytes.len();
            self.bytes.push_str(&other.bytes[start..end]);
            self.spans.extend(
                spans
                    .iter()
                    .map(|&(from, to)| (from - start + base, to - start + base)),
            );
            return;
        }
        self.spans.reserve(range.len());
        self.bytes
            .reserve(range.clone().map(|row| other.get(row).len()).sum());
        for row in range {
            self.push(other.get(row));
        }
    }

    /// Overwrite one row; the replaced bytes stay in the arena.
    fn set(&mut self, index: usize, value: &str) {
        let start = self.bytes.len();
        self.bytes.push_str(value);
        self.spans[index] = (start, self.bytes.len());
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
                for _ in 0..missing {
                    strings.push_empty();
                }
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

    /// Grow to `len` rows with missing values.
    pub(super) fn resize_missing(&mut self, len: usize) {
        if len < self.len() {
            self.truncate(len);
        }
        if let Self::Missing(rows) = self {
            *rows = len;
            return;
        }
        while self.len() < len {
            self.push_missing();
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
