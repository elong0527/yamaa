//! File admission: XML versus TAR detection and TAR member selection.
//!
//! One opened file handle serves detection and both archive passes, so every
//! pass reads the same file. The first pass counts every member and selects
//! the XML member; the second streams only that member into the parser.
use super::{
    archive::{self, Member, Stream, Tar, BLOCK},
    items,
    xml::Xml,
    Error, Fault,
};
use crate::public_table::PublicTable;
use flate2::read::MultiGzDecoder;
use std::{
    fs::File,
    io::{self, BufReader, Seek},
    path::Path,
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

pub(super) fn read(path: &Path, options: &Options) -> Result<PublicTable, Error> {
    let display = path.display().to_string();
    let not_found = || odm_items::Error::NotFound {
        path: display.clone(),
    };
    if !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(not_found().into());
    }
    options.validate()?;
    let io_error = |error: io::Error| -> Error { Fault::from_io(&error, &display).into() };
    let file = File::open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(not_found().into());
    }
    let length = metadata.len();
    let depth = options.max_xml_depth;
    match detect(&file, path, &display)? {
        Format::Xml => {
            if options.archive_member.is_some() {
                return Err(odm_items::Error::Member(MemberRefusal::RequiresArchive).into());
            }
            if length > options.max_expanded_bytes {
                return Err(
                    odm_items::Error::Limit(Limit::XmlBytes(options.max_expanded_bytes)).into(),
                );
            }
            let mut handle = &file;
            handle.rewind().map_err(io_error)?;
            items::read(Xml::new(
                &file,
                Some(options.max_expanded_bytes),
                depth,
                display.clone(),
            ))
        }
        Format::Tar(compression) => {
            let limit = options.max_expanded_bytes;
            let mut first = open_tar(&file, compression, length, limit).map_err(io_error)?;
            let (index, selected) = select(&mut first, options, &display)?;
            let mut tar = open_tar(&file, compression, length, limit).map_err(io_error)?;
            for position in 0..=index {
                let member = tar.next_member().map_err(io_error)?.ok_or_else(|| {
                    odm_items::Error::Archive {
                        path: display.clone(),
                        reason: "TAR member changed between reads",
                    }
                })?;
                if position == index
                    && (member.name != selected.name || member.size != selected.size)
                {
                    return Err(odm_items::Error::Archive {
                        path: display.clone(),
                        reason: "TAR member changed between reads",
                    }
                    .into());
                }
            }
            items::read(Xml::new(
                tar.into_member(selected.size),
                None,
                depth,
                display.clone(),
            ))
        }
    }
}

/// Start one archive pass at the beginning of the file.
fn open_tar(file: &File, compression: Compression, length: u64, limit: u64) -> io::Result<Tar<'_>> {
    let mut handle = file;
    handle.rewind()?;
    Ok(Tar::new(match compression {
        Compression::None => Stream::plain(file, length),
        Compression::Gzip => Stream::gzip(file, limit),
    }))
}

/// Decide between plain XML and a plain or gzip TAR archive, refusing other
/// compressed archives and archive names that hold no archive.
fn detect(file: &File, path: &Path, display: &str) -> Result<Format, Error> {
    let io_error = |error: io::Error| -> Error { Fault::from_io(&error, display).into() };
    let archive = |reason| -> Error {
        odm_items::Error::Archive {
            path: display.into(),
            reason,
        }
        .into()
    };
    let mut handle = file;
    let mut head = [0_u8; BLOCK];
    let read = archive::read_up_to(&mut handle, &mut head).map_err(io_error)?;
    let head = &head[..read];
    let suffix = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|name| {
            [".tar", ".tar.gz", ".tgz"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        });
    let not_archive = || {
        if suffix {
            Err(archive("not a TAR archive"))
        } else {
            Ok(Format::Xml)
        }
    };
    if head.starts_with(&[0x1F, 0x8B]) {
        handle.rewind().map_err(io_error)?;
        let mut decoder = MultiGzDecoder::new(BufReader::new(file));
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
