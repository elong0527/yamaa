//! Borrowed Compact Protocol framing for Parquet metadata and pages.
//! Format: https://github.com/apache/thrift/blob/master/doc/specs/thrift-compact-protocol.md
//! Parquet still validates its own schema.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Malformed,
    /// A structurally valid payload cannot fit the canonical Arrow table.
    Representation,
}

#[derive(Debug, PartialEq)]
pub(super) enum Value<'a> {
    /// A selected field had a different wire shape. Keep its presence so a
    /// later duplicate can replace it, without retaining the irrelevant value.
    TypeMismatch,
    Bool(bool),
    Integer(i64),
    Double(&'a [u8]),
    Binary(&'a [u8]),
    Uuid(&'a [u8]),
    Struct(Vec<(i16, Value<'a>)>),
    Sequence(Vec<Value<'a>>),
    Map(Vec<(Value<'a>, Value<'a>)>),
}

#[derive(Clone, Copy)]
enum Fields {
    Metadata,
    SchemaElement,
    RowGroup,
    ColumnChunk,
    ColumnMetadata,
    Page,
    DataV1,
    Dictionary,
    DataV2,
}

#[derive(Clone, Copy)]
enum Projection {
    #[cfg(test)]
    All,
    Skip,
    Integer,
    Bool,
    Struct(Fields),
    StructList(Fields),
}

impl Fields {
    fn field(self, id: i16) -> Projection {
        use Projection::{Bool, Integer, Skip, Struct, StructList};
        match (self, id) {
            (Self::Metadata, 2) => StructList(Self::SchemaElement),
            (Self::Metadata, 3) => Integer,
            (Self::Metadata, 4) => StructList(Self::RowGroup),
            (Self::SchemaElement, 1 | 5) => Integer,
            (Self::RowGroup, 1) => StructList(Self::ColumnChunk),
            (Self::RowGroup, 3) => Integer,
            (Self::ColumnChunk, 3) => Struct(Self::ColumnMetadata),
            (Self::ColumnMetadata, 4 | 7 | 9 | 11) => Integer,
            (Self::Page, 1..=3) => Integer,
            (Self::Page, 5) => Struct(Self::DataV1),
            (Self::Page, 7) => Struct(Self::Dictionary),
            (Self::Page, 8) => Struct(Self::DataV2),
            (Self::DataV1, 1..=4) | (Self::Dictionary, 1 | 2) => Integer,
            (Self::DataV2, 1 | 2 | 4..=6) => Integer,
            (Self::DataV2, 7) => Bool,
            _ => Skip,
        }
    }
}

impl Projection {
    fn accepts(self, kind: u8) -> bool {
        match self {
            #[cfg(test)]
            Self::All => true,
            Self::Skip => true,
            Self::Integer => matches!(kind, 3..=6),
            Self::Bool => matches!(kind, 1 | 2),
            Self::Struct(_) => kind == 12,
            Self::StructList(_) => matches!(kind, 9 | 10),
        }
    }
}

impl<'a> Value<'a> {
    /// Like the downstream decoder, the last occurrence supplies a field value.
    pub(super) fn field(&self, id: i16) -> Option<&Self> {
        let Self::Struct(fields) = self else {
            return None;
        };
        fields
            .iter()
            .rev()
            .find(|(key, _)| *key == id)
            .map(|(_, value)| value)
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

    fn element(
        &mut self,
        kind: u8,
        depth: usize,
        projection: Projection,
    ) -> Result<Option<Value<'a>>, Error> {
        let kind = match kind {
            1 => match self.byte()? {
                boolean @ (1 | 2) => Ok(boolean),
                _ => Err(Error::Malformed),
            }?,
            0 | 2 => return Err(Error::Malformed),
            kind => kind,
        };
        self.value(kind, depth, projection)
    }

    fn value(
        &mut self,
        kind: u8,
        depth: usize,
        projection: Projection,
    ) -> Result<Option<Value<'a>>, Error> {
        Self::nesting(depth)?;
        let mismatch = !projection.accepts(kind);
        let projection = if mismatch {
            Projection::Skip
        } else {
            projection
        };
        let value = match kind {
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
                let wrong_element =
                    matches!(projection, Projection::StructList(_)) && count != 0 && kind != 12;
                let child_projection = match projection {
                    Projection::StructList(fields) if !wrong_element => Projection::Struct(fields),
                    #[cfg(test)]
                    Projection::All => Projection::All,
                    _ => Projection::Skip,
                };
                let mut values = Vec::new();
                for _ in 0..count {
                    if let Some(value) = self.element(kind, depth + 1, child_projection)? {
                        values.push(value);
                    }
                }
                if wrong_element {
                    Value::TypeMismatch
                } else {
                    Value::Sequence(values)
                }
            }
            11 => {
                let count = self.length()?;
                let mut values = Vec::new();
                if count != 0 {
                    let kinds = self.byte()?;
                    self.collection(count.checked_mul(2).ok_or(Error::Malformed)?)?;
                    #[cfg(not(test))]
                    let child_projection = Projection::Skip;
                    #[cfg(test)]
                    let child_projection = if matches!(projection, Projection::All) {
                        Projection::All
                    } else {
                        Projection::Skip
                    };
                    for _ in 0..count {
                        let key = self.element(kinds >> 4, depth + 1, child_projection)?;
                        let value = self.element(kinds & 15, depth + 1, child_projection)?;
                        if let (Some(key), Some(value)) = (key, value) {
                            values.push((key, value));
                        }
                    }
                }
                Value::Map(values)
            }
            12 => {
                let mut fields = Vec::new();
                let mut previous = 0i16;
                loop {
                    let header = self.byte()?;
                    let kind = header & 15;
                    if kind == 0 {
                        break;
                    }
                    let id = if header >> 4 == 0 {
                        i16::try_from(self.integer()?).map_err(|_| Error::Malformed)?
                    } else {
                        previous
                            .checked_add(i16::from(header >> 4))
                            .ok_or(Error::Malformed)?
                    };
                    let child_projection = match projection {
                        Projection::Struct(layout) => layout.field(id),
                        #[cfg(test)]
                        Projection::All => Projection::All,
                        _ => Projection::Skip,
                    };
                    if let Some(value) = self.value(kind, depth + 1, child_projection)? {
                        // Only the last duplicate is read. Projected layouts
                        // retain a fixed set of fields, not overwritten trees.
                        if matches!(projection, Projection::Struct(_)) {
                            if let Some((_, previous)) =
                                fields.iter_mut().find(|(key, _)| *key == id)
                            {
                                *previous = value;
                            } else {
                                fields.push((id, value));
                            }
                        } else {
                            fields.push((id, value));
                        }
                    }
                    previous = id;
                }
                Value::Struct(fields)
            }
            13 => Value::Uuid(self.take(16)?),
            _ => return Err(Error::Malformed),
        };
        Ok(if mismatch {
            Some(Value::TypeMismatch)
        } else if matches!(projection, Projection::Skip) {
            None
        } else {
            Some(value)
        })
    }
}

/// Parse one struct prefix; return its exact extent for page-body slicing.
/// Binary payloads borrow the held input and collection counts must fit it.
/// Unselected fields use this same validation without retaining their values.
fn parse_projected(bytes: &[u8], projection: Projection) -> Result<(Value<'_>, usize), Error> {
    let mut reader = Reader { bytes, position: 0 };
    let value = reader.value(12, 0, projection)?.ok_or(Error::Malformed)?;
    Ok((value, reader.position))
}

pub(super) fn metadata(bytes: &[u8]) -> Result<(Value<'_>, usize), Error> {
    parse_projected(bytes, Projection::Struct(Fields::Metadata))
}

pub(super) fn page(bytes: &[u8]) -> Result<(Value<'_>, usize), Error> {
    parse_projected(bytes, Projection::Struct(Fields::Page))
}

#[cfg(test)]
fn parse(bytes: &[u8]) -> Result<(Value<'_>, usize), Error> {
    parse_projected(bytes, Projection::All)
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
    fn ignored_field_fanout_is_consumed_without_retaining_its_tree() {
        // schema=[{num_children:0}], rows=7, row_groups=[]; unknown field 15
        // contains many structs, each with a bool list and a byte->struct map.
        let mut bytes = vec![0x29, 0x1c, 0x55, 0, 0, 0x16, 14, 0x19, 0x0c, 0xb9, 0xfc];
        unsigned(65_536, &mut bytes);
        for _ in 0..65_536 {
            bytes.extend([0x19, 0x21, 1, 2, 0x1b, 1, 0x3c, 0xff, 0, 0]);
        }
        // The known field after the skipped collection still uses its explicit
        // ID, replacing the original row count rather than adding a new field.
        bytes.extend([0x06, 6, 18, 0]);
        let (root, used) = metadata(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(root.field(3), Some(&Value::Integer(9)));
        assert!(root.field(15).is_none());
        let Value::Struct(fields) = root else {
            panic!()
        };
        assert_eq!(fields.len(), 3);
    }

    #[test]
    fn wrong_selected_container_shapes_keep_only_a_type_marker() {
        // schema is a bool->bool map, rows is a bool list, and row_groups is a
        // bool list. Every byte is well formed, but none has its selected shape.
        let mut bytes = vec![0x2b];
        unsigned(65_536, &mut bytes);
        bytes.push(0x11);
        for _ in 0..65_536 {
            bytes.extend([1, 2]);
        }
        for _ in 0..2 {
            bytes.extend([0x19, 0xf1]);
            unsigned(65_536, &mut bytes);
            bytes.resize(bytes.len() + 65_536, 1);
        }
        bytes.push(0);
        let (root, used) = metadata(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        for field in [2, 3, 4] {
            assert_eq!(root.field(field), Some(&Value::TypeMismatch));
        }
        let Value::Struct(fields) = root else {
            panic!()
        };
        assert_eq!(fields.len(), 3);
    }

    #[test]
    fn projected_duplicates_keep_the_last_value_and_wrong_type_presence() {
        for (bytes, expected) in [
            // A wrong-type selected value may be superseded by a valid one.
            (vec![0x39, 0x31, 1, 2, 1, 0x06, 6, 18, 0], Value::Integer(9)),
            // A final wrong-type value still shadows the earlier valid value.
            (
                vec![0x36, 18, 0x09, 6, 0x31, 1, 2, 1, 0],
                Value::TypeMismatch,
            ),
        ] {
            let (root, used) = metadata(&bytes).unwrap();
            assert_eq!(used, bytes.len());
            assert_eq!(root.field(3), Some(&expected));
            let Value::Struct(fields) = root else {
                panic!()
            };
            assert_eq!(fields.len(), 1);
        }
        // A child mismatch also must not reject a schema list that is replaced
        // by a later duplicate of its ancestor field.
        let bytes = [0x29, 0x1c, 0x59, 0x11, 1, 0, 0x09, 4, 0x0c, 0];
        let (root, used) = metadata(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(root.field(2), Some(&Value::Sequence(vec![])));
        // Invalid wire data remains invalid even if a later duplicate would
        // otherwise replace it: the skipped bool element must still be read.
        assert_eq!(
            metadata(&[0x39, 0x11, 3, 0x06, 6, 18, 0]),
            Err(Error::Malformed)
        );
    }

    #[test]
    fn skipped_fields_still_validate_lengths_scalars_bool_elements_and_depth() {
        for bytes in [
            vec![0x98, 5, 1],                            // truncated binary
            vec![0x99, 0x21, 1, 3, 0],                   // invalid bool list element
            vec![0x9b, 1, 0x11, 1, 3, 0],                // invalid bool map value
            vec![0x9b, 1, 0x2c, 1, 0],                   // invalid map key kind
            vec![0x9c, 0x15],                            // truncated nested integer
            vec![0x99, 0xf3, 0x80, 0x80, 1, 0],          // impossible collection count
            vec![0x95, 0x80, 0x80, 0x80, 0x80, 0x10, 0], // i32 overflow
        ] {
            assert_eq!(metadata(&bytes), Err(Error::Malformed), "{bytes:?}");
            assert_eq!(page(&bytes), Err(Error::Malformed), "{bytes:?}");
        }
        for (depth, accepted) in [
            (yamaa_core::limits::Limits::SEMANTIC_DEPTH, true),
            (yamaa_core::limits::Limits::SEMANTIC_DEPTH + 1, false),
        ] {
            let mut bytes = vec![0x9c]; // unknown field 9, nested struct
            bytes.extend(vec![0x1c; depth - 1]);
            bytes.extend(vec![0; depth + 1]);
            assert_eq!(metadata(&bytes).is_ok(), accepted);
            assert_eq!(page(&bytes).is_ok(), accepted);
        }
    }

    #[test]
    fn ordinary_footer_and_page_projections_keep_every_framing_field() {
        // Two schema nodes (message + INT64 X), one row, one chunk. Unknown
        // names/version/type details are consumed; framing's integers remain.
        let footer = [
            0x15, 2, 0x19, 0x2c, 0x48, 1, b's', 0x15, 2, 0, 0x15, 4, 0x25, 0, 0x18, 1, b'X', 0,
            0x16, 2, 0x19, 0x1c, 0x19, 0x1c, 0x3c, 0x15, 4, 0x35, 0, 0x36, 8, 0x26, 8, 0x26, 8, 0,
            0, 0x26, 2, 0, 0,
        ];
        let (root, used) = metadata(&footer).unwrap();
        assert_eq!(used, footer.len());
        assert!(root.field(1).is_none());
        assert_eq!(root.field(3), Some(&Value::Integer(1)));
        let Some(Value::Sequence(schema)) = root.field(2) else {
            panic!()
        };
        assert_eq!(schema[0].field(5), Some(&Value::Integer(1)));
        assert_eq!(schema[1].field(1), Some(&Value::Integer(2)));
        let Some(Value::Sequence(groups)) = root.field(4) else {
            panic!()
        };
        assert_eq!(groups[0].field(3), Some(&Value::Integer(1)));
        let Some(Value::Sequence(chunks)) = groups[0].field(1) else {
            panic!()
        };
        let chunk = chunks[0].field(3).unwrap();
        for (id, expected) in [(4, 0), (7, 4), (9, 4), (11, 4)] {
            assert_eq!(chunk.field(id), Some(&Value::Integer(expected)));
        }

        let bytes = [
            0x15, 0, 0x15, 8, 0x15, 8, 0x2c, 0x15, 2, 0x15, 0, 0x15, 6, 0x15, 6, 0x1c, 0, 0, 0x4b,
            0, 0, 0xaa, 0xbb, 0xcc, 0xdd,
        ];
        let (root, used) = page(&bytes).unwrap();
        assert_eq!(used, bytes.len() - 4);
        assert_eq!(root.field(1), Some(&Value::Integer(0)));
        assert_eq!(root.field(2), Some(&Value::Integer(4)));
        assert_eq!(root.field(3), Some(&Value::Integer(4)));
        assert!(root.field(9).is_none());
        let detail = root.field(5).unwrap();
        for (id, expected) in [(1, 1), (2, 0), (3, 3), (4, 3)] {
            assert_eq!(detail.field(id), Some(&Value::Integer(expected)));
        }
        assert!(detail.field(5).is_none());
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
        assert_eq!(root.field(1), Some(&Value::Integer(150)));
        assert_eq!(root.field(5), Some(&Value::Binary(&data[5..8])));
        if let Some(Value::Binary(bytes)) = root.field(5) {
            assert_eq!(bytes.as_ptr(), data[5..].as_ptr());
        } else {
            panic!();
        }
        assert_eq!(root.field(6), Some(&Value::Bool(false)));
        assert_eq!(
            root.field(7),
            Some(&Value::Sequence(vec![
                Value::Bool(true),
                Value::Bool(false)
            ]))
        );
        assert_eq!(
            root.field(8),
            Some(&Value::Map(vec![(
                Value::Integer(-1),
                Value::Struct(vec![])
            )]))
        );
    }

    #[test]
    fn explicit_field_ids_integer_boundaries_and_duplicate_fields() {
        let data = [
            0x06, 2, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1, 0x06, 2, 0xfe, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1, 0,
        ];
        let (value, used) = parse(&data).unwrap();
        assert_eq!(used, data.len());
        let Value::Struct(fields) = &value else {
            panic!();
        };
        assert_eq!(fields[0], (1, Value::Integer(i64::MIN)));
        assert_eq!(value.field(1), Some(&Value::Integer(i64::MAX)));
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
