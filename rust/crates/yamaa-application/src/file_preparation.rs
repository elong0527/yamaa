//! Original file preparation through retained native resources and the shipped schema.
use crate::{
    file_resources::{Error as ResourceError, Resources, RunScope},
    specification_run::{Attempt, PreparedRun, RejectedRun},
    specification_source::{InheritanceError, Source},
};
use std::sync::Arc;
use yamaa_core::host_functions::HostFunctions;

#[derive(Debug)]
pub enum Error {
    /// The caller's own entry, the file a preparation without a declaring
    /// file starts from, could not be resolved or captured.
    Entry(ResourceError),
    Resource(ResourceError),
    Inventory(yamaa_core::submission_inventory::MetadataError),
    Preparation(InheritanceError<ResourceError>),
    ResourceFinding {
        error: ResourceError,
        diagnostic: Box<yamaa_core::diagnostic::Diagnostic>,
    },
    PreparationFinding {
        error: InheritanceError<ResourceError>,
        diagnostic: Box<yamaa_core::diagnostic::Diagnostic>,
    },
    /// The file a referencing field names declares a version the engine
    /// does not read, or another type: refused at that field before the
    /// file is prepared (SCHEMA-2, [`refused_header`]).
    Reference(Box<yamaa_core::diagnostic::Diagnostic>),
    Compile(crate::specification_run::Error),
}
impl Error {
    pub(crate) fn resource_at(error: ResourceError, field: &str, written: &str) -> Self {
        if error == ResourceError::Changed {
            Self::ResourceFinding {
                error,
                diagnostic: Box::new(
                    yamaa_core::resource::ResourceFailure::ContentChanged
                        .field_diagnostic(field, written),
                ),
            }
        } else {
            Self::Resource(error)
        }
    }
    pub(crate) fn entry_field(self, field: &str, written: &str) -> Self {
        match self {
            Self::ResourceFinding {
                error,
                mut diagnostic,
            } if diagnostic.spec_paths == ["specification"] => {
                *diagnostic = yamaa_core::resource::ResourceFailure::ContentChanged
                    .field_diagnostic(field, written);
                Self::ResourceFinding { error, diagnostic }
            }
            error => error,
        }
    }
    /// A finding in the 1.x header of a file a referencing field names, which
    /// that file's preparation reports as its entry's, names the file as
    /// `source` and `entry`, as a finding further into it does (SCHEMA-2).
    fn named_header(mut self) -> Self {
        if let Self::Preparation(InheritanceError::Entry(
            crate::specification_source::Error::Findings(captured),
        )) = &mut self
        {
            captured.name_file();
        }
        self
    }
}

/// One prepared specification, alone or with its project, and its
/// compile-time metadata authority. Study inputs, their snapshots and Save
/// targets belong to each run's [`RunScope`].
pub struct FileSpecification {
    run: PreparedRun,
    resources: Resources,
    /// Inputs a project run may serve from artifacts it already built.
    linked: Vec<crate::file_resources::LinkedInput>,
    /// The entry path as written and the producing specifications its
    /// `schema` links name: the declarations a run-start refusal names.
    entry: String,
    links: Vec<crate::specification_source::LinkedSpecification>,
}
/// A compilation refusal still owns the original native snapshots and the
/// documents it compiled.
pub struct Rejected {
    run: Box<RejectedRun>,
    entry: String,
    resources: Resources,
}

impl std::fmt::Debug for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rejected")
            .field("entry", &self.entry)
            .field("error", &self.run.rejection.error)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}
