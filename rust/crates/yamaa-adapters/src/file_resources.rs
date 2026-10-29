//! The snapshots an operation holds, captured through one resolver: the
//! local file resolver by default, or the caller's. Captured equality compares
//! retained bytes. This module opens no file and interprets no study data.
use std::{collections::BTreeMap, sync::Arc};

use crate::{
    file_resolver::FileResolver,
    host_resources::{Failure, Kind, PathRefusal, ResourceKind},
};
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::limits::{LimitExceeded, Limits};
use yamaa_core::resolver::{self, Captured, File, Located, Name, Origin, Resolver};

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Host(crate::host_resources::Failure),
    Missing,
    Unreadable,
    NotRegularFile,
    InvalidRoot,
    InvalidBase,
    InvalidPath,
    WrittenForm(yamaa_core::resource::ResourceFailure),
    OutsideRoots,
    Symlink,
    Changed,
    Limit(LimitExceeded),
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Host(_) => "host resource request failed",
            Self::Missing => "resource path missing",
            Self::Unreadable => "resource path is not readable",
            Self::NotRegularFile => "resource path is not a regular file",
            Self::InvalidRoot => "invalid approved resource root",
            Self::InvalidBase => "invalid resource base directory",
            Self::InvalidPath => "invalid resource path spelling",
            Self::WrittenForm(_) => "invalid resource path spelling",
            Self::OutsideRoots => "resource path outside approved roots",
            Self::Symlink => "resource path contains a symbolic link",
            Self::Changed => "captured resource content changed",
            Self::Limit(_) => "resource capture limit",
        }
    }
    /// The engine's judgement of a name its resolver reports as `kind`, the
    /// only source of FILE-7's and FILE-8's conditions: a link of any sort,
    /// a junction or another reparse point included, contains a symbolic
    /// link; any other entry that is not a regular file is refused as one;
    /// a missing name is missing.
    pub(crate) fn from_kind(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Missing => Self::Missing,
            ResourceKind::SymbolicLink | ResourceKind::Junction | ResourceKind::ReparsePoint => {
                Self::Symlink
            }
            ResourceKind::File | ResourceKind::Directory | ResourceKind::Other => {
                Self::NotRegularFile
            }
        }
    }
    /// The engine's reading of a resolver failure, whichever resolver
    /// reported it.
    pub(crate) fn from_failure(failure: Failure) -> Self {
        match failure {
            Failure::Missing => Self::Missing,
            Failure::Kind(kind) => Self::from_kind(kind),
            Failure::Changed => Self::Changed,
            Failure::Unreadable => Self::Unreadable,
            Failure::Path(PathRefusal::WrittenForm(failure)) => Self::WrittenForm(failure),
            Failure::Path(PathRefusal::Invalid) => Self::InvalidPath,
            Failure::Path(PathRefusal::Root) => Self::InvalidRoot,
            Failure::Path(PathRefusal::OutsideRoots) => Self::OutsideRoots,
            Failure::Limit(_)
            | Failure::Cancelled
            | Failure::Rejected
            | Failure::Publication
            | Failure::Cleanup => Self::Host(failure),
        }
    }
    /// A capture failure: a resource over the capture's `maximum` is the
    /// per-file boundary of its kind.
    fn from_capture(failure: Failure, kind: Kind, maximum: usize) -> Self {
        match failure {
            Failure::Limit(actual) => Self::Limit(LimitExceeded {
                resource: if kind == Kind::Metadata {
                    "spec_bytes"
                } else {
                    "source_bytes"
                },
                limit: maximum,
                actual: actual.and_then(|actual| usize::try_from(actual).ok()),
            }),
            failure => Self::from_failure(failure),
        }
    }
}

pub struct Resources {
    /// Every inspection and capture goes through this store's resolver.
    store: crate::host_resources::Resources,
    /// The local file resolver behind `store` when it is the default one:
    /// it anchors lexical locations and publication targets and selects
    /// caller parents.
    file: Option<Arc<FileResolver>>,
    host_sources: RetainedSources,
    /// The canonical identity of the last selected entry: the declaring
    /// identity a run resolver receives for that entry's study requests.
    entry: Option<String>,
    reads: usize,
}

/// Whether two resource locations name one file: ordinal case-insensitive on
/// Windows, where child opens are, and exact elsewhere.
pub fn same_location(left: &str, right: &str) -> bool {
    #[cfg(windows)]
    {
        crate::windows_file::equal_name(left, right)
    }
    #[cfg(unix)]
    {
        left == right
    }
}

/// Serialized artifacts a project run has already built, by producer. An
/// input whose `schema` link names that producer and whose `path` names the
/// held artifact's location reads those bytes instead of the saved file, still
/// under the producer's output contract.
#[derive(Clone, Debug, Default)]
pub struct HeldArtifacts {
    artifacts: BTreeMap<String, (String, Arc<[u8]>)>,
}
impl HeldArtifacts {
    pub fn new() -> Self {
        Self::default()
    }
    /// Hold the artifact the producer with canonical identity `producer`
    /// built for its `output.path` location `location`.
    pub fn insert(&mut self, producer: String, location: String, bytes: Arc<[u8]>) {
        self.artifacts.insert(producer, (location, bytes));
    }
    /// The held artifact of `producer`, when it was built for `location`.
    pub fn get(&self, producer: &str, location: &str) -> Option<Arc<[u8]>> {
        self.artifacts
            .get(producer)
            .filter(|(held, _)| same_location(held, location))
            .map(|(_, bytes)| Arc::clone(bytes))
    }
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}

