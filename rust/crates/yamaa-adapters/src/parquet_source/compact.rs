//! Borrowed Compact Protocol framing for Parquet metadata and pages.
//! Format: https://github.com/apache/thrift/blob/master/doc/specs/thrift-compact-protocol.md
//! Parquet still validates its own schema.

use std::{borrow::Cow, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Malformed,
    /// A structurally valid payload cannot fit the canonical Arrow table.
    Representation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Value<'a> {
    Bool(bool),
    Integer(i64),
    Double(&'a [u8]),
    Binary(&'a [u8]),
    Uuid(&'a [u8]),
    Struct(&'a [u8]),
    Sequence(Sequence<'a>),
    Map(&'a [u8]),
}

impl<'a> Value<'a> {
    /// The last occurrence supplies a field value. Admission refuses a
    /// repeated field that the native decoder knows before it reads any
    /// (`wire::check`), since the decoder does not always keep the last copy.
    pub(super) fn field(&self, id: i16) -> Result<Option<Self>, Error> {
        let Self::Struct(bytes) = self else {
            return Ok(None);
        };
        let mut reader = Reader::new(bytes);
        let mut previous = 0;
        let mut found = None;
        while let Some((key, value)) = reader.field(previous, 0)? {
            if key == id {
                found = Some(value);
            }
            previous = key;
        }
        Ok(found)
    }

    /// Every field of a struct with an id from 1 to `N - 1`, read in one
    /// pass, by id, the last occurrence supplying each; with how many fields
    /// the struct holds and the id of the first, which name a union's
    /// variant. Any other value has none.
    pub(super) fn fields<const N: usize>(&self) -> Result<Fields<'a, N>, Error> {
        let mut fields = Fields {
            values: [None; N],
            count: 0,
            first: None,
        };
        let Self::Struct(bytes) = self else {
            return Ok(fields);
        };
        let mut reader = Reader::new(bytes);
        let mut previous = 0;
        while let Some((id, value)) = reader.field(previous, 0)? {
            fields.count += 1;
            fields.first.get_or_insert(id);
            if let Some(slot) = usize::try_from(id)
                .ok()
                .and_then(|id| fields.values.get_mut(id))
            {
                *slot = Some(value);
            }
            previous = id;
        }
        Ok(fields)
    }
}

/// A struct's fields, read in one pass (`Value::fields`).
pub(super) struct Fields<'a, const N: usize> {
    /// Each field by its id.
    pub values: [Option<Value<'a>>; N],
    /// How many fields the struct holds, any id.
    pub count: usize,
    /// The id of its first field.
    pub first: Option<i16>,
}

/// Child nodes borrow their validated wire ranges, including unknown fields.
/// Neither a list element nor a map pair allocates a retained metadata node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Sequence<'a> {
    bytes: &'a [u8],
    kind: u8,
    count: usize,
}

impl Sequence<'_> {
    pub(super) fn len(self) -> usize {
        self.count
    }
}

/// A forward walk over an admitted struct that reads each byte once. After
/// each field header the caller descends into the field's struct or list, or
/// skips its value, before it reads the next header.
pub(super) struct Cursor<'a>(Reader<'a>);

impl<'a> Cursor<'a> {
    /// Walk the fields of an admitted struct. Any other value has no fields.
    pub(super) fn new(value: Value<'a>) -> Self {
        Self(Reader::new(if let Value::Struct(bytes) = value {
            bytes
        } else {
            &[0]
        }))
    }

    /// The next field's id and held wire type, or `None` at the struct's stop.
    /// This reads the header as `Reader::field` does; a helper shared with
    /// that hot loop measurably slowed every parse.
    pub(super) fn field(&mut self, previous: i16) -> Result<Option<(i16, u8)>, Error> {
        let header = self.0.byte()?;
        let kind = header & 15;
        if kind == 0 {
            return Ok(None);
        }
        let id = match header >> 4 {
            0 => i16::try_from(self.0.integer()?).map_err(|_| Error::Malformed)?,
            delta => previous
                .checked_add(i16::from(delta))
                .ok_or(Error::Malformed)?,
        };
        Ok(Some((id, kind)))
    }

    /// Skip the value of a field whose wire type is `kind`.
    pub(super) fn skip_value(&mut self, kind: u8) -> Result<(), Error> {
        self.0.value(kind, 0).map(drop)
    }

    /// Skip one collection element whose wire type is `kind`.
    pub(super) fn skip_element(&mut self, kind: u8) -> Result<(), Error> {
        self.0.element(kind, 0).map(drop)
    }

    /// Read a list or set header as its element wire type and element count.
    pub(super) fn collection(&mut self) -> Result<(u8, usize), Error> {
        let header = self.0.byte()?;
        let count = if header >> 4 == 15 {
            self.0.length()?
        } else {
            usize::from(header >> 4)
        };
        Ok((header & 15, count))
    }

    /// The bytes of the walked struct not yet read.
    pub(super) fn remaining(&self) -> usize {
        self.0.bytes.len() - self.0.position
    }

    /// Where the next byte lies, counted from the start of the walked struct.
    pub(super) fn position(&self) -> usize {
        self.0.position
    }

    /// The bytes at `range` of the walked struct.
    pub(super) fn held(&self, range: Range<usize>) -> &'a [u8] {
        &self.0.bytes[range]
    }

    /// Read the value of a binary field or element, and return where its
    /// bytes lie.
    pub(super) fn binary(&mut self) -> Result<Range<usize>, Error> {
        let length = self.0.length()?;
        let start = self.0.position;
        self.0.take(length)?;
        Ok(start..self.0.position)
    }
}

pub(super) struct Elements<'a> {
    reader: Reader<'a>,
    kind: u8,
    remaining: usize,
}

