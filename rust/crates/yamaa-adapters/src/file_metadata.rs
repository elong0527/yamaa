//! One descriptor-backed reader for build metadata and original document aliases.
//! Parsing, semantic admission and route selection remain in the engine.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    project_source::{Kind, ProjectCapturePort, Reply, Request},
    specification_source::Source,
};
use yamaa_core::inheritance::{Source as Identity, SourceError};

pub(crate) struct Reader<'a> {
    pub(crate) resources: &'a mut Resources,
}
impl Reader<'_> {
    fn resource_identity(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, ResourceError> {
        let identity = self.resources.resolve_from(declaring, written)?;
        let display_path = if is_absolute_spelling(written) {
            written.to_owned()
        } else {
            format!("{}/{written}", directory(declaring)?)
        };
        Ok(Identity {
            identity,
            display_path,
        })
    }
}
fn available(error: ResourceError) -> SourceError<ResourceError> {
    match error {
        ResourceError::Missing | ResourceError::Unreadable | ResourceError::NotRegularFile => {
            SourceError::Unavailable
        }
        error => SourceError::Raised(error),
    }
}
impl yamaa_core::file_preparation::NativePort for Reader<'_> {
    type Error = ResourceError;
    fn resolve_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
    ) -> Result<String, ResourceError> {
        match declaring {
            Some(declaring) => self.resources.resolve_from(declaring, written),
            None => self.resources.resolve(written),
        }
    }
    fn capture_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
    ) -> Result<Vec<u8>, ResourceError> {
        let (bytes, _) = match declaring {
            Some(declaring) => self.resources.capture_metadata_from(declaring, written),
            None => self.resources.capture_metadata(written),
        }?;
        Ok(bytes.to_vec())
    }
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<ResourceError>> {
        self.resource_identity(declaring, written)
            .map_err(available)
    }
    fn canonicalize_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<ResourceError>> {
        self.resource_identity(declaring, written)
            .map_err(SourceError::Raised)
    }
    fn capture_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Vec<u8>, SourceError<ResourceError>> {
        self.resources
            .capture_metadata_from(declaring, written)
            .map(|(bytes, _)| bytes.to_vec())
            .map_err(SourceError::Raised)
    }
    fn resource_failure(
        &self,
        error: &ResourceError,
    ) -> Option<yamaa_core::resource::ResourceFailure> {
        use yamaa_core::resource::ResourceFailure;
        match error {
            ResourceError::Missing => Some(ResourceFailure::Missing),
            ResourceError::NotRegularFile => Some(ResourceFailure::NotRegularFile),
            ResourceError::WrittenForm(failure) => Some(*failure),
            ResourceError::Changed => Some(ResourceFailure::ContentChanged),
            _ => None,
        }
    }

    fn capture_parent(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Vec<u8>, SourceError<ResourceError>> {
        self.resources
            .capture_metadata_from(declaring, written)
            .map(|(bytes, _)| bytes.to_vec())
            .map_err(available)
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
    ) -> Result<String, ResourceError> {
        rebase(&layer.identity, &entry.identity, written)
    }
    fn accept_alias(&mut self, declaring: &str, written: &str) -> Result<(), ResourceError> {
        self.resources
            .capture_metadata_from(declaring, written)
            .map(|_| ())
    }
    fn select_entry_base(&mut self, identity: &str) -> Result<(), ResourceError> {
        self.resources.select_entry_base(identity)
    }
    fn missing_alias() -> ResourceError {
        ResourceError::InvalidPath
    }
}

