//! Original file preparation through retained native resources and the shipped schema.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    specification_run::{CapturedAttempt, PreparedRun},
    specification_source::{InheritanceError, InheritancePort, Limits, Source},
};
use std::collections::BTreeMap;
use yamaa_engine::inheritance::{Source as Identity, SourceError};

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Preparation(InheritanceError<ResourceError>),
    Compile(crate::specification_run::Error),
}
impl Error {
    pub fn into_message(self) -> String {
        match self {
            Self::Resource(error) => error.message().into(),
            Self::Preparation(error) => {
                crate::specification_diagnostics::inheritance_failure(error)
                    .unwrap_or_else(|error| error.message().into())
            }
            Self::Compile(error) => crate::specification_diagnostics::failure(&error, None),
        }
    }
}

/// The prepared model and all byte authority remain owned in one application handle.
pub struct FileSpecification {
    run: PreparedRun,
    resources: Resources,
}
impl FileSpecification {
    pub fn prepare(mut resources: Resources, written: &str) -> Result<Self, Error> {
        let identity = resources.resolve(written).map_err(Error::Resource)?;
        let (bytes, _) = resources
            .capture(written, Limits::default().captured_bytes)
            .map_err(Error::Resource)?;
        let source = Source {
            identity: identity.clone(),
            bytes: bytes.to_vec(),
        };
        let mut parents = Parents {
            resources: &mut resources,
            paths: BTreeMap::new(),
        };
        let document = crate::shipped_schema::prepare(source, identity.clone(), &mut parents)
            .map_err(Error::Preparation)?;
        // Traversal may resolve a shared parent through several authored paths but
        // read it once. Retain each successful spelling only after semantic success.
        for parent in document.parents() {
            let paths = parents
                .paths
                .get(&parent.source().identity)
                .ok_or(Error::Resource(ResourceError::InvalidPath))?;
            for (declaring, written) in paths {
                parents
                    .resources
                    .capture_from(declaring, written, parent.source().bytes.len())
                    .map_err(Error::Resource)?;
            }
        }
        resources
            .select_entry_base(&identity)
            .map_err(Error::Resource)?;
        let run = PreparedRun::prepare(document).map_err(Error::Compile)?;
        Ok(Self { run, resources })
    }
    pub fn run(&self) -> &PreparedRun {
        &self.run
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub(crate) fn publication_target(
        &self,
    ) -> Result<crate::file_publication::AnchoredTarget, ResourceError> {
        self.resources
            .publication_target(self.run.compiled().output_path())
    }
    pub fn build(&mut self) -> CapturedAttempt<ResourceError> {
        self.run.execute_with_port(&mut self.resources)
    }
}

pub fn opaque_resource_failure(attempt: &CapturedAttempt<ResourceError>) -> Option<&ResourceError> {
    use crate::specification_run::PortError;
    match &attempt.result {
        Err(PortError::Capture(error))
            if !matches!(
                error,
                ResourceError::Missing | ResourceError::NotRegularFile
            ) =>
        {
            Some(error)
        }
        Err(PortError::Inspect(errors)) => errors
            .iter()
            .find(|error| error.failure.is_none())
            .map(|error| &error.error),
        _ => None,
    }
}

struct Parents<'a> {
    resources: &'a mut Resources,
    paths: BTreeMap<String, Vec<(String, String)>>,
}
fn available(error: ResourceError) -> SourceError<ResourceError> {
    match error {
        ResourceError::Missing | ResourceError::Unreadable | ResourceError::NotRegularFile => {
            SourceError::Unavailable
        }
        error => SourceError::Raised(error),
    }
}
impl InheritancePort for Parents<'_> {
    type Error = ResourceError;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<Self::Error>> {
        let identity = self
            .resources
            .resolve_from(declaring, written)
            .map_err(available)?;
        let display_path = if std::path::Path::new(written).is_absolute() {
            written.to_owned()
        } else {
            format!("{}/{written}", directory(declaring).map_err(available)?)
        };
        if display_path.len() > Limits::default().identity_bytes {
            return Err(SourceError::Raised(ResourceError::Limit));
        }
        let paths = self.paths.entry(identity.clone()).or_default();
        let path = (declaring.into(), written.into());
        if !paths.contains(&path) {
            paths.push(path);
        }
        Ok(Identity {
            identity,
            display_path,
        })
    }
    fn capture(
        &mut self,
        source: &Identity,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<Self::Error>> {
        let (declaring, written) = self
            .paths
            .get(&source.identity)
            .and_then(|paths| paths.first())
            .ok_or(SourceError::Raised(ResourceError::InvalidPath))?;
        self.resources
            .capture_from(declaring, written, maximum)
            .map(|(bytes, _)| bytes.to_vec())
            .map_err(available)
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
        maximum: usize,
    ) -> Result<String, Self::Error> {
        rebase(&layer.identity, &entry.identity, written, maximum)
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
