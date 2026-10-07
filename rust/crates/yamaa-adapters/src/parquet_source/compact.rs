//! Bounded, borrowed Compact Protocol framing for Parquet metadata/page admission.
//! Format: https://github.com/apache/thrift/blob/master/doc/specs/thrift-compact-protocol.md
//! This admits framing and allocation work; Parquet still validates its own schema.

#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub nodes: usize,
    pub depth: usize,
    pub bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Malformed,
    Limit,
}

#[derive(Debug, PartialEq)]
pub(super) enum Value<'a> {
    Bool(bool),
    Integer(i64),
    Double(&'a [u8]),
    Binary(&'a [u8]),
    Uuid(&'a [u8]),
    Struct(Vec<(i16, Value<'a>)>),
    Sequence(Vec<Value<'a>>),
    Map(Vec<(Value<'a>, Value<'a>)>),
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
    nodes: usize,
    limits: Limits,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.position.checked_add(count).ok_or(Error::Malformed)?;
        let value = self.bytes.get(self.position..end).ok_or(Error::Malformed)?;
        if end > self.limits.bytes {
            return Err(Error::Limit);
        }
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

    fn node(&mut self, depth: usize) -> Result<(), Error> {
        if depth > self.limits.depth || self.nodes >= self.limits.nodes {
            return Err(Error::Limit);
        }
        self.nodes += 1;
        Ok(())
    }

    fn collection(&self, count: usize) -> Result<(), Error> {
        // Every element consumes at least a byte (including empty structs and
        // element booleans). Check before allocation or a count-driven loop.
        if count > self.bytes.len() - self.position {
            return Err(Error::Malformed);
        }
        if count > self.limits.nodes - self.nodes {
            return Err(Error::Limit);
        }
        Ok(())
    }

    fn element(&mut self, kind: u8, depth: usize) -> Result<Value<'a>, Error> {
        if kind == 1 {
            self.node(depth)?;
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

    fn value(&mut self, kind: u8, depth: usize) -> Result<Value<'a>, Error> {
        self.node(depth)?;
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
                let mut values = Vec::new();
                for _ in 0..count {
                    values.push(self.element(kind, depth + 1)?);
                }
                Value::Sequence(values)
            }
            11 => {
                let count = self.length()?;
                let mut values = Vec::new();
                if count != 0 {
                    let kinds = self.byte()?;
                    self.collection(count.checked_mul(2).ok_or(Error::Malformed)?)?;
                    for _ in 0..count {
                        let key = self.element(kinds >> 4, depth + 1)?;
                        let value = self.element(kinds & 15, depth + 1)?;
                        values.push((key, value));
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
                    let value = self.value(kind, depth + 1)?;
                    fields.push((id, value));
                    previous = id;
                }
                Value::Struct(fields)
            }
            13 => Value::Uuid(self.take(16)?),
            _ => return Err(Error::Malformed),
        })
    }
}

/// Parse one struct prefix; return its exact byte extent for page-body slicing.
/// Binary payloads borrow the held input. The node budget bounds all owned AST
/// collections before count-driven work; no allocation uses an unchecked count.
pub(super) fn parse(bytes: &[u8], limits: Limits) -> Result<(Value<'_>, usize), Error> {
    // Bound recursion independently of caller policy so a trusted but overly
    // permissive request cannot turn malformed nesting into a stack overflow.
    let limits = Limits {
        depth: limits.depth.min(64),
        ..limits
    };
    let mut reader = Reader {
        bytes,
        position: 0,
        nodes: 0,
        limits,
    };
    let value = reader.value(12, 0)?;
    Ok((value, reader.position))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> Limits {
        Limits {
            nodes: 128,
            depth: 8,
            bytes: 256,
        }
    }

    #[test]
    fn authored_fields_boolean_elements_and_borrowed_binary() {
        // i32 field 1 = 150; binary field 5 = ff 00 61; field 6 = false;
        // field 7 = bool list [true, false]; field 8 = map {byte -1: {}}.
        let data = [
            0x15, 0xac, 0x02, 0x48, 3, 0xff, 0, 0x61, 0x12, 0x19, 0x21, 1, 2, 0x1b, 1, 0x3c, 0xff,
            0, 0, 0x99,
        ];
        let (root, used) = parse(&data, limits()).unwrap();
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
        let (value, used) = parse(&data, limits()).unwrap();
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
            assert_eq!(parse(&data, limits()), Err(Error::Malformed), "{data:?}");
        }
    }

    #[test]
    fn policy_limits_are_distinct_and_include_unknown_nested_fields() {
        let data = [0x19, 0x3c, 0, 0, 0, 0]; // Three empty structs in a list.
        assert!(
            parse(
                &data,
                Limits {
                    nodes: 5,
                    ..limits()
                }
            )
            .is_ok()
        );
        assert_eq!(
            parse(
                &data,
                Limits {
                    nodes: 4,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        );
        assert!(
            parse(
                &data,
                Limits {
                    bytes: 6,
                    ..limits()
                }
            )
            .is_ok()
        );
        assert_eq!(
            parse(
                &data,
                Limits {
                    bytes: 5,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        );
        assert!(
            parse(
                &data,
                Limits {
                    depth: 2,
                    ..limits()
                }
            )
            .is_ok()
        );
        assert_eq!(
            parse(
                &data,
                Limits {
                    depth: 1,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        );
        assert_eq!(
            parse(
                &[0],
                Limits {
                    nodes: 0,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        );
        let mut nested = vec![0x1c; 65];
        nested.extend(vec![0; 66]);
        assert_eq!(
            parse(
                &nested,
                Limits {
                    depth: usize::MAX,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        );
        assert!(parse(&[0x19, 0, 0], limits()).is_ok());
    }
}
