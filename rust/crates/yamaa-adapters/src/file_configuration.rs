//! Select the explicitly requested entry without project discovery.
use crate::file_resources::{Error as ResourceError, Resources};
use std::{io, path::Path};

#[derive(Debug)]
pub enum Error {
    Path(ResourceError),
}

fn text(path: &Path) -> Result<&str, Error> {
    path.to_str().ok_or(Error::Path(ResourceError::InvalidPath))
}

/// Caller paths use ordinary host argument resolution. Each later declaration
/// resolves once at its own file; no yamaa-project.yaml is discovered or read.
pub fn resources(written: &str) -> Result<(Resources, String), Error> {
    if written.is_empty() || written.contains('\0') {
        return Err(Error::Path(ResourceError::InvalidPath));
    }
    let absolute =
        std::path::absolute(written).map_err(|_| Error::Path(ResourceError::InvalidPath))?;
    let parent = absolute
        .parent()
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    // Preserve the parent's kind before descriptor selection turns a refused
    // root into InvalidRoot. Caller-selected directory aliases are allowed.
    let metadata =
        std::fs::metadata(parent).map_err(|error| Error::Path(parent_failure(parent, &error)))?;
    if !metadata.is_dir() {
        return Err(Error::Path(ResourceError::NotRegularFile));
    }
    let resources = Resources::for_entry(text(parent)?).map_err(Error::Path)?;
    let entry = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    Ok((resources, entry.into()))
}

/// A file can stop the parent walk before its last segment. Inspect the
/// nearest existing ancestor to keep that kind even on hosts that report
/// the deeper path as not found. Other IO failures remain opaque.
fn parent_failure(parent: &Path, error: &io::Error) -> ResourceError {
    for ancestor in parent.ancestors().skip(1) {
        if let Ok(metadata) = std::fs::metadata(ancestor) {
            if !metadata.is_dir() {
                return ResourceError::NotRegularFile;
            }
            break;
        }
    }
    if error.kind() == io::ErrorKind::NotFound {
        ResourceError::Missing
    } else {
        ResourceError::Unreadable
    }
}

/// The identity compilation gives the file a caller path names, resolved now:
/// the path is made absolute against the current working directory, as
/// [`resources`] and a project path's admission make it, and its parent
/// directory canonical, so a changed working directory or a retargeted link
/// in the parent gives another identity. Reads no file. A kept plan is reused
/// only while its caller paths still resolve to the identities it recorded.
pub fn caller_identity(written: &str) -> Result<String, Error> {
    if written.is_empty() || written.contains('\0') {
        return Err(Error::Path(ResourceError::InvalidPath));
    }
    let absolute =
        std::path::absolute(written).map_err(|_| Error::Path(ResourceError::InvalidPath))?;
    let parent = absolute
        .parent()
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    let parent = crate::canonical_path::canonicalize(parent)
        .map_err(|_| Error::Path(ResourceError::Missing))?;
    let name = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    crate::file_resolver::file_identity(&parent, name)
        .ok_or(Error::Path(ResourceError::InvalidPath))
}

/// The identity compilation gives the existing directory `path` names,
/// resolved now: canonical, as [`resources`] selects an entry's directory, in
/// the engine's spelling with a trailing separator. It starts the identity of
/// every file compilation reads beneath the directory, such as the `file` of
/// an issue's location, and every spelling of the directory has it: a path
/// through a link, and on Windows 8.3 short names, another letter case,
/// forward slashes or the verbatim prefix. Reads no file.
pub fn directory_identity(path: &Path) -> Result<String, Error> {
    let canonical = crate::canonical_path::canonicalize(path)
        .map_err(|_| Error::Path(ResourceError::Missing))?;
    crate::file_resolver::canonical_directory_identity(&canonical)
        .ok_or(Error::Path(ResourceError::InvalidPath))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_of_a_directory_has_one_identity() {
        // The temporary directory as the environment spells it: on Windows
        // runners an 8.3 short path, such as `C:\Users\RUNNER~1\...`.
        let directory =
            std::env::temp_dir().join(format!("yamaa-directory-identity-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("nested")).unwrap();
        let canonical = crate::canonical_path::canonicalize(&directory).unwrap();
        let mut spellings = vec![
            directory.clone(),
            canonical.clone(),
            directory.join("."),
            directory.join("nested").join(".."),
        ];
        #[cfg(unix)]
        let link = directory.with_extension("link");
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&directory, &link).unwrap();
            spellings.push(link.clone());
        }
        #[cfg(windows)]
        {
            // Another drive-letter case, every name in capitals, forward
            // slashes, the verbatim prefix, and the 8.3 short names.
            let text = canonical.to_str().unwrap();
            let (drive, rest) = text.split_at(1);
            spellings.extend(
                [
                    format!("{}{rest}", drive.to_ascii_lowercase()),
                    text.to_uppercase(),
                    text.replace('\\', "/"),
                    format!(r"\\?\{text}"),
                ]
                .map(std::path::PathBuf::from),
            );
            spellings.push(crate::windows_file::short_path(&canonical).unwrap());
        }
        let identities = spellings
            .iter()
            .map(|spelling| directory_identity(spelling).ok())
            .collect::<Vec<_>>();
        let file = caller_identity(directory.join("spec.yaml").to_str().unwrap()).ok();
        #[cfg(unix)]
        std::fs::remove_file(&link).unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        // The canonical path in the engine's spelling, with a trailing `/`...
        let identity = format!("{}/", canonical.to_str().unwrap().replace('\\', "/"));
        for (spelling, found) in spellings.iter().zip(&identities) {
            assert_eq!(found.as_ref(), Some(&identity), "{}", spelling.display());
        }
        // ...which starts the identity of each file compilation reads in it.
        assert_eq!(file, Some(format!("{identity}spec.yaml")));
        assert!(directory_identity(&directory).is_err());
    }
}
