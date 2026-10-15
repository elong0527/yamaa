//! Verify actual page expansion before the ordinary codec sees the same held bytes.
//! Stream codecs write into a fixed scratch buffer; advertised lengths never
//! authorize unchecked read_to_end growth. This is not a total-RSS guarantee.
use super::framing::Page;
use std::io::Read;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Malformed,
    Unavailable(i64),
}

fn exact_stream(mut reader: impl Read, expected: usize) -> Result<Vec<u8>, Error> {
    let mut result = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let remaining = expected - result.len();
        // One extra byte detects a lying length; memory does not grow with the
        // stream. The page header supplies `expected`.
        let length = buffer.len().min(remaining.saturating_add(1));
        let read = reader
            .read(&mut buffer[..length])
            .map_err(|_| Error::Malformed)?;
        if read == 0 {
            return if result.len() == expected {
                Ok(result)
            } else {
                Err(Error::Malformed)
            };
        }
        if read > remaining {
            return Err(Error::Malformed);
        }
        result.try_reserve(read).map_err(|_| Error::Malformed)?;
        result.extend_from_slice(&buffer[..read]);
    }
}

fn fixed_codec(codec: i64, mut input: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    // A Snappy copy tag emits at most 64 bytes. LZ4 length-extension bytes
    // contribute at most 255 bytes each; 256 also covers its token/base length.
    // Neither format can justify a giant output from only a tiny header. Check
    // this before reserving the non-streaming decoder's destination buffer.
    let expansion = if codec == 1 { 64 } else { 256 };
    if expected > input.len().saturating_mul(expansion) {
        return Err(Error::Malformed);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(expected)
        .map_err(|_| Error::Malformed)?;
    output.resize(expected, 0);
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
        Ok(output)
    } else {
        Err(Error::Malformed)
    }
}

