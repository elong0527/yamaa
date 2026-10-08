//! Preserve the current entry-project configuration and approved-root defaults.
//! This remains separate from the pending declaring-file-only policy change.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    yaml_decode::{decode_yaml, DecodeFailure},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use yamaa_core::schema::{Document, DocumentNode as N};

#[derive(Debug)]
pub enum Error {
    Path(ResourceError),
    Decode(DecodeFailure),
    Configuration,
}

fn text(path: &Path) -> Result<&str, Error> {
    path.to_str()
        .filter(|value| value.len() <= 65536)
        .ok_or(Error::Path(ResourceError::InvalidPath))
}

fn declared_roots(document: &Document, root: &Path) -> Result<Vec<String>, Error> {
    let N::Mapping(fields) = &document.nodes()[document.root()] else {
        return Err(Error::Configuration);
    };
    let mut names = BTreeSet::new();
    for &(key, _) in fields {
        let N::Text(name) = &document.nodes()[key] else {
            return Err(Error::Configuration);
        };
        if !matches!(name.as_str(), "version" | "data_roots") || !names.insert(name.as_str()) {
            return Err(Error::Configuration);
        }
    }
    let version = document
        .field(document.root(), "version")
        .ok_or(Error::Configuration)?;
    if !matches!(&document.nodes()[version],N::Text(value) if value == "1.0") {
        return Err(Error::Configuration);
    }
    let Some(id) = document.field(document.root(), "data_roots") else {
        return Ok(Vec::new());
    };
    let items = match &document.nodes()[id] {
        N::Null => return Ok(Vec::new()),
        N::Sequence(items) if items.len() < 64 => items,
        _ => return Err(Error::Configuration),
    };
    items
        .iter()
        .map(|&id| {
            let N::Text(value) = &document.nodes()[id] else {
                return Err(Error::Configuration);
            };
            if value.is_empty() {
                return Err(Error::Configuration);
            }
            let path = PathBuf::from(value);
            let path = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            Ok(text(&path)?.into())
        })
        .collect()
}

/// Root selection depends only on the caller's entry and its project config.
/// No captured specification, parent layer or study value can add an approved root.
pub fn resources(written: &str) -> Result<(Resources, String), Error> {
    if written.is_empty() || written.len() > 65536 || written.contains('\0') {
        return Err(Error::Path(ResourceError::InvalidPath));
    }
    let absolute =
        std::path::absolute(written).map_err(|_| Error::Path(ResourceError::InvalidPath))?;
    let parent = absolute
        .parent()
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    let parent = std::fs::canonicalize(parent).map_err(|_| Error::Path(ResourceError::Missing))?;
    let name = absolute
        .file_name()
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    let configuration = parent
        .ancestors()
        .map(|directory| directory.join("yamaa-project.yaml"))
        .find(|path| path.is_file());
    let root = configuration
        .as_ref()
        .and_then(|path| path.parent())
        .unwrap_or(&parent);
    let mut resources = Resources::new(text(root)?, text(&parent)?, &[]).map_err(Error::Path)?;
    if configuration.is_some() {
        let depth = parent
            .strip_prefix(root)
            .map_err(|_| Error::Path(ResourceError::InvalidBase))?
            .components()
            .count();
        let written_configuration = format!("{}yamaa-project.yaml", "../".repeat(depth));
        let (bytes, _) = resources
            .capture(&written_configuration, 16_777_216)
            .map_err(Error::Path)?;
        let document = decode_yaml(&bytes, Default::default())
            .map_err(Error::Decode)?
            .document;
        let declared = declared_roots(&document, root)?;
        resources = resources
            .with_configuration_roots(&declared)
            .map_err(Error::Path)?;
    }
    let entry = name
        .to_str()
        .filter(|name| !name.is_empty() && name.len() <= 65536)
        .ok_or(Error::Path(ResourceError::InvalidPath))?;
    Ok((resources, entry.into()))
}
