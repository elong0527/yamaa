//! A bounded TAR reader over a plain or gzip-compressed archive. It reads
//! ustar, GNU and PAX headers and streams one member; it never extracts.
//!
//! Headers follow Python's `tarfile` where it is well defined: checksums may
//! be signed or unsigned, numbers octal or base-256, AREGTYPE names ending in
//! `/` are directories, directory names drop trailing slashes, and GNU long
//! names and PAX `path`/`size` override the header. A PAX global header is
//! validated and otherwise ignored. A zero block or the end of input at a
//! header boundary ends the archive; any other malformed or truncated header
//! or member is refused rather than read as the end of the archive.
use super::Fault;
use flate2::read::MultiGzDecoder;
use std::{
    fs::File,
    io::{self, BufReader, Read},
};
use yamaa_core::odm_items::Limit;

pub(super) const BLOCK: usize = 512;
/// GNU long names and PAX records describe one member; real ones are tiny.
const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Where an archive's bytes come from: a local file, read and skipped in
/// place so that no pass holds the archive, or the bytes a caller's resolver
/// captured.
pub(super) enum Source<'f> {
    File(BufReader<&'f File>),
    Bytes(&'f [u8]),
}

impl Source<'_> {
    /// Skip `amount` bytes that the source holds.
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let truncated = || Fault::Archive("truncated TAR member data").into_io();
        match self {
            Self::File(file) => file.seek_relative(i64::try_from(amount).map_err(|_| truncated())?),
            Self::Bytes(data) => {
                let amount = usize::try_from(amount)
                    .ok()
                    .filter(|amount| *amount <= data.len())
                    .ok_or_else(truncated)?;
                *data = &data[amount..];
                Ok(())
            }
        }
    }
}

impl Read for Source<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read(output),
            Self::Bytes(data) => data.read(output),
        }
    }
}

/// The archive byte stream. Plain data is skipped within the source's known
/// length; gzip data is decompressed and counted against the expansion bound
/// across the whole archive.
pub(super) enum Stream<'f> {
    Plain {
        source: Source<'f>,
        position: u64,
        length: u64,
    },
    Gzip {
        decoder: Box<MultiGzDecoder<Source<'f>>>,
        expanded: u64,
        limit: u64,
    },
}

impl<'f> Stream<'f> {
    /// A plain archive of `length` bytes, read from the source's start.
    pub(super) fn plain(source: Source<'f>, length: u64) -> Self {
        Self::Plain {
            source,
            position: 0,
            length,
        }
    }

    /// A gzip archive read from the source's start.
    pub(super) fn gzip(source: Source<'f>, limit: u64) -> Self {
        Self::Gzip {
            decoder: Box::new(MultiGzDecoder::new(source)),
            expanded: 0,
            limit,
        }
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        match self {
            Self::Plain {
                source,
                position,
                length,
            } => {
                if amount > length.saturating_sub(*position) {
                    return Err(Fault::Archive("truncated TAR member data").into_io());
                }
                source.skip(amount)?;
                *position += amount;
                Ok(())
            }
            Self::Gzip { .. } => {
                let copied = io::copy(&mut Read::by_ref(self).take(amount), &mut io::sink())?;
                if copied < amount {
                    return Err(Fault::Archive("truncated TAR member data").into_io());
                }
                Ok(())
            }
        }
    }
}

impl Read for Stream<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain {
                source, position, ..
            } => {
                let read = source.read(output)?;
                *position += read as u64;
                Ok(read)
            }
            Self::Gzip {
                decoder,
                expanded,
                limit,
            } => {
                let read = loop {
                    match decoder.read(output) {
                        Ok(read) => break read,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => return Err(Fault::Archive("invalid gzip data").into_io()),
                    }
                };
                *expanded = expanded.saturating_add(read as u64);
                if *expanded > *limit {
                    return Err(Fault::Limit(Limit::ExpandedBytes(*limit)).into_io());
                }
                Ok(read)
            }
        }
    }
}