/// One input that reads a producing specification's artifact through a
/// `schema` link, with the location its `path` names.
#[derive(Clone, Debug)]
pub struct LinkedInput {
    dataset: String,
    producer: String,
    location: String,
}

/// The linked inputs of one prepared document. Locations are lexical: no file
/// is opened, and an input whose location cannot be formed reads its file.
pub fn linked_inputs(
    resources: &Resources,
    document: &crate::specification_source::PreparedDocument,
    links: &[crate::specification_source::LinkedSpecification],
) -> Vec<LinkedInput> {
    links
        .iter()
        .filter_map(|link| {
            let origin = document.written_input_origin(&link.dataset, "path")?;
            let location = resources
                .location_from(origin.base_source, origin.written)
                .ok()?;
            Some(LinkedInput {
                dataset: link.dataset.clone(),
                producer: link.identity.clone(),
                location,
            })
        })
        .collect()
}

/// A run's study port: alias frames first, then held producer artifacts for
/// linked inputs, and every other request through the run's resources.
pub struct HeldPort<'a> {
    resources: &'a mut Resources,
    held: Option<Arc<HeldArtifacts>>,
    inputs: &'a [LinkedInput],
    frames: std::collections::BTreeMap<String, Arc<crate::column_table::ColumnTable>>,
}
impl<'a> HeldPort<'a> {
    pub fn new(run: &'a mut RunScope, inputs: &'a [LinkedInput]) -> Self {
        let held = run.held();
        let frames = run.frames.clone();
        Self {
            resources: run.resources_mut(),
            held,
            frames,
            inputs,
        }
    }
    /// The artifact a linked input reads, held by the run, with the location
    /// the input's path names.
    fn held(
        &self,
        source: &yamaa_core::specification::SourceDeclaration,
    ) -> Option<(String, Arc<[u8]>)> {
        let held = self.held.as_ref()?;
        let input = self
            .inputs
            .iter()
            .find(|input| input.dataset == source.name)?;
        held.get(&input.producer, &input.location)
            .map(|bytes| (input.location.clone(), bytes))
    }
}
/// The resolver port of one run: an alias override replaces that input;
/// otherwise a linked input reads a held artifact and other requests use the
/// configured resource scope.
impl yamaa_core::resolver::Resolver for HeldPort<'_> {
    type Error = Error;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, Error> {
        if let Name::Input(source) = name {
            if self.frames.contains_key(&source.name) {
                return Ok(Located {
                    identity: format!("memory:{}", source.name),
                    display_path: format!("memory:{}", source.name),
                });
            }
            if let Some((location, _)) = self.held(source) {
                return Ok(Located {
                    identity: location,
                    display_path: source.path.clone(),
                });
            }
        }
        Resolver::inspect(&mut *self.resources, name)
    }
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, Error> {
        if let Name::Input(source) = name {
            if let Some(table) = self.frames.get(&source.name) {
                return Ok(Captured {
                    bytes: Arc::from([]),
                    created: false,
                    origin: Origin::Memory,
                    table: Some(table.clone()),
                });
            }
            if let Some((_, bytes)) = self.held(source) {
                return Ok(Captured {
                    bytes,
                    created: false,
                    origin: Origin::Generated,
                    table: None,
                });
            }
        }
        Resolver::capture(&mut *self.resources, name)
    }
    fn failure(&self, name: Name<'_>, error: &Error) -> Option<resolver::Failure> {
        Resolver::failure(&*self.resources, name, error)
    }
    fn rebase(&mut self, layer: &Located, entry: &Located, written: &str) -> Result<String, Error> {
        Resolver::rebase(&mut *self.resources, layer, entry, written)
    }
    fn rebase_failure(
        &self,
        error: &Error,
        layer: &Located,
        entry: &Located,
        written: &str,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
        Resolver::rebase_failure(&*self.resources, error, layer, entry, written)
    }
}

/// The study half of one run's resource authority. A prepared value keeps
/// its compile-time [`Resources`] (the metadata snapshots, read during a run
/// only to verify them); each run captures study inputs, retains host
/// sources and resolves Save targets in its own scope, so nothing one run
/// captured is visible to another.
pub struct RunScope {
    resources: Resources,
    /// Compile evidence listed before a fresh scope's own captures, so the
    /// retained sources of a run behind its own resolver still begin with
    /// the metadata that compiled it. Empty when the scope inherits them.
    compiled: RetainedSources,
    /// Artifacts a project run already built, offered to this run's saved
    /// producer links.
    held: Option<Arc<HeldArtifacts>>,
    frames: std::collections::BTreeMap<String, Arc<crate::column_table::ColumnTable>>,
}
impl RunScope {
    /// Supply immutable frames for this run only, keyed by declared alias.
    pub fn with_inputs(
        mut self,
        frames: std::collections::BTreeMap<String, Arc<crate::column_table::ColumnTable>>,
    ) -> Self {
        self.frames = frames;
        self
    }

    /// Aliases supplied in memory, including inputs an attempt never reaches.
    /// Reporting reads this provenance without inspecting or capturing data.
    pub fn input_aliases(&self) -> impl Iterator<Item = &str> {
        self.frames.keys().map(String::as_str)
    }

