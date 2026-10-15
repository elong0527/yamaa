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

/// Visit the page's declared values, retaining no per-block allocation. Return the
/// exact next encoding offset, including padding in the last used miniblock.
pub(super) fn scan(
    bytes: &[u8],
    bits: u32,
    expected_values: usize,
    mut visit: impl FnMut(i64) -> Result<(), Error>,
) -> Result<(usize, usize), Error> {
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
    let mut previous = cursor.signed(bits)?;
    if count != 0 {
        visit(previous)?;
    }
    let mut remaining = count.saturating_sub(1);
    while remaining != 0 {
        let minimum = cursor.signed(bits)?;
        // `take` checks the stored bytes before interpreting any integer as an
        // allocation/work request. Even a giant claimed block stays borrowed.
        let widths = cursor.take(miniblocks)?;
        for &width in widths {
            if remaining == 0 {
                break;
            }
            if u32::from(width) > bits {
                return Err(Error::Malformed);
            }
            let encoded = per_mini
                .checked_mul(usize::from(width))
                .ok_or(Error::Malformed)?
                / 8;
            let payload = cursor.take(encoded)?;
            let used = remaining.min(per_mini);
            for index in 0..used {
                let bit = index
                    .checked_mul(usize::from(width))
                    .ok_or(Error::Malformed)?;
                let start = bit / 8;
                let shift = bit % 8;
                let octets = (shift + usize::from(width)).div_ceil(8);
                let mut packed = 0u128;
                for (offset, byte) in payload[start..start + octets].iter().enumerate() {
                    packed |= u128::from(*byte) << (offset * 8);
                }
                let mask = (1u128 << width) - 1;
                let delta = ((packed >> shift) & mask) as u64;
                previous = if bits == 32 {
                    (previous as i32)
                        .wrapping_add(minimum as i32)
                        .wrapping_add(delta as i32) as i64
                } else {
                    previous.wrapping_add(minimum).wrapping_add(delta as i64)
                };
                visit(previous)?;
            }
            remaining -= used;
        }
    }
    Ok((cursor.offset, count))
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
            let mut prefixes = Vec::new();
            let (offset, count) = scan(bytes, 32, expected_values, |value| {
                prefixes.push(length(value)?);
                Ok(())
            })?;
            let mut index = 0;
            let mut previous = 0;
            let mut reconstructed = 0usize;
            let mut stored = 0usize;
            let (suffix_offset, suffix_count) =
                scan(&bytes[offset..], 32, expected_values, |value| {
                    let suffix = length(value)?;
                    let prefix = *prefixes.get(index).ok_or(Error::Malformed)?;
                    if prefix > previous {
                        return Err(Error::Malformed);
                    }
                    previous = prefix.checked_add(suffix).ok_or(Error::Malformed)?;
                    // These are the actual bytes the physical decoder will
                    // retain, before Arrow's signed-32-bit offsets are built.
                    reconstructed = reconstructed
                        .checked_add(previous)
                        .filter(|&total| total <= i32::MAX as usize)
                        .ok_or(Error::Malformed)?;
                    stored = stored.checked_add(suffix).ok_or(Error::Malformed)?;
                    index += 1;
                    Ok(())
                })?;
            if count != expected_values
                || count != suffix_count
                || stored > bytes.len() - offset - suffix_offset
            {
                return Err(Error::Malformed);
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
                    Err(Error::Malformed)
                }
            );
        }
    }
}