impl Rejected {
    pub(crate) fn retained_metadata_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_metadata_sources()
    }
    pub fn run(&self) -> &RejectedRun {
        &self.run
    }
    pub(crate) fn into_run(self) -> Box<RejectedRun> {
        self.run
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_sources()
    }
}
impl FileSpecification {
    /// Prepare a specification alone from `resources`, without a project.
    pub fn prepare(mut resources: Resources, written: &str) -> Result<Self, Error> {
        let document = prepare_document(&mut resources, written)?;
        let links = capture_links(&mut resources, &document, &mut None)?;
        let environment = crate::project_source::OwnedEnvironment::without_environment(Some(
            yamaa_core::project_function::Language::Python,
        ));
        Self::compile(resources, written.into(), document, links, environment).map_err(|rejected| {
            Error::Compile(crate::specification_run::Error::Prepare(
                rejected.run.rejection.error,
            ))
        })
    }
    pub(crate) fn from_workflow(
        entry: crate::file_entry::CapturedEntry,
    ) -> Result<Self, Box<Rejected>> {
        let (resources, entry, document, links, environment) = entry.into_parts();
        Self::compile(resources, entry, document, links, environment)
    }
    /// Compile a captured document against its environment, admitting its
    /// output locations before any study read.
    pub(crate) fn compile(
        resources: Resources,
        entry: String,
        document: crate::specification_source::PreparedDocument,
        links: Vec<crate::specification_source::LinkedSpecification>,
        environment: crate::project_source::OwnedEnvironment,
    ) -> Result<Self, Box<Rejected>> {
        let linked = crate::file_resources::linked_inputs(&resources, &document, &links);
        let result = PreparedRun::prepare_with_profile_inspection(
            document,
            &links,
            environment,
            |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            },
        )
        .and_then(|run| {
            let findings = crate::run_report::location_findings(&resources, &run, &links);
            if findings.is_empty() {
                Ok(run)
            } else {
                Err(run.reject(yamaa_core::specification::PrepareError::Invalid(findings)))
            }
        });
        match result {
            Ok(run) => Ok(Self {
                run,
                resources,
                linked,
                entry,
                links,
            }),
            Err(run) => Err(Box::new(Rejected {
                run,
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

    pub fn resolved_specification(
        &self,
    ) -> Result<crate::resolved_artifact::ResolvedArtifact, crate::resolved_artifact::Error> {
        crate::resolved_artifact::single(
            self.run.document(),
            &self.resources,
            self.run
                .captured_environment()
                .root_optional()
                .map(|root| root.source().identity.clone()),
        )
    }

    /// Open one run's study scope; see [`Resources::run_scope`].
    pub fn run_scope(
        &self,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
    ) -> RunScope {
        self.resources.run_scope(resolver)
    }
    /// Verify every held metadata snapshot at run start, before activation or
    /// any study read: the entry, its layers, dictionaries and linked
    /// producing specifications and, with a project, the project file and
    /// the lock, function files and codelist files it declares. A changed or
    /// missing file refuses the run naming its declaration; see
    /// [`crate::file_application::verify_held`].
    pub fn verify_metadata(&self) -> Result<(), crate::file_application::Failure> {
        use crate::changed_metadata as changed;
        let captured = self.run.captured_environment();
        let held = changed::Held {
            entry: &self.entry,
            document: self.run.document(),
            links: &self.links,
            project: captured.project(),
            inventory: captured.inventory_documents(),
            environment: captured
                .root_optional()
                .zip(captured.project())
                .zip(captured.written())
                .map(|((root, project), written)| changed::Environment {
                    written,
                    host: self.run.environment().host(),
                    root,
                    project,
                    lock: captured.lock(),
                    captures: captured.captures(),
                }),
        };
        crate::file_application::verify_held(&self.resources, |identity, failure| {
            changed::declaration(&held, identity, failure)
        })
    }
    /// Bounded aliases of the held metadata bytes; no file access or replay.
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_sources()
    }
    /// The save target of the output file `field` declares, resolved in
    /// `run`; a target its resolver cannot reach is refused as a save's walk
    /// refuses it, at `output.<field>`.
    pub(crate) fn publication_field_target(
        &self,
        run: &RunScope,
        field: &str,
    ) -> Result<crate::publisher::AnchoredTarget, crate::file_application::Failure> {
        use crate::file_application::{resource_failure, unresolved_target};
        use yamaa_core::application_issue::Step;
        let origin = self
            .run
            .document()
            .written_output_field_origin(field)
            .ok_or_else(|| resource_failure(Step::Output, &ResourceError::InvalidPath))?;
        run.resources()
            .publication_target_from(origin.base_source, origin.written)
            .map_err(|error| {
                unresolved_target(&error, Some(&format!("output.{field}")), origin.written)
            })
    }
    /// Activate through `functions`, the build's host-functions port, then
    /// capture, decode, bind and execute in `run`; this value is unchanged. A
    /// specification that calls no project function never enters `functions`.
    pub fn build<A: HostFunctions>(
        &self,
        run: &mut RunScope,
        functions: &mut A,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Attempt<ResourceError, A::Error> {
        let mut port = crate::file_resources::HeldPort::new(run, &self.linked);
        self.run.execute_with_ports(functions, &mut port, scope)
    }
}

/// Retain one native capture/inheritance path for ordinary and project
/// specifications, read with the package-owned 1.0 closures.
pub(crate) fn prepare_document(
    resources: &mut Resources,
    written: &str,
) -> Result<crate::specification_source::PreparedDocument, Error> {
    prepare_document_from(resources, None, written)
}

/// Recursive metadata uses the original declaring identity and written spelling.
/// A reader-returned canonical identity is never reinterpreted as authored text.
pub(crate) fn prepare_document_from(
    resources: &mut Resources,
    declaring: Option<&str>,
    written: &str,
) -> Result<crate::specification_source::PreparedDocument, Error> {
    prepare_with(resources, declaring, written, |source, display, aliases| {
        crate::shipped_schema::prepare(source, display, aliases)
    })
}

/// [`prepare_document_from`] of the entry a caller names, `written`, as a
/// fragment checked on the parent a caller names (#3694): that file is read
/// as a file the caller names (FILE-1), and a fragment that declares `parent`
/// or `include` refuses it (LAYER-4).
pub(crate) fn prepare_fragment_from(
    resources: &mut Resources,
    written: &str,
    parent: yamaa_core::inheritance::CallerParent<'_>,
) -> Result<crate::specification_source::PreparedDocument, Error> {
    prepare_with(resources, None, written, |source, display, aliases| {
        crate::specification_source::prepare_fragment(source, display, parent, aliases)
    })
}

/// Capture the entry `written` names, prepare its document with `prepare`
/// through a cache that keeps every spelling that located a metadata file,
/// and, only once it has prepared, capture every spelling of each file it
/// holds again (FILE-9, FILE-10). Only then does the entry become the scope's
/// base.
fn prepare_with(
    resources: &mut Resources,
    declaring: Option<&str>,
    written: &str,
    prepare: impl FnOnce(
        Source,
        String,
        &mut yamaa_core::file_preparation::AliasCache<'_, Resources>,
    ) -> Result<
        crate::specification_source::PreparedDocument,
        InheritanceError<ResourceError>,
    >,
) -> Result<crate::specification_source::PreparedDocument, Error> {
    use yamaa_core::file_preparation::{self as application, AliasCache, Error as Failure};
    let prepared = (|| {
        let source = application::capture_entry(&mut *resources, declaring, written)?;
        let display = source.identity.clone();
        let mut aliases = AliasCache::new(&mut *resources);
        let mut document =
            prepare(source, display, &mut aliases).map_err(|error| aliases.refused(error))?;
        let held = aliases.accept(document.held(), |declaring, written, failure| {
            document.resource_diagnostic(declaring, written, failure)
        })?;
        document.retain_resource_aliases(held);
        Ok(document)
    })();
    let document = prepared.map_err(|error| match error {
        // Only an entry no file declares is the caller's own; a producer a
        // `schema` link names was resolved from its declaring file first.
        Failure::Entry(error) if declaring.is_none() => Error::Entry(error),
        Failure::Entry(error) | Failure::Resource(error) => Error::Resource(error),
        Failure::Preparation(error) => Error::Preparation(error),
        Failure::ResourceFinding { error, diagnostic } => {
            Error::ResourceFinding { error, diagnostic }
        }
        Failure::PreparationFinding { error, diagnostic } => {
            Error::PreparationFinding { error, diagnostic }
        }
    })?;
    // Data the entry declares reads from the identity its resolver gave it.
    resources
        .select_entry_base(&document.source().identity)
        .map_err(Error::Resource)?;
    Ok(document)
}

/// The refusal at `field` of the specification it names, `written` from
/// `declaring` and resolved as `identity`, read before that file is prepared
/// (SCHEMA-2), as a 1.0 project reads a function or codelist file it lists:
/// a header that declares a version the engine does not read, including a
/// `schema_version` file of language 0.5, or a type other than `spec`, is
/// refused at the field, naming the file as `source`. A malformed header is a
/// finding in the file, and a file that cannot be read or decoded fails as
/// before, each when the file is prepared.
pub(crate) fn refused_header(
    resources: &mut Resources,
    declaring: &str,
    written: &str,
    identity: &str,
    field: &str,
) -> Option<yamaa_core::diagnostic::Diagnostic> {
    use yamaa_core::language::{
        self, FileType, HeaderFault, PathBase, Reference, ReferenceFault, LANGUAGE_VERSION,
    };
    let (bytes, _) = resources.capture_metadata_from(declaring, written).ok()?;
    let document = crate::yaml_decode::decode_yaml(&bytes).ok()?.document;
    let fault @ (HeaderFault::Legacy { .. }
    | HeaderFault::Version { .. }
    | HeaderFault::FileType { .. }) =
        language::read_header(&document, Some(LANGUAGE_VERSION), FileType::Spec).err()?
    else {
        return None;
    };
    let reference = Reference {
        field: field.into(),
        written: written.into(),
        expected: FileType::Spec,
        base: PathBase::Declaring,
    };
    match reference.refuse(identity, &fault) {
        ReferenceFault::Field(diagnostic) => Some(diagnostic),
        ReferenceFault::Finding(_) => None,
    }
}

/// A classified failure observed for a `schema` link keeps its authored field.
fn link_resource(error: ResourceError, field: &str, written: &str) -> Error {
    match crate::file_resources::resource_failure(&error) {
        Some(failure) => Error::ResourceFinding {
            error,
            diagnostic: Box::new(failure.field_diagnostic(field, written)),
        },
        None => Error::Resource(error),
    }
}

/// Read each producing specification a `schema` link names once, as a validated
/// specification document; aliases share the one prepared document. The
/// consumer checks the producer's header at its link before the producer is
/// prepared ([`refused_header`], SCHEMA-2). A linked
/// producer's own links are not followed: a link supplies only the stored
/// artifact's output contract, and the producer is never compiled or executed.
/// `origin` names the input being read when a failure returns.
pub(crate) fn capture_links(
    resources: &mut Resources,
    document: &crate::specification_source::PreparedDocument,
    origin: &mut Option<String>,
) -> Result<Vec<crate::specification_source::LinkedSpecification>, Error> {
    use crate::specification_source::LinkedSpecification;
    use yamaa_core::schema::DocumentNode as N;
    let d = document.document();
    let Some(N::Mapping(entries)) = d.field(d.root(), "input").map(|id| &d.nodes()[id]) else {
        return Ok(Vec::new());
    };
    let mut links: Vec<LinkedSpecification> = Vec::new();
    let mut prepared_any = false;
    for &(name, declaration) in entries {
        let N::Text(dataset) = &d.nodes()[name] else {
            continue;
        };
        if !d
            .field(declaration, "schema")
            .is_some_and(|id| !matches!(d.nodes()[id], N::Null))
        {
            continue;
        }
        // Without an authored origin no link is supplied; link admission then
        // reports the declaration's missing metadata.
        let Some(link) = document.written_input_origin(dataset, "schema") else {
            continue;
        };
        *origin = Some(dataset.clone());
        let field = format!("input.{dataset}.schema");
        let identity = resources
            .resolve_from(link.base_source, link.written)
            .map_err(|error| link_resource(error, &field, link.written))?;
        let linked = if identity == document.source().identity {
            // A link naming the consuming specification is refused by link
            // admission; it holds no separate document.
            None
        } else if let Some(known) = links
            .iter()
            .filter_map(|prior| prior.document.as_ref())
            .find(|prior| prior.source().identity == identity)
        {
            // An alias spelling verifies its own snapshot but reuses the one
            // canonical prepared document.
            let known = Arc::clone(known);
            resources
                .capture_metadata_from(link.base_source, link.written)
                .map_err(|error| link_resource(error, &field, link.written))?;
            Some(known)
        } else {
            if let Some(refused) =
                refused_header(resources, link.base_source, link.written, &identity, &field)
            {
                return Err(Error::Reference(Box::new(refused)));
            }
            let prepared =
                prepare_document_from(resources, Some(link.base_source), link.written)
                    .map_err(|error| error.entry_field(&field, link.written).named_header())?;
            prepared_any = true;
            if prepared.source().identity != identity {
                return Err(Error::resource_at(
                    ResourceError::Changed,
                    &field,
                    link.written,
                ));
            }
            Some(Arc::new(prepared))
        };
        links.push(LinkedSpecification {
            dataset: dataset.clone(),
            identity,
            document: linked,
        });
    }
    *origin = None;
    if prepared_any {
        // Producing-specification preparation selects its own entry base.
        // Restore the consumer's before its source paths acquire read authority.
        resources
            .select_entry_base(&document.source().identity)
            .map_err(Error::Resource)?;
    }
    Ok(links)
}

/// Return only unclassified native failures; observed facts retain their source owner.
pub fn opaque_resource_failure<E>(
    attempt: &crate::specification_run::CapturedAttempt<ResourceError, E>,
) -> Option<&ResourceError> {
    use crate::specification_run::PortError;
    match &attempt.result {
        Err(PortError::Capture(error))
            if crate::file_resources::resource_failure(error).is_none() =>
        {
            Some(error)
        }
        Err(PortError::Inspect(errors)) => errors
            .iter()
            .find(|error| crate::file_resources::resource_failure(&error.error).is_none())
            .map(|error| &error.error),
        _ => None,
    }
}

#[cfg(all(test, any(unix, windows)))]
#[cfg(feature = "yaml")]
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
        let source = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/unknown}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
        fs::write(root.join("parent/base.yaml"), source).unwrap();
        fs::write(
            root.join("entry/root.yaml"),
            "yamaa: '1.0'\ntype: spec\nparent: ../parent/base.yaml\n",
        )
        .unwrap();
        let root = crate::canonical_path::canonicalize(root).unwrap();
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
            let document = prepare_document(&mut resources, "root.yaml").unwrap();
            let reads = resources.capture_reads();
            assert_eq!(reads, 2);
            let Err(rejected) = PreparedRun::prepare_with_profile_inspection(
                document,
                &[],
                crate::project_source::OwnedEnvironment::without_environment(Some(
                    yamaa_core::project_function::Language::Python,
                )),
                |document, dataset, path| {
                    crate::source_profile_inspection::inspect(document, &resources, dataset, path)
                },
            ) else {
                panic!("invalid source")
            };
            let yamaa_core::specification::PrepareError::Invalid(findings) =
                &rejected.rejection.error
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
                    yamaa_core::json::Value::String("input/unknown".into())
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

#[cfg(all(test, feature = "yaml"))]
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
            Self(crate::canonical_path::canonicalize(root).unwrap())
        }
        fn check(&self) {
            let parent = "yamaa: '1.0'\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let child = "yamaa: '1.0'\ntype: spec\nparent: './base.yaml'\ndomain: PRODUCER\n";
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
            let document =
                prepare_document_from(&mut resources, Some(&declaring), "../producer/child.yaml")
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

#[cfg(all(test, feature = "yaml"))]
mod dispatch_policy_tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    const PARENT: &str = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
    const ROOT: &str = "yamaa: '1.0'\ntype: spec\nparent: ../parent/base.yaml\n";
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
            Self(crate::canonical_path::canonicalize(path).unwrap())
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
    fn native_success_replays_aliases_from_the_same_retained_snapshots() {
        let study = Study::new(ROOT);
        let mut resources = study.resources();
        let document = prepare_document_from(&mut resources, None, "entry/root.yaml").unwrap();
        assert_eq!(document.parents().len(), 1);
        assert_eq!(resources.snapshot_count(), 2);
        assert_eq!(resources.capture_reads(), 2);
        assert_eq!(
            resources.resolve("root.yaml").unwrap(),
            document.source().identity
        );
        let parent = document.parents()[0].source().identity.clone();
        assert_eq!(
            resources
                .capture_metadata_from(&parent, "./base.yaml")
                .unwrap()
                .0
                .as_ref(),
            PARENT.as_bytes()
        );
        assert_eq!(resources.capture_reads(), 2);
    }
    #[test]
    fn native_semantic_failure_keeps_original_entry_base_and_drops_success_alias_authority() {
        let root = ROOT.replace("'1.0'", "'99.0'");
        let study = Study::new(&root);
        let mut resources = study.resources();
        let original = resources.resolve("root.yaml").unwrap();
        assert!(matches!(
            prepare_document_from(&mut resources, None, "entry/root.yaml"),
            Err(Error::Preparation(_))
        ));
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(resources.resolve("root.yaml").unwrap(), original);
    }
    struct ChangeAfterSuccess {
        path: PathBuf,
    }
    /// A content change at `field`, naming the path as `written` and, at a
    /// `parent`, the layer that writes it as `source`.
    fn assert_content_change(
        diagnostic: &yamaa_core::diagnostic::Diagnostic,
        field: &str,
        written: &str,
        source: Option<&str>,
    ) {
        assert_eq!(diagnostic.definition().phase, "ingest");
        assert_eq!(
            diagnostic.code,
            yamaa_core::diagnostic::ConditionCode::ResourceContentChanged
        );
        assert_eq!(diagnostic.spec_paths, [field]);
        let mut context = yamaa_core::diagnostic::Context::new();
        context.insert(
            "path".into(),
            yamaa_core::json::Value::String(written.into()),
        );
        if let Some(source) = source {
            context.insert(
                "source".into(),
                yamaa_core::json::Value::String(source.into()),
            );
        }
        assert_eq!(diagnostic.context, context);
    }
    #[test]
    fn changed_entry_rejects_replacement_before_decoding_and_keeps_original_error() {
        let study = Study::new(ROOT);
        let mut resources = study.resources();
        let original_base = resources.resolve("root.yaml").unwrap();
        let original = resources.capture_metadata("entry/root.yaml").unwrap().0;
        fs::write(study.0.join("entry/root.yaml"), b"invalid replacement").unwrap();
        let Err(Error::ResourceFinding { error, diagnostic }) =
            prepare_document_from(&mut resources, None, "entry/root.yaml")
        else {
            panic!("changed entry retains its original native error");
        };
        assert_eq!(error, ResourceError::Changed);
        assert_content_change(&diagnostic, "specification", "entry/root.yaml", None);
        assert_eq!(original.as_ref(), ROOT.as_bytes());
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(resources.resolve("root.yaml").unwrap(), original_base);
    }
    #[test]
    fn changed_parent_capture_keeps_original_host_payload_before_semantic_success() {
        let study = Study::new(ROOT);
        let mut resources = study.resources();
        let original_base = resources.resolve("root.yaml").unwrap();
        let entry = resources.resolve("entry/root.yaml").unwrap();
        let original = resources
            .capture_metadata_from(&entry, "../parent/base.yaml")
            .unwrap()
            .0;
        fs::write(study.0.join("parent/base.yaml"), b"invalid replacement").unwrap();
        let Err(Error::PreparationFinding { error, diagnostic }) =
            prepare_document_from(&mut resources, None, "entry/root.yaml")
        else {
            panic!("changed parent capture retains original preparation");
        };
        assert!(matches!(error, InheritanceError::Versioned { error, .. }
        if matches!(*error, crate::document::SpecificationError::Traversal(
            yamaa_core::inheritance::Error::Host(crate::specification_source::SourceFailure::Host(ResourceError::Changed))
        ))));
        // The entry writes the `parent`.
        assert_content_change(&diagnostic, "parent", "../parent/base.yaml", Some(&entry));
        assert_eq!(original.as_ref(), PARENT.as_bytes());
        assert_eq!(resources.capture_reads(), 2);
        assert_eq!(resources.resolve("root.yaml").unwrap(), original_base);
        assert!(!study.0.join("entry/never-read.csv").exists());
        assert!(!study.0.join("entry/never-written.csv").exists());
    }
    #[test]
    fn changed_dictionary_capture_uses_exact_declaring_expression_without_replacement() {
        let root = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: str, derivation: {mapping: {source: SRC.ID, dict: dict.yaml}}}]\n";
        let study = Study::new(root);
        fs::write(study.0.join("entry/dict.yaml"), b"A: answer\n").unwrap();
        let mut resources = study.resources();
        let entry = resources.resolve("entry/root.yaml").unwrap();
        let original = resources
            .capture_metadata_from(&entry, "dict.yaml")
            .unwrap()
            .0;
        fs::write(study.0.join("entry/dict.yaml"), b"B: replaced\n").unwrap();
        let Err(Error::Preparation(error)) =
            prepare_document_from(&mut resources, None, "entry/root.yaml")
        else {
            panic!("changed dictionary keeps original preparation error");
        };
        let crate::specification_diagnostics::Rejection::Invalid(findings) =
            crate::specification_diagnostics::inheritance_failure_ref(&error).unwrap()
        else {
            panic!("a changed dictionary is a finding");
        };
        assert_eq!(
            findings,
            [yamaa_core::resource::ResourceFailure::ContentChanged
                .field_diagnostic("columns.ID.derivation.mapping.dict", "dict.yaml")]
        );
        let InheritanceError::Dictionary { error, .. } = error else {
            panic!("dictionary owner missing");
        };
        assert!(matches!(
            error.cause,
            yamaa_core::dictionary_preparation::Cause::Capture {
                error: yamaa_core::inheritance::SourceError::Raised(ResourceError::Changed),
                ..
            }
        ));
        assert_eq!(original.as_ref(), b"A: answer\n");
        assert_eq!(resources.capture_reads(), 2);
        assert!(!study.0.join("entry/never-read.csv").exists());
        assert!(!study.0.join("entry/never-written.csv").exists());
    }
    impl ChangeAfterSuccess {
        /// Prepare `entry/root.yaml`, changing the file at this path once the
        /// document has prepared.
        fn prepare(
            &self,
            resources: &mut Resources,
        ) -> Result<crate::specification_source::PreparedDocument, Error> {
            prepare_with(
                resources,
                None,
                "entry/root.yaml",
                |source, display, aliases| {
                    let document = crate::shipped_schema::prepare(source, display, aliases)?;
                    // A deterministic external change occurs after semantic success and
                    // before the application accepts any successful authored alias.
                    fs::write(&self.path, b"changed after semantic success").unwrap();
                    Ok(document)
                },
            )
        }
    }
    #[test]
    fn changed_dictionary_after_semantic_success_is_original_failure() {
        let root="yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: str, derivation: {mapping: {source: SRC.ID, dict: dict.yaml}}}]\n";
        let study = Study::new(root);
        fs::write(study.0.join("entry/dict.yaml"), b"A: answer\n").unwrap();
        let mut resources = study.resources();
        let result = ChangeAfterSuccess {
            path: study.0.join("entry/dict.yaml"),
        }
        .prepare(&mut resources);
        let Err(Error::ResourceFinding { error, diagnostic }) = result else {
            panic!("original changed dictionary with authored diagnostic");
        };
        assert_eq!(error, ResourceError::Changed);
        assert_eq!(diagnostic.definition().phase, "ingest");
        assert_eq!(
            diagnostic.code,
            yamaa_core::diagnostic::ConditionCode::ResourceContentChanged
        );
        assert_eq!(
            diagnostic.spec_paths,
            ["columns.ID.derivation.mapping.dict"]
        );
        assert_eq!(
            diagnostic.context,
            [(
                "path".into(),
                yamaa_core::json::Value::String("dict.yaml".into())
            )]
            .into()
        );
        assert_eq!(resources.capture_reads(), 2);
        assert!(!study.0.join("entry/never-read.csv").exists());
    }
    #[test]
    fn changed_parent_after_semantic_success_is_original_resource_failure() {
        let study = Study::new(ROOT);
        let mut resources = study.resources();
        let entry = resources.resolve("entry/root.yaml").unwrap();
        let result = ChangeAfterSuccess {
            path: study.0.join("parent/base.yaml"),
        }
        .prepare(&mut resources);
        let Err(Error::ResourceFinding { error, diagnostic }) = result else {
            panic!("original changed parent with authored diagnostic");
        };
        assert_eq!(error, ResourceError::Changed);
        // The entry writes the `parent`.
        assert_content_change(&diagnostic, "parent", "../parent/base.yaml", Some(&entry));
        assert_eq!(resources.capture_reads(), 2);
    }
}