/// Fill `buffer` until it is full or the input ends; return the bytes read.
pub(super) fn read_up_to(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

/// `size` rounded up to whole blocks, or `None` beyond `u64`.
fn padded(size: u64) -> Option<u64> {
    size.checked_next_multiple_of(BLOCK as u64)
}

/// The bytes before the first NUL.
fn until_nul(field: &[u8]) -> &[u8] {
    field
        .iter()
        .position(|&byte| byte == 0)
        .map_or(field, |end| &field[..end])
}

/// A numeric header field: base-256 with a 0x80 or 0xFF marker, or octal
/// text before the first NUL, surrounded by optional whitespace.
fn number(field: &[u8]) -> Option<i128> {
    match field.first()? {
        0x80 => Some(
            field[1..]
                .iter()
                .fold(0_i128, |value, &byte| (value << 8) | i128::from(byte)),
        ),
        0xFF => {
            let magnitude = field[1..]
                .iter()
                .fold(0_i128, |value, &byte| (value << 8) | i128::from(byte));
            Some(magnitude - (1_i128 << (8 * (field.len() - 1))))
        }
        _ => {
            let text = until_nul(field);
            let is_blank = |byte: &u8| matches!(byte, b'\t'..=b'\r' | 0x1C..=b' ');
            let start = text
                .iter()
                .position(|byte| !is_blank(byte))
                .unwrap_or(text.len());
            let end = text
                .iter()
                .rposition(|byte| !is_blank(byte))
                .map_or(start, |end| end + 1);
            text[start..end].iter().try_fold(0_i128, |value, &byte| {
                matches!(byte, b'0'..=b'7').then(|| value * 8 + i128::from(byte - b'0'))
            })
        }
    }
}

/// One validated 512-byte header block.
struct Header<'b> {
    name: &'b [u8],
    prefix: &'b [u8],
    typeflag: u8,
    size: u64,
    extended_sparse: bool,
}

impl<'b> Header<'b> {
    fn parse(block: &'b [u8; BLOCK]) -> Option<Self> {
        let stored = number(&block[148..156])?;
        let unsigned: i128 = block
            .iter()
            .enumerate()
            .map(|(index, &byte)| {
                if (148..156).contains(&index) {
                    32
                } else {
                    i128::from(byte)
                }
            })
            .sum();
        let signed: i128 = block
            .iter()
            .enumerate()
            .map(|(index, &byte)| {
                if (148..156).contains(&index) {
                    32
                } else {
                    i128::from(byte as i8)
                }
            })
            .sum();
        if stored != unsigned && stored != signed {
            return None;
        }
        for range in [100..108, 108..116, 116..124, 136..148, 329..337, 337..345] {
            number(&block[range])?;
        }
        let size = u64::try_from(number(&block[124..136])?).ok()?;
        let typeflag = block[156];
        // POSIX ustar headers carry a name prefix; GNU headers reuse the area.
        let prefix = if &block[257..263] == b"ustar\0" {
            until_nul(&block[345..500])
        } else {
            &[][..]
        };
        Some(Self {
            name: until_nul(&block[..100]),
            prefix,
            typeflag,
            size,
            extended_sparse: typeflag == b'S' && block[482] != 0,
        })
    }
}

/// Whether a first block opens a TAR archive: a valid header or the
/// end-of-archive zero block of an empty archive.
pub(super) fn is_header(block: &[u8; BLOCK]) -> bool {
    block.iter().all(|&byte| byte == 0) || Header::parse(block).is_some()
}

/// Overrides that PAX extended records supply to the next member.
#[derive(Default)]
struct Pax {
    path: Option<Vec<u8>>,
    size: Option<u64>,
    sparse: bool,
}

/// Parse `"%d %s=%s\n"` records, up to optional NUL padding.
fn parse_pax(data: &[u8], pax: &mut Pax) -> io::Result<()> {
    let invalid = || Fault::Archive("invalid PAX extended header").into_io();
    let mut rest = data;
    while rest.first().is_some_and(|&byte| byte != 0) {
        let space = rest
            .iter()
            .position(|&byte| byte == b' ')
            .ok_or_else(invalid)?;
        let digits = &rest[..space];
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) || digits.len() > 18 {
            return Err(invalid());
        }
        let length = std::str::from_utf8(digits)
            .ok()
            .and_then(|digits| digits.parse::<usize>().ok())
            .filter(|&length| length >= 5 && length <= rest.len() && length > space + 1)
            .ok_or_else(invalid)?;
        let record = &rest[..length];
        if record[length - 1] != b'\n' {
            return Err(invalid());
        }
        let body = &record[space + 1..length - 1];
        let equals = body
            .iter()
            .position(|&byte| byte == b'=')
            .filter(|&equals| equals > 0)
            .ok_or_else(invalid)?;
        let (key, value) = (&body[..equals], &body[equals + 1..]);
        match key {
            b"path" => pax.path = Some(value.to_vec()),
            b"GNU.sparse.name" => {
                pax.path = Some(value.to_vec());
                pax.sparse = true;
            }
            b"size" => {
                if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
                    return Err(invalid());
                }
                pax.size = Some(
                    std::str::from_utf8(value)
                        .ok()
                        .and_then(|value| value.parse::<u64>().ok())
                        .ok_or_else(invalid)?,
                );
            }
            key if key.starts_with(b"GNU.sparse.") => pax.sparse = true,
            _ => {}
        }
        rest = &rest[length..];
    }
    Ok(())
}

