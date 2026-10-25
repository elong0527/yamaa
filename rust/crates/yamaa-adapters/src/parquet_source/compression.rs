//! Bound page expansion before allocation from an advertised decoded length.
//! Streaming codecs grow through fixed scratch buffers; raw codecs preflight
//! borrowed tags, and Zstandard grows only after destination-size errors.
//! This is not a total-RSS guarantee.
use super::closed_profile;
use super::format::Codec;
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
        // One extra byte detects a lying length without growing the result past
        // the page header's `expected` size.
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

fn take<'a>(input: &mut &'a [u8], length: usize) -> Result<&'a [u8], Error> {
    let (head, tail) = input.split_at_checked(length).ok_or(Error::Malformed)?;
    *input = tail;
    Ok(head)
}

fn expanded(written: &mut usize, length: usize, expected: usize) -> Result<(), Error> {
    *written = written
        .checked_add(length)
        .filter(|&length| length <= expected)
        .ok_or(Error::Malformed)?;
    Ok(())
}

// Raw Snappy and LZ4 require contiguous destinations. Prove their actual output
// lengths from borrowed tags before allowing a header to size that destination.
fn snappy_length(mut input: &[u8], expected: usize) -> Result<(), Error> {
    if snap::raw::decompress_len(input).map_err(|_| Error::Malformed)? != expected {
        return Err(Error::Malformed);
    }
    while take(&mut input, 1)?[0] & 128 != 0 {}
    let mut written = 0usize;
    while !input.is_empty() {
        let tag = take(&mut input, 1)?[0];
        let (length, offset) = match tag & 3 {
            0 => {
                let raw = usize::from(tag >> 2);
                let length = if raw < 60 {
                    raw + 1
                } else {
                    let octets = take(&mut input, raw - 59)?;
                    let mut value = [0u8; 4];
                    value[..octets.len()].copy_from_slice(octets);
                    (u32::from_le_bytes(value) as usize)
                        .checked_add(1)
                        .ok_or(Error::Malformed)?
                };
                take(&mut input, length)?;
                (length, None)
            }
            1 => (
                usize::from((tag >> 2) & 7) + 4,
                Some((usize::from(tag & 224) << 3) | usize::from(take(&mut input, 1)?[0])),
            ),
            2 => (
                usize::from(tag >> 2) + 1,
                Some(u16::from_le_bytes(take(&mut input, 2)?.try_into().unwrap()) as usize),
            ),
            _ => (
                usize::from(tag >> 2) + 1,
                Some(u32::from_le_bytes(take(&mut input, 4)?.try_into().unwrap()) as usize),
            ),
        };
        if offset.is_some_and(|offset| offset == 0 || offset > written) {
            return Err(Error::Malformed);
        }
        expanded(&mut written, length, expected)?;
    }
    if written == expected {
        Ok(())
    } else {
        Err(Error::Malformed)
    }
}

fn lz4_length(mut input: &[u8], expected: usize) -> Result<(), Error> {
    fn length(input: &mut &[u8], base: u8) -> Result<usize, Error> {
        let mut length = usize::from(base);
        if base == 15 {
            loop {
                let extra = take(input, 1)?[0];
                length = length
                    .checked_add(usize::from(extra))
                    .ok_or(Error::Malformed)?;
                if extra != 255 {
                    break;
                }
            }
        }
        Ok(length)
    }
    let mut written = 0usize;
    while !input.is_empty() {
        let token = take(&mut input, 1)?[0];
        let literals = length(&mut input, token >> 4)?;
        take(&mut input, literals)?;
        expanded(&mut written, literals, expected)?;
        if input.is_empty() {
            break;
        }
        let offset = u16::from_le_bytes(take(&mut input, 2)?.try_into().unwrap()) as usize;
        if offset == 0 || offset > written {
            return Err(Error::Malformed);
        }
        let matched = length(&mut input, token & 15)?
            .checked_add(4)
            .ok_or(Error::Malformed)?;
        expanded(&mut written, matched, expected)?;
    }
    if written == expected {
        Ok(())
    } else {
        Err(Error::Malformed)
    }
}

