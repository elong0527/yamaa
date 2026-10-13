//! Original file preparation through retained native resources and the shipped schema.
use crate::{
    file_metadata::Reader as Native,
    file_resources::{Error as ResourceError, Resources},
    specification_run::{CapturedAttempt, PreparedRun},
    specification_source::{InheritanceError, Limits, Source},
};
use std::sync::Arc;

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
/// A plain compilation refusal still owns the original native snapshots.
pub struct RejectedPlain {
    error: Error,
    entry: String,
    resources: Resources,
}
impl std::fmt::Debug for RejectedPlain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedPlain")
            .field("error", &self.error)
            .field("entry", &self.entry)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}
impl RejectedPlain {
    pub fn error(&self) -> &Error {
        &self.error
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub fn retained_sources(
        &self,
        aliases: usize,
        text: usize,
    ) -> Result<crate::file_resources::RetainedSources, crate::file_resources::RetentionFailure>
    {
        self.resources.retain_sources(aliases, text)
    }
}
impl FileSpecification {
    pub fn prepare(mut resources: Resources, written: &str) -> Result<Self, Error> {
        let document = prepare_document(&mut resources, written, None)?;
        let run =
            PreparedRun::prepare_with_profile_inspection(document, |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            })
            .map_err(Error::Compile)?;
        let findings = crate::verification_logs::location_findings(&resources, &run);
        if !findings.is_empty() {
            return Err(Error::Compile(crate::specification_run::Error::Prepare(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )));
        }
        Ok(Self { run, resources })
    }
    pub(crate) fn from_workflow(
        entry: crate::file_graph::CapturedEntry,
    ) -> Result<Self, Box<RejectedPlain>> {
        let (mut resources, entry, document, _) = entry.into_ordinary_parts();
        let result =
            PreparedRun::prepare_with_profile_inspection(document, |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            })
            .and_then(|run| {
                let findings = crate::verification_logs::location_findings(&resources, &run);
                if findings.is_empty() {
                    Ok(run)
                } else {
                    Err(crate::specification_run::Error::Prepare(
                        yamaa_core::specification::PrepareError::Invalid(findings),
                    ))
                }
            });
        match result {
            Ok(run) => {
                resources.finish_metadata_budget();
                Ok(Self { run, resources })
            }
            Err(error) => Err(Box::new(RejectedPlain {
                error: Error::Compile(error),
                entry,
                resources,
            })),
        }
    }
    pub fn run(&self) -> &PreparedRun {
        &self.run
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub fn capture_input_metadata(
        &mut self,
        index: usize,
        maximum: usize,
    ) -> Result<Arc<[u8]>, Vec<crate::issue_rows::Issue>> {
        let descriptors = crate::source_metadata::inputs(self.run()).map_err(|_| {
            crate::file_application::rejected("source_metadata", "descriptor_admission")
        })?;
        crate::source_metadata::capture(&mut self.resources, &descriptors, index, maximum)
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
                ResourceError::Missing | ResourceError::NotRegularFile | ResourceError::Symlink
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

#[cfg(all(test, any(unix, windows)))]
mod profile_inspection_tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn inherited_physical_kind_and_regular_unknown_profile_read_no_study_bytes() {
        let root = std::env::temp_dir().join(format!(
            "yamaa-profile-inspection-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("parent/input")).unwrap();
        fs::create_dir(root.join("entry")).unwrap();
        let source = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/unknown}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
        fs::write(root.join("parent/base.yaml"), source).unwrap();
        fs::write(
            root.join("entry/root.yaml"),
            "schema_version: '1.0'\nparents: [../parent/base.yaml]\n",
        )
        .unwrap();
        let root = fs::canonicalize(root).unwrap();
        use yamaa_core::diagnostic::ConditionCode as C;
        for (kind, condition) in [
            (0, C::ResourceNotRegularFile),
            (1, C::SourceUnknownProfile),
            (2, C::SourceUnknownProfile),
            (3, C::ResourceSymlink),
        ] {
            let input = root.join("parent/input/unknown");
            match kind {
                0 => fs::create_dir(&input).unwrap(),
                1 => fs::write(&input, b"ID\n1\n").unwrap(),
                2 => (),
                _ => {
                    fs::write(root.join("parent/input/target"), b"ID\n1\n").unwrap();
                    #[cfg(unix)]
                    std::os::unix::fs::symlink("target", &input).unwrap();
                    #[cfg(windows)]
                    std::os::windows::fs::symlink_file("target", &input).unwrap();
                }
            }
            let mut resources = Resources::new(
                root.to_str().unwrap(),
                root.join("entry").to_str().unwrap(),
                &[],
            )
            .unwrap();
            let document = prepare_document(&mut resources, "root.yaml", None).unwrap();
            let reads = resources.capture_reads();
            assert_eq!(reads, 2);
            let Err(crate::specification_run::Error::Prepare(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )) = PreparedRun::prepare_with_profile_inspection(
                document,
                |document, dataset, path| {
                    crate::source_profile_inspection::inspect(document, &resources, dataset, path)
                },
            )
            else {
                panic!("invalid source")
            };
            assert_eq!(resources.capture_reads(), reads);
            let issue = findings[0].diagnostic();
            assert_eq!(issue.code, condition);
            assert_eq!(issue.spec_paths, ["input.SRC.path"]);
            if kind == 0 || kind == 3 {
                assert_eq!(
                    issue.context["path"],
                    yamaa_core::diagnostic::ContextValue::Scalar(yamaa_core::value::Value::Str(
                        "input/unknown".into()
                    ))
                );
            }
            match kind {
                0 => fs::remove_dir(&input).unwrap(),
                2 => (),
                _ => fs::remove_file(&input).unwrap(),
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
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
    fn changed_dictionary_after_semantic_success_is_original_failure_before_entry_selection() {
        let root="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: str, derivation: {mapping: {source: SRC.ID, dict: dict.yaml}}}]\n";
        let study = Study::new(root);
        fs::write(study.0.join("entry/dict.yaml"), b"A: answer\n").unwrap();
        let mut resources = study.resources();
        let original = resources.resolve("root.yaml").unwrap();
        let result = yamaa_engine::file_preparation::prepare(
            &mut Native {
                resources: &mut resources,
            },
            &mut ChangeAfterSuccess {
                path: study.0.join("entry/dict.yaml"),
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
        assert!(!study.0.join("entry/never-read.csv").exists());
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