    /// Offer artifacts the caller already built to this run's saved links.
    /// Links without a held artifact read the saved file their path names.
    pub fn hold(&mut self, held: Arc<HeldArtifacts>) {
        self.held = Some(held);
    }
    pub fn held(&self) -> Option<Arc<HeldArtifacts>> {
        self.held.clone()
    }
    /// Snapshots created so far in this scope, including those it inherited.
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    /// Owned aliases of every held byte source: compile evidence first, then
    /// this run's captures. This performs no resource access.
    pub fn retained_sources(&self) -> RetainedSources {
        let mut sources = self.compiled.clone();
        sources.extend(self.resources.retain_sources());
        sources
    }
    pub fn resources(&self) -> &Resources {
        &self.resources
    }
    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }
}

pub type RetainedSources = Vec<(String, Arc<[u8]>)>;
impl Resources {
    pub fn with_resolver(resolver: Arc<dyn crate::host_resources::Resolver>, base: &str) -> Self {
        Self {
            store: crate::host_resources::Resources::new(resolver, base.into()),
            file: None,
            host_sources: vec![],
            entry: None,
            reads: 0,
        }
    }
    /// Snapshots captured through the local file resolver, whose undeclared
    /// requests come from `base`.
    fn through_files((resolver, base): (FileResolver, String)) -> Self {
        let resolver = Arc::new(resolver);
        Self::with_file_resolver(
            Arc::clone(&resolver) as Arc<dyn crate::host_resources::Resolver>,
            base,
            resolver,
        )
    }
    /// Snapshots captured through `resolver`, whose undeclared requests come
    /// from `base`, with the local file resolver `files` that it stands in
    /// front of anchoring locations and publication targets.
    pub fn with_file_resolver(
        resolver: Arc<dyn crate::host_resources::Resolver>,
        base: String,
        files: Arc<FileResolver>,
    ) -> Self {
        Self {
            store: crate::host_resources::Resources::at(resolver, base),
            file: Some(files),
            host_sources: vec![],
            entry: None,
            reads: 0,
        }
    }
    /// Open one run's scope over this compile-time authority.
    ///
    /// Without a run resolver the scope reads where compile did and starts
    /// from a copy of the compile snapshots, so its reads, aliases, created
    /// snapshot counts and retained sources match a one-shot build exactly.
    /// A run resolver starts an empty scope at the same entry identity: its
    /// references never meet the compile resolver's. Either way the scope
    /// shares the approved roots and holds nothing a previous run captured.
    pub fn run_scope(
        &self,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
    ) -> RunScope {
        match resolver {
            None => RunScope {
                resources: Self {
                    store: self.store.scope(),
                    file: self.file.clone(),
                    host_sources: self.host_sources.clone(),
                    entry: self.entry.clone(),
                    reads: self.reads,
                },
                compiled: Vec::new(),
                held: None,
                frames: Default::default(),
            },
            Some(resolver) => {
                let entry = self
                    .entry
                    .clone()
                    .or_else(|| self.file.is_none().then(|| self.store.base.clone()))
                    .unwrap_or_else(|| "/".into());
                RunScope {
                    resources: Self {
                        store: crate::host_resources::Resources::at(resolver, entry.clone()),
                        file: None,
                        host_sources: vec![],
                        entry: Some(entry),
                        reads: 0,
                    },
                    compiled: self.retain_sources(),
                    held: None,
                    frames: Default::default(),
                }
            }
        }
    }

