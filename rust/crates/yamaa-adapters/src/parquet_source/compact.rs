//! Borrowed Compact Protocol framing for Parquet metadata and pages.
//! Format: https://github.com/apache/thrift/blob/master/doc/specs/thrift-compact-protocol.md
//! Parquet still validates its own schema.

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
    /// Like the downstream decoder, the last occurrence supplies a field value.
    pub(super) fn field(&self, id: i16) -> Result<Option<Self>, Error> {
        let Self::Struct(bytes) = self else {
            return Ok(None);
        };
        let mut reader = Reader { bytes, position: 0 };
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
            reader: Reader {
                bytes: self.bytes,
                position: 0,
            },
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
}

impl<'a> Reader<'a> {
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
        let header = self.byte()?;
        let kind = header & 15;
        if kind == 0 {
            return Ok(None);
        }
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
                let count = self.length()?;
                if count != 0 {
                    let kinds = self.byte()?;
                    self.collection(count.checked_mul(2).ok_or(Error::Malformed)?)?;
                    for _ in 0..count {
                        self.element(kinds >> 4, depth + 1)?;
                        self.element(kinds & 15, depth + 1)?;
                    }
                }
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
            13 => Value::Uuid(self.take(16)?),
            _ => return Err(Error::Malformed),
        })
    }
}

/// Parse one struct prefix; return its exact extent for page-body slicing.
/// Every container and binary payload borrows the held input. Validate all
/// children, including ignored metadata, without retaining a node per child.
pub(super) fn parse(bytes: &[u8]) -> Result<(Value<'_>, usize), Error> {
    let mut reader = Reader { bytes, position: 0 };
    let value = reader.value(12, 0)?;
    Ok((value, reader.position))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut fields = Reader { bytes, position: 0 };
        assert_eq!(
            fields.field(0, 0).unwrap(),
            Some((1, Value::Integer(i64::MIN)))
        );
        assert_eq!(value.field(1).unwrap(), Some(Value::Integer(i64::MAX)));
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
