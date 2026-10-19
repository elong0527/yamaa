//! The files a lint call names: directory expansion, one capture of each file
//! through the local file resolver, and atomic replacement of fixed files.

use std::path::{Path, PathBuf};

use super::checks::{check, suppressed};
use super::document::{Document, Value};
use super::{fields, fix, Error, Finding, Linted};
use crate::yaml_decode::DecodeFailure;

/// The findings that remain in `text` once suppressions apply, one-based.
pub(super) fn findings(text: &[u8]) -> Result<Vec<Finding>, DecodeFailure> {
    let document = Document::parse(text)?;
    Ok(suppressed(&document, check(&document))
        .into_iter()
        .map(|item| Finding {
            line: item.line + 1,
            column: item.column + 1,
            finding: item.name,
            spec_path: item.spec_path,
            message: item.message,
        })
        .collect())
}

/// One file to lint: the path to read and its absolute spelling with
/// forward slashes.
struct File {
    path: PathBuf,
    display: String,
}

impl File {
    fn new(path: PathBuf) -> Self {
        let absolute = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
        Self {
            display: absolute.to_string_lossy().replace('\\', "/"),
            path: absolute,
        }
    }
}

pub(super) fn lint(paths: &[String], fix: bool) -> Result<Vec<Linted>, Error> {
    let files = paths
        .iter()
        .flat_map(|path| expand(Path::new(path)))
        .collect::<Vec<_>>();
    // Read and admit every file before writing any, so a file that cannot be
    // read or admitted refuses the call with nothing changed.
    let mut sources = Vec::with_capacity(files.len());
    for file in files {
        let bytes = read(&file)?;
        let document = Document::parse(&bytes).map_err(|failure| Error::Source {
            file: file.display.clone(),
            failure,
        })?;
        sources.push((file, document.text));
    }
    let mut linted = Vec::with_capacity(sources.len());
    for (file, text) in sources {
        let fixed = if fix {
            fix::fix_text(&text)
        } else {
            text.clone()
        };
        let changed = fixed != text;
        if changed {
            write(&file, fixed.as_bytes())?;
        }
        let findings = findings(fixed.as_bytes()).map_err(|failure| Error::Source {
            file: file.display.clone(),
            failure,
        })?;
        linted.push(Linted {
            file: file.display,
            findings,
            fixed: changed,
        });
    }
    Ok(linted)
}

/// A named file is always linted. A directory names the `.yaml` files beneath
/// it, outside hidden directories, in sorted path order, that are
/// specifications: a root mapping holding `schema_version` and only root
/// fields, so a study document, a project environment and a project
/// configuration are left out.
fn expand(path: &Path) -> Vec<File> {
    if !path.is_dir() {
        return vec![File::new(path.to_path_buf())];
    }
    let mut found = Vec::new();
    collect(path, &mut Vec::new(), &mut found);
    found.sort();
    found
        .into_iter()
        .map(|parts| {
            parts
                .iter()
                .fold(path.to_path_buf(), |path, part| path.join(part))
        })
        .filter(|candidate| is_specification(candidate))
        .map(File::new)
        .collect()
}

/// Every `.yaml` file beneath `directory`, as path components relative to the
/// directory expanded; hidden directories and linked directories are not
/// entered.
fn collect(directory: &Path, parents: &mut Vec<String>, found: &mut Vec<Vec<String>>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if !name.starts_with('.') {
                parents.push(name);
                collect(&entry.path(), parents, found);
                parents.pop();
            }
        } else if name.ends_with(".yaml") {
            let mut parts = parents.clone();
            parts.push(name);
            found.push(parts);
        }
    }
}

fn is_specification(path: &Path) -> bool {
    let Some(text) = std::fs::read(path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
    else {
        return false;
    };
    // Admission refuses any text that is not ASCII, which linting then
    // reports; for telling a specification apart, each other character only
    // stands in a scalar or a comment.
    let ascii = text
        .chars()
        .map(|character| if character.is_ascii() { character } else { 'x' })
        .collect::<String>();
    let Ok(document) = Document::structure(ascii.as_bytes()) else {
        return false;
    };
    let Some(Value::Mapping { pairs, .. }) = document.root.map(|root| &document.node(root).value)
    else {
        return false;
    };
    let mut schema_version = false;
    for &(key, _) in pairs {
        // A key that is not text is never a root field.
        let Some(name) = document.scalar(key) else {
            return false;
        };
        if !fields::ROOT_CLASS.contains(&name) {
            return false;
        }
        schema_version |= name == "schema_version";
    }
    schema_version
}

/// Capture a file once through the local file resolver: its parent is
/// selected and the file must be a regular file, never a link (FILE-7,
/// FILE-8).
fn read(file: &File) -> Result<Vec<u8>, Error> {
    let refused = |error| Error::Read {
        file: file.display.clone(),
        error,
    };
    let written = file
        .path
        .to_str()
        .ok_or_else(|| refused(crate::file_resources::Error::InvalidPath))?;
    let (mut resources, name) = crate::file_configuration::resources(written)
        .map_err(|crate::file_configuration::Error::Path(error)| refused(error))?;
    let (bytes, _) = resources.capture_metadata(&name).map_err(refused)?;
    Ok(bytes.to_vec())
}

/// Replace a fixed file atomically through the file publisher, keeping its
/// permissions.
fn write(file: &File, content: &[u8]) -> Result<(), Error> {
    let refused = |error| Error::Write {
        file: file.display.clone(),
        error,
    };
    let target = file
        .path
        .to_str()
        .ok_or_else(|| refused(crate::file_publication::Error::InvalidTarget))?;
    let permissions = std::fs::metadata(&file.path).map(|metadata| metadata.permissions());
    let mut publisher = crate::file_publication::Publisher::new(target, target).map_err(refused)?;
    publisher.publish(target, content).map_err(refused)?;
    // The publisher stages a private file; the replacement keeps the mode the
    // author's file had. The bytes are already in place, so a mode that cannot
    // be restored does not undo the fix.
    if let Ok(permissions) = permissions {
        let _ = std::fs::set_permissions(&file.path, permissions);
    }
    Ok(())
}