    /// Public entry paths select their existing parent once. Document paths
    /// resolve at their declaring file without configuration or fallback roots.
    /// The retained parent admits caller-owned aliases; every declared component
    /// outside it is walked from its filesystem root with the same link checks.
    pub fn for_entry(base: &str) -> Result<Self, Error> {
        FileResolver::for_entry(base).map(Self::through_files)
    }
    /// Select the parent of the project file a caller passes, as
    /// [`FileResolver::select_caller_parent`] describes. A caller resolver
    /// selects nothing. Configuration calls this before any run scope shares
    /// the roots.
    pub fn select_caller_parent(&mut self, path: &str) {
        if let Some(file) = &self.file {
            file.select_caller_parent(path);
        }
    }
    /// The first root is the project root; later roots retain fallback order.
    pub fn new(root: &str, base: &str, data_roots: &[String]) -> Result<Self, Error> {
        FileResolver::new(root, base, data_roots).map(Self::through_files)
    }
    pub fn capture_reads(&self) -> usize {
        self.reads
    }
    pub fn snapshot_count(&self) -> usize {
        self.store.len()
    }
    /// Immutable raw prefix evidence survives failed decoding and inheritance.
    /// This view grants no resource, execution or publication port.
    pub fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.host_sources
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.as_ref()))
    }
    /// Verify every held snapshot again, naming a failed one by the identity
    /// its documents carry: the canonical path of a local file, the logical
    /// location of a host object.
    pub fn verify_held(&self) -> Result<(), (String, Error)> {
        self.store
            .verify(self.store.len())
            .map_err(|(identity, failure)| (identity, Error::from_failure(failure)))
    }
    /// Own immutable raw evidence without copying content or reopening files.
    pub fn retain_sources(&self) -> RetainedSources {
        self.store.sources(None)
    }
    /// Metadata captures alone can hold YAML diagnostic locations. Input
    /// data remains in the complete evidence returned by `retain_sources`.
    pub fn retain_metadata_sources(&self) -> RetainedSources {
        self.store.sources(Some(Kind::Metadata))
    }
    /// Metadata only: no bytes read and no snapshot created.
    pub fn inspect(&self, written: &str) -> Result<(), Error> {
        self.inspect_from(&self.store.base, written)
    }
    /// Inspect relative to a canonical declaring entry without changing another
    /// node's resource base, reading bytes or creating a snapshot.
    pub fn inspect_from(&self, declaring: &str, written: &str) -> Result<(), Error> {
        self.store
            .resolve(declaring, written, Kind::Input)
            .map(drop)
            .map_err(Error::from_failure)
    }
    /// Capture source bytes under the fixed per-file source boundary.
    pub fn capture(&mut self, written: &str) -> Result<(Arc<[u8]>, bool), Error> {
        self.capture_as(None, written, Kind::Input)
    }
    /// Capture specification bytes under the fixed per-file metadata boundary.
    pub fn capture_metadata(&mut self, written: &str) -> Result<(Arc<[u8]>, bool), Error> {
        self.capture_as(None, written, Kind::Metadata)
    }
    /// A declaring identity comes from this reader's resolver.
    pub fn resolve_from(&self, declaring: &str, written: &str) -> Result<String, Error> {
        self.store
            .resolve(declaring, written, Kind::Metadata)
            .map_err(Error::from_failure)
    }
    pub fn resolve(&self, written: &str) -> Result<String, Error> {
        self.resolve_from(&self.store.base, written)
    }
    /// A lexical declaration location under the selected resolution policy.
    /// The target may be absent. This fact opens no file and grants no byte or
    /// publication authority; declaring identities come from this reader.
    pub fn location_from(&self, declaring: &str, written: &str) -> Result<String, Error> {
        match &self.file {
            Some(file) => file.location(declaring, written),
            None => {
                crate::host_resources::Resources::location(declaring, written).map_err(Error::Host)
            }
        }
    }
    /// Publication holds the declaration location, including absent targets.
    /// Retain its physical descriptor; the parent chain is checked at save.
    /// Retain the original declaring-layer location without rebasing through
    /// another entry. The declaring identity comes from this reader.
    pub fn publication_target_from(
        &self,
        declaring: &str,
        written: &str,
    ) -> Result<crate::publisher::AnchoredTarget, Error> {
        match &self.file {
            Some(file) => file
                .publication_target(declaring, written)
                .map(crate::publisher::AnchoredTarget::File),
            None => Ok(crate::publisher::AnchoredTarget::Host {
                resolver: Arc::clone(&self.store.resolver),
                declaring: declaring.into(),
                written: written.into(),
                reference: self
                    .store
                    .target(declaring, written)
                    .map_err(Error::from_failure)?,
            }),
        }
    }
    pub fn capture_from(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        self.capture_as(Some(declaring), written, Kind::Input)
    }
    pub fn capture_metadata_from(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        self.capture_as(Some(declaring), written, Kind::Metadata)
    }
    /// The caller's own file `name` in the selected entry directory, opened
    /// for reading; see [`FileResolver::open_named`]. Only the local file
    /// resolver opens a file.
    #[cfg(feature = "odm")]
    pub(crate) fn open_named(&self, name: &str) -> Result<std::fs::File, Error> {
        let file = self.file.as_ref().ok_or(Error::InvalidPath)?;
        file.open_named(&self.store.base, name)
            .map_err(Error::from_failure)
    }
    /// The caller's own file `name` in the selected entry directory, captured
    /// under `maximum` bytes; see [`FileResolver::capture_named`].
    #[cfg(feature = "odm")]
    pub(crate) fn capture_named(&self, name: &str, maximum: usize) -> Result<Arc<[u8]>, Error> {
        let file = self.file.as_ref().ok_or(Error::InvalidPath)?;
        file.capture_named(&self.store.base, name, maximum)
            .map_err(|failure| Error::from_capture(failure, Kind::Input, maximum))
    }
    /// The publication target of a file the caller names: through the local
    /// file resolver, its own file `name` in the selected entry directory
    /// (see [`FileResolver::publication_target_named`]); through a caller's
    /// resolver, the object that resolver's target reply names for `name`
    /// from the entry, as it names a specification's output.
    pub fn caller_target(&self, name: &str) -> Result<crate::publisher::AnchoredTarget, Error> {
        match &self.file {
            Some(file) => file
                .publication_target_named(&self.store.base, name)
                .map(crate::publisher::AnchoredTarget::File)
                .map_err(Error::from_failure),
            None => self.publication_target_from(&self.store.base, name),
        }
    }
    /// Keep the same path authority and captures for data declared by this
    /// entry: the identity its resolver gave it.
    pub fn select_entry_base(&mut self, identity: &str) -> Result<(), Error> {
        self.store.base = identity.into();
        self.entry = Some(identity.into());
        Ok(())
    }
    /// One capture through the resolver, under the per-file boundary of its
    /// kind: specification bytes for metadata, source bytes for inputs.
    fn capture_as(
        &mut self,
        declaring: Option<&str>,
        written: &str,
        kind: Kind,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        let maximum = if kind == Kind::Metadata {
            Limits::SPEC_BYTES
        } else {
            Limits::SOURCE_BYTES
        };
        let declaring = declaring.unwrap_or(&self.store.base).to_owned();
        let result = self
            .store
            .capture(&declaring, written, kind, maximum)
            .map_err(|failure| Error::from_capture(failure, kind, maximum))?;
        self.reads += usize::from(result.1);
        self.host_sources = self.store.sources(None);
        Ok(result)
    }
}

pub fn resource_failure(error: &Error) -> Option<yamaa_core::resource::ResourceFailure> {
    use yamaa_core::resource::ResourceFailure;
    match error {
        Error::Missing => Some(ResourceFailure::Missing),
        Error::NotRegularFile => Some(ResourceFailure::NotRegularFile),
        Error::Symlink => Some(ResourceFailure::Symlink),
        Error::WrittenForm(failure) => Some(*failure),
        Error::Changed => Some(ResourceFailure::ContentChanged),
        Error::Limit(bound) => Some(ResourceFailure::Boundary(*bound)),
        _ => None,
    }
}