fn directory(identity: &str) -> Result<&str, ResourceError> {
    identity
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .filter(|_| is_absolute_spelling(identity))
        .ok_or(ResourceError::InvalidPath)
}
/// Lexical root test for declaring identities and authored spellings:
/// `/`-rooted or drive-rooted (`X:/...`). These spellings are judged by
/// the resource grammar, never by host path semantics: `Path::is_absolute`
/// is host-dependent (on Windows a prefix-less root such as `/spec.yaml`
/// is not absolute), so it rejects canonical declaring identities there
/// and every parent, dictionary and cross-layer rebase fails as
/// `InvalidPath` (see #2769). Mirrors the admission grammar in
/// `yamaa_core::resource::written_path_failure`.
fn is_absolute_spelling(path: &str) -> bool {
    path.starts_with('/')
        || (path.len() >= 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':'
            && path.as_bytes()[2] == b'/')
}
/// Lexical only: preparation never inspects or opens a study/output path.
fn rebase(layer: &str, entry: &str, written: &str) -> Result<String, ResourceError> {
    let layer = directory(layer)?;
    let entry = directory(entry)?;
    let drive_rooted = written
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && written.as_bytes().get(1) == Some(&b':')
        && written.as_bytes().get(2) == Some(&b'/');
    if written.starts_with('/') || drive_rooted || layer == entry {
        return Ok(written.into());
    }
    let mut target = layer
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    for part in written.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                target.pop();
            }
            part => target.push(part),
        }
    }
    let entry = entry
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let common = target
        .iter()
        .zip(&entry)
        .take_while(|(a, b)| a == b)
        .count();
    let result = std::iter::repeat_n("..", entry.len() - common)
        .chain(target[common..].iter().copied())
        .collect::<Vec<_>>()
        .join("/");
    let result = if result.is_empty() {
        ".".into()
    } else {
        result
    };
    Ok(result)
}

impl ProjectCapturePort for Reader<'_> {
    type Error = ResourceError;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        let identity = self
            .resources
            .resolve_from(request.declaring_source, request.written)?;
        let (bytes, _) = self
            .resources
            .capture_metadata_from(request.declaring_source, request.written)?;
        let source = Source {
            identity,
            bytes: bytes.to_vec(),
        };
        Ok(match request.kind {
            Kind::Lock => Reply::Lock {
                kind: crate::project_lock::kind(&source.bytes)
                    .map_err(|_| ResourceError::InvalidLock)?,
                source,
            },
            Kind::Environment | Kind::Function | Kind::Codelist => Reply::Document(source),
        })
    }
}

impl yamaa_core::build::MetadataAdmissionPort for Reader<'_> {
    type Error = ResourceError;

    fn verify_metadata(&mut self) -> Result<(), (String, Self::Error)> {
        self.resources.verify_captured()
    }
}

#[cfg(test)]
mod spelling_tests {
    use super::*;

    #[test]
    fn absolute_spelling_covers_both_root_forms_lexically() {
        assert!(is_absolute_spelling("/spec.yaml"));
        assert!(is_absolute_spelling("/layers/parent.yaml"));
        assert!(is_absolute_spelling("C:/study/spec.yaml"));
        assert!(!is_absolute_spelling("spec.yaml"));
        assert!(!is_absolute_spelling("layers/parent.yaml"));
        assert!(!is_absolute_spelling("../input/dm.csv"));
        assert!(!is_absolute_spelling("C:study/spec.yaml"));
        assert!(!is_absolute_spelling(""));
    }

    #[test]
    fn declaring_directory_accepts_canonical_identities_on_every_host() {
        // Canonical declaring identities are `/`-rooted on every host.
        // Judging them with `Path::is_absolute` rejects them on Windows,
        // where a prefix-less root is not absolute, so every parent,
        // dictionary and cross-layer rebase failed there as `InvalidPath`
        // (see #2769).
        assert_eq!(directory("/spec.yaml").unwrap(), "");
        assert_eq!(directory("/layers/parent.yaml").unwrap(), "/layers");
        assert_eq!(directory("C:/study/spec.yaml").unwrap(), "C:/study");
        assert!(directory("relative/spec.yaml").is_err());
        assert!(directory("spec.yaml").is_err());
    }

    #[test]
    fn rebase_reaches_parent_relative_paths_from_canonical_layers() {
        // The schema-inheritance-project-root layer declares its input as
        // `../input/dm.csv` beside `common/`; the entry rebases it to its
        // own directory. Same-directory layers keep their spelling.
        assert_eq!(
            rebase(
                "/common/spec_common.yaml",
                "/spec_study.yaml",
                "../input/dm.csv"
            )
            .unwrap(),
            "input/dm.csv"
        );
        assert_eq!(
            rebase(
                "/spec_organization.yaml",
                "/spec_study.yaml",
                "input/lb.csv"
            )
            .unwrap(),
            "input/lb.csv"
        );
    }
}