impl<'a> IntoIterator for Sequence<'a> {
    type Item = Result<Value<'a>, Error>;
    type IntoIter = Elements<'a>;

    fn into_iter(self) -> Self::IntoIter {
        Elements {
            reader: Reader::new(self.bytes),
            kind: self.kind,
            remaining: self.count,
        }
    }
}

impl<'a> Iterator for Elements<'a> {
    type Item = Result<Value<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(self.reader.element(self.kind, 0))
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    native_metadata: Option<Cow<'a, [u8]>>,
}

/// The kind the native decoder skips in place of `kind`: it cannot skip a
/// set or map in a footer or a UUID anywhere, so a set becomes the
/// identically encoded list and a map or UUID becomes a binary of the same
/// length (`native_binary`).
pub(super) fn native_opaque(kind: u8) -> u8 {
    match kind {
        10 => 9,
        11 | 13 => 8,
        kind => kind,
    }
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            native_metadata: None,
        }
    }

    fn native_header(&mut self, position: usize, header: u8) {
        if header != self.bytes[position] {
            if let Some(bytes) = &mut self.native_metadata {
                bytes.to_mut()[position] = header;
            }
        }
    }

    /// Present the value at `start..position` to the native copy as one binary
    /// of the same length, so no later offset moves: its first bytes become
    /// the varint length of the rest, padded to fill exactly the bytes they
    /// replace. The native decoder skips the value without interpreting it,
    /// but reads a skipped binary as UTF-8 text, so a rest that is not UTF-8
    /// becomes zero bytes.
    fn native_binary(&mut self, start: usize) {
        let Some(bytes) = &mut self.native_metadata else {
            return;
        };
        // Every value holds at least one byte. A value up to 128 bytes takes a
        // one-byte prefix, and each further prefix byte covers far more bytes
        // than it costs, so the prefix never outgrows the value.
        let length = self.position - start;
        let mut width = 1;
        while (length - width) >> (7 * width) != 0 {
            width += 1;
        }
        let mut prefix = [0u8; 10];
        let mut rest = length - width;
        for (index, byte) in prefix[..width].iter_mut().enumerate() {
            let more = if index + 1 < width { 0x80 } else { 0 };
            *byte = (rest & 0x7f) as u8 | more;
            rest >>= 7;
        }
        let range = start..start + width;
        if bytes[range.clone()] != prefix[..width] {
            bytes.to_mut()[range].copy_from_slice(&prefix[..width]);
        }
        let rest = start + width..self.position;
        if std::str::from_utf8(&bytes[rest.clone()]).is_err() {
            bytes.to_mut()[rest].fill(0);
        }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.position.checked_add(count).ok_or(Error::Malformed)?;
        let value = self.bytes.get(self.position..end).ok_or(Error::Malformed)?;

        self.position = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn unsigned(&mut self) -> Result<u64, Error> {
        let mut value = 0u64;
        for index in 0..10 {
            let byte = self.byte()?;
            if index == 9 && byte > 1 {
                return Err(Error::Malformed);
            }
            value |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Error::Malformed)
    }

    fn integer(&mut self) -> Result<i64, Error> {
        let value = self.unsigned()?;
        Ok((value >> 1) as i64 ^ -((value & 1) as i64))
    }

    fn length(&mut self) -> Result<usize, Error> {
        let count = self.unsigned()?;
        if count > i32::MAX as u64 {
            return Err(Error::Malformed);
        }
        usize::try_from(count).map_err(|_| Error::Malformed)
    }

    fn nesting(depth: usize) -> Result<(), Error> {
        if depth > yamaa_core::limits::Limits::SEMANTIC_DEPTH {
            return Err(Error::Malformed);
        }
        Ok(())
    }

    fn collection(&self, count: usize) -> Result<(), Error> {
        // Every element consumes at least a byte (including empty structs and
        // element booleans). Check before allocation or a count-driven loop.
        if count > self.bytes.len() - self.position {
            return Err(Error::Malformed);
        }

        Ok(())
    }

    fn element(&mut self, kind: u8, depth: usize) -> Result<Value<'a>, Error> {
        if kind == 1 {
            Self::nesting(depth)?;
            return match self.byte()? {
                1 => Ok(Value::Bool(true)),
                2 => Ok(Value::Bool(false)),
                _ => Err(Error::Malformed),
            };
        }
        if kind == 0 || kind == 2 {
            return Err(Error::Malformed);
        }
        self.value(kind, depth)
    }

    fn field(&mut self, previous: i16, depth: usize) -> Result<Option<(i16, Value<'a>)>, Error> {
        let start = self.position;
        let header = self.byte()?;
        let kind = header & 15;
        if kind == 0 {
            return Ok(None);
        }
        self.native_header(start, (header & 0xf0) | native_opaque(kind));
        let id = if header >> 4 == 0 {
            i16::try_from(self.integer()?).map_err(|_| Error::Malformed)?
        } else {
            previous
                .checked_add(i16::from(header >> 4))
                .ok_or(Error::Malformed)?
        };
        Ok(Some((id, self.value(kind, depth)?)))
    }

    fn value(&mut self, kind: u8, depth: usize) -> Result<Value<'a>, Error> {
        Self::nesting(depth)?;
        Ok(match kind {
            1 => Value::Bool(true),
            2 => Value::Bool(false),
            3 => Value::Integer(i64::from(self.byte()? as i8)),
            4 => Value::Integer(i64::from(
                i16::try_from(self.integer()?).map_err(|_| Error::Malformed)?,
            )),
            5 => Value::Integer(i64::from(
                i32::try_from(self.integer()?).map_err(|_| Error::Malformed)?,
            )),
            6 => Value::Integer(self.integer()?),
            7 => Value::Double(self.take(8)?),
            8 => {
                let length = self.length()?;
                Value::Binary(self.take(length)?)
            }
            9 | 10 => {
                let position = self.position;
                let header = self.byte()?;
                let count = if header >> 4 == 15 {
                    self.length()?
                } else {
                    usize::from(header >> 4)
                };
                let kind = header & 15;
                // Existing Parquet writers may emit type zero for an empty list.
                if !(kind == 0 && count == 0) && (kind == 0 || kind == 2 || kind > 13) {
                    return Err(Error::Malformed);
                }
                self.collection(count)?;
                // The native decoder refuses a UUID element type even when the
                // collection is empty.
                if count != 0 || kind == 13 {
                    let native = if kind == 1 { 3 } else { native_opaque(kind) };
                    self.native_header(position, (header & 0xf0) | native);
                }
                let start = self.position;
                for _ in 0..count {
                    self.element(kind, depth + 1)?;
                }
                Value::Sequence(Sequence {
                    bytes: &self.bytes[start..self.position],
                    kind,
                    count,
                })
            }
            11 => {
                let start = self.position;
                // The native copy presents the whole map as one opaque binary,
                // so nothing inside it is presented differently.
                let native = self.native_metadata.take();
                let count = self.length()?;
                if count != 0 {
                    let kinds = self.byte()?;
                    self.collection(count.checked_mul(2).ok_or(Error::Malformed)?)?;
                    for _ in 0..count {
                        self.element(kinds >> 4, depth + 1)?;
                        self.element(kinds & 15, depth + 1)?;
                    }
                }
                self.native_metadata = native;
                // An empty map's zero count already reads as an empty binary.
                self.native_binary(start);
                Value::Map(&self.bytes[start..self.position])
            }
            12 => {
                let start = self.position;
                let mut previous = 0i16;
                while let Some((id, _)) = self.field(previous, depth + 1)? {
                    previous = id;
                }
                Value::Struct(&self.bytes[start..self.position])
            }
            13 => {
                let start = self.position;
                let uuid = self.take(16)?;
                self.native_binary(start);
                Value::Uuid(uuid)
            }
            _ => return Err(Error::Malformed),
        })
    }
}