/// The closed code of the opaque refusal a resource failure reports. A bound
/// reports its facts beside the code (`resource_limit`).
pub fn resource_fault(error: &Error) -> Fault {
    use Error as E;
    match error {
        E::Host(_) => Fault::HostResourceFailure,
        E::Missing => Fault::ResourceMissing,
        E::Unreadable => Fault::ResourceUnreadable,
        E::NotRegularFile => Fault::ResourceNotRegular,
        E::InvalidRoot => Fault::ResourceRoot,
        E::InvalidBase => Fault::ResourceBase,
        E::InvalidPath => Fault::ResourcePath,
        E::WrittenForm(_) => Fault::ResourcePath,
        E::OutsideRoots => Fault::ResourceOutsideRoots,
        E::Symlink => Fault::ResourceLink,
        E::Changed => Fault::ResourceChanged,
        E::Limit(_) => Fault::ResourceLimit,
    }
}
/// The refusal of a file a caller names that cannot be read: a classified
/// failure is its file rule's condition naming the path as given (FILE-7,
/// FILE-8, FILE-11), and any other stays an opaque resource refusal.
pub fn caller_read_failure(path: &str, error: &Error) -> Diagnostic {
    match resource_failure(error) {
        Some(failure) => failure.caller_diagnostic(path),
        None => yamaa_core::application_issue::rejected(Step::Prepare, resource_fault(error)),
    }
}

#[cfg(test)]
mod classification_tests {
    use super::*;
    use yamaa_core::resource::ResourceFailure;

    #[test]
    fn explicit_physical_facts_are_classified_and_other_failures_stay_opaque() {
        let boundary = Limits::source_bytes(Limits::SOURCE_BYTES + 1).unwrap_err();
        for (error, expected) in [
            (Error::Missing, ResourceFailure::Missing),
            (Error::NotRegularFile, ResourceFailure::NotRegularFile),
            (Error::Symlink, ResourceFailure::Symlink),
            (
                Error::WrittenForm(ResourceFailure::Backslash),
                ResourceFailure::Backslash,
            ),
            (Error::Changed, ResourceFailure::ContentChanged),
            (Error::Limit(boundary), ResourceFailure::Boundary(boundary)),
        ] {
            assert_eq!(resource_failure(&error), Some(expected));
        }
        for error in [
            Error::Unreadable,
            Error::InvalidRoot,
            Error::InvalidBase,
            Error::InvalidPath,
            Error::OutsideRoots,
        ] {
            assert_eq!(resource_failure(&error), None);
        }
    }
}

/// The resolver port on one scope's snapshots. An input resolves from the
/// selected entry, an entry from the caller's base or the project file that
/// lists it, and a metadata file from the file that writes it. A located
/// file is captured by a name that located it, which only a scope that keeps
/// those names knows: a located name alone is an invalid path.
impl yamaa_core::resolver::Resolver for Resources {
    type Error = Error;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, Error> {
        match name {
            Name::Input(source) => Ok(Located {
                identity: self
                    .store
                    .resolve(&self.store.base, &source.path, Kind::Input)
                    .map_err(Error::from_failure)?,
                display_path: source.path.clone(),
            }),
            Name::Entry { declaring, written } => {
                let identity = match declaring {
                    Some(declaring) => self.resolve_from(declaring, written)?,
                    None => self.resolve(written)?,
                };
                Ok(Located {
                    display_path: identity.clone(),
                    identity,
                })
            }
            Name::Written {
                file,
                declaring,
                written,
            } => {
                // A parent is judged as written before any resolver is asked
                // for it (FILE-4, FILE-5, LAYER-2), as schema admission judges
                // every other declared path: a backslash, an empty segment or
                // a trailing separator, which a network share's
                // `//server/share` root also writes, is refused as written
                // whichever resolver serves the layers.
                if file == File::Parent {
                    if let Some(failure) = yamaa_core::resource::written_path_failure(written) {
                        return Err(Error::WrittenForm(failure));
                    }
                }
                let identity = self.resolve_from(declaring, written)?;
                let display_path = match file {
                    File::Parent | File::Resource => {
                        crate::file_metadata::display_path(declaring, written)?
                    }
                    File::Project(_) => identity.clone(),
                };
                Ok(Located {
                    identity,
                    display_path,
                })
            }
            Name::Located { .. } => Err(Error::InvalidPath),
        }
    }
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, Error> {
        match name {
            Name::Input(source) => {
                let (bytes, created) = Resources::capture(self, &source.path)?;
                Ok(Captured {
                    table: self
                        .store
                        .frame(&self.store.base, &source.path)
                        .map(|table| table as Arc<dyn std::any::Any + Send + Sync>),
                    ..Captured::resource(bytes, created)
                })
            }
            Name::Entry { declaring, written } => {
                let (bytes, created) = match declaring {
                    Some(declaring) => self.capture_metadata_from(declaring, written)?,
                    None => self.capture_metadata(written)?,
                };
                Ok(Captured::resource(bytes, created))
            }
            Name::Written {
                declaring, written, ..
            } => {
                let (bytes, created) = self.capture_metadata_from(declaring, written)?;
                Ok(Captured::resource(bytes, created))
            }
            Name::Located { .. } => Err(Error::InvalidPath),
        }
    }
    /// An input over its limit is a boundary finding at its declaration. A
    /// metadata file over its limit has no finding of its own: the operation
    /// that reads it reports the limit (`resource_limit`).
    fn failure(&self, name: Name<'_>, error: &Error) -> Option<resolver::Failure> {
        match (name, error) {
            (_, Error::Unreadable) => Some(resolver::Failure::Unreadable),
            (Name::Entry { .. } | Name::Written { .. } | Name::Located { .. }, Error::Limit(_)) => {
                None
            }
            (_, error) => resource_failure(error).map(resolver::Failure::Resource),
        }
    }
    fn rebase(&mut self, layer: &Located, entry: &Located, written: &str) -> Result<String, Error> {
        crate::file_metadata::rebased(&layer.identity, &entry.identity, written)
    }
    /// The rebase is lexical, so its refusal is derived again, with both
    /// roots, from the same identities.
    fn rebase_failure(
        &self,
        _error: &Error,
        layer: &Located,
        entry: &Located,
        written: &str,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
        crate::file_metadata::share_refusal(layer, entry, written)
    }
}

