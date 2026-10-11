//! Select the explicitly requested entry without project discovery.
use crate::file_resources::{Error as ResourceError, Resources};
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Path(ResourceError),
}

fn text(path: &Path) -> Result<&str, Error> {
    path.to_str()
        .filter(|value| value.len() <= 65536)
        .ok_or(Error::Path(ResourceError::InvalidPath))
}

/// Caller paths use ordinary host argument resolution. Each later declaration
/// resolves once at its own file; no yamaa-project.yaml is discovered or read.
pub fn resources(written: &str) -> Result<(Resources, String), Error> {
    if written.is_empty() || written.len() > 65536 || written.contains('\0') {
        return Err(Error::Path(ResourceError::InvalidPath));
    }
    let absolute =
        std::path::absolute(written).map_err(|_| Error::Path(ResourceError::InvalidPath))?;
    let parent = absolute
        .parent()
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    // Preserve caller-path failure ownership before selecting its descriptor.
    std::fs::canonicalize(parent).map_err(|_| Error::Path(ResourceError::Missing))?;
    let resources = Resources::for_entry(text(parent)?).map_err(Error::Path)?;
    let entry = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && name.len() <= 65536)
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    Ok((resources, entry.into()))
}
