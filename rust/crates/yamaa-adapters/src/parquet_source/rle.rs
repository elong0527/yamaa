//! Allocation-free admission of Parquet RLE/bit-packed levels and indices.
use super::compact::Error;

fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], Error> {
    let head = bytes.get(..length).ok_or(Error::Malformed)?;
    *bytes = &bytes[length..];
    Ok(head)
}
fn unsigned(bytes: &mut &[u8]) -> Result<u32, Error> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = take(bytes, 1)?[0];
        if index == 4 && byte > 15 {
            return Err(Error::Malformed);
        }
        value |= u32::from(byte & 127) << (index * 7);
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(Error::Malformed)
}

pub(super) fn packed(
    bytes: &[u8],
    width: u32,
    values: usize,
    mut visit: impl FnMut(u32, usize) -> Result<(), Error>,
) -> Result<(), Error> {
    if width > 32 {
        return Err(Error::Malformed);
    }
    let size = values
        .checked_mul(width as usize)
        .ok_or(Error::Malformed)?
        .div_ceil(8);
    if size > bytes.len() {
        return Err(Error::Malformed);
    }
    for index in 0..values {
        let bit = index * width as usize;
        let offset = bit / 8;
        let shift = bit % 8;
        let octets = (shift + width as usize).div_ceil(8);
        let mut value = 0u64;
        for (index, &byte) in bytes[offset..offset + octets].iter().enumerate() {
            value |= u64::from(byte) << (index * 8);
        }
        visit(((value >> shift) & ((1u64 << width) - 1)) as u32, 1)?;
    }
    Ok(())
}

/// Visit exactly the caller-admitted number of values. Ignore trailing group
/// padding, but require its stored bytes before any access. RLE runs are visited
/// in one bounded operation, even when a tiny stream claims a very long run.
pub(super) fn hybrid(
    mut bytes: &[u8],
    width: u32,
    mut values: usize,
    mut visit: impl FnMut(u32, usize) -> Result<(), Error>,
) -> Result<(), Error> {
    if width > 32 {
        return Err(Error::Malformed);
    }
    while values != 0 {
        let indicator = unsigned(&mut bytes)?;
        let count = (indicator >> 1) as usize;
        if count == 0 {
            return Err(Error::Malformed);
        }
        if indicator & 1 == 0 {
            let raw = take(&mut bytes, width.div_ceil(8) as usize)?;
            let value = raw.iter().enumerate().fold(0u32, |value, (i, byte)| {
                value | (u32::from(*byte) << (i * 8))
            });
            if u64::from(value) >= (1u64 << width) {
                return Err(Error::Malformed);
            }
            let used = count.min(values);
            visit(value, used)?;
            values -= used;
        } else {
            let count = count.checked_mul(8).ok_or(Error::Malformed)?;
            let size = count.checked_mul(width as usize).ok_or(Error::Malformed)? / 8;
            let raw = take(&mut bytes, size)?;
            let used = count.min(values);
            packed(raw, width, used, &mut visit)?;
            values -= used;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_mixed_runs_and_padding_have_exact_index_order() {
        let mut out = Vec::new();
        hybrid(&[4, 2, 3, 0b0001_1000, 0b1110_0100], 2, 7, |v, n| {
            out.extend(std::iter::repeat_n(v, n));
            Ok(())
        })
        .unwrap();
        assert_eq!(out, [2, 2, 0, 2, 1, 0, 0]);
        let mut count = 0;
        hybrid(&[254, 255, 255, 255, 15], 0, 3, |v, n| {
            assert_eq!(v, 0);
            count += n;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 3);
        assert!(hybrid(&[3, 0], 1, 8, |_, _| Ok(())).is_ok());
    }
    #[test]
    fn malformed_widths_varints_and_truncated_groups_are_checked() {
        for data in [&[0][..], &[1], &[128], &[255, 255, 255, 255, 31], &[3, 0]] {
            assert!(matches!(
                hybrid(data, 3, 8, |_, _| Ok(())),
                Err(Error::Malformed)
            ));
        }
        assert!(matches!(
            hybrid(&[2, 255], 1, 1, |_, _| Ok(())),
            Err(Error::Malformed)
        ));
        assert!(matches!(
            hybrid(&[], 33, 0, |_, _| Ok(())),
            Err(Error::Malformed)
        ));
        hybrid(&[], 1, 0, |_, _| panic!("empty stream")).unwrap();
    }
}
