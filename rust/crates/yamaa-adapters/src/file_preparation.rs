//! Original file preparation through retained native resources and the shipped schema.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    specification_run::{CapturedAttempt, PreparedRun},
    specification_source::{InheritanceError, InheritancePort, Limits, Source},
};
use std::collections::BTreeMap;
use std::sync::Arc;
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
        let document = prepare_document(&mut resources, written, None)?;
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

/// Retain one native capture/inheritance path for ordinary and project models.
/// An explicit candidate closure does not replace the package-owned public schema.
pub(crate) fn prepare_document(
    resources: &mut Resources,
    written: &str,
    schema: Option<Arc<crate::specification_source::CapturedSchema>>,
) -> Result<crate::specification_source::PreparedDocument, Error> {
    prepare_document_from(resources, None, written, schema)
}

/// Recursive metadata uses the original declaring identity and written spelling.
/// A reader-returned canonical identity is never reinterpreted as authored text.
pub(crate) fn prepare_document_from(
    resources: &mut Resources,
    declaring: Option<&str>,
    written: &str,
    schema: Option<Arc<crate::specification_source::CapturedSchema>>,
) -> Result<crate::specification_source::PreparedDocument, Error> {
    let identity = match declaring {
        Some(declaring) => resources.resolve_from(declaring, written),
        None => resources.resolve(written),
    }
    .map_err(Error::Resource)?;
    let (bytes, _) = match declaring {
        Some(declaring) => {
            resources.capture_from(declaring, written, Limits::default().captured_bytes)
        }
        None => resources.capture(written, Limits::default().captured_bytes),
    }
    .map_err(Error::Resource)?;
    let source = Source {
        identity: identity.clone(),
        bytes: resources
            .preparation_copy(&bytes)
            .map_err(Error::Resource)?,
    };
    let mut parents = Parents {
        resources,
        paths: BTreeMap::new(),
    };
    let document = match schema {
        Some(schema) => schema.prepare_document(source, identity.clone(), &mut parents),
        None => crate::shipped_schema::prepare(source, identity.clone(), &mut parents),
    }
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
    Ok(document)
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
        self.resources
            .charge_metadata_text(identity.len().saturating_add(display_path.len()))
            .map_err(available)?;
        let paths = self.paths.entry(identity.clone()).or_default();
        let path = (declaring.into(), written.into());
        if !paths.contains(&path) {
            self.resources
                .charge_metadata_text(
                    declaring
                        .len()
                        .saturating_add(written.len())
                        .saturating_add(identity.len()),
                )
                .map_err(available)?;
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
            .and_then(|(bytes, _)| self.resources.preparation_copy(&bytes))
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

#[cfg(test)]
mod declaring_origin_tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Study(PathBuf);
    impl Study {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!(
                    "yamaa-declaring-origin-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ))
                .join(name);
            fs::create_dir_all(root.join("entry")).unwrap();
            fs::create_dir(root.join("producer")).unwrap();
            Self(fs::canonicalize(root).unwrap())
        }
        fn check(&self) {
            let parent = "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let child = "schema_version: '1.0'\nparents: ['./base.yaml']\ndomain: PRODUCER\n";
            fs::write(self.0.join("entry/root.yaml"), b"declaring metadata").unwrap();
            fs::write(self.0.join("producer/base.yaml"), parent).unwrap();
            fs::write(self.0.join("producer/child.yaml"), child).unwrap();
            let mut resources = Resources::new(
                self.0.to_str().unwrap(),
                self.0.join("entry").to_str().unwrap(),
                &[],
            )
            .unwrap();
            let declaring = resources.resolve("root.yaml").unwrap();
            let document = prepare_document_from(
                &mut resources,
                Some(&declaring),
                "../producer/child.yaml",
                None,
            )
            .unwrap();
            assert_eq!(document.source().bytes, child.as_bytes());
            assert_eq!(document.parents().len(), 1);
            assert_eq!(document.parents()[0].source().bytes, parent.as_bytes());
            let output = document.written_output_origin().unwrap();
            assert!(output.declaring_source.ends_with("/producer/base.yaml"));
            assert_eq!(output.written, "never-written.csv");
            assert_eq!(
                resources.resolve("child.yaml").unwrap(),
                document.source().identity
            );
            assert_eq!(resources.capture_reads(), 2);
            assert!(!self.0.join("producer/never-read.csv").exists());
            assert!(!self.0.join("producer/never-written.csv").exists());
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            let parent = self.0.parent().unwrap();
            fs::remove_dir_all(parent).unwrap();
        }
    }
    #[test]
    fn original_declaring_origin_captures_inherited_metadata_and_selects_its_entry_base() {
        Study::new("ordinary").check();
    }
    #[cfg(unix)]
    #[test]
    fn canonical_declaring_origin_preserves_literal_backslashes_in_selected_root_names() {
        Study::new("named\\root").check();
    }
}