/// Parse one struct prefix; return its exact extent for page-body slicing.
/// Every container and binary payload borrows the held input. Validate all
/// children, including ignored metadata, without retaining a node per child.
pub(super) fn parse(bytes: &[u8]) -> Result<(Value<'_>, usize), Error> {
    let mut reader = Reader::new(bytes);
    let value = reader.value(12, 0)?;
    Ok((value, reader.position))
}

/// Work around parquet 55.2.0's unknown-field skip in one held footer or page
/// header. The footer decoder's compact protocol cannot read a set or map at
/// all, neither decoder knows the UUID type, and both read a skipped binary
/// as UTF-8 text. (parquet 57.3.0 also misread boolean collection elements,
/// which 55 reads byte by byte like the bytes they are presented as.) The
/// current Parquet footer and page header schemas have no known boolean
/// collections and no known set, map or UUID fields. Present validated
/// one-byte boolean elements as bytes, a set as the identically encoded list,
/// and a map or UUID as one opaque binary of the same length whose bytes are
/// UTF-8, only to parquet 55's decoder; every field header and id stays. The
/// result has the input's length, so a page header is presented in place in
/// the held file. A skipped binary whose bytes are not UTF-8, which only the
/// struct's shape tells from a known one, is presented afterwards by
/// `wire::Presentation`, as is what parquet 55 does not know of the footer.
/// Keep the original metadata for admission and the original data pages for
/// reads. This independently
/// authored correction copies the struct only when needed, changes no values
/// that decoder reads from a well-typed struct, and never turns invalid
/// booleans into bytes. The native reader asks only whether a page header
/// would be presented otherwise (`reader::refuse_shared_presented_pages`).
pub(super) fn native_metadata(bytes: &[u8]) -> Result<Cow<'_, [u8]>, Error> {
    let mut reader = Reader::new(bytes);
    reader.native_metadata = Some(Cow::Borrowed(bytes));
    reader.value(12, 0)?;
    Ok(reader.native_metadata.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_skip_compatibility_changes_only_unskippable_values() {
        // Inline true/false fields, an opaque binary payload, then a nested
        // struct containing a bool list and bool->list<bool> map.
        let bytes = [
            0x11, 0x12, 0x18, 3, 0x11, 0x21, 0x11, 0x1c, 0x19, 0x21, 1, 2, 0x1b, 1, 0x19, 2, 0x21,
            2, 1, 0, 0,
        ];
        // The bool list header gains byte elements. The map field becomes one
        // six-byte binary: a prefix of five, then the map's other bytes as held.
        let mut expected = bytes;
        for (position, byte) in [(9, 0x23), (12, 0x18), (13, 5)] {
            expected[position] = byte;
        }
        let native = native_metadata(&bytes).unwrap();
        assert_eq!(native.as_ref(), expected);
        assert!(matches!(native, Cow::Owned(_)));
        // An empty map becomes an empty binary in place: its zero count is
        // the binary's zero length, so no prefix is inserted.
        assert_eq!(
            native_metadata(&[0x19, 0x01, 0x1b, 0, 0]).unwrap().as_ref(),
            &[0x19, 0x01, 0x18, 0, 0]
        );
        // An empty UUID list or set still needs a binary element type, in
        // short and long collection header form.
        for (bytes, expected) in [
            (&[0x19, 0x0d, 0][..], &[0x19, 0x08, 0][..]),
            (&[0x1a, 0x0d, 0], &[0x19, 0x08, 0]),
            (&[0x19, 0xfd, 0, 0], &[0x19, 0xf8, 0, 0]),
        ] {
            assert_eq!(native_metadata(bytes).unwrap().as_ref(), expected);
        }
        // A bool set becomes a byte list; each UUID becomes a 16-byte binary,
        // a prefix of 15 and then its last 15 bytes, both as a field and as
        // list elements.
        let uuid = [7u8; 16];
        let mut bytes = vec![0x1a, 0x21, 1, 2, 0x1d];
        bytes.extend(uuid);
        bytes.extend([0x19, 0x2d]);
        bytes.extend(uuid);
        bytes.extend(uuid);
        bytes.push(0);
        let mut expected = bytes.clone();
        for (position, byte) in [
            (0, 0x19),  // set -> list
            (1, 0x23),  // of bytes
            (4, 0x18),  // uuid -> binary
            (5, 15),    // of fifteen more bytes
            (22, 0x28), // list<uuid> -> list<binary>
            (23, 15),
            (39, 15),
        ] {
            expected[position] = byte;
        }
        assert_eq!(native_metadata(&bytes).unwrap().as_ref(), expected);
        // A prefix takes as many bytes as the rest's length needs: 259 map
        // bytes take a two-byte prefix of 257.
        let mut wide = vec![0x1b, 0x80, 0x01, 0x11]; // map: 128 pairs, bool/bool
        wide.extend((0..128).flat_map(|_| [1, 2]));
        assert_eq!(wide.len() - 1, 259);
        wide.push(0);
        let mut native = wide.clone();
        native[0] = 0x18;
        native[1..3].copy_from_slice(&[0x81, 0x02]);
        assert_eq!(native_metadata(&wide).unwrap().as_ref(), native);
        // 129 map bytes are one too many for a one-byte prefix of 128, so a
        // two-byte prefix holds 127, padded with a zero continuation byte.
        // The last value, 64, takes two bytes, the first a continuation byte
        // that is not UTF-8, so the binary's other bytes become zero.
        let mut odd = vec![0x1b, 63, 0x14]; // map<bool,i16>: 63 pairs
        odd.extend((0..62).flat_map(|_| [1, 0]));
        odd.extend([1, 0x80, 0x01]);
        assert_eq!(odd.len() - 1, 129);
        odd.push(0);
        let mut native = odd.clone();
        native[0] = 0x18;
        native[1..3].copy_from_slice(&[0xff, 0x00]);
        native[3..130].fill(0);
        assert_eq!(native_metadata(&odd).unwrap().as_ref(), native);
        let (root, used) = parse(&native).unwrap();
        assert_eq!(used, native.len());
        assert_eq!(root.field(1).unwrap(), Some(Value::Binary(&[0; 127])));
        // A UUID whose bytes are not UTF-8 likewise presents zero bytes.
        let mut opaque = vec![0x1d];
        opaque.extend([0xff; 16]);
        opaque.push(0);
        let mut native = vec![0x18, 15];
        native.extend([0; 15]);
        native.push(0);
        assert_eq!(native_metadata(&opaque).unwrap().as_ref(), native);
        // Ordinary metadata and empty collections keep borrowing the source.
        for bytes in [&[0x11, 0x12, 0][..], &[0x19, 0x01, 0x19, 0x0b, 0]] {
            assert!(matches!(native_metadata(bytes).unwrap(), Cow::Borrowed(_)));
        }
        assert_eq!(
            native_metadata(&[0x19, 0x21, 1, 3, 0]),
            Err(Error::Malformed)
        );
    }

    fn unsigned(mut value: usize, output: &mut Vec<u8>) {
        while value >= 128 {
            output.push(value as u8 | 128);
            value >>= 7;
        }
        output.push(value as u8);
    }

    #[test]
    fn known_schema_and_group_lists_borrow_empty_elements_before_admission() {
        let mut bytes = vec![0x29, 0xfc]; // schema: a wide struct list
        unsigned(65_536, &mut bytes);
        let schema_start = bytes.len();
        bytes.resize(bytes.len() + 65_536, 0);
        bytes.extend([0x29, 0xfc]); // row_groups: another wide struct list
        unsigned(65_536, &mut bytes);
        let groups_start = bytes.len();
        bytes.resize(bytes.len() + 65_536, 0);
        bytes.push(0);
        let (root, used) = parse(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        for (id, start) in [(2, schema_start), (4, groups_start)] {
            let Some(Value::Sequence(values)) = root.field(id).unwrap() else {
                panic!()
            };
            assert_eq!(values.len(), 65_536);
            assert_eq!(values.bytes.as_ptr(), bytes[start..].as_ptr());
            assert_eq!(values.bytes.len(), 65_536);
        }
    }

    #[test]
    fn mixed_nested_unknown_fanout_stays_borrowed_and_keeps_later_fields() {
        // Unknown field 15 contains structs with a bool list and byte->struct
        // map. An explicit duplicate of the row field follows the collection.
        let mut bytes = vec![0x36, 14, 0xc9, 0xfc];
        unsigned(65_536, &mut bytes);
        let elements = bytes.len();
        for _ in 0..65_536 {
            bytes.extend([0x19, 0x21, 1, 2, 0x1b, 1, 0x3c, 0xff, 0, 0]);
        }
        bytes.extend([0x06, 6, 18, 0]);
        let (root, used) = parse(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(root.field(3).unwrap(), Some(Value::Integer(9)));
        let Some(Value::Sequence(values)) = root.field(15).unwrap() else {
            panic!()
        };
        assert_eq!(values.bytes.as_ptr(), bytes[elements..].as_ptr());
        assert_eq!(values.len(), 65_536);
    }

    #[test]
    fn duplicates_keep_the_last_value_even_when_its_wire_shape_changes() {
        let bytes = [0x39, 0x31, 1, 2, 1, 0x06, 6, 18, 0];
        let (root, used) = parse(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(root.field(3).unwrap(), Some(Value::Integer(9)));

        let bytes = [0x36, 18, 0x09, 6, 0x31, 1, 2, 1, 0];
        let (root, used) = parse(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        let Some(Value::Sequence(values)) = root.field(3).unwrap() else {
            panic!()
        };
        assert_eq!(
            values.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
            [Value::Bool(true), Value::Bool(false), Value::Bool(true)]
        );
        // A child with an unexpected shape can be superseded together with its
        // ancestor. Invalid wire data must still fail before that replacement.
        let bytes = [0x29, 0x1c, 0x59, 0x11, 1, 0, 0x09, 4, 0x0c, 0];
        let (root, _) = parse(&bytes).unwrap();
        let Some(Value::Sequence(values)) = root.field(2).unwrap() else {
            panic!()
        };
        assert_eq!(values.len(), 0);
        assert_eq!(
            parse(&[0x39, 0x11, 3, 0x06, 6, 18, 0]),
            Err(Error::Malformed)
        );
    }

    #[test]
    fn ignored_fields_still_validate_lengths_scalars_bool_elements_and_depth() {
        for bytes in [
            vec![0x98, 5, 1],                            // truncated binary
            vec![0x99, 0x21, 1, 3, 0],                   // invalid bool list element
            vec![0x9b, 1, 0x11, 1, 3, 0],                // invalid bool map value
            vec![0x9b, 1, 0x2c, 1, 0],                   // invalid map key kind
            vec![0x9c, 0x15],                            // truncated nested integer
            vec![0x99, 0xf3, 0x80, 0x80, 1, 0],          // impossible collection count
            vec![0x95, 0x80, 0x80, 0x80, 0x80, 0x10, 0], // i32 overflow
        ] {
            assert_eq!(parse(&bytes), Err(Error::Malformed), "{bytes:?}");
        }
        for (depth, accepted) in [
            (yamaa_core::limits::Limits::SEMANTIC_DEPTH, true),
            (yamaa_core::limits::Limits::SEMANTIC_DEPTH + 1, false),
        ] {
            let mut bytes = vec![0x9c]; // unknown field 9, nested struct
            bytes.extend(vec![0x1c; depth - 1]);
            bytes.extend(vec![0; depth + 1]);
            assert_eq!(parse(&bytes).is_ok(), accepted);
        }
    }

    #[test]
    fn ordinary_footer_and_page_fields_keep_metadata_and_exact_body_extent() {
        let footer = [
            0x15, 2, 0x19, 0x2c, 0x48, 1, b's', 0x15, 2, 0, 0x15, 4, 0x25, 0, 0x18, 1, b'X', 0,
            0x16, 2, 0x19, 0x1c, 0x19, 0x1c, 0x3c, 0x15, 4, 0x35, 0, 0x36, 8, 0x26, 8, 0x26, 8, 0,
            0, 0x26, 2, 0, 0,
        ];
        let (root, used) = parse(&footer).unwrap();
        assert_eq!(used, footer.len());
        assert_eq!(root.field(1).unwrap(), Some(Value::Integer(1)));
        assert_eq!(root.field(3).unwrap(), Some(Value::Integer(1)));
        let Some(Value::Sequence(schema)) = root.field(2).unwrap() else {
            panic!()
        };
        let mut schema = schema.into_iter();
        assert_eq!(
            schema.next().unwrap().unwrap().field(5).unwrap(),
            Some(Value::Integer(1))
        );
        assert_eq!(
            schema.next().unwrap().unwrap().field(1).unwrap(),
            Some(Value::Integer(2))
        );
        let Some(Value::Sequence(groups)) = root.field(4).unwrap() else {
            panic!()
        };
        let group = groups.into_iter().next().unwrap().unwrap();
        assert_eq!(group.field(3).unwrap(), Some(Value::Integer(1)));
        let Some(Value::Sequence(chunks)) = group.field(1).unwrap() else {
            panic!()
        };
        let chunk = chunks
            .into_iter()
            .next()
            .unwrap()
            .unwrap()
            .field(3)
            .unwrap()
            .unwrap();
        for (id, expected) in [(4, 0), (7, 4), (9, 4), (11, 4)] {
            assert_eq!(chunk.field(id).unwrap(), Some(Value::Integer(expected)));
        }

        let bytes = [
            0x15, 0, 0x15, 8, 0x15, 8, 0x2c, 0x15, 2, 0x15, 0, 0x15, 6, 0x15, 6, 0x1c, 0, 0, 0x4b,
            0, 0, 0xaa, 0xbb, 0xcc, 0xdd,
        ];
        let (root, used) = parse(&bytes).unwrap();
        assert_eq!(used, bytes.len() - 4);
        assert_eq!(root.field(1).unwrap(), Some(Value::Integer(0)));
        assert_eq!(root.field(2).unwrap(), Some(Value::Integer(4)));
        assert_eq!(root.field(3).unwrap(), Some(Value::Integer(4)));
        assert!(matches!(root.field(9).unwrap(), Some(Value::Map(_))));
        let detail = root.field(5).unwrap().unwrap();
        for (id, expected) in [(1, 1), (2, 0), (3, 3), (4, 3)] {
            assert_eq!(detail.field(id).unwrap(), Some(Value::Integer(expected)));
        }
        assert!(matches!(detail.field(5).unwrap(), Some(Value::Struct(_))));
    }

    #[test]
    fn authored_fields_boolean_elements_and_borrowed_binary() {
        // i32 field 1 = 150; binary field 5 = ff 00 61; field 6 = false;
        // field 7 = bool list [true, false]; field 8 = map {byte -1: {}}.
        let data = [
            0x15, 0xac, 0x02, 0x48, 3, 0xff, 0, 0x61, 0x12, 0x19, 0x21, 1, 2, 0x1b, 1, 0x3c, 0xff,
            0, 0, 0x99,
        ];
        let (root, used) = parse(&data).unwrap();
        assert_eq!(used, 19);
        assert_eq!(root.field(1).unwrap(), Some(Value::Integer(150)));
        assert_eq!(root.field(5).unwrap(), Some(Value::Binary(&data[5..8])));
        if let Some(Value::Binary(bytes)) = root.field(5).unwrap() {
            assert_eq!(bytes.as_ptr(), data[5..].as_ptr());
        } else {
            panic!();
        }
        assert_eq!(root.field(6).unwrap(), Some(Value::Bool(false)));
        let Some(Value::Sequence(values)) = root.field(7).unwrap() else {
            panic!();
        };
        assert_eq!(
            values.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
            vec![Value::Bool(true), Value::Bool(false)]
        );
        assert_eq!(root.field(8).unwrap(), Some(Value::Map(&data[14..18])));
    }

    #[test]
    fn explicit_field_ids_integer_boundaries_and_duplicate_fields() {
        let data = [
            0x06, 2, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1, 0x06, 2, 0xfe, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1, 0,
        ];
        let (value, used) = parse(&data).unwrap();
        assert_eq!(used, data.len());
        let Value::Struct(bytes) = value else {
            panic!();
        };
        let mut fields = Reader::new(bytes);
        assert_eq!(
            fields.field(0, 0).unwrap(),
            Some((1, Value::Integer(i64::MIN)))
        );
        assert_eq!(value.field(1).unwrap(), Some(Value::Integer(i64::MAX)));
    }

    #[test]
    fn a_cursor_reads_each_header_once_and_descends_or_skips() {
        // i32 field 1 = 1; byte field 2 = 1; list<i64> field 3 = [1]; struct
        // field 4 holding binary field 1 = "x"; then explicit id 300, a set.
        let bytes = [
            0x15, 2, 0x13, 1, 0x19, 0x16, 2, 0x1c, 0x18, 1, b'x', 0, 0x0a, 0xd8, 4, 0x15, 2, 0,
        ];
        let (root, used) = parse(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        let mut cursor = Cursor::new(root);
        assert_eq!(cursor.field(0).unwrap(), Some((1, 5)));
        cursor.skip_value(5).unwrap();
        assert_eq!(cursor.field(1).unwrap(), Some((2, 3)));
        cursor.skip_value(3).unwrap();
        assert_eq!(cursor.field(2).unwrap(), Some((3, 9)));
        assert_eq!(cursor.collection().unwrap(), (6, 1));
        cursor.skip_element(6).unwrap();
        // Descend into the struct, then continue after its stop.
        assert_eq!(cursor.field(3).unwrap(), Some((4, 12)));
        assert_eq!(cursor.field(0).unwrap(), Some((1, 8)));
        cursor.skip_value(8).unwrap();
        assert_eq!(cursor.field(1).unwrap(), None);
        assert_eq!(cursor.field(4).unwrap(), Some((300, 10)));
        cursor.skip_value(10).unwrap();
        assert_eq!(cursor.field(300).unwrap(), None);
        // Only a struct has fields.
        assert_eq!(Cursor::new(Value::Integer(1)).field(0).unwrap(), None);
    }

    #[test]
    fn wide_unknown_collections_borrow_the_source_and_still_validate_every_child() {
        fn unsigned(bytes: &mut Vec<u8>, mut value: usize) {
            while value >= 128 {
                bytes.push(value as u8 | 128);
                value >>= 7;
            }
            bytes.push(value as u8);
        }
        const COUNT: usize = 65536;
        for kind in [1, 12, 11] {
            // Unknown field 99 is a bool list, empty-struct list, or bool map.
            // A known field follows it, so lookup must consume the whole value.
            let mut data = vec![if kind == 11 { 0x0b } else { 0x09 }, 0xc6, 1];
            let value_start = data.len();
            if kind != 11 {
                data.push(0xf0 | kind);
            }
            unsigned(&mut data, COUNT);
            if kind == 11 {
                data.push(0x11);
            }
            let elements = data.len();
            for _ in 0..COUNT {
                match kind {
                    1 => data.push(1),
                    12 => data.push(0),
                    11 => data.extend([1, 2]),
                    _ => unreachable!(),
                }
            }
            let last_element = data.len() - 1;
            data.extend([0x06, 6, 14, 0]); // field 3 = i64 7; root STOP
            let (root, used) = parse(&data).unwrap();
            assert_eq!(used, data.len());
            assert_eq!(root.field(3).unwrap(), Some(Value::Integer(7)));
            match root.field(99).unwrap().unwrap() {
                Value::Sequence(values) => {
                    assert_eq!(values.len(), COUNT);
                    assert_eq!(values.bytes.as_ptr(), data[elements..].as_ptr());
                }
                Value::Map(bytes) => assert_eq!(bytes.as_ptr(), data[value_start..].as_ptr()),
                _ => panic!(),
            }
            data[last_element] = if kind == 12 { 0x1e } else { 3 };
            assert_eq!(parse(&data), Err(Error::Malformed));
        }
    }

    #[test]
    fn framing_counts_overflow_and_truncation_fail_without_large_allocations() {
        for data in [
            vec![],
            vec![0x1e],
            vec![0x18, 5, 1],
            vec![0x19, 0xf6, 0xff, 0xff, 0xff, 0xff, 7, 0],
            vec![0x19, 0xf6, 0xff, 0xff, 0xff, 0xff, 0x0f, 0],
            vec![0x19, 0x11, 3, 0],
            vec![0x19, 0x12, 1, 0],
            vec![
                0x16, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2, 0,
            ],
            vec![0x14, 0x80, 0x80, 4, 0],
            vec![0x05, 0xfe, 0xff, 3, 0, 0x15, 0, 0],
        ] {
            assert_eq!(parse(&data), Err(Error::Malformed), "{data:?}");
        }
    }

    #[test]
    fn unknown_collections_and_shared_recursion_guard_are_checked() {
        assert!(parse(&[0x19, 0x3c, 0, 0, 0, 0]).is_ok());
        assert!(parse(&[0x19, 0, 0]).is_ok());
        for (depth, accepted) in [
            (yamaa_core::limits::Limits::SEMANTIC_DEPTH - 1, true),
            (yamaa_core::limits::Limits::SEMANTIC_DEPTH + 1, false),
        ] {
            let mut nested = vec![0x1c; depth];
            nested.extend(vec![0; depth + 1]);
            assert_eq!(parse(&nested).is_ok(), accepted);
        }
    }
}
