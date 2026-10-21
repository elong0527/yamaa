//! The files a lint call names: directory expansion, one capture of each file
//! through the local file resolver, and atomic replacement of fixed files.

use std::path::{Path, PathBuf};

use yamaa_core::language::FileType;

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
    let mut files = Vec::new();
    for path in paths {
        files.extend(expand(Path::new(path))?);
    }
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
/// specifications: a root that declares the 1.x header and `type: spec`, or a
/// root mapping holding `schema_version` and only 0.5 root fields, so a study
/// document, a project environment, a project configuration and a 1.x file of
/// another type are left out.
fn expand(path: &Path) -> Result<Vec<File>, Error> {
    if !path.is_dir() {
        return Ok(vec![File::new(path.to_path_buf())]);
    }
    let mut found = Vec::new();
    collect(path, &mut Vec::new(), &mut found)?;
    found.sort();
    let mut files = Vec::new();
    for parts in found {
        let candidate = parts
            .iter()
            .fold(path.to_path_buf(), |path, part| path.join(part));
        if is_specification(&candidate)? {
            files.push(File::new(candidate));
        }
    }
    Ok(files)
}

/// Discovery failures refuse the call just as failures capturing a named file
/// do; a partial list must never reach the fix pass.
fn discovery_error(path: &Path, error: std::io::Error) -> Error {
    Error::Read {
        file: File::new(path.to_path_buf()).display,
        error: if error.kind() == std::io::ErrorKind::NotFound {
            crate::file_resources::Error::Missing
        } else {
            crate::file_resources::Error::Unreadable
        },
    }
}

/// Every `.yaml` file beneath `directory`, as path components relative to the
/// directory expanded; hidden directories and linked directories are not
/// entered.
fn collect(
    directory: &Path,
    parents: &mut Vec<String>,
    found: &mut Vec<Vec<String>>,
) -> Result<(), Error> {
    let entries =
        std::fs::read_dir(directory).map_err(|error| discovery_error(directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| discovery_error(directory, error))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let kind = entry
            .file_type()
            .map_err(|error| discovery_error(&entry.path(), error))?;
        if kind.is_dir() {
            if !name.starts_with('.') {
                parents.push(name);
                collect(&entry.path(), parents, found)?;
                parents.pop();
            }
        } else if name.ends_with(".yaml") {
            // Inspect link targets without opening them: a linked pipe must
            // not block discovery either. Regular file links stay candidates
            // so the resolver can refuse them after specification discovery.
            let regular = if kind.is_symlink() {
                std::fs::metadata(entry.path())
                    .map_err(|error| discovery_error(&entry.path(), error))?
                    .is_file()
            } else {
                kind.is_file()
            };
            if regular {
                let mut parts = parents.clone();
                parts.push(name);
                found.push(parts);
            }
        }
    }
    Ok(())
}

fn is_specification(path: &Path) -> Result<bool, Error> {
    let bytes = std::fs::read(path).map_err(|error| discovery_error(path, error))?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(false);
    };
    // Admission refuses any text that is not ASCII, which linting then
    // reports; for telling a specification apart, each other character only
    // stands in a scalar or a comment.
    let ascii = text
        .chars()
        .map(|character| if character.is_ascii() { character } else { 'x' })
        .collect::<String>();
    let Ok(document) = Document::structure(ascii.as_bytes()) else {
        return Ok(false);
    };
    let Some(Value::Mapping { pairs, .. }) = document.root.map(|root| &document.node(root).value)
    else {
        return Ok(false);
    };
    let mut names = Vec::with_capacity(pairs.len());
    for &(key, _) in pairs {
        // A key that is not text is never a root field.
        let Some(name) = document.scalar(key) else {
            return Ok(false);
        };
        names.push(name);
    }
    let declares = |name| names.contains(&name);
    if declares("yamaa") && !declares("schema_version") {
        // A 1.x document declares its own type (SCHEMA-2).
        let declared = document
            .member(document.root, "type")
            .and_then(|value| document.scalar(value));
        return Ok(declared == Some(FileType::Spec.name()));
    }
    Ok(declares("schema_version") && names.iter().all(|name| fields::LEGACY.root.contains(name)))
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