/// One archive member as admitted from its headers.
pub(super) struct Member {
    pub(super) name: Vec<u8>,
    pub(super) regular: bool,
    pub(super) sparse: bool,
    pub(super) size: u64,
}

/// Sequential members of one archive stream.
pub(super) struct Tar<'f> {
    stream: Stream<'f>,
    pending: u64,
    done: bool,
}

impl<'f> Tar<'f> {
    pub(super) fn new(stream: Stream<'f>) -> Self {
        Self {
            stream,
            pending: 0,
            done: false,
        }
    }

    fn read_block(&mut self, block: &mut [u8; BLOCK]) -> io::Result<usize> {
        read_up_to(&mut self.stream, block)
    }

    /// Read a metadata payload (PAX records or a GNU long name) and its padding.
    fn metadata(&mut self, size: u64) -> io::Result<Vec<u8>> {
        if size > MAX_METADATA_BYTES {
            return Err(Fault::Archive("TAR metadata header is too large").into_io());
        }
        let mut data = vec![0; size as usize];
        if read_up_to(&mut self.stream, &mut data)? != data.len() {
            return Err(Fault::Archive("truncated TAR metadata header").into_io());
        }
        self.stream
            .skip(padded(size).expect("bounded metadata size") - size)?;
        Ok(data)
    }

    /// The next member, skipping the unread data of the previous one.
    pub(super) fn next_member(&mut self) -> io::Result<Option<Member>> {
        if self.done {
            return Ok(None);
        }
        if self.pending > 0 {
            let pending = std::mem::take(&mut self.pending);
            self.stream.skip(pending)?;
        }
        let mut long_name = None;
        let mut pax = Pax::default();
        let mut extended = false;
        loop {
            let mut block = [0_u8; BLOCK];
            match self.read_block(&mut block)? {
                BLOCK => {}
                0 if !extended => {
                    self.done = true;
                    return Ok(None);
                }
                0 => return Err(Fault::Archive("TAR extension header has no member").into_io()),
                _ => return Err(Fault::Archive("truncated TAR header").into_io()),
            }
            if block.iter().all(|&byte| byte == 0) {
                if extended {
                    return Err(Fault::Archive("TAR extension header has no member").into_io());
                }
                self.done = true;
                return Ok(None);
            }
            let header = Header::parse(&block)
                .ok_or_else(|| Fault::Archive("invalid TAR header").into_io())?;
            let (typeflag, size) = (header.typeflag, header.size);
            match typeflag {
                b'x' | b'X' => {
                    let data = self.metadata(size)?;
                    parse_pax(&data, &mut pax)?;
                    extended = true;
                    continue;
                }
                b'g' => {
                    let data = self.metadata(size)?;
                    parse_pax(&data, &mut Pax::default())?;
                    continue;
                }
                b'L' => {
                    let data = self.metadata(size)?;
                    long_name = Some(until_nul(&data).to_vec());
                    extended = true;
                    continue;
                }
                b'K' => {
                    self.metadata(size)?;
                    extended = true;
                    continue;
                }
                _ => {}
            }
            let directory_by_slash = typeflag == 0 && header.name.ends_with(b"/");
            let mut name = if header.prefix.is_empty() || typeflag == b'S' {
                header.name.to_vec()
            } else {
                [header.prefix, b"/", header.name].concat()
            };
            let extended_sparse = header.extended_sparse;
            if let Some(long_name) = long_name {
                name = long_name;
            }
            if let Some(path) = pax.path {
                name = path;
                while name.ends_with(b"/") {
                    name.pop();
                }
            }
            let directory = typeflag == b'5' || directory_by_slash;
            if directory {
                while name.ends_with(b"/") {
                    name.pop();
                }
            }
            let regular = !directory && matches!(typeflag, b'0' | 0 | b'7' | b'S');
            let sparse = typeflag == b'S' || pax.sparse;
            if extended_sparse {
                // Old GNU sparse maps continue in extension blocks before the data.
                loop {
                    let mut extension = [0_u8; BLOCK];
                    if self.read_block(&mut extension)? != BLOCK {
                        return Err(Fault::Archive("truncated TAR sparse header").into_io());
                    }
                    if extension[504] == 0 {
                        break;
                    }
                }
            }
            let size = pax.size.unwrap_or(size);
            // Links, devices, FIFOs and directories carry no data blocks
            // whatever their size field says; unknown types are skipped.
            let has_data = regular || !(directory || matches!(typeflag, b'1'..=b'6'));
            self.pending = if has_data {
                padded(size).ok_or_else(|| Fault::Archive("invalid TAR member size").into_io())?
            } else {
                0
            };
            return Ok(Some(Member {
                name,
                regular,
                sparse,
                size,
            }));
        }
    }

