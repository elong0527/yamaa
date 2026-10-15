//! Physical payload validation after footer framing and actual decompression.
//! These checks do not replace the general reader's full format validation.
use super::{
    compact::Error,
    delta,
    framing::{Page, Payload},
    rle,
};
use parquet::{basic::Type, schema::types::ColumnDescriptor};

fn levels(bytes: &[u8], maximum: i16, encoding: i64, values: usize) -> Result<&[u8], Error> {
    if maximum == 0 {
        return Ok(bytes);
    }
    let length = match encoding {
        3 => {
            let raw = bytes.get(..4).ok_or(Error::Malformed)?;
            let size = i32::from_le_bytes(raw.try_into().unwrap());
            usize::try_from(size)
                .map_err(|_| Error::Malformed)?
                .checked_add(4)
                .ok_or(Error::Malformed)?
        }
        4 => {
            let width = 16 - (maximum as u16).leading_zeros() as usize;
            values
                .checked_mul(width)
                .ok_or(Error::Malformed)?
                .div_ceil(8)
        }
        _ => return Err(Error::Malformed),
    };
    bytes.get(length..).ok_or(Error::Malformed)
}

fn present(bytes: &[u8], maximum: i16, encoding: i64, values: usize) -> Result<usize, Error> {
    if maximum == 0 {
        return Ok(values);
    }
    if maximum < 0 {
        return Err(Error::Malformed);
    }
    let width = 16 - (maximum as u16).leading_zeros();
    let mut present = 0;
    let mut visit = |value: u32, count: usize| {
        if value > maximum as u32 {
            return Err(Error::Malformed);
        }
        if value == maximum as u32 {
            present += count;
        }
        Ok(())
    };
    match encoding {
        3 => rle::hybrid(bytes, width, values, &mut visit)?,
        4 => rle::packed(bytes, width, values, &mut visit)?,
        _ => return Err(Error::Malformed),
    }
    Ok(present)
}
fn v1_levels(
    bytes: &[u8],
    maximum: i16,
    encoding: i64,
    values: usize,
) -> Result<(&[u8], usize), Error> {
    let tail = levels(bytes, maximum, encoding, values)?;
    if maximum == 0 {
        return Ok((tail, values));
    }
    let used = bytes.len() - tail.len();
    let raw = &bytes[if encoding == 3 { 4 } else { 0 }..used];
    Ok((tail, present(raw, maximum, encoding, values)?))
}

pub(super) struct Values<'a> {
    pub encoding: i64,
    pub count: usize,
    pub body: &'a [u8],
}

/// Return encoding, actual non-null count and checked value bytes. Level
/// validation precedes string and dictionary decoding.
pub(super) fn values<'a>(
    page: &Page<'_>,
    decoded: &'a [u8],
    column: &ColumnDescriptor,
) -> Result<Option<Values<'a>>, Error> {
    let result = match page.payload {
        Payload::Index => return Ok(None),
        Payload::Dictionary { encoding, values } => (encoding, values, decoded),
        Payload::DataV2 {
            encoding,
            values,
            nulls,
            definition_bytes,
            repetition_bytes,
        } => {
            let rep = decoded.get(..repetition_bytes).ok_or(Error::Malformed)?;
            let end = repetition_bytes
                .checked_add(definition_bytes)
                .ok_or(Error::Malformed)?;
            let def = decoded.get(repetition_bytes..end).ok_or(Error::Malformed)?;
            present(rep, column.max_rep_level(), 3, values)?;
            let non_null = present(def, column.max_def_level(), 3, values)?;
            if nulls != values - non_null {
                return Err(Error::Malformed);
            }
            (encoding, non_null, &decoded[end..])
        }
        Payload::DataV1 {
            encoding,
            values,
            definition_encoding,
            repetition_encoding,
        } => {
            let (body, _) =
                v1_levels(decoded, column.max_rep_level(), repetition_encoding, values)?;
            let (body, non_null) =
                v1_levels(body, column.max_def_level(), definition_encoding, values)?;
            (encoding, non_null, body)
        }
    };
    let (encoding, count, body) = result;
    Ok(Some(Values {
        encoding,
        count,
        body,
    }))
}

/// Validate delta counts and strings against their physical page framing.
fn delta_values(page: &Page<'_>, decoded: &[u8], column: &ColumnDescriptor) -> Result<(), Error> {
    let Some(Values {
        encoding,
        count: values,
        body,
    }) = values(page, decoded, column)?
    else {
        return Ok(());
    };
    let physical = column.physical_type();
    match encoding {
        5 if matches!(physical, Type::INT32 | Type::INT64) => delta::admit(
            body,
            encoding,
            if physical == Type::INT32 { 32 } else { 64 },
            values,
        ),
        6 if physical == Type::BYTE_ARRAY => delta::admit(body, encoding, 32, values),
        7 if matches!(physical, Type::BYTE_ARRAY | Type::FIXED_LEN_BYTE_ARRAY) => {
            delta::admit(body, encoding, 32, values)
        }
        // The physical reader validates the remaining encodings and types.
        _ => Ok(()),
    }
}

/// Preserve dictionary state across pages of each physical column chunk.
pub(super) struct Validation {
    chunk: Option<(usize, usize)>,
    dictionary: Option<usize>,
}
impl Validation {
    pub(super) fn new() -> Self {
        Self {
            chunk: None,
            dictionary: None,
        }
    }