fn fixed_length(codec: i64, mut input: &[u8], expected: usize) -> Result<(), Error> {
    match codec {
        1 => snappy_length(input, expected),
        7 => lz4_length(input, expected),
        5 => {
            let mut written = 0usize;
            while !input.is_empty() {
                let header = take(&mut input, 8)?;
                let decoded = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
                let encoded = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
                lz4_length(take(&mut input, encoded)?, decoded)?;
                expanded(&mut written, decoded, expected)?;
            }
            if written == expected {
                Ok(())
            } else {
                Err(Error::Malformed)
            }
        }
        _ => Err(Error::Malformed),
    }
}

fn fixed_codec(codec: i64, mut input: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    fixed_length(codec, input, expected)?;
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
    // The bulk decoder uses the destination as history, so a valid frame's
    // large window does not cause a separate history allocation. Its typed
    // destination error follows block expansion, not the advertised frame
    // content size. Retry geometrically instead of reserving `expected`.
    let too_small = (zstd::zstd_safe::zstd_sys::ZSTD_ErrorCode::ZSTD_error_dstSize_tooSmall
        as usize)
        .wrapping_neg();
    let mut decoder = zstd::zstd_safe::DCtx::try_create().ok_or(Error::Malformed)?;
    let mut result = Vec::new();
    let mut length = expected.min(8192);
    loop {
        result
            .try_reserve_exact(length - result.len())
            .map_err(|_| Error::Malformed)?;
        result.resize(length, 0);
        match decoder.decompress(&mut result[..], input) {
            Ok(written) if written == expected => return Ok(result),
            Err(error) if error == too_small && length < expected => {
                length = length.saturating_mul(2).max(1).min(expected);
            }
            _ => return Err(Error::Malformed),
        }
    }
}

/// Validate every actual byte of expansion using the same immutable page body.
/// Legacy LZ4 retains the library's Hadoop → framed → raw compatibility order.
/// A recognized codec without an implementation stays explicit, not malformed.
#[cfg(test)]
fn verify((page, file): &(Page, &[u8])) -> Result<(), Error> {
    decode(page, file).map(|_| ())
}

