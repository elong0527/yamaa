//! Column storage of the engine's own tables: fixed-width values beside a
//! validity bitmap, and text in one byte arena. A layout, never a value:
//! every read gives back exactly the value that was stored, and a missing
//! slot holds no payload of its own.

/// Which slots of a column hold a value. No bitmap is kept until a slot is
/// missing, so a column without missing values carries none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Validity {
    len: usize,
    /// One bit per slot, least significant first, set when the slot holds a
    /// value; `None` while every slot does. Bits past `len` stay clear.
    words: Option<Vec<u64>>,
}

impl Validity {
    /// Append one slot.
    pub fn push(&mut self, present: bool) {
        let index = self.len;
        self.len += 1;
        match &mut self.words {
            Some(words) => {
                if index % 64 == 0 {
                    words.push(0);
                }
                if present {
                    words[index / 64] |= 1 << (index % 64);
                }
            }
            None if present => {}
            None => {
                // The first missing slot: every earlier slot holds a value.
                let mut words = vec![u64::MAX; index / 64];
                if index % 64 != 0 {
                    words.push((1u64 << (index % 64)) - 1);
                } else {
                    words.push(0);
                }
                self.words = Some(words);
            }
        }
    }

    /// Whether slot `index`, which must be below [`Validity::len`], holds a
    /// value.
    pub fn get(&self, index: usize) -> bool {
        debug_assert!(index < self.len);
        self.words
            .as_ref()
            .is_none_or(|words| words[index / 64] >> (index % 64) & 1 == 1)
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the column has no slots.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether any slot in `range` is missing.
    pub fn any_missing(&self, range: std::ops::Range<usize>) -> bool {
        self.words.is_some() && range.into_iter().any(|index| !self.get(index))
    }

    /// The bitmap, one bit per slot, least significant first, set when the
    /// slot holds a value; `None` while every slot does.
    pub fn words(&self) -> Option<&[u64]> {
        self.words.as_deref()
    }

    /// Slots `range`, a word at a time; still no bitmap when none of them
    /// is missing.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Self {
        let len = range.len();
        let Some(words) = &self.words else {
            return Self { len, words: None };
        };
        let shift = range.start % 64;
        let first = range.start / 64;
        let mut sliced: Vec<u64> = (0..len.div_ceil(64))
            .map(|index| {
                let low = words[first + index] >> shift;
                let high = match (shift, words.get(first + index + 1)) {
                    (0, _) | (_, None) => 0,
                    (_, Some(&next)) => next << (64 - shift),
                };
                low | high
            })
            .collect();
        if len % 64 != 0 {
            if let Some(last) = sliced.last_mut() {
                *last &= (1u64 << (len % 64)) - 1;
            }
        }
        let full = |index: usize, &word: &u64| {
            let used = (len - index * 64).min(64);
            word == u64::MAX >> (64 - used)
        };
        let all_present = sliced
            .iter()
            .enumerate()
            .all(|(index, word)| full(index, word));
        Self {
            len,
            words: (!all_present).then_some(sliced),
        }
    }
}

impl FromIterator<bool> for Validity {
    fn from_iter<I: IntoIterator<Item = bool>>(slots: I) -> Self {
        let mut validity = Self::default();
        validity.extend(slots);
        validity
    }
}

impl Extend<bool> for Validity {
    fn extend<I: IntoIterator<Item = bool>>(&mut self, slots: I) {
        for present in slots {
            self.push(present);
        }
    }
}