#[cfg(all(test, feature = "yaml"))]
mod default_resolver_tests {
    use crate::file_resolver::FileResolver;
    use crate::file_resources::Resources;
    use crate::host_resources::{Failure, Kind, Operation, Reply, Request, Resolver};
    use std::sync::{Arc, Mutex};

    /// The local file resolver, with every request it serves recorded.
    struct Recording {
        files: Arc<FileResolver>,
        requests: Mutex<Vec<(Operation, Kind, String)>>,
    }
    impl Resolver for Recording {
        fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
            self.requests.lock().unwrap().push((
                request.operation,
                request.kind,
                request.written.to_owned(),
            ));
            self.files.request(request)
        }
    }

    #[test]
    fn a_build_on_local_files_reads_every_file_through_the_file_resolver() {
        let directory =
            std::env::temp_dir().join(format!("yamaa-default-resolver-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("input")).unwrap();
        std::fs::write(
            directory.join("spec.yaml"),
            "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/src.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n",
        )
        .unwrap();
        std::fs::write(directory.join("input/src.csv"), "ID\n1\n2\n").unwrap();
        let (files, base) = FileResolver::for_entry(directory.to_str().unwrap()).unwrap();
        let files = Arc::new(files);
        let recording = Arc::new(Recording {
            files: Arc::clone(&files),
            requests: Mutex::new(Vec::new()),
        });
        let resources = Resources::with_file_resolver(
            Arc::clone(&recording) as Arc<dyn crate::host_resources::Resolver>,
            base,
            files,
        );
        let prepared =
            crate::file_preparation::FileSpecification::prepare(resources, "spec.yaml").unwrap();
        let mut run = prepared.run_scope(None);
        let attempt = prepared.build(
            &mut run,
            &mut yamaa_core::host_functions::Empty::<std::convert::Infallible>::new(),
            yamaa_core::application_observer::Scope::default(),
        );
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(attempt.dataset.result.is_ok());
        assert_eq!(run.capture_reads(), 2);
        let requests = recording.requests.lock().unwrap().clone();
        let captured = |kind, written: &str| {
            requests
                .iter()
                .any(|request| *request == (Operation::Capture, kind, written.to_owned()))
        };
        assert!(captured(Kind::Metadata, "spec.yaml"), "{requests:?}");
        assert!(captured(Kind::Input, "input/src.csv"), "{requests:?}");
        assert!(requests.iter().all(|(operation, _, written)| {
            matches!(operation, Operation::Inspect | Operation::Capture)
                && ["spec.yaml", "input/src.csv"].contains(&written.as_str())
        }));
    }
}
