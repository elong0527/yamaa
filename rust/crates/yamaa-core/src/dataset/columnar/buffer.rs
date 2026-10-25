//! Typed column buffers: one per closed value type with a validity mask,
//! text in one byte arena, and a mixed buffer for raw results whose type
//! varies by row. A buffer changes layout, never a value: every read lends
//! the same `ValueRef` a table port would.
use crate::{
    table::ValueRef,
    temporal::{Date, DateTime},
    value::{FiniteFloat, Value},
};
use std::collections::TryReserveError;

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
    pub(super) fn gather(&self, rows: &[usize]) -> Self {
        let mut bits = Self::new(0, false);
        for &row in rows {
            bits.push(self.get(row));
        }
        bits
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

    /// The same value on every row.
    pub(super) fn repeat(value: ValueRef<'_>, len: usize) -> Self {
        let mut values = Self::Missing(0);
        for _ in 0..len {
            values.push(value);
        }
        values
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

    /// Overwrite one row with an owned value.
    pub(super) fn set_value(&mut self, index: usize, value: Value) {
        match self {
            Self::Mixed(values) => values[index] = value,
            _ => self.set(index, ValueRef::from(&value)),
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

    /// The values at `rows`, in that order.
    pub(crate) fn gather(&self, rows: &[usize]) -> Self {
        let mut gathered = Self::Missing(0);
        for &row in rows {
            gathered.push(self.get(row));
        }
        gathered
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
        assert_eq!(bits.gather(&[0, 65]).len(), 2);
    }
}
