//! Verify actual page expansion before the ordinary codec sees the same held bytes.
//! Stream codecs write into a fixed scratch buffer; advertised lengths never
//! authorize unchecked read_to_end growth. This is not a total-RSS guarantee.
use super::framing::Page;
use std::io::Read;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Malformed,
    Limit,
    Unavailable(i64),
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub page_bytes: usize,
    pub window_log: u32,
}

fn exact_stream(mut reader: impl Read, expected: usize) -> Result<(), Error> {
    let mut count = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        let remaining = expected - count;
        // One extra byte detects a lying length; memory does not grow with the
        // stream. The framing budget has already admitted `expected`.
        let length = buffer.len().min(remaining.saturating_add(1));
        let read = reader
            .read(&mut buffer[..length])
            .map_err(|_| Error::Malformed)?;
        if read == 0 {
            return if count == expected {
                Ok(())
            } else {
                Err(Error::Malformed)
            };
        }
        if read > remaining {
            return Err(Error::Malformed);
        }
        count += read;
    }
}

fn fixed_codec(codec: i64, mut input: &[u8], expected: usize) -> Result<(), Error> {
    let mut output = vec![0; expected];
    let written = match codec {
        1 => snap::raw::Decoder::new()
            .decompress(input, &mut output)
            .map_err(|_| Error::Malformed)?,
        7 => lz4_flex::block::decompress_into(input, &mut output).map_err(|_| Error::Malformed)?,
        5 => {
            let mut written = 0usize;
            while !input.is_empty() {
                let header = input.get(..8).ok_or(Error::Malformed)?;
                let decoded = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
                let encoded = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
                let end = 8usize.checked_add(encoded).ok_or(Error::Malformed)?;
                let body = input.get(8..end).ok_or(Error::Malformed)?;
                let output_end = written.checked_add(decoded).ok_or(Error::Malformed)?;
                let destination = output
                    .get_mut(written..output_end)
                    .ok_or(Error::Malformed)?;
                let count = lz4_flex::block::decompress_into(body, destination)
                    .map_err(|_| Error::Malformed)?;
                if count != decoded {
                    return Err(Error::Malformed);
                }
                written = output_end;
                input = &input[end..];
            }
            written
        }
        _ => return Err(Error::Malformed),
    };
    if written == expected {
        Ok(())
    } else {
        Err(Error::Malformed)
    }
}

