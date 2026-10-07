//! Admit decoded cell bytes before Arrow expands dictionaries or delta strings.
//! Page storage and shape have separate budgets; this is not a total RSS bound.
use super::{
    compact::Error,
    framing::{Page, Payload},
    payload, rle,
};
use parquet::{basic::Type, schema::types::ColumnDescriptor};

fn charge(used: &mut usize, amount: usize, maximum: usize) -> Result<(), Error> {
    *used = used
        .checked_add(amount)
        .filter(|v| *v <= maximum)
        .ok_or(Error::Limit)?;
    Ok(())
}
fn fixed_bytes(column: &ColumnDescriptor) -> Result<usize, Error> {
    Ok(match column.physical_type() {
        Type::BOOLEAN => 1,
        Type::INT32 | Type::FLOAT => 4,
        Type::INT64 | Type::DOUBLE => 8,
        Type::INT96 => 12,
        Type::FIXED_LEN_BYTE_ARRAY => {
            usize::try_from(column.type_length()).map_err(|_| Error::Malformed)?
        }
        Type::BYTE_ARRAY => return Err(Error::Malformed),
    })
}
fn plain_lengths(
    mut bytes: &[u8],
    values: usize,
    column: &ColumnDescriptor,
) -> Result<Vec<usize>, Error> {
    let mut lengths = Vec::new();
    if column.physical_type() == Type::BYTE_ARRAY {
        for _ in 0..values {
            let header = bytes.get(..4).ok_or(Error::Malformed)?;
            let length = u32::from_le_bytes(header.try_into().unwrap()) as usize;
            let end = length.checked_add(4).ok_or(Error::Malformed)?;
            bytes = bytes.get(end..).ok_or(Error::Malformed)?;
            lengths.push(length);
        }
    } else {
        let length = fixed_bytes(column)?;
        let stored = if column.physical_type() == Type::BOOLEAN {
            values.div_ceil(8)
        } else {
            values.checked_mul(length).ok_or(Error::Malformed)?
        };
        if stored > bytes.len() {
            return Err(Error::Malformed);
        }
        lengths.resize(values, length);
    }
    Ok(lengths)
}

/// Pages arrive in framing's row-group/column order. Retain only the current
/// chunk's dictionary lengths; entries and indices never become owned strings.
pub(super) struct Admission {
    chunk: Option<(usize, usize)>,
    dictionary: Option<Vec<usize>>,
    used: usize,
    maximum: usize,
}
impl Admission {
    pub(super) fn new(maximum: usize) -> Self {
        Self {
            chunk: None,
            dictionary: None,
            used: 0,
            maximum,
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
        let Some(payload::Values {
            encoding,
            count: values,
            body,
        }) = payload::values(page, decoded, column)?
        else {
            return Ok(());
        };
        if matches!(page.payload, Payload::Dictionary { .. }) {
            if self.dictionary.is_some() || !matches!(encoding, 0 | 2) {
                return Err(Error::Malformed);
            }
            self.dictionary = Some(plain_lengths(body, values, column)?);
            return Ok(());
        }
        let used = &mut self.used;
        let maximum = self.maximum;
        match encoding {
            2 | 8 => {
                let dictionary = self.dictionary.as_ref().ok_or(Error::Malformed)?;
                let (&width, indices) = body.split_first().ok_or(Error::Malformed)?;
                rle::hybrid(indices, u32::from(width), values, |index, count| {
                    let length = *dictionary.get(index as usize).ok_or(Error::Malformed)?;
                    charge(
                        used,
                        length.max(32).checked_mul(count).ok_or(Error::Limit)?,
                        maximum,
                    )
                })?;
            }
            0 => {
                for length in plain_lengths(body, values, column)? {
                    charge(used, length.max(32), maximum)?;
                }
            }
            5..=7 => {
                let reconstruction =
                    payload::admit(page, decoded, column, maximum.saturating_sub(*used))?;
                charge(used, reconstruction, maximum)?;
                charge(used, values.checked_mul(32).ok_or(Error::Limit)?, maximum)?;
            }
            _ => {
                // Fixed-width encodings have no variable string reconstruction.
                // The codec remains responsible for encoding/type compatibility.
                let length = if column.physical_type() == Type::BYTE_ARRAY {
                    32
                } else {
                    fixed_bytes(column)?.max(32)
                };
                charge(
                    used,
                    values.checked_mul(length).ok_or(Error::Limit)?,
                    maximum,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn actual_indices_bound_expansion_without_charging_unused_large_entries() {
        let schema = column();
        let column = schema.column(0);
        let mut state = Admission::new(32);
        state
            .admit(
                &page(Payload::Dictionary {
                    encoding: 0,
                    values: 2,
                }),
                &dictionary(&[1, 1000]),
                &column,
            )
            .unwrap();
        state.admit(&data(1), &[1, 2, 0], &column).unwrap(); // one index 0
        let mut state = Admission::new(999);
        state
            .admit(
                &page(Payload::Dictionary {
                    encoding: 0,
                    values: 1,
                }),
                &dictionary(&[100]),
                &column,
            )
            .unwrap();
        assert_eq!(state.admit(&data(10), &[0, 20], &column), Err(Error::Limit));
        let mut state = Admission::new(1000);
        state
            .admit(
                &page(Payload::Dictionary {
                    encoding: 0,
                    values: 1,
                }),
                &dictionary(&[100]),
                &column,
            )
            .unwrap();
        state.admit(&data(10), &[0, 20], &column).unwrap();
        assert_eq!(state.admit(&data(1), &[0, 2], &column), Err(Error::Limit));
    }
    #[test]
    fn missing_dictionary_bad_indices_and_truncated_plain_strings_fail_safely() {
        let schema = column();
        let column = schema.column(0);
        let mut state = Admission::new(4096);
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
            assert!(matches!(
                plain_lengths(bytes, 1, &column),
                Err(Error::Malformed)
            ));
        }
    }
}
