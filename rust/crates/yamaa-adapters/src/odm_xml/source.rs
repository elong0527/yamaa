//! Source admission: XML versus TAR detection and TAR member selection.
//!
//! A local file is a caller path, opened once through the local file
//! resolver: detection and both archive passes stream that handle, so an
//! archive is never held whole, and a plain XML file is refused above
//! `max_expanded_bytes` before it is read and captured under that bound.
//!
//! A caller's resolver serves a source as one capture, which every pass
//! reads. The resolver protocol has no streaming read, so the source is held
//! whole, under the per-file source bound of ten gigabytes and its own
//! [`Limit::SourceBytes`], before anything detects it or checks
//! `max_expanded_bytes`: a plain XML source above that option is refused only
//! once it is held. The first pass counts every member and selects the XML
//! member; the second streams only that member into the parser.
use super::{
    archive::{self, Member, Source, Stream, Tar, BLOCK},
    items,
    xml::Xml,
    Error, Fault,
};
use crate::{file_resources::Resources, public_table::PublicTable};
use flate2::read::MultiGzDecoder;
use std::{
    io::{self, BufReader, Seek},
    path::Path,
    sync::Arc,
};
use yamaa_core::odm_items::{self, Limit, Member as MemberRefusal, Options};

/// How the archive bytes are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compression {
    None,
    Gzip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Xml,
    Tar(Compression),
}

/// A caller path, read through the local file resolver: its parent is
/// selected once, and the file is opened as one name below it, a regular file
/// and never a link (FILE-7, FILE-8). The name is the caller's own file name,
/// not a written path, so it may hold a `:` or, on Unix, a `\`. Options are
/// checked once the file is found, before any of it is read.
pub(super) fn read(path: &Path, options: &Options) -> Result<PublicTable, Error> {
    let written = path
        .to_str()
        .ok_or(Error::Resource(crate::file_resources::Error::InvalidPath))?;
    let (resources, name) = crate::file_configuration::resources(written)
        .map_err(|crate::file_configuration::Error::Path(error)| refusal(error, Limit::XmlBytes))?;
    let file = resources
        .open_named(&name)
        .map_err(|error| refusal(error, Limit::XmlBytes))?;
    options.validate()?;
    let io_error = |error: io::Error| -> Error { Fault::from_io(&error).into() };
    let length = file.metadata().map_err(io_error)?.len();
    let open = || -> io::Result<Source<'_>> {
        let mut handle = &file;
        handle.rewind()?;
        Ok(Source::File(BufReader::new(&file)))
    };
    match detect(open, &name)? {
        Format::Xml => {
            admit_xml(length, options)?;
            let maximum = usize::try_from(options.max_expanded_bytes).unwrap_or(usize::MAX);
            let bytes = resources
                .capture_named(&name, maximum)
                .map_err(|error| refusal(error, Limit::XmlBytes))?;
            xml(&bytes, options)
        }
        Format::Tar(compression) => tar(open, compression, length, options),
    }
}

/// A source a caller's resolver serves under `source`, captured once after the
/// options are checked and held whole: under the per-file source bound, a
/// larger one refused as [`Limit::SourceBytes`], before `max_expanded_bytes`
/// applies to what it holds.
pub(super) fn read_with_resolver(
    resolver: Arc<dyn crate::host_resources::Resolver>,
    source: &str,
    options: &Options,
) -> Result<PublicTable, Error> {
    options.validate()?;
    let (bytes, _) = Resources::with_resolver(resolver, source)
        .capture(source)
        .map_err(|error| refusal(error, Limit::SourceBytes))?;
    let name = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let open = || -> io::Result<Source<'_>> { Ok(Source::Bytes(&bytes)) };
    match detect(open, name)? {
        Format::Xml => {
            admit_xml(length, options)?;
            xml(&bytes, options)
        }
        Format::Tar(compression) => tar(open, compression, length, options),
    }
}

/// A capture over its bound is over `limit`, the read's limit that bound
/// enforces. Any other failure is the resolver's: a name that reaches no
/// regular file, or one the resolver cannot serve, which the caller's path
/// reports as any caller's path does (FILE-7, FILE-8).
fn refusal(error: crate::file_resources::Error, limit: fn(u64) -> Limit) -> Error {
    match error {
        crate::file_resources::Error::Limit(bound) => {
            odm_items::Error::Limit(limit(u64::try_from(bound.limit).unwrap_or(u64::MAX))).into()
        }
        error => Error::Resource(error),
    }
}

/// A plain XML source takes no archive member and holds at most
/// `max_expanded_bytes`, refused before it is read.
fn admit_xml(length: u64, options: &Options) -> Result<(), Error> {
    if options.archive_member.is_some() {
        return Err(odm_items::Error::Member(MemberRefusal::RequiresArchive).into());
    }
    if length > options.max_expanded_bytes {
        return Err(odm_items::Error::Limit(Limit::XmlBytes(options.max_expanded_bytes)).into());
    }
    Ok(())
}

/// Read the clinical items of the captured plain XML `bytes`.
fn xml(bytes: &[u8], options: &Options) -> Result<PublicTable, Error> {
    items::read(Xml::new(
        bytes,
        Some(options.max_expanded_bytes),
        options.max_xml_depth,
    ))
}