fn brotli_window(input: &[u8]) -> Result<(), Error> {
    // Validate the Brotli WBITS field, including the large-window extension,
    // before constructing the decoder.
    let first = *input.first().ok_or(Error::Malformed)?;
    let _log = if first & 1 == 0 {
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
    Ok(())
}

fn zstd(input: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    let decoder = zstd::stream::read::Decoder::new(input).map_err(|_| Error::Malformed)?;
    exact_stream(decoder, expected)
}

/// Validate every actual byte of expansion using the same immutable page body.
/// Legacy LZ4 retains the library's Hadoop → framed → raw compatibility order.
/// A recognized codec without an implementation stays explicit, not malformed.
#[cfg(test)]
fn verify(page: &Page<'_>) -> Result<(), Error> {
    decode(page).map(|_| ())
}

/// Decode the exact page size before validating its encoded values.
pub(super) fn decode(page: &Page<'_>) -> Result<Vec<u8>, Error> {
    // Parquet stores uncompressed_page_size as i32. The compact parser's wider
    // integer representation must not turn an invalid header into an allocation.
    if page.decoded_bytes > i32::MAX as usize
        || page.decoded_bytes > yamaa_core::limits::Limits::SOURCE_BYTES
    {
        return Err(Error::Malformed);
    }
    let prefix = page.uncompressed_prefix;
    if prefix > page.decoded_bytes || prefix > page.body.len() {
        return Err(Error::Malformed);
    }
    if !page.compressed || page.codec == 0 {
        return if page.body.len() == page.decoded_bytes {
            Ok(page.body.to_vec())
        } else {
            Err(Error::Malformed)
        };
    }
    let input = &page.body[prefix..];
    let expected = page.decoded_bytes - prefix;
    let mut decoded = match page.codec {
        1 => fixed_codec(1, input, expected),
        2 => exact_stream(flate2::read::MultiGzDecoder::new(input), expected),
        3 => Err(Error::Unavailable(3)),
        4 => {
            brotli_window(input)?;
            exact_stream(brotli::Decompressor::new(input, 4096), expected)
        }
        5 => fixed_codec(5, input, expected)
            .or_else(|_| exact_stream(lz4_flex::frame::FrameDecoder::new(input), expected))
            .or_else(|_| fixed_codec(7, input, expected)),
        6 => zstd(input, expected),
        7 => fixed_codec(7, input, expected),
        _ => Err(Error::Malformed),
    }?;
    if prefix != 0 {
        let mut with_prefix = Vec::new();
        with_prefix
            .try_reserve_exact(page.decoded_bytes)
            .map_err(|_| Error::Malformed)?;
        with_prefix.extend_from_slice(&page.body[..prefix]);
        with_prefix.append(&mut decoded);
        return Ok(with_prefix);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn page(body: &[u8], codec: i64, decoded_bytes: usize) -> Page<'_> {
        Page {
            body,
            codec,
            decoded_bytes,
            uncompressed_prefix: 0,
            compressed: true,
            column: 0,
            row_group: 0,
            payload: Default::default(),
        }
    }
    #[test]
    fn stream_expansion_is_capped_even_when_the_header_lies() {
        let plain = vec![b'x'; 100_000];
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&plain).unwrap();
        let gzip = gzip.finish().unwrap();
        assert_eq!(verify(&page(&gzip, 2, plain.len())), Ok(()));
        assert_eq!(verify(&page(&gzip, 2, 1)), Err(Error::Malformed));
        assert_eq!(
            verify(&page(&gzip, 2, plain.len() + 1)),
            Err(Error::Malformed)
        );
        let mut brotli = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut brotli, 4096, 3, 22);
            writer.write_all(&plain).unwrap();
        }
        assert_eq!(verify(&page(&brotli, 4, plain.len())), Ok(()));
        assert_eq!(verify(&page(&brotli, 4, 1)), Err(Error::Malformed));
        let zstd = zstd::stream::encode_all(&plain[..], 1).unwrap();
        assert_eq!(verify(&page(&zstd, 6, plain.len())), Ok(()));
        assert_eq!(verify(&page(&zstd, 6, 1)), Err(Error::Malformed));
        for (bytes, codec) in [(&gzip, 2), (&brotli, 4), (&zstd, 6)] {
            assert_eq!(decode(&page(bytes, codec, plain.len())).unwrap(), plain);
        }
    }
    #[test]
    fn malformed_compression_is_distinct_from_unavailable_codecs() {
        assert_eq!(verify(&page(&[0x91, 30], 4, 1)), Err(Error::Malformed));
        // A valid empty Zstandard frame may declare a large history window.
        let data = [0x28, 0xb5, 0x2f, 0xfd, 0, 0x78, 1, 0, 0];
        assert_eq!(verify(&page(&data, 6, 0)), Ok(()));
        assert_eq!(verify(&page(&[0], 6, 0)), Err(Error::Malformed));
        assert_eq!(verify(&page(&[], 3, 0)), Err(Error::Unavailable(3)));
    }
    #[test]
    fn hostile_page_lengths_do_not_allocate_the_claimed_output() {
        // A structurally impossible i64 page size is refused before any codec
        // can reserve it. The wire field itself only permits signed 32 bits.
        for codec in 0..=7 {
            assert_eq!(
                verify(&page(&[], codec, i32::MAX as usize + 1)),
                Err(Error::Malformed)
            );
        }
        // These lengths fit the wire field but cannot follow from the tiny
        // payload. Stream decoders discover their actual end; block decoders
        // reject impossible format expansion before reserving the destination.
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(b"x").unwrap();
        let gzip = gzip.finish().unwrap();
        let mut brotli = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut brotli, 4096, 3, 22);
            writer.write_all(b"x").unwrap();
        }
        let zstd = zstd::stream::encode_all(&b"x"[..], 1).unwrap();
        for (bytes, codec) in [
            (&b"\xff\xff\xff\xff\x07"[..], 1),
            (&gzip, 2),
            (&brotli, 4),
            (&b"\0"[..], 5),
            (&zstd, 6),
            (&b"\0"[..], 7),
        ] {
            assert_eq!(
                verify(&page(bytes, codec, i32::MAX as usize)),
                Err(Error::Malformed)
            );
        }
    }
    #[test]
    fn format_expansion_checks_keep_valid_high_ratio_block_streams() {
        let plain = vec![b'x'; 512 * 1024];
        let snappy = snap::raw::Encoder::new().compress_vec(&plain).unwrap();
        let lz4 = lz4_flex::block::compress(&plain);
        assert!(lz4.len() * 200 < plain.len());
        for (bytes, codec) in [(&snappy, 1), (&lz4, 7), (&lz4, 5)] {
            assert_eq!(decode(&page(bytes, codec, plain.len())).unwrap(), plain);
        }
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
            assert_eq!(verify(&page(&bytes, id, plain.len())), Ok(()));
            assert_eq!(decode(&page(&bytes, id, plain.len())).unwrap(), plain);
            assert_eq!(verify(&page(&bytes, id, 1)), Err(Error::Malformed));
        }
        let mut frame = lz4_flex::frame::FrameEncoder::new(Vec::new());
        frame.write_all(plain).unwrap();
        let frame = frame.finish().unwrap();
        assert_eq!(verify(&page(&frame, 5, plain.len())), Ok(()));
        assert_eq!(decode(&page(&frame, 5, plain.len())).unwrap(), plain);
        assert_eq!(verify(&page(&frame, 5, 1)), Err(Error::Malformed));
        let raw = lz4_flex::block::compress(plain);
        assert_eq!(verify(&page(&raw, 5, plain.len())), Ok(()));
        assert_eq!(decode(&page(&raw, 5, plain.len())).unwrap(), plain);
    }
    #[test]
    fn v2_levels_stay_outside_compressed_bytes() {
        let plain = b"body";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(plain).unwrap();
        let gzip = gzip.finish().unwrap();
        let mut bytes = vec![1, 2, 3];
        bytes.extend(gzip);
        let mut data = page(&bytes, 2, 7);
        data.uncompressed_prefix = 3;
        assert_eq!(verify(&data), Ok(()));
        assert_eq!(decode(&data).unwrap(), b"\x01\x02\x03body");
        data.uncompressed_prefix = 8;
        assert_eq!(verify(&data), Err(Error::Malformed));
        let mut data = page(plain, 2, 4);
        data.compressed = false;
        assert_eq!(verify(&data), Ok(()));
    }
}