#[cfg(test)]
mod identity_cache_tests {
    use super::*;
    #[test]
    fn reused_numeric_identity_without_a_current_physical_witness_is_a_fresh_capture() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let dir = Directory(
            std::env::temp_dir().join(format!("yamaa-stale-inode-{}", std::process::id())),
        );
        std::fs::create_dir(&dir.0).unwrap();
        std::fs::write(dir.0.join("a"), b"same bytes").unwrap();
        let root = dir.0.to_str().unwrap();
        let mut resources = Resources::new(root, root, &[]).unwrap();
        assert!(resources.capture("a").unwrap().1);
        // The filesystem may give a new file the device and inode of a
        // removed one. Snapshots are keyed by the canonical path the walk
        // reached; a physical identity binds a held snapshot only while the
        // held path still reaches that file. The removed path reaches
        // nothing, so equal bytes at the new path are a fresh capture and the
        // removed path stays missing.
        std::fs::remove_file(dir.0.join("a")).unwrap();
        std::fs::write(dir.0.join("c"), b"same bytes").unwrap();
        assert!(resources.capture("c").unwrap().1);
        assert_eq!(resources.capture_reads(), 2);
        assert_eq!(resources.capture("a"), Err(Error::Missing));
        assert!(!resources.capture("c").unwrap().1);
        assert_eq!(resources.capture_reads(), 2);
    }
}

#[cfg(test)]
mod physical_capture_tests {
    use super::*;
    use std::fs::File;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-physical-capture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(crate::canonical_path::canonicalize(path).unwrap())
        }
        fn resources(&self) -> Resources {
            let root = self.0.to_str().unwrap();
            Resources::new(root, root, &[]).unwrap()
        }
        fn identity(&self, name: &str) -> String {
            let path = self.0.join(name).to_str().unwrap().to_owned();
            #[cfg(windows)]
            let path = path.replace('\\', "/");
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn metadata_boundary_is_per_file_and_the_held_evidence_keeps_its_bytes() {
        let fixture = Fixture::new();
        let file = File::create(fixture.0.join("first.yaml")).unwrap();
        file.set_len(Limits::SPEC_BYTES as u64).unwrap();
        let file = File::create(fixture.0.join("second.yaml")).unwrap();
        file.set_len(Limits::SPEC_BYTES as u64).unwrap();
        let mut resources = fixture.resources();
        let (first, new) = resources.capture_metadata("first.yaml").unwrap();
        assert!(new);
        let (second, new) = resources.capture_metadata("second.yaml").unwrap();
        assert!(new);
        assert_eq!(first.len(), Limits::SPEC_BYTES);
        assert_eq!(second.len(), Limits::SPEC_BYTES);
        let retained = resources.retain_sources();
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0].0, fixture.identity("first.yaml"));
        assert_eq!(retained[1].0, fixture.identity("second.yaml"));
        assert!(Arc::ptr_eq(&retained[0].1, &first));
        assert!(Arc::ptr_eq(&retained[1].1, &second));
        std::fs::OpenOptions::new()
            .write(true)
            .open(fixture.0.join("first.yaml"))
            .unwrap()
            .set_len((Limits::SPEC_BYTES + 1) as u64)
            .unwrap();
        assert_eq!(
            resources.capture_metadata("first.yaml"),
            Err(Error::Changed)
        );
        std::fs::remove_file(fixture.0.join("first.yaml")).unwrap();
        std::fs::remove_file(fixture.0.join("second.yaml")).unwrap();
        assert!(retained
            .iter()
            .all(|(_, bytes)| bytes.iter().all(|b| *b == 0)));
        assert_eq!(resources.capture_reads(), 2);
    }

    #[test]
    fn source_and_metadata_boundaries_refuse_oversized_files_before_snapshot_creation() {
        let fixture = Fixture::new();
        let file = File::create(fixture.0.join("large.yaml")).unwrap();
        file.set_len((Limits::SPEC_BYTES + 1) as u64).unwrap();
        let mut resources = fixture.resources();
        assert_eq!(
            resources.capture_metadata("large.yaml"),
            Err(Error::Limit(
                Limits::spec_bytes(Limits::SPEC_BYTES + 1).unwrap_err()
            ))
        );
        assert_eq!(resources.capture_reads(), 0);
        assert_eq!(resources.snapshot_count(), 0);
        let (source, new) = resources.capture("large.yaml").unwrap();
        assert!(new);
        assert_eq!(source.len(), Limits::SPEC_BYTES + 1);
        assert_eq!(
            resources.capture_metadata("large.yaml"),
            Err(Error::Limit(
                Limits::spec_bytes(Limits::SPEC_BYTES + 1).unwrap_err()
            ))
        );
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(resources.snapshot_count(), 1);
        #[cfg(unix)]
        {
            // Unix truncation creates a sparse extent; refuse its logical size
            // before reading or allocating the ten-gigabyte source contents.
            file.set_len((Limits::SOURCE_BYTES + 1) as u64).unwrap();
            assert_eq!(resources.capture("large.yaml"), Err(Error::Changed));
            let mut uncaptured = fixture.resources();
            assert_eq!(
                uncaptured.capture("large.yaml"),
                Err(Error::Limit(
                    Limits::source_bytes(Limits::SOURCE_BYTES + 1).unwrap_err()
                ))
            );
            assert_eq!(uncaptured.capture_reads(), 0);
            assert_eq!(uncaptured.snapshot_count(), 0);
        }
    }

    #[test]
    fn repeated_alias_verification_keeps_original_bytes_and_checks_every_witness() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("source.yaml"), b"held original bytes").unwrap();
        let mut resources = fixture.resources();
        let (before, new) = resources.capture_metadata("source.yaml").unwrap();
        assert!(new);
        let (alias, new) = resources.capture_metadata("./source.yaml").unwrap();
        assert!(!new);
        assert!(Arc::ptr_eq(&before, &alias));
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(
            resources.captured_sources().next().unwrap().1,
            before.as_ref()
        );
        // A hard link reaches the same physical file: it binds the held
        // snapshot and is verified at its own path.
        std::fs::hard_link(fixture.0.join("source.yaml"), fixture.0.join("other.yaml")).unwrap();
        let (linked, new) = resources.capture_metadata("other.yaml").unwrap();
        assert!(!new);
        assert!(Arc::ptr_eq(&before, &linked));
        assert_eq!(resources.capture_reads(), 1);
        std::fs::remove_file(fixture.0.join("other.yaml")).unwrap();
        std::fs::write(fixture.0.join("other.yaml"), b"changed alias").unwrap();
        assert_eq!(
            resources.verify_held().unwrap_err(),
            (fixture.identity("other.yaml"), Error::Changed)
        );
        std::fs::write(fixture.0.join("source.yaml"), b"changed bytes").unwrap();
        assert_eq!(
            resources.verify_held().unwrap_err(),
            (fixture.identity("source.yaml"), Error::Changed)
        );
        assert_eq!(
            resources.captured_sources().next().unwrap().1,
            before.as_ref()
        );
    }
}