/// Read the selected XML member of an archive of `length` bytes: one pass
/// counts the members and selects the XML member, and a second pass from the
/// start streams that member. `open` gives the archive from its start.
fn tar<'f>(
    open: impl Fn() -> io::Result<Source<'f>>,
    compression: Compression,
    length: u64,
    options: &Options,
) -> Result<PublicTable, Error> {
    let io_error = |error: io::Error| -> Error { Fault::from_io(&error).into() };
    let changed = || odm_items::Error::Archive {
        reason: "TAR member changed between reads",
    };
    let limit = options.max_expanded_bytes;
    let mut first = open_tar(open().map_err(io_error)?, compression, length, limit);
    let (index, selected) = select(&mut first, options)?;
    let mut tar = open_tar(open().map_err(io_error)?, compression, length, limit);
    for position in 0..=index {
        let member = tar.next_member().map_err(io_error)?.ok_or_else(changed)?;
        if position == index && (member.name != selected.name || member.size != selected.size) {
            return Err(changed().into());
        }
    }
    items::read(Xml::new(
        tar.into_member(selected.size),
        None,
        options.max_xml_depth,
    ))
}

/// Start one archive pass at the beginning of the source.
fn open_tar(source: Source<'_>, compression: Compression, length: u64, limit: u64) -> Tar<'_> {
    Tar::new(match compression {
        Compression::None => Stream::plain(source, length),
        Compression::Gzip => Stream::gzip(source, limit),
    })
}

/// Decide between plain XML and a plain or gzip TAR archive, refusing other
/// compressed archives and archive names that hold no archive. `open` gives
/// the source from its start; only its head is read.
fn detect<'f>(open: impl Fn() -> io::Result<Source<'f>>, name: &str) -> Result<Format, Error> {
    let io_error = |error: io::Error| -> Error { Fault::from_io(&error).into() };
    let archive = |reason| -> Error { odm_items::Error::Archive { reason }.into() };
    let mut head = [0_u8; BLOCK];
    let read = archive::read_up_to(&mut open().map_err(io_error)?, &mut head).map_err(io_error)?;
    let head = &head[..read];
    let name = name.to_ascii_lowercase();
    let suffix = [".tar", ".tar.gz", ".tgz"]
        .iter()
        .any(|suffix| name.ends_with(suffix));
    let not_archive = || {
        if suffix {
            Err(archive("not a TAR archive"))
        } else {
            Ok(Format::Xml)
        }
    };
    if head.starts_with(&[0x1F, 0x8B]) {
        let mut decoder = MultiGzDecoder::new(open().map_err(io_error)?);
        let mut block = [0_u8; BLOCK];
        let expanded = archive::read_up_to(&mut decoder, &mut block)
            .map_err(|_| archive("invalid gzip data"))?;
        return if expanded == BLOCK && archive::is_header(&block) {
            Ok(Format::Tar(Compression::Gzip))
        } else {
            not_archive()
        };
    }
    if let Ok(block) = <&[u8; BLOCK]>::try_from(head) {
        if archive::is_header(block) {
            return Ok(Format::Tar(Compression::None));
        }
    }
    let compressed: [(&[u8], &str); 3] = [
        (b"BZh", "bzip2"),
        (&[0xFD, b'7', b'z', b'X', b'Z', 0x00], "xz"),
        (&[0x28, 0xB5, 0x2F, 0xFD], "zstd"),
    ];
    for (magic, format) in compressed {
        if head.starts_with(magic) {
            return Err(odm_items::Error::Compression { format }.into());
        }
    }
    not_archive()
}

/// The first pass: count every member and select the one XML member.
fn select(tar: &mut Tar<'_>, options: &Options) -> Result<(usize, Member), Error> {
    let mut count = 0_usize;
    let mut candidates = 0_usize;
    let mut selected = None;
    while let Some(member) = tar
        .next_member()
        .map_err(|error| -> Error { Fault::from_io(&error).into() })?
    {
        count += 1;
        if count > options.max_archive_members {
            return Err(odm_items::Error::Limit(Limit::ArchiveMembers(
                options.max_archive_members,
            ))
            .into());
        }
        let candidate = match &options.archive_member {
            None => archive::is_xml_name(&member.name) && !archive::is_os_metadata(&member.name),
            Some(name) => member.name == name.as_bytes(),
        };
        if candidate {
            candidates += 1;
            if selected.is_none() {
                selected = Some((count - 1, member));
            }
        }
    }
    let refusal = match &options.archive_member {
        None => MemberRefusal::Candidates { count: candidates },
        Some(name) => MemberRefusal::Matches {
            name: name.clone(),
            count: candidates,
        },
    };
    let (index, member) = match selected {
        Some(selected) if candidates == 1 => selected,
        _ => return Err(odm_items::Error::Member(refusal).into()),
    };
    let name = String::from_utf8_lossy(&member.name).into_owned();
    if !archive::is_safe_name(&member.name) {
        return Err(odm_items::Error::Member(MemberRefusal::Unsafe { name }).into());
    }
    if !member.regular {
        return Err(odm_items::Error::Member(MemberRefusal::Irregular { name }).into());
    }
    if member.sparse {
        return Err(odm_items::Error::Member(MemberRefusal::Sparse { name }).into());
    }
    if member.size > options.max_expanded_bytes {
        return Err(odm_items::Error::Limit(Limit::MemberBytes(options.max_expanded_bytes)).into());
    }
    Ok((index, member))
}