/// Decode the exact page size before validating its encoded values: the
/// body of `page`, a page of the held file `file`.
pub(super) fn decode(page: &Page, file: &[u8]) -> Result<Vec<u8>, Error> {
    let body = file.get(page.body.clone()).ok_or(Error::Malformed)?;
    // Parquet PageHeader.uncompressed_page_size is a signed 32-bit field.
    // The compact scanner retains i64 integers, so enforce that physical
    // representation before any codec allocates or constructs its decoder.
    i32::try_from(page.decoded_bytes).map_err(|_| Error::Malformed)?;
    let prefix = page.uncompressed_prefix;
    if prefix > page.decoded_bytes || prefix > body.len() {
        return Err(Error::Malformed);
    }
    let codec = closed_profile::codec(page.codec);
    if !page.compressed || matches!(codec, Some((Codec::Uncompressed, _))) {
        return if body.len() == page.decoded_bytes {
            Ok(body.to_vec())
        } else {
            Err(Error::Malformed)
        };
    }
    let input = &body[prefix..];
    let expected = page.decoded_bytes - prefix;
    let mut decoded = match codec {
        None => Err(Error::Malformed),
        Some((Codec::Snappy, true)) => fixed_codec(1, input, expected),
        Some((Codec::Gzip, true)) => {
            exact_stream(flate2::read::MultiGzDecoder::new(input), expected)
        }
        Some((Codec::Brotli, true)) => {
            brotli_window(input)?;
            exact_stream(brotli::Decompressor::new(input, 4096), expected)
        }
        Some((Codec::Lz4, true)) => fixed_codec(5, input, expected)
            .or_else(|_| exact_stream(lz4_flex::frame::FrameDecoder::new(input), expected))
            .or_else(|_| fixed_codec(7, input, expected)),
        Some((Codec::Zstd, true)) => zstd(input, expected),
        Some((Codec::Lz4Raw, true)) => fixed_codec(7, input, expected),
        // A codec the profile marks unavailable, which this build has no
        // decoder for. An uncompressed page was read whole above.
        Some((Codec::Uncompressed | Codec::Lzo, _) | (_, false)) => {
            Err(Error::Unavailable(page.codec))
        }
    }?;
    if prefix != 0 {
        let mut with_prefix = Vec::new();
        with_prefix
            .try_reserve_exact(page.decoded_bytes)
            .map_err(|_| Error::Malformed)?;
        with_prefix.extend_from_slice(&body[..prefix]);
        with_prefix.append(&mut decoded);
        return Ok(with_prefix);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A page whose body is all of `body`, and that body as its file.
    fn page(body: &[u8], codec: i64, decoded_bytes: usize) -> (Page, &[u8]) {
        let page = Page {
            header: 0..0,
            body: 0..body.len(),
            codec,
            decoded_bytes,
            uncompressed_prefix: 0,
            compressed: true,
            column: 0,
            row_group: 0,
            payload: Default::default(),
        };
        (page, body)
    }

    fn decoded((page, file): &(Page, &[u8])) -> Result<Vec<u8>, Error> {
        decode(page, file)
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
            assert_eq!(decoded(&page(bytes, codec, plain.len())).unwrap(), plain);
        }
    }
    #[test]
    fn advertised_page_sizes_do_not_allocate_before_actual_expansion_is_checked() {
        for codec in 0..=7 {
            assert_eq!(
                decoded(&page(&[], codec, i32::MAX as usize + 1)),
                Err(Error::Malformed)
            );
        }
        // These tiny streams claim a legal but enormous page. Invalid raw tags
        // and short streaming output must be rejected without allocating it.
        for codec in [1, 2, 4, 5, 6, 7] {
            assert_eq!(
                decoded(&page(&[0], codec, i32::MAX as usize)),
                Err(Error::Malformed)
            );
        }
        assert_eq!(
            decoded(&page(&[255, 255, 255, 255, 7], 1, i32::MAX as usize)),
            Err(Error::Malformed)
        );
        assert_eq!(
            decoded(&page(
                &[127, 255, 255, 255, 0, 0, 0, 0],
                5,
                i32::MAX as usize
            )),
            Err(Error::Malformed)
        );
        let empty_gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default())
            .finish()
            .unwrap();
        let empty_zstd = zstd::stream::encode_all(&[][..], 1).unwrap();
        for (body, codec) in [(empty_gzip, 2), (empty_zstd, 6)] {
            assert_eq!(
                decoded(&page(&body, codec, i32::MAX as usize)),
                Err(Error::Malformed)
            );
        }
        assert_eq!(exact_stream(&[][..], usize::MAX), Err(Error::Malformed));
    }

    #[test]
    fn zstd_frame_and_page_claims_do_not_size_the_destination() {
        // This much stored input passes a format-wide expansion-ratio bound,
        // but invalid magic still must not allocate the claimed 2 GiB page.
        let stored = vec![0; 49_152];
        assert_eq!(
            decoded(&page(&stored, 6, i32::MAX as usize)),
            Err(Error::Malformed)
        );

        // A valid empty frame followed by a large skippable frame also has
        // ample input for that bound, but produces no decoded output.
        let mut empty_and_skippable = vec![0x28, 0xb5, 0x2f, 0xfd, 0, 0xa8, 1, 0, 0];
        empty_and_skippable.extend([0x50, 0x2a, 0x4d, 0x18]);
        empty_and_skippable.extend((stored.len() as u32).to_le_bytes());
        empty_and_skippable.extend(&stored);
        assert_eq!(decoded(&page(&empty_and_skippable, 6, 0)), Ok(Vec::new()));
        assert_eq!(
            decoded(&page(&empty_and_skippable, 6, i32::MAX as usize)),
            Err(Error::Malformed)
        );

        // A single-segment frame advertises a 2 GiB content size but ends with
        // an empty raw block. Its own false content size must not size output.
        let frame_size_lie = [
            0x28, 0xb5, 0x2f, 0xfd, 0xa0, 0xff, 0xff, 0xff, 0x7f, 1, 0, 0,
        ];
        assert_eq!(
            decoded(&page(&frame_size_lie, 6, i32::MAX as usize)),
            Err(Error::Malformed)
        );

        // The same large history window remains valid with actual raw output.
        let raw = [
            0x28, 0xb5, 0x2f, 0xfd, 0, 0xa8, 33, 0, 0, b'd', b'a', b't', b'a',
        ];
        assert_eq!(decoded(&page(&raw, 6, 4)).unwrap(), b"data");
    }

    #[test]
    fn raw_codec_preflight_handles_copy_forms_extended_lengths_and_invalid_offsets() {
        // One literal x, followed by an overlapping four-byte copy. The three
        // independently authored streams exercise each Snappy copy tag.
        for bytes in [
            &[5, 0, b'x', 1, 1][..],
            &[5, 0, b'x', 14, 1, 0][..],
            &[5, 0, b'x', 15, 1, 0, 0, 0][..],
        ] {
            assert_eq!(fixed_codec(1, bytes, 5).unwrap(), b"xxxxx");
        }
        let mut long_literal = vec![64, 240, 63];
        long_literal.extend([b'x'; 64]);
        assert_eq!(fixed_codec(1, &long_literal, 64).unwrap(), [b'x'; 64]);
        for bytes in [&[5, 0, b'x', 1, 0][..], &[5, 0, b'x', 1, 2][..]] {
            assert_eq!(fixed_codec(1, bytes, 5), Err(Error::Malformed));
        }
        let mut long_literal = vec![0xf0, 49];
        long_literal.extend([b'x'; 64]);
        assert_eq!(fixed_codec(7, &long_literal, 64).unwrap(), [b'x'; 64]);
        for bytes in [&[0x10, b'x', 0, 0][..], &[0x10, b'x', 2, 0][..]] {
            assert_eq!(fixed_codec(7, bytes, 5), Err(Error::Malformed));
        }
        let mut truncated_lz4 = long_literal;
        truncated_lz4.pop();
        assert_eq!(fixed_codec(7, &truncated_lz4, 64), Err(Error::Malformed));
    }

    #[test]
    fn raw_codec_preflight_preserves_empty_literal_and_repeated_encoder_outputs() {
        let mut state = 0x1234_5678u32;
        for length in [0usize, 1, 4, 59, 60, 61, 256, 4096, 65_536] {
            let literals: Vec<_> = (0..length)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect();
            for plain in [literals, vec![b'x'; length]] {
                let snappy = snap::raw::Encoder::new().compress_vec(&plain).unwrap();
                let lz4 = lz4_flex::block::compress(&plain);
                assert_eq!(fixed_codec(1, &snappy, length).unwrap(), plain);
                assert_eq!(fixed_codec(7, &lz4, length).unwrap(), plain);
            }
        }
    }

    #[test]
    fn malformed_compression_is_distinct_from_unavailable_codecs() {
        assert_eq!(verify(&page(&[0x91, 30], 4, 1)), Err(Error::Malformed));
        // Valid empty Zstandard frames may declare 32 MiB or 2 GiB history
        // windows without requiring any decoded bytes or history allocation.
        for window in [0x78, 0xa8] {
            let data = [0x28, 0xb5, 0x2f, 0xfd, 0, window, 1, 0, 0];
            assert_eq!(verify(&page(&data, 6, 0)), Ok(()));
        }
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
        let zstd = zstd::stream::encode_all(&plain[..], 1).unwrap();
        for (bytes, codec) in [(&snappy, 1), (&lz4, 7), (&lz4, 5), (&zstd, 6)] {
            assert_eq!(decoded(&page(bytes, codec, plain.len())).unwrap(), plain);
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
            assert_eq!(decoded(&page(&bytes, id, plain.len())).unwrap(), plain);
            assert_eq!(verify(&page(&bytes, id, 1)), Err(Error::Malformed));
        }
        let mut frame = lz4_flex::frame::FrameEncoder::new(Vec::new());
        frame.write_all(plain).unwrap();
        let frame = frame.finish().unwrap();
        assert_eq!(verify(&page(&frame, 5, plain.len())), Ok(()));
        assert_eq!(decoded(&page(&frame, 5, plain.len())).unwrap(), plain);
        assert_eq!(verify(&page(&frame, 5, 1)), Err(Error::Malformed));
        let raw = lz4_flex::block::compress(plain);
        assert_eq!(verify(&page(&raw, 5, plain.len())), Ok(()));
        assert_eq!(decoded(&page(&raw, 5, plain.len())).unwrap(), plain);
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
        data.0.uncompressed_prefix = 3;
        assert_eq!(verify(&data), Ok(()));
        assert_eq!(decoded(&data).unwrap(), b"\x01\x02\x03body");
        data.0.uncompressed_prefix = 8;
        assert_eq!(verify(&data), Err(Error::Malformed));
        let mut data = page(plain, 2, 4);
        data.0.compressed = false;
        assert_eq!(verify(&data), Ok(()));
    }
}
