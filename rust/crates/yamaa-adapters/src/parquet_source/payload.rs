//! Payload admission after bounded footer parsing and actual decompression.
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
/// validation precedes string-length/dictionary allocation admission.
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

/// Return bytes reconstructed by delta string decoding for aggregate admission.
pub(super) fn admit(
    page: &Page<'_>,
    decoded: &[u8],
    column: &ColumnDescriptor,
    reconstructed_bytes: usize,
) -> Result<usize, Error> {
    let Some(Values {
        encoding,
        count: values,
        body,
    }) = values(page, decoded, column)?
    else {
        return Ok(0);
    };
    let physical = column.physical_type();
    match encoding {
        5 if matches!(physical, Type::INT32 | Type::INT64) => delta::admit(
            body,
            encoding,
            if physical == Type::INT32 { 32 } else { 64 },
            delta::Limits {
                values,
                reconstructed_bytes,
            },
        ),
        6 if physical == Type::BYTE_ARRAY => delta::admit(
            body,
            encoding,
            32,
            delta::Limits {
                values,
                reconstructed_bytes,
            },
        ),
        7 if matches!(physical, Type::BYTE_ARRAY | Type::FIXED_LEN_BYTE_ARRAY) => delta::admit(
            body,
            encoding,
            32,
            delta::Limits {
                values,
                reconstructed_bytes,
            },
        ),
        // The remaining encodings use bounded caller buffers or borrowed byte
        // slices in the pinned library. It still validates their data/types.
        _ => Ok(0),
    }
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
}