    pub(super) fn admit(
        &mut self,
        page: &Page<'_>,
        decoded: &[u8],
        column: &ColumnDescriptor,
    ) -> Result<(), Error> {
        let chunk = (page.row_group, page.column);
        if self.chunk != Some(chunk) {
            self.chunk = Some(chunk);
            self.dictionary = None;
        }
        let Some(Values {
            encoding,
            count,
            body,
        }) = values(page, decoded, column)?
        else {
            return Ok(());
        };
        if matches!(page.payload, Payload::Dictionary { .. }) {
            if self.dictionary.is_some() || !matches!(encoding, 0 | 2) {
                return Err(Error::Malformed);
            }
            plain(body, count, column)?;
            self.dictionary = Some(count);
            return Ok(());
        }
        match encoding {
            2 | 8 => {
                let entries = self.dictionary.ok_or(Error::Malformed)?;
                let (&width, indices) = body.split_first().ok_or(Error::Malformed)?;
                rle::hybrid(indices, u32::from(width), count, |index, _| {
                    if index as usize >= entries {
                        Err(Error::Malformed)
                    } else {
                        Ok(())
                    }
                })
            }
            0 => plain(body, count, column),
            5..=7 => delta_values(page, decoded, column),
            _ => Ok(()), // The physical reader checks the remaining encodings/types.
        }
    }
}

fn plain(mut bytes: &[u8], values: usize, column: &ColumnDescriptor) -> Result<(), Error> {
    if column.physical_type() == Type::BYTE_ARRAY {
        for _ in 0..values {
            let header = bytes.get(..4).ok_or(Error::Malformed)?;
            let length = u32::from_le_bytes(header.try_into().unwrap()) as usize;
            let end = length.checked_add(4).ok_or(Error::Malformed)?;
            bytes = bytes.get(end..).ok_or(Error::Malformed)?;
        }
    } else {
        let length = match column.physical_type() {
            Type::BOOLEAN => 1,
            Type::INT32 | Type::FLOAT => 4,
            Type::INT64 | Type::DOUBLE => 8,
            Type::INT96 => 12,
            Type::FIXED_LEN_BYTE_ARRAY => {
                usize::try_from(column.type_length()).map_err(|_| Error::Malformed)?
            }
            Type::BYTE_ARRAY => unreachable!(),
        };
        let stored = if column.physical_type() == Type::BOOLEAN {
            values.div_ceil(8)
        } else {
            values.checked_mul(length).ok_or(Error::Malformed)?
        };
        if stored > bytes.len() {
            return Err(Error::Malformed);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v1_level_lengths_are_checked_before_slicing_encoded_values() {
        assert!(levels(&[], 0, -1, usize::MAX).unwrap().is_empty());
        assert_eq!(levels(&[1, 0, 0, 0, 7, 42], 1, 3, 2).unwrap(), [42]);
        assert!(matches!(
            levels(&[255, 255, 255, 255], 1, 3, 1),
            Err(Error::Malformed)
        ));
        assert!(matches!(
            levels(&[8, 0, 0, 0], 1, 3, 1),
            Err(Error::Malformed)
        ));
        assert!(matches!(levels(&[0], 2, 4, 5), Err(Error::Malformed)));
        assert_eq!(levels(&[0, 0, 42], 2, 4, 5).unwrap(), [42]);
        assert!(matches!(levels(&[0], 1, 9, 1), Err(Error::Malformed)));
    }
    use parquet::{
        basic::Repetition,
        schema::types::{SchemaDescriptor, Type as SchemaType},
    };
    use std::sync::Arc;
    fn column() -> SchemaDescriptor {
        let field = Arc::new(
            SchemaType::primitive_type_builder("S", Type::BYTE_ARRAY)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap(),
        );
        SchemaDescriptor::new(Arc::new(
            SchemaType::group_type_builder("s")
                .with_fields(vec![field])
                .build()
                .unwrap(),
        ))
    }
    fn page(payload: Payload) -> Page<'static> {
        Page {
            codec: 0,
            body: &[],
            decoded_bytes: 0,
            uncompressed_prefix: 0,
            compressed: false,
            column: 0,
            row_group: 0,
            payload,
        }
    }
    fn dictionary(lengths: &[usize]) -> Vec<u8> {
        let mut out = Vec::new();
        for &length in lengths {
            out.extend((length as u32).to_le_bytes());
            out.extend(std::iter::repeat_n(b'x', length));
        }
        out
    }
    fn data(values: usize) -> Page<'static> {
        page(Payload::DataV1 {
            encoding: 8,
            values,
            definition_encoding: 3,
            repetition_encoding: 3,
        })
    }
    #[test]
    fn missing_dictionary_bad_indices_and_truncated_plain_strings_fail_safely() {
        let schema = column();
        let column = schema.column(0);
        let mut state = Validation::new();
        assert_eq!(
            state.admit(&data(1), &[1, 2, 0], &column),
            Err(Error::Malformed)
        );
        state
            .admit(
                &page(Payload::Dictionary {
                    encoding: 0,
                    values: 1,
                }),
                &dictionary(&[1]),
                &column,
            )
            .unwrap();
        assert_eq!(
            state.admit(&data(1), &[1, 2, 1], &column),
            Err(Error::Malformed)
        );
        let mut next = data(1);
        next.row_group = 1;
        assert_eq!(
            state.admit(&next, &[1, 2, 0], &column),
            Err(Error::Malformed)
        );
        for bytes in [&[0][..], &[5, 0, 0, 0, 1], &[255, 255, 255, 255]] {
            assert!(matches!(plain(bytes, 1, &column), Err(Error::Malformed)));
        }
    }
}