#[cfg(test)]
mod hard_link_tests {
    use super::*;
    use std::io::Write;

    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn directory(name: &str) -> Directory {
        let path = std::env::temp_dir().join(format!("yamaa-{name}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Directory(crate::canonical_path::canonicalize(path).unwrap())
    }
    /// Rewrite the file `path` names in place, keeping its physical file.
    fn rewrite(path: &std::path::Path, bytes: &[u8]) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
    }

    /// FILE-9: two hard links name one physical file, so a rewrite between
    /// their captures is a change, not a second snapshot with other bytes.
    #[test]
    fn a_hard_link_rewritten_between_captures_is_a_change() {
        let dir = directory("hard-link-rewrite");
        let root = dir.0.to_str().unwrap();
        std::fs::write(dir.0.join("a.csv"), b"ID\n1\n").unwrap();
        std::fs::hard_link(dir.0.join("a.csv"), dir.0.join("b.csv")).unwrap();
        for (rewritten, replacement) in [("a.csv", &b"ID\n2\n"[..]), ("b.csv", b"ID\n22\n")] {
            std::fs::write(dir.0.join("a.csv"), b"ID\n1\n").unwrap();
            let mut resources = Resources::new(root, root, &[]).unwrap();
            let (first, new) = resources.capture("a.csv").unwrap();
            assert!(new);
            rewrite(&dir.0.join(rewritten), replacement);
            assert_eq!(resources.capture("b.csv"), Err(Error::Changed));
            assert_eq!(first.as_ref(), b"ID\n1\n");
            assert_eq!(resources.capture_reads(), 1);
        }
        // Unchanged, the second link binds the held snapshot.
        std::fs::write(dir.0.join("a.csv"), b"ID\n3\n").unwrap();
        let mut resources = Resources::new(root, root, &[]).unwrap();
        let (first, _) = resources.capture("a.csv").unwrap();
        let (second, new) = resources.capture("b.csv").unwrap();
        assert!(!new);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(resources.capture_reads(), 1);
    }

    /// FILE-9 after the captured name moves: when `a.csv` names a new file,
    /// `b.csv` still names the file its snapshot was taken from, so a rewrite
    /// of it is a change, while its unchanged bytes are a snapshot of their
    /// own and `a.csv` reads its new file.
    #[test]
    fn a_hard_link_rewritten_after_its_captured_name_is_replaced_is_a_change() {
        let dir = directory("hard-link-replaced");
        let root = dir.0.to_str().unwrap();
        for rewritten in [true, false] {
            std::fs::write(dir.0.join("a.csv"), b"ID\n1\n").unwrap();
            std::fs::hard_link(dir.0.join("a.csv"), dir.0.join("b.csv")).unwrap();
            let mut resources = Resources::new(root, root, &[]).unwrap();
            let (first, _) = resources.capture("a.csv").unwrap();
            // `a.csv` now names a new file; `b.csv` still names the original.
            std::fs::remove_file(dir.0.join("a.csv")).unwrap();
            std::fs::write(dir.0.join("a.csv"), b"ID\n2\n").unwrap();
            if rewritten {
                rewrite(&dir.0.join("b.csv"), b"ID\n3\n");
                assert_eq!(resources.capture("b.csv"), Err(Error::Changed));
                assert_eq!(resources.capture_reads(), 1);
            } else {
                let (second, new) = resources.capture("b.csv").unwrap();
                assert!(new);
                assert_eq!(second, first);
                assert_eq!(resources.capture_reads(), 2);
            }
            assert_eq!(first.as_ref(), b"ID\n1\n");
            assert_eq!(resources.capture("a.csv"), Err(Error::Changed));
            let mut fresh = Resources::new(root, root, &[]).unwrap();
            assert_eq!(fresh.capture("a.csv").unwrap().0.as_ref(), b"ID\n2\n");
            std::fs::remove_file(dir.0.join("b.csv")).unwrap();
        }
    }

    /// The case #3201 names: inputs `dm.csv` and `raw/dm.csv` are hard links;
    /// after `dm.csv` is captured, a job renames it and rewrites
    /// `raw/dm.csv` in place. The second capture reaches the file the first
    /// one read, so its new bytes are a change (FILE-9), and so is the
    /// renamed name.
    #[cfg(unix)]
    #[test]
    fn a_hard_link_rewritten_after_its_captured_name_is_renamed_is_a_change() {
        let dir = directory("hard-link-renamed");
        let root = dir.0.to_str().unwrap();
        std::fs::create_dir(dir.0.join("raw")).unwrap();
        std::fs::write(dir.0.join("dm.csv"), b"USUBJID\n01\n").unwrap();
        std::fs::hard_link(dir.0.join("dm.csv"), dir.0.join("raw/dm.csv")).unwrap();
        let mut resources = Resources::new(root, root, &[]).unwrap();
        let (first, new) = resources.capture("dm.csv").unwrap();
        assert!(new);
        std::fs::rename(dir.0.join("dm.csv"), dir.0.join("raw/dm.old")).unwrap();
        rewrite(&dir.0.join("raw/dm.csv"), b"USUBJID\n02\n");
        assert_eq!(resources.capture("raw/dm.csv"), Err(Error::Changed));
        assert_eq!(resources.capture("raw/dm.old"), Err(Error::Changed));
        assert_eq!(resources.capture("dm.csv"), Err(Error::Missing));
        assert_eq!(first.as_ref(), b"USUBJID\n01\n");
        assert_eq!(resources.capture_reads(), 1);
        // A run that starts after the move reads the rewritten file.
        let mut later = Resources::new(root, root, &[]).unwrap();
        assert_eq!(
            later.capture("raw/dm.csv").unwrap().0.as_ref(),
            b"USUBJID\n02\n"
        );
    }
}

#[cfg(test)]
mod declaring_publication_tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Study(PathBuf);
    impl Study {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-layer-publication-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            for name in ["entry/out", "shared/out"] {
                fs::create_dir_all(path.join(name)).unwrap();
            }
            fs::write(path.join("entry/root.yaml"), b"entry").unwrap();
            fs::write(path.join("shared/base.yaml"), b"parent").unwrap();
            Self(crate::canonical_path::canonicalize(path).unwrap())
        }
        fn text(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().replace('\\', "/")
        }
        fn resources(&self) -> Resources {
            Resources::for_entry(&self.text("entry")).unwrap()
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn identical_output_spellings_publish_at_each_original_layer_without_metadata_rereads() {
        let study = Study::new();
        let resources = study.resources();
        let entry = resources.resolve("root.yaml").unwrap();
        let parent = resources
            .resolve_from(&entry, "../shared/base.yaml")
            .unwrap();
        let entry_target = resources
            .publication_target_from(&entry, "out/held.csv")
            .unwrap();
        let parent_target = resources
            .publication_target_from(&parent, "out/held.csv")
            .unwrap();
        fs::remove_file(study.0.join("entry/root.yaml")).unwrap();
        fs::remove_file(study.0.join("shared/base.yaml")).unwrap();
        assert!(!study.0.join("entry/out/held.csv").exists());
        assert!(!study.0.join("shared/out/held.csv").exists());
        parent_target
            .publisher("out/held.csv")
            .unwrap()
            .publish("out/held.csv", b"ID\n7\n")
            .unwrap();
        entry_target
            .publisher("out/held.csv")
            .unwrap()
            .publish("out/held.csv", b"ID\n8\n")
            .unwrap();
        assert_eq!(
            fs::read(study.0.join("shared/out/held.csv")).unwrap(),
            b"ID\n7\n"
        );
        assert_eq!(
            fs::read(study.0.join("entry/out/held.csv")).unwrap(),
            b"ID\n8\n"
        );
        assert_eq!(resources.capture_reads(), 0);
    }

    #[test]
    fn outside_layer_publication_refuses_linked_parent_substitution() {
        let study = Study::new();
        let resources = study.resources();
        let parent = resources.resolve("../shared/base.yaml").unwrap();
        let target = resources
            .publication_target_from(&parent, "out/held.csv")
            .unwrap();
        fs::rename(study.0.join("shared/out"), study.0.join("shared/moved")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../entry/out", study.0.join("shared/out")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir("../entry/out", study.0.join("shared/out")).unwrap();
        assert!(matches!(
            target.publisher("out/held.csv"),
            Err(crate::file_publication::Error::Unreachable(
                yamaa_core::resource::ResourceFailure::Symlink
            ))
        ));
        assert!(!study.0.join("entry/out/held.csv").exists());
        assert!(!study.0.join("shared/moved/held.csv").exists());
        assert_eq!(resources.capture_reads(), 0);
    }
}
