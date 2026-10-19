//! Source admission: capture through a resolver, XML versus TAR detection
//! and TAR member selection.
//!
//! The source is captured once, through the local file resolver or a caller's
//! resolver, and every pass reads that one snapshot. The first pass counts
//! every member and selects the XML member; the second streams only that
//! member into the parser.
use super::{
    archive::{self, Member, Stream, Tar, BLOCK},
    items,
    xml::Xml,
    Error, Fault,
};
use crate::{file_resources::Resources, public_table::PublicTable};
use flate2::read::MultiGzDecoder;
use std::{path::Path, sync::Arc};
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

/// A caller path, captured through the local file resolver: its parent is
/// selected once and the file itself must be a regular file (FILE-7, FILE-8).
pub(super) fn read(path: &Path, options: &Options) -> Result<PublicTable, Error> {
    let display = path.display().to_string();
    let written = path.to_str().ok_or_else(|| not_found(&display))?;
    let (resources, name) = crate::file_configuration::resources(written)
        .map_err(|crate::file_configuration::Error::Path(error)| refusal(&error, &display))?;
    capture(resources, &name, &display, options)
}

/// A source a caller's resolver serves under `source`.
pub(super) fn read_with_resolver(
    resolver: Arc<dyn crate::host_resources::Resolver>,
    source: &str,
    options: &Options,
) -> Result<PublicTable, Error> {
    capture(
        Resources::with_resolver(resolver, source),
        source,
        source,
        options,
    )
}

fn not_found(display: &str) -> Error {
    odm_items::Error::NotFound {
        path: display.into(),
    }
    .into()
}

/// A resource the resolver does not serve as a regular file is not found;
/// any other failure is the read's own I/O failure.
fn refusal(error: &crate::file_resources::Error, display: &str) -> Error {
    use crate::file_resources::Error as E;
    match error {
        E::Missing
        | E::NotRegularFile
        | E::Symlink
        | E::InvalidPath
        | E::WrittenForm(_)
        | E::OutsideRoots
        | E::InvalidRoot
        | E::InvalidBase => not_found(display),
        error => odm_items::Error::Io {
            path: display.into(),
            reason: error.message().into(),
        }
        .into(),
    }
}

fn capture(
    mut resources: Resources,
    written: &str,
    display: &str,
    options: &Options,
) -> Result<PublicTable, Error> {
    let (bytes, _) = resources
        .capture(written)
        .map_err(|error| refusal(&error, display))?;
    options.validate()?;
    let name = written.rsplit(['/', '\\']).next().unwrap_or(written);
    decode(&bytes, name, display, options)
}

/// Read the captured `bytes` of the source named `name`.
fn decode(
    bytes: &[u8],
    name: &str,
    display: &str,
    options: &Options,
) -> Result<PublicTable, Error> {
    let io_error = |error: std::io::Error| -> Error { Fault::from_io(&error, display).into() };
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let depth = options.max_xml_depth;
    match detect(bytes, name, display)? {
        Format::Xml => {
            if options.archive_member.is_some() {
                return Err(odm_items::Error::Member(MemberRefusal::RequiresArchive).into());
            }
            if length > options.max_expanded_bytes {
                return Err(
                    odm_items::Error::Limit(Limit::XmlBytes(options.max_expanded_bytes)).into(),
                );
            }
            items::read(Xml::new(
                bytes,
                Some(options.max_expanded_bytes),
                depth,
                display.to_owned(),
            ))
        }
        Format::Tar(compression) => {
            let limit = options.max_expanded_bytes;
            let mut first = open_tar(bytes, compression, limit);
            let (index, selected) = select(&mut first, options, display)?;
            let mut tar = open_tar(bytes, compression, limit);
            for position in 0..=index {
                let member = tar.next_member().map_err(io_error)?.ok_or_else(|| {
                    odm_items::Error::Archive {
                        path: display.to_owned(),
                        reason: "TAR member changed between reads",
                    }
                })?;
                if position == index
                    && (member.name != selected.name || member.size != selected.size)
                {
                    return Err(odm_items::Error::Archive {
                        path: display.to_owned(),
                        reason: "TAR member changed between reads",
                    }
                    .into());
                }
            }
            items::read(Xml::new(
                tar.into_member(selected.size),
                None,
                depth,
                display.to_owned(),
            ))
        }
    }
}

/// Start one archive pass at the beginning of the source.
fn open_tar(bytes: &[u8], compression: Compression, limit: u64) -> Tar<'_> {
    Tar::new(match compression {
        Compression::None => Stream::plain(bytes),
        Compression::Gzip => Stream::gzip(bytes, limit),
    })
}

/// Decide between plain XML and a plain or gzip TAR archive, refusing other
/// compressed archives and archive names that hold no archive.
fn detect(bytes: &[u8], name: &str, display: &str) -> Result<Format, Error> {
    let archive = |reason| -> Error {
        odm_items::Error::Archive {
            path: display.into(),
            reason,
        }
        .into()
    };
    let head = &bytes[..bytes.len().min(BLOCK)];
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
        let mut decoder = MultiGzDecoder::new(bytes);
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
            return Err(odm_items::Error::Compression {
                path: display.into(),
                format,
            }
            .into());
        }
    }
    not_archive()
}

/// The first pass: count every member and select the one XML member.
fn select(tar: &mut Tar<'_>, options: &Options, display: &str) -> Result<(usize, Member), Error> {
    let mut count = 0_usize;
    let mut candidates = 0_usize;
    let mut selected = None;
    while let Some(member) = tar
        .next_member()
        .map_err(|error| -> Error { Fault::from_io(&error, display).into() })?
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