    /// Stream the data of the member just returned by [`Tar::next_member`].
    pub(super) fn into_member(self, size: u64) -> MemberData<'f> {
        MemberData {
            stream: self.stream,
            remaining: size,
        }
    }
}

/// Exactly the bytes of one member; a short archive is refused.
pub(super) struct MemberData<'f> {
    stream: Stream<'f>,
    remaining: u64,
}

impl Read for MemberData<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || output.is_empty() {
            return Ok(0);
        }
        let wanted = output
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let read = self.stream.read(&mut output[..wanted])?;
        if read == 0 {
            return Err(Fault::Archive("truncated TAR member data").into_io());
        }
        self.remaining -= read as u64;
        Ok(read)
    }
}

/// The final path component and every component, as `PurePosixPath` sees them.
fn components(name: &[u8]) -> impl Iterator<Item = &[u8]> {
    name.split(|&byte| byte == b'/')
        .filter(|component| !component.is_empty() && *component != b".")
}

/// macOS metadata: an AppleDouble `._*` file or anything under `__MACOSX`.
pub(super) fn is_os_metadata(name: &[u8]) -> bool {
    components(name)
        .last()
        .is_some_and(|last| last.starts_with(b"._"))
        || components(name).any(|component| component == b"__MACOSX")
}

/// A candidate XML member name, compared ASCII case-insensitively.
pub(super) fn is_xml_name(name: &[u8]) -> bool {
    name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(b".xml")
}

/// A relative member path without parent references.
pub(super) fn is_safe_name(name: &[u8]) -> bool {
    !name.starts_with(b"/") && !components(name).any(|component| component == b"..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_octal_text_and_both_base_256_signs() {
        assert_eq!(number(b"0000644\0"), Some(0o644));
        assert_eq!(number(b"  644 \0\0"), Some(0o644));
        assert_eq!(number(b"\0\0\0\0\0\0\0\0"), Some(0));
        assert_eq!(number(b"        "), Some(0));
        assert_eq!(number(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]), Some(256));
        assert_eq!(number(&[0xFF; 8]), Some(-1));
        for invalid in [&b"0000648\0"[..], b"+0000644", b"0_644\0\0\0", b"\x81abc"] {
            assert_eq!(number(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn member_names_follow_posix_path_components() {
        assert!(is_xml_name(b"study/ODM.XML"));
        assert!(!is_xml_name(b"study/odm.xml.gz"));
        assert!(!is_xml_name(b"xml"));
        assert!(is_os_metadata(b"study/._odm.xml"));
        assert!(is_os_metadata(b"__MACOSX/study/odm.xml"));
        assert!(is_os_metadata(b"./__MACOSX/odm.xml"));
        assert!(!is_os_metadata(b"__MACOSX_/odm.xml"));
        assert!(!is_os_metadata(b"study/_odm.xml"));
        assert!(is_safe_name(b"study/odm.xml"));
        assert!(is_safe_name(b"study/..odm.xml"));
        assert!(!is_safe_name(b"/study/odm.xml"));
        assert!(!is_safe_name(b"study/../odm.xml"));
        assert!(!is_safe_name(b"../odm.xml"));
    }

    #[test]
    fn pax_records_need_exact_framing() {
        let mut pax = Pax::default();
        parse_pax(
            b"27 path=study/long/odm.xml\n15 size=123456\n\0\0",
            &mut pax,
        )
        .unwrap();
        assert_eq!(pax.path.as_deref(), Some(&b"study/long/odm.xml"[..]));
        assert_eq!(pax.size, Some(123_456));
        assert!(!pax.sparse);
        for invalid in [
            &b"26 path=study/long/odm.xml\n"[..],
            b"4 a=\n",
            b"x path=a\n",
            b"10 =value\n",
            b"13 size=12a4\n",
            b"99 path=a\n",
        ] {
            assert!(
                parse_pax(invalid, &mut Pax::default()).is_err(),
                "{invalid:?}"
            );
        }
        let mut sparse = Pax::default();
        parse_pax(b"22 GNU.sparse.major=1\n", &mut sparse).unwrap();
        assert!(sparse.sparse);
    }
}
