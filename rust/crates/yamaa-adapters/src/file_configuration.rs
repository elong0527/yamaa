//! Select the explicitly requested entry without project discovery.
use crate::file_resources::{Error as ResourceError, Resources};
use std::path::Path;

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
    // Preserve caller-path failure ownership before selecting its descriptor.
    crate::canonical_path::canonicalize(parent).map_err(|_| Error::Path(ResourceError::Missing))?;
    let resources = Resources::for_entry(text(parent)?).map_err(Error::Path)?;
    let entry = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    Ok((resources, entry.into()))
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