impl Validity {
    /// Append a slot for each of `slots`, as [`Validity::push`] does one at
    /// a time: still no bitmap while every slot holds a value, and whole
    /// words at a time once there is one.
    pub fn extend_from_slice(&mut self, mut slots: &[bool]) {
        if self.words.is_none() {
            let Some(first) = slots.iter().position(|present| !present) else {
                self.len += slots.len();
                return;
            };
            self.len += first;
            self.push(false);
            slots = &slots[first + 1..];
        }
        // Up to the next word boundary a slot at a time, then a word of 64.
        let head = (64 - self.len % 64) % 64;
        let (unaligned, rest) = slots.split_at(head.min(slots.len()));
        for &present in unaligned {
            self.push(present);
        }
        let words = self
            .words
            .as_mut()
            .expect("a bitmap once a slot is missing");
        let mut chunks = rest.chunks_exact(64);
        for chunk in &mut chunks {
            let word = chunk
                .iter()
                .enumerate()
                .fold(0u64, |word, (bit, &present)| {
                    word | u64::from(present) << bit
                });
            words.push(word);
        }
        self.len += rest.len() - chunks.remainder().len();
        for &present in chunks.remainder() {
            self.push(present);
        }
    }
}

impl Validity {
    /// Every one of `len` slots present, as a bitmap.
    fn present_words(len: usize) -> Vec<u64> {
        let mut words = vec![u64::MAX; len / 64];
        if len % 64 != 0 {
            words.push((1u64 << (len % 64)) - 1);
        }
        words
    }

    /// Append slots `range` of `other`, a word at a time: still no bitmap
    /// while every slot holds a value (#4212).
    pub fn extend_from(&mut self, other: &Self, range: std::ops::Range<usize>) {
        let len = range.len();
        let added = other.slice(range);
        let Some(added) = added.words else {
            match self.words {
                Some(_) => self.append_words(&Self::present_words(len), len),
                None => self.len += len,
            }
            return;
        };
        if self.words.is_none() {
            self.words = Some(Self::present_words(self.len));
        }
        self.append_words(&added, len);
    }

    /// Append `len` slots whose bits `added` holds, least significant first,
    /// to a validity that keeps a bitmap.
    fn append_words(&mut self, added: &[u64], len: usize) {
        let words = self.words.as_mut().expect("a bitmap to append to");
        let shift = self.len % 64;
        for &word in added {
            match (shift, words.last_mut()) {
                (0, _) | (_, None) => words.push(word),
                (_, Some(last)) => {
                    *last |= word << shift;
                    words.push(word >> (64 - shift));
                }
            }
        }
        self.len += len;
        words.truncate(self.len.div_ceil(64));
    }
}

/// Fixed-width values with their validity. A missing slot holds
/// `T::default()`, so a slot's payload never outlives its value.
#[derive(Clone, Debug, Default)]
pub struct Values<T> {
    values: Vec<T>,
    validity: Validity,
}

impl<T: Copy + Default> Values<T> {
    /// No slots yet, with room for `len`.
    pub fn with_capacity(len: usize) -> Self {
        Self {
            values: Vec::with_capacity(len),
            validity: Validity::default(),
        }
    }

    /// Every slot's payload with the slots' validity, as [`Values::payloads`]
    /// and [`Values::validity`] give them back: a missing slot's payload
    /// must be `T::default()`. `None` when their lengths differ.
    pub fn from_parts(values: Vec<T>, validity: Validity) -> Option<Self> {
        (values.len() == validity.len()).then_some(Self { values, validity })
    }

    /// The payloads and the validity, as [`Values::from_parts`] takes them.
    pub fn into_parts(self) -> (Vec<T>, Validity) {
        (self.values, self.validity)
    }

    /// Append one value, or a missing slot.
    pub fn push(&mut self, value: Option<T>) {
        self.validity.push(value.is_some());
        self.values.push(value.unwrap_or_default());
    }

    /// The value at `index`, which must be below [`Values::len`].
    pub fn get(&self, index: usize) -> Option<T> {
        self.validity.get(index).then(|| self.values[index])
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the column has no slots.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Every slot's payload, `T::default()` where missing.
    pub fn payloads(&self) -> &[T] {
        &self.values
    }

    pub fn validity(&self) -> &Validity {
        &self.validity
    }

    /// Slots `range`, copied whole.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Self {
        Self {
            validity: self.validity.slice(range.clone()),
            values: self.values[range].to_vec(),
        }
    }

