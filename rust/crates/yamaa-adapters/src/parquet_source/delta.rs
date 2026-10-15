//! Admission of counts and reconstructed byte lengths inside delta encodings.
//! A page's declared value count does not constrain the generic delta decoder.
use super::compact::Error;

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.checked_add(count).ok_or(Error::Malformed)?;
        let bytes = self.bytes.get(self.offset..end).ok_or(Error::Malformed)?;
        self.offset = end;
        Ok(bytes)
    }
    fn unsigned(&mut self) -> Result<u64, Error> {
        let mut value = 0u64;
        for index in 0..10 {
            let byte = self.take(1)?[0];
            if index == 9 && byte > 1 {
                return Err(Error::Malformed);
            }
            value |= u64::from(byte & 127) << (7 * index);
            if byte & 128 == 0 {
                return Ok(value);
            }
        }
        Err(Error::Malformed)
    }
    fn count(&mut self) -> Result<usize, Error> {
        usize::try_from(self.unsigned()?).map_err(|_| Error::Malformed)
    }
    fn signed(&mut self, bits: u32) -> Result<i64, Error> {
        let value = self.unsigned()?;
        let value = (value >> 1) as i64 ^ -((value & 1) as i64);
        if bits == 32 && i32::try_from(value).is_err() {
            return Err(Error::Malformed);
        }
        Ok(value)
    }
}

fn length(value: i64) -> Result<usize, Error> {
    usize::try_from(value).map_err(|_| Error::Malformed)
}

/// One delta integer stream, borrowing its current miniblock and width bytes.
/// Keeping the cursor resumable lets byte-array prefixes and suffixes advance
/// together without retaining one prefix length per value.
struct Integers<'a> {
    cursor: Cursor<'a>,
    bits: u32,
    count: usize,
    first: Option<i64>,
    previous: i64,
    remaining: usize,
    miniblocks: usize,
    per_mini: usize,
    minimum: i64,
    widths: &'a [u8],
    width: u8,
    payload: &'a [u8],
    index: usize,
    mini_remaining: usize,
}

impl<'a> Integers<'a> {
    fn new(bytes: &'a [u8], bits: u32, expected_values: usize) -> Result<Self, Error> {
        if bits != 32 && bits != 64 {
            return Err(Error::Malformed);
        }
        let mut cursor = Cursor { bytes, offset: 0 };
        let block = cursor.count()?;
        let miniblocks = cursor.count()?;
        let count = cursor.count()?;
        if count > expected_values {
            return Err(Error::Malformed);
        }
        if block == 0
            || !block.is_multiple_of(128)
            || miniblocks == 0
            || !block.is_multiple_of(miniblocks)
        {
            return Err(Error::Malformed);
        }
        let per_mini = block / miniblocks;
        if per_mini == 0 || !per_mini.is_multiple_of(32) {
            return Err(Error::Malformed);
        }
        let previous = cursor.signed(bits)?;
        Ok(Self {
            cursor,
            bits,
            count,
            first: (count != 0).then_some(previous),
            previous,
            remaining: count.saturating_sub(1),
            miniblocks,
            per_mini,
            minimum: 0,
            widths: &[],
            width: 0,
            payload: &[],
            index: 0,
            mini_remaining: 0,
        })
    }

    fn next_value(&mut self) -> Result<Option<i64>, Error> {
        if let Some(first) = self.first.take() {
            return Ok(Some(first));
        }
        if self.remaining == 0 {
            return Ok(None);
        }
        if self.mini_remaining == 0 {
            if self.widths.is_empty() {
                self.minimum = self.cursor.signed(self.bits)?;
                // All width bytes are stored, including unused miniblocks.
                self.widths = self.cursor.take(self.miniblocks)?;
            }
            self.width = self.widths[0];
            self.widths = &self.widths[1..];
            if u32::from(self.width) > self.bits {
                return Err(Error::Malformed);
            }
            let encoded = self
                .per_mini
                .checked_mul(usize::from(self.width))
                .ok_or(Error::Malformed)?
                / 8;
            // Consume the complete padded miniblock before yielding a value.
            // Unused miniblock widths remain uninterpreted when the stream ends.
            self.payload = self.cursor.take(encoded)?;
            self.mini_remaining = self.remaining.min(self.per_mini);
            self.index = 0;
        }
        let bit = self
            .index
            .checked_mul(usize::from(self.width))
            .ok_or(Error::Malformed)?;
        let start = bit / 8;
        let shift = bit % 8;
        let octets = (shift + usize::from(self.width)).div_ceil(8);
        let mut packed = 0u128;
        for (offset, byte) in self.payload[start..start + octets].iter().enumerate() {
            packed |= u128::from(*byte) << (offset * 8);
        }
        let mask = (1u128 << self.width) - 1;
        let delta = ((packed >> shift) & mask) as u64;
        self.previous = if self.bits == 32 {
            (self.previous as i32)
                .wrapping_add(self.minimum as i32)
                .wrapping_add(delta as i32) as i64
        } else {
            self.previous
                .wrapping_add(self.minimum)
                .wrapping_add(delta as i64)
        };
        self.index += 1;
        self.mini_remaining -= 1;
        self.remaining -= 1;
        Ok(Some(self.previous))
    }
}

