//! One descriptor-backed reader for build metadata and original document aliases.
//! Parsing, semantic admission and route selection remain in the engine.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    project_source::{CapturePort, Kind, Reply, Request},
    specification_source::{Limits, Source},
};
use yamaa_engine::inheritance::{Source as Identity, SourceError};

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
        let display_path = if std::path::Path::new(written).is_absolute() {
            written.to_owned()
        } else {
            format!("{}/{written}", directory(declaring)?)
        };
        if display_path.len() > Limits::default().identity_bytes {
            return Err(ResourceError::Limit);
        }
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
impl yamaa_engine::file_preparation::NativePort for Reader<'_> {
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
        maximum: usize,
    ) -> Result<Vec<u8>, ResourceError> {
        let (bytes, _) = match declaring {
            Some(declaring) => self.resources.capture_from(declaring, written, maximum),
            None => self.resources.capture(written, maximum),
        }?;
        self.resources.preparation_copy(&bytes)
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
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<ResourceError>> {
        self.resources
            .capture_from(declaring, written, maximum)
            .and_then(|(bytes, _)| self.resources.preparation_copy(&bytes))
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
            _ => None,
        }
    }
    fn admit_alias_text(&mut self, bytes: usize) -> Result<(), SourceError<ResourceError>> {
        self.resources
            .charge_metadata_text(bytes)
            .map_err(available)
    }
    fn capture_parent(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<ResourceError>> {
        self.resources
            .capture_from(declaring, written, maximum)
            .and_then(|(bytes, _)| self.resources.preparation_copy(&bytes))
            .map_err(available)
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
        maximum: usize,
    ) -> Result<String, ResourceError> {
        rebase(&layer.identity, &entry.identity, written, maximum)
    }
    fn accept_alias(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<(), ResourceError> {
        self.resources
            .capture_from(declaring, written, maximum)
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
        .filter(|_| std::path::Path::new(identity).is_absolute())
        .ok_or(ResourceError::InvalidPath)
}
/// Lexical only: preparation never inspects or opens a study/output path.
fn rebase(
    layer: &str,
    entry: &str,
    written: &str,
    maximum: usize,
) -> Result<String, ResourceError> {
    if written.len() > maximum {
        return Err(ResourceError::Limit);
    }
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
    if result.len() > maximum {
        Err(ResourceError::Limit)
    } else {
        Ok(result)
    }
}

impl CapturePort for Reader<'_> {
    type Error = ResourceError;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        let identity = self
            .resources
            .resolve_from(request.declaring_source, request.written)?;
        if identity.len() > request.remaining_identity_bytes {
            return Err(ResourceError::Limit);
        }
        let (bytes, _) = self.resources.capture_from(
            request.declaring_source,
            request.written,
            request.remaining_bytes,
        )?;
        let source = Source {
            identity,
            bytes: self.resources.preparation_copy(&bytes)?,
        };
        Ok(match request.kind {
            Kind::Lock => Reply::Lock {
                kind: crate::project_lock::kind(&source.bytes)
                    .map_err(|_| ResourceError::InvalidLock)?,
                source,
            },
            Kind::Function | Kind::Codelist => Reply::Document(source),
        })
    }
}

impl yamaa_engine::build::MetadataAdmissionPort for Reader<'_> {
    type Error = ResourceError;
    fn charge_work(&mut self, work: usize) -> Result<(), Self::Error> {
        self.resources.charge_work(work)
    }
    fn verify_metadata(&mut self) -> Result<(), (String, Self::Error)> {
        self.resources.verify_captured()
    }
}