    /// Append slots `range` of `other`, copied whole (#4212).
    pub fn extend_from(&mut self, other: &Self, range: std::ops::Range<usize>) {
        self.validity.extend_from(&other.validity, range.clone());
        self.values.extend_from_slice(&other.values[range]);
    }
}

impl<T: Copy + Default> FromIterator<Option<T>> for Values<T> {
    fn from_iter<I: IntoIterator<Item = Option<T>>>(values: I) -> Self {
        let mut column = Self::default();
        for value in values {
            column.push(value);
        }
        column
    }
}

/// Values compared exactly: a float by its bits, so `-0.0` differs from
/// `0.0`, as the bytes of the value do.
pub trait Exact: Copy {
    fn exact(self, other: Self) -> bool;
}

impl Exact for i32 {
    fn exact(self, other: Self) -> bool {
        self == other
    }
}

impl Exact for i64 {
    fn exact(self, other: Self) -> bool {
        self == other
    }
}

impl Exact for f32 {
    fn exact(self, other: Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Exact for f64 {
    fn exact(self, other: Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl<T: Exact + Default> PartialEq for Values<T> {
    /// The same slots, missing in the same places, with exactly the same
    /// values.
    fn eq(&self, other: &Self) -> bool {
        self.validity == other.validity
            && (0..self.len()).all(|index| match (self.get(index), other.get(index)) {
                (Some(left), Some(right)) => left.exact(right),
                (left, right) => left.is_none() && right.is_none(),
            })
    }
}

/// Text values in one byte arena: value `i` is `bytes[offsets[i]..offsets[i + 1]]`.
#[derive(Clone, Debug)]
pub struct Text {
    offsets: Vec<usize>,
    bytes: String,
    validity: Validity,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            offsets: vec![0],
            bytes: String::new(),
            validity: Validity::default(),
        }
    }
}

impl Text {
    /// No slots yet, with room for `len` and an arena of `bytes`.
    pub fn with_capacity(len: usize, bytes: usize) -> Self {
        let mut offsets = Vec::with_capacity(len + 1);
        offsets.push(0);
        Self {
            offsets,
            bytes: String::with_capacity(bytes),
            validity: Validity::default(),
        }
    }

    /// The text of byte arena `bytes`, value `i` being
    /// `bytes[offsets[i]..offsets[i + 1]]` where `validity` marks slot `i`
    /// present: `None` unless every value is UTF-8, each offset follows the
    /// one before it, the last is the arena's end, and a missing slot holds
    /// no bytes. The arena is UTF-8 and each offset a character boundary
    /// exactly when every value is UTF-8, so the arena is validated once.
    pub fn from_utf8_parts(
        offsets: Vec<usize>,
        bytes: Vec<u8>,
        validity: Validity,
    ) -> Option<Self> {
        let bytes = String::from_utf8(bytes).ok()?;
        // Every offset of ASCII text is a character boundary, and a column
        // without a missing slot has no slot to hold no bytes.
        let shaped = offsets.len() == validity.len() + 1
            && offsets.first() == Some(&0)
            && offsets.last() == Some(&bytes.len())
            && offsets.windows(2).all(|pair| pair[0] <= pair[1])
            && (bytes.is_ascii() || offsets.iter().all(|&offset| bytes.is_char_boundary(offset)))
            && (validity.words().is_none()
                || (offsets.windows(2).enumerate())
                    .all(|(index, pair)| pair[0] == pair[1] || validity.get(index)));
        shaped.then_some(Self {
            offsets,
            bytes,
            validity,
        })
    }

    /// Append one value, or a missing slot, which holds no bytes.
    pub fn push(&mut self, value: Option<&str>) {
        self.validity.push(value.is_some());
        self.bytes.push_str(value.unwrap_or_default());
        self.offsets.push(self.bytes.len());
    }

    /// The value at `index`, which must be below [`Text::len`].
    pub fn get(&self, index: usize) -> Option<&str> {
        self.validity
            .get(index)
            .then(|| &self.bytes[self.offsets[index]..self.offsets[index + 1]])
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    /// Whether the column has no slots.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bytes that the slots in `range` hold together.
    pub fn bytes_in(&self, range: std::ops::Range<usize>) -> usize {
        self.offsets[range.end] - self.offsets[range.start]
    }

    /// Each slot's start in [`Text::arena`], then the arena's end.
    pub fn offsets(&self) -> &[usize] {
        &self.offsets
    }

    pub fn arena(&self) -> &str {
        &self.bytes
    }

    pub fn validity(&self) -> &Validity {
        &self.validity
    }

    /// Slots `range`, their text copied as one run of the arena.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Self {
        let base = self.offsets[range.start];
        Self {
            offsets: self.offsets[range.start..=range.end]
                .iter()
                .map(|&offset| offset - base)
                .collect(),
            bytes: self.bytes[base..self.offsets[range.end]].to_owned(),
            validity: self.validity.slice(range),
        }
    }

    /// Append slots `range` of `other`, their text copied as one run of
    /// its arena (#4212).
    pub fn extend_from(&mut self, other: &Self, range: std::ops::Range<usize>) {
        let (base, end) = (other.offsets[range.start], other.offsets[range.end]);
        let at = self.bytes.len();
        self.bytes.push_str(&other.bytes[base..end]);
        self.offsets.extend(
            other.offsets[range.start + 1..=range.end]
                .iter()
                .map(|&offset| offset - base + at),
        );
        self.validity.extend_from(&other.validity, range);
    }

    /// The same values, missing wherever `missing` holds for a present one.
    /// `None` when no present value is, and the column stays as it is.
    pub fn missing_where(&self, missing: impl Fn(&str) -> bool) -> Option<Self> {
        // Along the offsets, reading no validity when no slot is missing.
        let value = |pair: &[usize]| &self.bytes[pair[0]..pair[1]];
        let marked = match self.validity.words() {
            None => self.offsets.windows(2).any(|pair| missing(value(pair))),
            Some(_) => (self.offsets.windows(2).enumerate())
                .any(|(index, pair)| self.validity.get(index) && missing(value(pair))),
        };
        if !marked {
            return None;
        }
        let mut text = Self::default();
        for index in 0..self.len() {
            text.push(self.get(index).filter(|value| !missing(value)));
        }
        Some(text)
    }
}

impl<'a> FromIterator<Option<&'a str>> for Text {
    fn from_iter<I: IntoIterator<Item = Option<&'a str>>>(values: I) -> Self {
        let mut text = Self::default();
        for value in values {
            text.push(value);
        }
        text
    }
}

impl PartialEq for Text {
    /// The same slots, missing in the same places, with the same text.
    fn eq(&self, other: &Self) -> bool {
        self.validity == other.validity
            && (0..self.len()).all(|index| self.get(index) == other.get(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity_keeps_no_bitmap_until_a_slot_is_missing_and_reads_each_slot_back() {
        let mut validity = Validity::default();
        for _ in 0..70 {
            validity.push(true);
        }
        assert_eq!(validity.words, None);
        validity.push(false);
        validity.push(true);
        assert_eq!(validity.len(), 72);
        let slots = (0..72).map(|index| validity.get(index)).collect::<Vec<_>>();
        let mut expected = vec![true; 72];
        expected[70] = false;
        assert_eq!(slots, expected);
        assert!(validity.any_missing(64..71));
        assert!(!validity.any_missing(0..70));
        // A bitmap that starts on a word boundary, and one built from slots.
        let at_boundary = (0..65).map(|index| index != 64).collect::<Validity>();
        assert_eq!(
            (0..65)
                .map(|index| at_boundary.get(index))
                .collect::<Vec<_>>(),
            (0..65).map(|index| index != 64).collect::<Vec<_>>()
        );
        assert_eq!(at_boundary, (0..65).map(|index| index != 64).collect());
    }

    #[test]
    fn a_validity_extended_by_a_slice_equals_the_one_its_slots_build() {
        // Slots appended after none, after 5 with the third missing, and
        // after 64 all present, across word boundaries:
        // none missing, one missing at each end of a word, and every
        // third missing, in runs that fill whole words and leave a rest.
        type Pattern = (&'static str, fn(usize) -> bool);
        let patterns: [Pattern; 4] = [
            ("all present", |_| true),
            ("slot 63 missing", |slot| slot != 63),
            ("slot 64 missing", |slot| slot != 64),
            ("every third missing", |slot| slot % 3 != 0),
        ];
        for before in [0, 5, 64] {
            for (label, present) in patterns {
                for count in [0, 1, 63, 64, 65, 200] {
                    let slots: Vec<bool> = (0..count).map(present).collect();
                    let mut extended = (0..before)
                        .map(|slot| before != 5 || slot != 2)
                        .collect::<Validity>();
                    let mut pushed = extended.clone();
                    extended.extend_from_slice(&slots);
                    for &slot in &slots {
                        pushed.push(slot);
                    }
                    assert_eq!(extended, pushed, "{label}, {count} slots after {before}");
                }
            }
        }
    }

    #[test]
    fn a_sliced_validity_equals_the_one_its_slots_build() {
        // Slot 70 is the only missing one, so a range without it keeps no
        // bitmap, as pushing its slots keeps none (#4190).
        let validity = (0..150).map(|index| index != 70).collect::<Validity>();
        for range in [0..150, 3..130, 64..71, 71..150, 0..70, 70..70] {
            let built = range.clone().map(|index| index != 70).collect::<Validity>();
            assert_eq!(validity.slice(range.clone()), built, "slots {range:?}");
        }
    }

    #[test]
    fn a_validity_extended_from_another_equals_the_one_its_slots_build() {
        // Slots of a validity whose only missing slot is 70, appended after
        // none, after 5 with the third missing, and after 64 all present:
        // ranges with and without slot 70, starting and ending inside a
        // word and on its boundaries, so each case either keeps no bitmap
        // or shifts whole words into one (#4212).
        let from = (0..150).map(|slot| slot != 70).collect::<Validity>();
        for before in [0, 5, 64] {
            for range in [
                0..150,
                3..130,
                64..71,
                71..150,
                0..70,
                70..70,
                70..71,
                1..65,
            ] {
                let mut extended = (0..before)
                    .map(|slot| before != 5 || slot != 2)
                    .collect::<Validity>();
                let mut pushed = extended.clone();
                extended.extend_from(&from, range.clone());
                for slot in range.clone() {
                    pushed.push(slot != 70);
                }
                assert_eq!(extended, pushed, "slots {range:?} after {before}");
            }
        }
    }

    #[test]
    fn missing_values_hold_no_payload_and_floats_compare_by_bits() {
        let values = [Some(-0.0), None, Some(1.5)]
            .into_iter()
            .collect::<Values<f64>>();
        assert_eq!(values.get(0).map(f64::to_bits), Some((-0.0f64).to_bits()));
        assert_eq!(values.get(1), None);
        assert_eq!(values.payloads(), [-0.0, 0.0, 1.5]);
        let positive = [Some(0.0), None, Some(1.5)]
            .into_iter()
            .collect::<Values<f64>>();
        assert_ne!(values, positive);
        assert_eq!(values, values.clone());
    }

    #[test]
    fn text_keeps_empty_and_missing_values_apart_and_marks_missing_values() {
        let text = [Some(""), None, Some("é"), Some("NA")]
            .into_iter()
            .collect::<Text>();
        assert_eq!(
            (0..4).map(|index| text.get(index)).collect::<Vec<_>>(),
            [Some(""), None, Some("é"), Some("NA")]
        );
        assert_eq!(text.offsets(), [0, 0, 0, 2, 4]);
        assert_eq!(text.bytes_in(2..4), 4);
        assert!(text.missing_where(|value| value == "absent").is_none());
        let marked = text.missing_where(|value| value == "NA").unwrap();
        assert_eq!(
            (0..4).map(|index| marked.get(index)).collect::<Vec<_>>(),
            [Some(""), None, Some("é"), None]
        );
        assert_eq!(marked.arena(), "é");
    }
}
