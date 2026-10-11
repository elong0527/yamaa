//! Original file preparation through retained native resources and the shipped schema.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    specification_run::{CapturedAttempt, PreparedRun},
    specification_source::{InheritanceError, Limits, Source},
};
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
        let findings = crate::verification_logs::location_findings(&resources, &run);
        if !findings.is_empty() {
            return Err(Error::Compile(crate::specification_run::Error::Prepare(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )));
        }
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
        self.publication_field_target("path")
    }
    pub(crate) fn publication_field_target(
        &self,
        field: &str,
    ) -> Result<crate::file_publication::AnchoredTarget, ResourceError> {
        let origin = self
            .run
            .document()
            .written_output_field_origin(field)
            .ok_or(ResourceError::InvalidPath)?;
        self.resources
            .publication_target_from(origin.declaring_source, origin.written)
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
    use yamaa_engine::file_preparation::{self as application, Error as Failure};
    application::prepare(
        &mut Native { resources },
        &mut Documents { schema },
        declaring,
        written,
        Limits::default().captured_bytes,
    )
    .map_err(|error| match error {
        Failure::Resource(error) => Error::Resource(error),
        Failure::Preparation(error) => Error::Preparation(error),
    })
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

struct Native<'a> {
    resources: &'a mut Resources,
}
struct Documents {
    schema: Option<Arc<crate::specification_source::CapturedSchema>>,
}
impl yamaa_engine::file_preparation::DocumentPort<ResourceError> for Documents {
    type Prepared = crate::specification_source::PreparedDocument;
    type Error = InheritanceError<ResourceError>;
    fn prepare<P: yamaa_engine::inheritance_capture::CapturePort<Error = ResourceError>>(
        &mut self,
        source: Source,
        display: String,
        port: &mut P,
    ) -> Result<Self::Prepared, Self::Error> {
        match &self.schema {
            Some(schema) => schema.prepare_document(source, display, port),
            None => crate::shipped_schema::prepare(source, display, port),
        }
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
impl yamaa_engine::file_preparation::NativePort for Native<'_> {
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
        Ok(Identity {
            identity,
            display_path,
        })
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

#[cfg(test)]
mod dispatch_policy_tests {
    use super::*;
    use crate::file_resources::MetadataLimits;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    const PARENT: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
    const ROOT: &str = "schema_version: '1.0'\nparents: [../parent/base.yaml, ../parent/./base.yaml, ../parent/base.yaml]\n";
    struct Study(PathBuf);
    impl Study {
        fn new(root: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-alias-policy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("entry")).unwrap();
            fs::create_dir(path.join("parent")).unwrap();
            fs::write(path.join("entry/root.yaml"), root).unwrap();
            fs::write(path.join("parent/base.yaml"), PARENT).unwrap();
            fs::write(path.join("root.yaml"), b"original base marker").unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn resources(&self) -> Resources {
            Resources::new(self.0.to_str().unwrap(), self.0.to_str().unwrap(), &[]).unwrap()
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn native_success_replay_spends_no_additional_preparation_bytes_and_continues_policy() {
        let study = Study::new(ROOT);
        let mut resources = study.resources();
        let limits = MetadataLimits {
            bytes: 2 * (ROOT.len() + PARENT.len()),
            snapshots: 8,
            text: 1_000_000,
            work: 1_000_000,
        };
        resources.ensure_metadata_budget(limits).unwrap();
        let document =
            prepare_document_from(&mut resources, None, "entry/root.yaml", None).unwrap();
        assert_eq!(document.parents().len(), 1);
        assert_eq!(resources.snapshot_count(), 2);
        assert_eq!(resources.capture_reads(), 2);
        let usage = resources.metadata_usage();
        assert_eq!(usage.0, 2 * (ROOT.len() + PARENT.len()));
        resources.ensure_metadata_budget(limits).unwrap();
        assert_eq!(resources.metadata_usage(), usage);
        assert_eq!(
            resources.resolve("root.yaml").unwrap(),
            document.source().identity
        );
        let parent = document.parents()[0].source().identity.clone();
        assert_eq!(
            resources
                .capture_from(&parent, "./base.yaml", PARENT.len())
                .unwrap()
                .0
                .as_ref(),
            PARENT.as_bytes()
        );
        assert_eq!(resources.metadata_usage().0, usage.0);
        assert_eq!(resources.capture_reads(), 2);
    }
    #[test]
    fn native_semantic_failure_keeps_original_entry_base_and_drops_success_alias_authority() {
        let root = ROOT.replace("'1.0'", "'99.0'");
        let study = Study::new(&root);
        let mut resources = study.resources();
        let original = resources.resolve("root.yaml").unwrap();
        assert!(matches!(
            prepare_document_from(&mut resources, None, "entry/root.yaml", None),
            Err(Error::Preparation(_))
        ));
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(resources.resolve("root.yaml").unwrap(), original);
    }
    struct ChangeAfterSuccess {
        path: PathBuf,
    }
    impl yamaa_engine::file_preparation::DocumentPort<ResourceError> for ChangeAfterSuccess {
        type Prepared = crate::specification_source::PreparedDocument;
        type Error = InheritanceError<ResourceError>;
        fn prepare<P: yamaa_engine::inheritance_capture::CapturePort<Error = ResourceError>>(
            &mut self,
            source: Source,
            display: String,
            port: &mut P,
        ) -> Result<Self::Prepared, Self::Error> {
            let document = crate::shipped_schema::prepare(source, display, port)?;
            // A deterministic external change occurs after semantic success and
            // before the application accepts any successful authored alias.
            fs::write(&self.path, b"changed after semantic success").unwrap();
            Ok(document)
        }
    }
    #[test]
    fn changed_parent_after_semantic_success_is_original_resource_failure_before_base_selection() {
        let study = Study::new(ROOT);
        let mut resources = study.resources();
        let original = resources.resolve("root.yaml").unwrap();
        let result = yamaa_engine::file_preparation::prepare(
            &mut Native {
                resources: &mut resources,
            },
            &mut ChangeAfterSuccess {
                path: study.0.join("parent/base.yaml"),
            },
            None,
            "entry/root.yaml",
            Limits::default().captured_bytes,
        );
        assert!(matches!(
            result,
            Err(yamaa_engine::file_preparation::Error::Resource(
                ResourceError::Changed
            ))
        ));
        assert_eq!(resources.capture_reads(), 2);
        assert_eq!(resources.resolve("root.yaml").unwrap(), original);
    }
}