/// Visit the page's declared values, retaining no per-block allocation. Return the
/// exact next encoding offset, including padding in the last used miniblock.
pub(super) fn scan(
    bytes: &[u8],
    bits: u32,
    expected_values: usize,
    mut visit: impl FnMut(i64) -> Result<(), Error>,
) -> Result<(usize, usize), Error> {
    let mut values = Integers::new(bytes, bits, expected_values)?;
    while let Some(value) = values.next_value()? {
        visit(value)?;
    }
    Ok((values.cursor.offset, values.count))
}

/// Validate lengths and reconstructed strings before the ordinary decoder
/// reserves its own vectors or copies a prefix-expanded string.
pub(super) fn admit(
    bytes: &[u8],
    encoding: i64,
    physical_bits: u32,
    expected_values: usize,
) -> Result<(), Error> {
    match encoding {
        5 => {
            if scan(bytes, physical_bits, expected_values, |_| Ok(()))?.1 != expected_values {
                return Err(Error::Malformed);
            }
        }
        6 => {
            let mut total = 0usize;
            let (offset, count) = scan(bytes, 32, expected_values, |value| {
                total = total.checked_add(length(value)?).ok_or(Error::Malformed)?;
                Ok(())
            })?;
            if count != expected_values || total > bytes.len() - offset {
                return Err(Error::Malformed);
            }
        }
        7 => {
            let (offset, count) = scan(bytes, 32, expected_values, |value| {
                length(value)?;
                Ok(())
            })?;
            let mut prefixes = Integers::new(&bytes[..offset], 32, expected_values)?;
            let mut previous = 0;
            let mut reconstructed = Some(0usize);
            let mut stored = 0usize;
            let (suffix_offset, suffix_count) =
                scan(&bytes[offset..], 32, expected_values, |value| {
                    let suffix = length(value)?;
                    let prefix = length(prefixes.next_value()?.ok_or(Error::Malformed)?)?;
                    if prefix > previous {
                        return Err(Error::Malformed);
                    }
                    previous = prefix.checked_add(suffix).ok_or(Error::Malformed)?;
                    // Count the actual bytes before the physical decoder can
                    // copy them. Keep validating the page if the total exceeds
                    // Arrow's representation, so malformed data keeps priority.
                    reconstructed = reconstructed.and_then(|total| total.checked_add(previous));
                    stored = stored.checked_add(suffix).ok_or(Error::Malformed)?;
                    Ok(())
                })?;
            if count != expected_values
                || count != suffix_count
                || stored > bytes.len() - offset - suffix_offset
            {
                return Err(Error::Malformed);
            }
            if reconstructed.is_none_or(|total| total > i32::MAX as usize) {
                return Err(Error::Representation);
            }
        }
        _ => return Err(Error::Malformed),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned(mut value: u64, output: &mut Vec<u8>) {
        while value > 127 {
            output.push((value as u8 & 127) | 128);
            value >>= 7;
        }
        output.push(value as u8);
    }
    fn signed(value: i64, output: &mut Vec<u8>) {
        unsigned(((value as u64) << 1) ^ ((value >> 63) as u64), output);
    }
    // Independently authored constant-delta stream, one 128-value block and
    // four 32-value miniblocks. There are no packed bytes when widths are zero.
    fn progression(first: i64, step: i64, count: usize) -> Vec<u8> {
        let mut bytes = vec![128, 1, 4];
        unsigned(count as u64, &mut bytes);
        signed(first, &mut bytes);
        let mut remaining = count.saturating_sub(1);
        while remaining != 0 {
            signed(step, &mut bytes);
            bytes.extend([0, 0, 0, 0]);
            remaining = remaining.saturating_sub(128);
        }
        bytes
    }
    #[test]
    fn integer_delta_scan_handles_blocks_padding_and_width_specific_wrapping() {
        let bytes = progression(10, -2, 140);
        let mut values = Vec::new();
        let (offset, count) = scan(&bytes, 64, 140, |v| {
            values.push(v);
            Ok(())
        })
        .unwrap();
        assert_eq!((offset, count), (bytes.len(), 140));
        assert_eq!(values, (0..140).map(|n| 10 - n * 2).collect::<Vec<_>>());
        for (first, bits, expected) in [
            (i32::MAX as i64, 32, i32::MIN as i64),
            (i64::MAX, 64, i64::MIN),
        ] {
            let mut values = Vec::new();
            scan(&progression(first, 1, 2), bits, 2, |v| {
                values.push(v);
                Ok(())
            })
            .unwrap();
            assert_eq!(values, [first, expected]);
        }
        let mut packed = vec![128, 1, 4, 3, 20, 0, 2, 255, 255, 255];
        packed.extend([0b0000_1001, 0, 0, 0, 0, 0, 0, 0]);
        let mut values = Vec::new();
        assert_eq!(
            scan(&packed, 32, 3, |v| {
                values.push(v);
                Ok(())
            })
            .unwrap(),
            (18, 3)
        );
        assert_eq!(values, [10, 11, 13]); // Unused miniblock widths are ignored.
    }
    #[test]
    fn stream_counts_and_malformed_headers_fail_before_visit_or_allocation() {
        // Page levels establish the actual non-null count. A shorter stream
        // must not reach a decoder that assumes its first value exists.
        for count in [0, 1, 2] {
            assert_eq!(
                admit(&progression(0, 0, count), 5, 64, 3),
                Err(Error::Malformed)
            );
        }
        let mut huge = vec![128, 1, 4];
        unsigned(u64::MAX, &mut huge);
        assert_eq!(
            scan(&huge, 64, 1024, |_| panic!("invalid count")),
            Err(Error::Malformed)
        );
        for bytes in [
            vec![0, 1, 1, 0],
            vec![128, 1, 0, 1, 0],
            vec![128, 1, 3, 1, 0],
            vec![128, 1, 4, 2, 0, 0, 65, 0, 0, 0],
            vec![255; 11],
        ] {
            assert!(matches!(
                scan(&bytes, 64, 1024, |_| Ok(())),
                Err(Error::Malformed)
            ));
        }
        let mut bytes = progression(0, 0, 2);
        bytes.pop();
        assert_eq!(scan(&bytes, 64, 1024, |_| Ok(())), Err(Error::Malformed));
    }
    #[test]
    fn byte_array_lengths_and_prefix_expansion_validate_actual_stored_strings() {
        let mut lengths = progression(3, 0, 2);
        lengths.extend(b"abcdef");
        admit(&lengths, 6, 32, 2).unwrap();
        lengths.pop();
        assert_eq!(admit(&lengths, 6, 32, 2), Err(Error::Malformed));
        assert_eq!(
            admit(&progression(-1, 0, 1), 6, 32, 3),
            Err(Error::Malformed)
        );
        let mut data = progression(0, 3, 3); // 0,3,6 prefix lengths
        data.extend(progression(3, 0, 3)); // abc,def,ghi suffixes
        data.extend(b"abcdefghi");
        admit(&data, 7, 32, 3).unwrap(); // abc,abcdef,abcdefghi = 18 bytes
        let mut bad = progression(1, 0, 1); // first value cannot have a prefix
        bad.extend(progression(0, 0, 1));
        assert_eq!(admit(&bad, 7, 32, 3), Err(Error::Malformed));
        let mut bad = progression(0, 0, 2);
        bad.extend(progression(0, 0, 1));
        assert_eq!(admit(&bad, 7, 32, 3), Err(Error::Malformed));
    }

    #[test]
    fn prefix_and_suffix_cursors_keep_independent_miniblock_padding() {
        // Prefixes 0,1,2 use a padded 1-bit miniblock. The unused width bytes
        // are deliberately outside the integer width and must stay ignored.
        let mut bytes = vec![128, 1, 4, 3, 0, 0, 1, 255, 255, 255, 3, 0, 0, 0];
        // Suffixes 1,1,1 use a different block/miniblock geometry: 256 / 8.
        bytes.extend([128, 2, 8, 3, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        bytes.extend(b"abc");
        admit(&bytes, 7, 32, 3).unwrap();
        bytes.pop();
        assert_eq!(admit(&bytes, 7, 32, 3), Err(Error::Malformed));
    }

    #[test]
    fn prefix_expansion_admits_actual_arrow_offset_boundary_before_copying_values() {
        // Each string appends 1024 bytes to its predecessor. Only about 2 MiB
        // is stored, while the sum of the actual reconstructed values crosses
        // signed-32-bit Arrow offsets between these two independently derived
        // triangular totals.
        for count in [2047usize, 2048] {
            let total = 1024 * count * (count + 1) / 2;
            let mut bytes = progression(0, 1024, count);
            bytes.extend(progression(1024, 0, count));
            bytes.resize(bytes.len() + 1024 * count, b'x');
            assert_eq!(
                admit(&bytes, 7, 32, count),
                if total <= i32::MAX as usize {
                    Ok(())
                } else {
                    Err(Error::Representation)
                }
            );
            if total > i32::MAX as usize {
                // A missing stored suffix remains invalid Parquet, even when
                // the claimed reconstructed values exceed Arrow's offsets.
                bytes.pop();
                assert_eq!(admit(&bytes, 7, 32, count), Err(Error::Malformed));
            }
        }
    }
    #[test]
    fn repeated_prefixes_cannot_expand_past_arrow_string_offsets() {
        // Only 64 KiB of stored suffixes, but prefix reconstruction produces
        // lengths 1, 2, ... count before the physical decoder copies them.
        // Crossing Arrow's offset range is a representation refusal, while the
        // stored Parquet suffixes remain structurally valid.
        for (count, expected) in [(65_535, Ok(())), (65_536, Err(Error::Representation))] {
            let mut data = progression(0, 1, count);
            data.extend(progression(1, 0, count));
            data.resize(data.len() + count, b'x');
            assert_eq!(admit(&data, 7, 32, count), expected);
        }
    }
}
