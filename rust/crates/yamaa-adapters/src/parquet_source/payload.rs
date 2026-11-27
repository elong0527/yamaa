//! Physical payload validation after footer framing and actual decompression.
//! These checks do not replace the general reader's full format validation.
use super::footer::Leaf;
use super::format::Physical;
use super::{
    closed_profile,
    compact::Error,
    delta,
    format::Encoding,
    framing::{Page, Payload},
    rle,
};

fn levels(bytes: &[u8], maximum: i16, encoding: i64, values: usize) -> Result<&[u8], Error> {
    if maximum == 0 {
        return Ok(bytes);
    }
    let length = match closed_profile::levels(encoding) {
        Some(Encoding::Rle) => {
            let raw = bytes.get(..4).ok_or(Error::Malformed)?;
            let size = i32::from_le_bytes(raw.try_into().unwrap());
            usize::try_from(size)
                .map_err(|_| Error::Malformed)?
                .checked_add(4)
                .ok_or(Error::Malformed)?
        }
        Some(Encoding::BitPacked) => {
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

pub(super) fn level_runs(
    bytes: &[u8],
    maximum: i16,
    encoding: i64,
    values: usize,
    mut visit: impl FnMut(u32, usize) -> Result<(), Error>,
) -> Result<(), Error> {
    if maximum < 0 {
        return Err(Error::Malformed);
    }
    if maximum == 0 {
        if values != 0 {
            visit(0, values)?;
        }
        return Ok(());
    }
    let width = 16 - (maximum as u16).leading_zeros();
    let mut checked = |value: u32, count: usize| {
        if value > maximum as u32 {
            return Err(Error::Malformed);
        }
        visit(value, count)
    };
    match closed_profile::levels(encoding) {
        Some(Encoding::Rle) => rle::hybrid(bytes, width, values, &mut checked),
        Some(Encoding::BitPacked) => rle::packed(bytes, width, values, &mut checked),
        _ => Err(Error::Malformed),
    }
}

fn present(bytes: &[u8], maximum: i16, encoding: i64, values: usize) -> Result<usize, Error> {
    let mut present = 0;
    level_runs(bytes, maximum, encoding, values, |value, count| {
        if value == maximum as u32 {
            present += count;
        }
        Ok(())
    })?;
    Ok(present)
}

fn v1_level_bytes(
    bytes: &[u8],
    maximum: i16,
    encoding: i64,
    values: usize,
) -> Result<(&[u8], &[u8]), Error> {
    let tail = levels(bytes, maximum, encoding, values)?;
    if maximum == 0 {
        return Ok((tail, &[]));
    }
    let used = bytes.len() - tail.len();
    let raw = &bytes[if encoding == 3 { 4 } else { 0 }..used];
    Ok((tail, raw))
}

fn v1_levels(
    bytes: &[u8],
    maximum: i16,
    encoding: i64,
    values: usize,
) -> Result<(&[u8], usize), Error> {
    let (tail, raw) = v1_level_bytes(bytes, maximum, encoding, values)?;
    Ok((tail, present(raw, maximum, encoding, values)?))
}

/// Reconcile actual record starts with one held column chunk before any value
/// decoder expands its counts. Repetition zero starts a row even for a null or
/// empty-list marker; later pages may continue a row with a nonzero level.
pub(super) struct Rows {
    remaining: usize,
    started: bool,
}

impl Rows {
    pub(super) fn new(rows: usize) -> Self {
        Self {
            remaining: rows,
            started: false,
        }
    }

    pub(super) fn admit(
        &mut self,
        page: &Page,
        decoded: &[u8],
        column: &Leaf,
    ) -> Result<(), Error> {
        let maximum = column.max_repetition;
        let (raw, encoding, values) = match page.payload {
            Payload::Index | Payload::Dictionary { .. } => return Ok(()),
            Payload::DataV1 {
                values,
                repetition_encoding,
                ..
            } => {
                let (_, raw) = v1_level_bytes(decoded, maximum, repetition_encoding, values)?;
                (raw, repetition_encoding, values)
            }
            Payload::DataV2 {
                values,
                repetition_bytes,
                ..
            } => (
                decoded.get(..repetition_bytes).ok_or(Error::Malformed)?,
                3,
                values,
            ),
        };
        level_runs(raw, maximum, encoding, values, |value, count| {
            if !self.started {
                if value != 0 {
                    return Err(Error::Malformed);
                }
                self.started = true;
            }
            if value == 0 {
                self.remaining = self.remaining.checked_sub(count).ok_or(Error::Malformed)?;
            }
            Ok(())
        })
    }

    pub(super) fn finish(self) -> Result<(), Error> {
        if self.remaining == 0 {
            Ok(())
        } else {
            Err(Error::Malformed)
        }
    }
}

pub(super) struct Values<'a> {
    pub encoding: i64,
    pub count: usize,
    pub body: &'a [u8],
}

/// Return encoding, actual non-null count and checked value bytes. Level
/// validation precedes string and dictionary decoding.
pub(super) fn values<'a>(
    page: &Page,
    decoded: &'a [u8],
    column: &Leaf,
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
            present(rep, column.max_repetition, 3, values)?;
            let non_null = present(def, column.max_definition, 3, values)?;
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
            let (body, _) = v1_levels(decoded, column.max_repetition, repetition_encoding, values)?;
            let (body, non_null) =
                v1_levels(body, column.max_definition, definition_encoding, values)?;
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

/// A data page as it holds its levels and values: its levels, the bytes of
/// its repetition and definition levels with their encodings, and its value
/// encoding and bytes.
pub(super) struct Data<'a> {
    pub levels: usize,
    pub repetition: (&'a [u8], i64),
    pub definition: (&'a [u8], i64),
    pub encoding: i64,
    pub body: &'a [u8],
}

/// The levels and values of a data page that admission has checked; none
/// for another page.
pub(super) fn data<'a>(
    page: &Page,
    decoded: &'a [u8],
    column: &Leaf,
) -> Result<Option<Data<'a>>, Error> {
    match page.payload {
        Payload::Index | Payload::Dictionary { .. } => Ok(None),
        Payload::DataV2 {
            encoding,
            values,
            definition_bytes,
            repetition_bytes,
            ..
        } => {
            let end = repetition_bytes
                .checked_add(definition_bytes)
                .ok_or(Error::Malformed)?;
            let slice = |range| decoded.get(range).ok_or(Error::Malformed);
            Ok(Some(Data {
                levels: values,
                repetition: (slice(0..repetition_bytes)?, 3),
                definition: (slice(repetition_bytes..end)?, 3),
                encoding,
                body: slice(end..decoded.len())?,
            }))
        }
        Payload::DataV1 {
            encoding,
            values,
            definition_encoding,
            repetition_encoding,
        } => {
            let (rest, repetition) =
                v1_level_bytes(decoded, column.max_repetition, repetition_encoding, values)?;
            let (body, definition) =
                v1_level_bytes(rest, column.max_definition, definition_encoding, values)?;
            Ok(Some(Data {
                levels: values,
                repetition: (repetition, repetition_encoding),
                definition: (definition, definition_encoding),
                encoding,
                body,
            }))
        }
    }
}

/// Validate delta counts and strings against their physical page framing.
fn delta_values(page: &Page, decoded: &[u8], column: &Leaf) -> Result<(), Error> {
    let Some(Values {
        encoding,
        count: values,
        body,
    }) = values(page, decoded, column)?
    else {
        return Ok(());
    };
    let physical = column.physical;
    match encoding {
        5 if matches!(physical, Physical::Int32 | Physical::Int64) => delta::admit(
            body,
            encoding,
            if physical == Physical::Int32 { 32 } else { 64 },
            values,
        ),
        6 if physical == Physical::ByteArray => delta::admit(body, encoding, 32, values),
        7 if matches!(physical, Physical::ByteArray | Physical::FixedLenByteArray) => {
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
        page: &Page,
        decoded: &[u8],
        column: &Leaf,
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
            let page =
                closed_profile::encoding(encoding).is_some_and(closed_profile::dictionary_page);
            if self.dictionary.is_some() || !page {
                return Err(Error::Malformed);
            }
            plain(body, count, column)?;
            self.dictionary = Some(count);
            return Ok(());
        }
        let encoding = closed_profile::encoding(encoding);
        if encoding.is_some_and(closed_profile::dictionary_data) {
            let entries = self.dictionary.ok_or(Error::Malformed)?;
            let (&width, indices) = body.split_first().ok_or(Error::Malformed)?;
            return rle::hybrid(indices, u32::from(width), count, |index, _| {
                if index as usize >= entries {
                    Err(Error::Malformed)
                } else {
                    Ok(())
                }
            });
        }
        match encoding {
            Some(Encoding::Plain) => plain(body, count, column),
            Some(
                Encoding::DeltaBinaryPacked
                | Encoding::DeltaLengthByteArray
                | Encoding::DeltaByteArray,
            ) => delta_values(page, decoded, column),
            Some(Encoding::ByteStreamSplit) => split(body, count, column),
            // The physical reader checks the remaining encodings and types.
            _ => Ok(()),
        }
    }
}

fn plain(mut bytes: &[u8], values: usize, column: &Leaf) -> Result<(), Error> {
    if column.physical == Physical::ByteArray {
        for _ in 0..values {
            let header = bytes.get(..4).ok_or(Error::Malformed)?;
            let length = u32::from_le_bytes(header.try_into().unwrap()) as usize;
            let end = length.checked_add(4).ok_or(Error::Malformed)?;
            bytes = bytes.get(end..).ok_or(Error::Malformed)?;
        }
    } else {
        let stored = match width(column)? {
            Some(width) => values.checked_mul(width).ok_or(Error::Malformed)?,
            None => values.div_ceil(8),
        };
        if stored > bytes.len() {
            return Err(Error::Malformed);
        }
    }
    Ok(())
}

/// The bytes of one value of `column`'s physical type; none for a boolean,
/// which takes a bit, or a byte array, which has no width.
fn width(column: &Leaf) -> Result<Option<usize>, Error> {
    Ok(Some(match column.physical {
        Physical::Int32 | Physical::Float => 4,
        Physical::Int64 | Physical::Double => 8,
        Physical::Int96 => 12,
        Physical::FixedLenByteArray => {
            usize::try_from(column.type_length).map_err(|_| Error::Malformed)?
        }
        Physical::Boolean | Physical::ByteArray => return Ok(None),
    }))
}

/// Byte stream split values of `column`, a type the encoding decodes: one
/// stream for each byte of the type's width, so the width divides the
/// page's bytes, which hold at least the page's `values`. parquet 55 split a
/// page of any length, from bytes it made up (#5071); a type the encoding
/// does not decode is the decoders' to refuse.
pub(super) fn split(bytes: &[u8], values: usize, column: &Leaf) -> Result<(), Error> {
    if !closed_profile::decodes(column.physical, Encoding::ByteStreamSplit) {
        return Ok(());
    }
    match width(column)? {
        Some(width) if width > 0 && bytes.len() % width == 0 && values <= bytes.len() / width => {
            Ok(())
        }
        _ => Err(Error::Malformed),
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
    use crate::parquet_source::format::Converted;
    /// A BYTE_ARRAY leaf `S` of these maximum definition and repetition
    /// levels.
    fn leaf(definition: i16, repetition: i16) -> Leaf {
        Leaf {
            node: 1,
            root: 0,
            physical: Physical::ByteArray,
            type_length: -1,
            precision: -1,
            scale: -1,
            converted: Converted::None,
            logical: None,
            max_definition: definition,
            max_repetition: repetition,
        }
    }
    fn page(payload: Payload) -> Page {
        Page {
            codec: 0,
            header: 0..0,
            body: 0..0,
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
    fn data(values: usize) -> Page {
        page(Payload::DataV1 {
            encoding: 8,
            values,
            definition_encoding: 3,
            repetition_encoding: 3,
        })
    }
    #[test]
    fn row_starts_use_run_cardinality_without_capping_repeated_values() {
        let column = leaf(1, 1);
        let page = data(i32::MAX as usize);
        // One RLE run claims i32::MAX distinct rows. Its cardinality is checked
        // without emitting those levels or invoking an ordinary value decoder.
        let mut rows = Rows::new(1);
        assert_eq!(
            rows.admit(&page, &[6, 0, 0, 0, 254, 255, 255, 255, 15, 0], &column),
            Err(Error::Malformed)
        );
        // The same value count fits one row when all later levels continue it.
        // This checks only row truth: the ordinary decoder still owns any
        // storage needed for actually decoding that admitted repeated data.
        let mut rows = Rows::new(1);
        rows.admit(
            &page,
            &[8, 0, 0, 0, 2, 0, 252, 255, 255, 255, 15, 1],
            &column,
        )
        .unwrap();
        rows.finish().unwrap();
    }

    #[test]
    fn row_starts_validate_chunk_boundaries_packed_levels_and_empty_chunks() {
        let column = leaf(1, 1);
        let mut packed = data(5);
        packed.payload = Payload::DataV1 {
            encoding: 7,
            values: 5,
            definition_encoding: 3,
            repetition_encoding: 4,
        };
        let mut rows = Rows::new(2);
        rows.admit(&packed, &[0b0001_0110], &column).unwrap(); // 0,1,1,0,1
        rows.finish().unwrap();
        let mut rows = Rows::new(1);
        assert_eq!(
            rows.admit(&data(1), &[2, 0, 0, 0, 2, 1], &column),
            Err(Error::Malformed)
        );
        let mut rows = Rows::new(2);
        rows.admit(
            &page(Payload::DataV2 {
                encoding: 7,
                values: 2,
                nulls: 2,
                definition_bytes: 0,
                repetition_bytes: 2,
            }),
            &[4, 0],
            &column,
        )
        .unwrap();
        rows.finish().unwrap();
        assert_eq!(Rows::new(1).finish(), Err(Error::Malformed));
        let mut empty = Rows::new(0);
        empty
            .admit(
                &page(Payload::Dictionary {
                    encoding: 0,
                    values: usize::MAX,
                }),
                &[],
                &column,
            )
            .unwrap();
        empty.admit(&page(Payload::Index), &[], &column).unwrap();
        empty.finish().unwrap();
    }
    #[test]
    fn missing_dictionary_bad_indices_and_truncated_plain_strings_fail_safely() {
        let column = leaf(0, 0);
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

    #[test]
    fn byte_stream_split_pages_that_do_not_hold_their_values_are_malformed() {
        // Admission of a page's value bytes, which no host fixture can hold:
        // the differential harness reads each through parquet 55 too, which
        // split a page of any length (#5071). Each type the profile decodes
        // from BYTE_STREAM_SPLIT, with its width in the format.
        let typed = |physical, type_length| Leaf {
            physical,
            type_length,
            ..leaf(0, 0)
        };
        let types = [
            ("INT32", typed(Physical::Int32, -1), 4),
            ("INT64", typed(Physical::Int64, -1), 8),
            ("FLOAT", typed(Physical::Float, -1), 4),
            ("DOUBLE", typed(Physical::Double, -1), 8),
            ("FLBA(3)", typed(Physical::FixedLenByteArray, 3), 3),
        ];
        for (name, column, width) in types {
            // Page bytes, the values the page declares and the admission: a
            // page of whole values admits, a length the width does not
            // divide or fewer values than declared does not.
            let cases = [
                (2 * width, 2, Ok(()), "two values"),
                (1, 1, Err(Error::Malformed), "1 byte"),
                (width - 1, 1, Err(Error::Malformed), "width - 1 bytes"),
                (width + 1, 1, Err(Error::Malformed), "width + 1 bytes"),
                (2 * width, 3, Err(Error::Malformed), "three of two values"),
            ];
            for (length, values, admitted, label) in cases {
                let page = page(Payload::DataV1 {
                    encoding: 9,
                    values,
                    definition_encoding: 3,
                    repetition_encoding: 3,
                });
                let bytes: Vec<u8> = (1..=length as u8).collect();
                let admission = Validation::new().admit(&page, &bytes, &column);
                assert_eq!(admission, admitted, "{name}, {label}");
            }
        }
    }
}
