//! Payload admission after bounded footer parsing and actual decompression.
//! These checks do not replace the general reader's full format validation.
use super::{
    compact::Error,
    delta,
    framing::{Page, Payload},
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

/// Return the bytes potentially reconstructed by delta string decoding, so the
/// caller can apply an aggregate budget across all pages before a general read.
pub(super) fn admit(
    page: &Page<'_>,
    decoded: &[u8],
    column: &ColumnDescriptor,
    reconstructed_bytes: usize,
) -> Result<usize, Error> {
    let (encoding, values, body) = match page.payload {
        Payload::Index => return Ok(0),
        Payload::Dictionary { encoding, values } => (encoding, values, decoded),
        Payload::DataV2 { encoding, values } => (
            encoding,
            values,
            decoded
                .get(page.uncompressed_prefix..)
                .ok_or(Error::Malformed)?,
        ),
        Payload::DataV1 {
            encoding,
            values,
            definition_encoding,
            repetition_encoding,
        } => {
            let body = levels(decoded, column.max_rep_level(), repetition_encoding, values)?;
            let body = levels(body, column.max_def_level(), definition_encoding, values)?;
            (encoding, values, body)
        }
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
        assert_eq!(levels(&[], 0, -1, usize::MAX).unwrap(), []);
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