fn brotli_window(input: &[u8], maximum: u32) -> Result<(), Error> {
    // The pinned decoder enables the large-window extension. Read WBITS before
    // constructing it; a capped output reader alone does not bound its history.
    let first = *input.first().ok_or(Error::Malformed)?;
    let log = if first & 1 == 0 {
        16
    } else {
        let high = (first >> 1) & 7;
        if high != 0 {
            17 + u32::from(high)
        } else {
            match (first >> 4) & 7 {
                0 => 17,
                1 => {
                    if first & 128 != 0 {
                        return Err(Error::Malformed);
                    }
                    let log = u32::from(*input.get(1).ok_or(Error::Malformed)? & 63);
                    if !(10..=30).contains(&log) {
                        return Err(Error::Malformed);
                    }
                    log
                }
                n => 8 + u32::from(n),
            }
        }
    };
    if log > maximum {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}

fn zstd(input: &[u8], expected: usize, maximum_window: u32) -> Result<(), Error> {
    use zstd::zstd_safe::{DCtx, DParameter, InBuffer, OutBuffer};
    let mut decoder = DCtx::try_create().ok_or(Error::Limit)?;
    decoder
        .set_parameter(DParameter::WindowLogMax(maximum_window))
        .map_err(|_| Error::Limit)?;
    let mut input = InBuffer { src: input, pos: 0 };
    let mut scratch = [0u8; 8192];
    let mut count = 0usize;
    loop {
        let remaining = expected - count;
        let length = scratch.len().min(remaining.saturating_add(1));
        let before = input.pos;
        let mut output = OutBuffer::around(&mut scratch[..length]);
        let hint = decoder
            .decompress_stream(&mut output, &mut input)
            .map_err(|code| {
                // zstd-safe retains the native error code, but exposes its stable
                // name instead of a safe error-enum conversion. Pin the specific
                // window-policy failure; do not label all decoder errors as limits.
                if zstd::zstd_safe::get_error_name(code)
                    == "Frame requires too much memory for decoding"
                {
                    Error::Limit
                } else {
                    Error::Malformed
                }
            })?;
        let written = output.pos();
        if written > remaining {
            return Err(Error::Malformed);
        }
        count += written;
        if hint == 0 && input.pos == input.src.len() {
            return if count == expected {
                Ok(())
            } else {
                Err(Error::Malformed)
            };
        }
        if before == input.pos && written == 0 {
            return Err(Error::Malformed);
        }
    }
}

/// Validate every actual byte of expansion using the same immutable page body.
/// Legacy LZ4 retains the library's Hadoop → framed → raw compatibility order.
/// A recognized codec without an implementation stays explicit, not malformed.
pub(super) fn verify(page: &Page<'_>, limits: Limits) -> Result<(), Error> {
    if page.decoded_bytes > limits.page_bytes
        || page.body.len() > limits.page_bytes
        || !(10..=24).contains(&limits.window_log)
    {
        return Err(Error::Limit);
    }
    let prefix = page.uncompressed_prefix;
    if prefix > page.decoded_bytes || prefix > page.body.len() {
        return Err(Error::Malformed);
    }
    if !page.compressed || page.codec == 0 {
        return if page.body.len() == page.decoded_bytes {
            Ok(())
        } else {
            Err(Error::Malformed)
        };
    }
    let input = &page.body[prefix..];
    let expected = page.decoded_bytes - prefix;
    match page.codec {
        1 => fixed_codec(1, input, expected),
        2 => exact_stream(flate2::read::MultiGzDecoder::new(input), expected),
        3 => Err(Error::Unavailable(3)),
        4 => {
            brotli_window(input, limits.window_log)?;
            exact_stream(brotli::Decompressor::new(input, 4096), expected)
        }
        5 => fixed_codec(5, input, expected)
            .or_else(|_| exact_stream(lz4_flex::frame::FrameDecoder::new(input), expected))
            .or_else(|_| fixed_codec(7, input, expected)),
        6 => zstd(input, expected, limits.window_log),
        7 => fixed_codec(7, input, expected),
        _ => Err(Error::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn limits() -> Limits {
        Limits {
            page_bytes: 1024 * 1024,
            window_log: 24,
        }
    }
    fn page(body: &[u8], codec: i64, decoded_bytes: usize) -> Page<'_> {
        Page {
            body,
            codec,
            decoded_bytes,
            uncompressed_prefix: 0,
            compressed: true,
        }
    }
    #[test]
    fn stream_expansion_is_capped_even_when_the_header_lies() {
        let plain = vec![b'x'; 100_000];
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&plain).unwrap();
        let gzip = gzip.finish().unwrap();
        assert_eq!(verify(&page(&gzip, 2, plain.len()), limits()), Ok(()));
        assert_eq!(verify(&page(&gzip, 2, 1), limits()), Err(Error::Malformed));
        assert_eq!(
            verify(&page(&gzip, 2, plain.len() + 1), limits()),
            Err(Error::Malformed)
        );
        let mut brotli = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut brotli, 4096, 3, 22);
            writer.write_all(&plain).unwrap();
        }
        assert_eq!(verify(&page(&brotli, 4, plain.len()), limits()), Ok(()));
        assert_eq!(
            verify(&page(&brotli, 4, 1), limits()),
            Err(Error::Malformed)
        );
        let zstd = zstd::stream::encode_all(&plain[..], 1).unwrap();
        assert_eq!(verify(&page(&zstd, 6, plain.len()), limits()), Ok(()));
        assert_eq!(verify(&page(&zstd, 6, 1), limits()), Err(Error::Malformed));
    }
    #[test]
    fn window_policy_is_checked_before_history_allocation() {
        assert_eq!(
            verify(&page(&[0x11, 30], 4, 1), limits()),
            Err(Error::Limit)
        );
        assert_eq!(
            verify(&page(&[0x91, 30], 4, 1), limits()),
            Err(Error::Malformed)
        );
        // Zstandard frame with 2^25-byte history and an empty final raw block.
        let data = [0x28, 0xb5, 0x2f, 0xfd, 0, 0x78, 1, 0, 0];
        assert_eq!(verify(&page(&data, 6, 0), limits()), Err(Error::Limit));
        assert_eq!(verify(&page(&[0], 6, 0), limits()), Err(Error::Malformed));
        assert_eq!(
            verify(&page(&[], 3, 0), limits()),
            Err(Error::Unavailable(3))
        );
    }
    #[test]
    fn fixed_output_and_legacy_lz4_do_not_silently_narrow_codec_support() {
        let plain = b"same bytes same bytes same bytes";
        let snappy = snap::raw::Encoder::new().compress_vec(plain).unwrap();
        let raw = lz4_flex::block::compress(plain);
        let mut hadoop = Vec::new();
        hadoop.extend((plain.len() as u32).to_be_bytes());
        hadoop.extend((raw.len() as u32).to_be_bytes());
        hadoop.extend(&raw);
        for (bytes, id) in [(snappy, 1), (hadoop, 5), (raw, 7)] {
            assert_eq!(verify(&page(&bytes, id, plain.len()), limits()), Ok(()));
            assert_eq!(
                verify(&page(&bytes, id, 1), limits()),
                Err(Error::Malformed)
            );
        }
        let mut frame = lz4_flex::frame::FrameEncoder::new(Vec::new());
        frame.write_all(plain).unwrap();
        let frame = frame.finish().unwrap();
        assert_eq!(verify(&page(&frame, 5, plain.len()), limits()), Ok(()));
        assert_eq!(verify(&page(&frame, 5, 1), limits()), Err(Error::Malformed));
        let raw = lz4_flex::block::compress(plain);
        assert_eq!(verify(&page(&raw, 5, plain.len()), limits()), Ok(()));
    }
    #[test]
    fn v2_levels_stay_outside_compressed_bytes_and_size_policy_is_explicit() {
        let plain = b"body";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(plain).unwrap();
        let gzip = gzip.finish().unwrap();
        let mut bytes = vec![1, 2, 3];
        bytes.extend(gzip);
        let mut data = page(&bytes, 2, 7);
        data.uncompressed_prefix = 3;
        assert_eq!(verify(&data, limits()), Ok(()));
        data.uncompressed_prefix = 8;
        assert_eq!(verify(&data, limits()), Err(Error::Malformed));
        assert_eq!(
            verify(
                &page(plain, 0, 4),
                Limits {
                    page_bytes: 3,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        );
        let mut data = page(plain, 2, 4);
        data.compressed = false;
        assert_eq!(verify(&data, limits()), Ok(()));
    }
}
