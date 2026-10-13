//! Shared build request and original metadata admission. Study and publication
//! authority enter only after the selected checked capability is complete.
use alloc::string::String;
use yamaa_core::schema::{Document, DocumentNode};

pub struct Request<'a> {
    pub specification: &'a str,
    pub environment: Option<&'a str>,
}

/// An absent environment has no declarations, captures, lock or activation
/// requirements. It does not manufacture a source file or discovery location.
pub fn without_environment(
    host: crate::host_values::Language,
) -> yamaa_core::project_environment::ExecutionEnvironment {
    use yamaa_core::project_environment::{Draft, Environment};
    Environment::admit(
        host,
        Draft {
            language: None,
            lock: None,
            functions: None,
            codelists: alloc::vec::Vec::new(),
            has_study: false,
            submissions: alloc::vec::Vec::new(),
        },
    )
    .expect("empty environment admission")
    .into_execution()
}

/// Native quota and snapshot observations; these methods neither decode nor
/// activate code, read study data, or publish artifacts.
pub trait MetadataAdmissionPort {
    type Error;
    fn charge_work(&mut self, work: usize) -> Result<(), Self::Error>;
    fn verify_metadata(&mut self) -> Result<(), (String, Self::Error)>;
}

/// Every build YAML resource uses the same native reader and original error
/// carrier. Existing document and environment applications own parsing and
/// dependency traversal; the reader owns physical capture and alias equality.
pub trait ResourceReader:
    MetadataAdmissionPort
    + crate::file_preparation::NativePort<Error = <Self as MetadataAdmissionPort>::Error>
    + crate::project_preparation::CapturePort<Error = <Self as MetadataAdmissionPort>::Error>
{
    /// Root metadata follows the same resolve/capture/copy order as original
    /// file preparation. Relative dependency requests retain their declaring file.
    fn capture_root(
        &mut self,
        written: &str,
        maximum: usize,
    ) -> Result<crate::project_preparation::Source, <Self as MetadataAdmissionPort>::Error> {
        let identity = crate::file_preparation::NativePort::resolve_entry(self, None, written)?;
        let bytes =
            crate::file_preparation::NativePort::capture_entry(self, None, written, maximum)?;
        Ok(crate::project_preparation::Source { identity, bytes })
    }
}
impl<T> ResourceReader for T where
    T: MetadataAdmissionPort
        + crate::file_preparation::NativePort<Error = <T as MetadataAdmissionPort>::Error>
        + crate::project_preparation::CapturePort<Error = <T as MetadataAdmissionPort>::Error>
{
}

/// Root capture and environment preparation keep separate original failure
/// families. A projection refusal cannot replace either retained native payload.
#[derive(Debug)]
pub enum EnvironmentFailure<E, D, X> {
    Resource(E),
    Preparation(crate::project_preparation::Failure<E, D, X>),
}

pub type EnvironmentPreparation<R, D> = Result<
    crate::project_preparation::PreparedEnvironment<
        <D as crate::project_preparation::DecodePort>::Document,
    >,
    EnvironmentFailure<
        <R as MetadataAdmissionPort>::Error,
        <D as crate::project_preparation::DecodePort>::Document,
        <D as crate::project_preparation::DecodePort>::Error,
    >,
>;

/// Compose the shared native reader and structural metadata codec with the
/// existing environment application. No activation, study, output or publisher
/// port is accepted at this boundary.
pub fn prepare_environment<R: ResourceReader, D: crate::project_preparation::DecodePort>(
    reader: &mut R,
    decoder: &mut D,
    written: &str,
    host: yamaa_core::project_function::Language,
    limits: crate::project_preparation::Limits,
) -> EnvironmentPreparation<R, D> {
    let source = reader
        .capture_root(written, limits.bytes)
        .map_err(EnvironmentFailure::Resource)?;
    crate::project_preparation::prepare(decoder, source, host, reader, limits)
        .map_err(EnvironmentFailure::Preparation)
}

pub struct EntryView<'a> {
    /// The retained pre-default arena; defaults cannot invent producer inputs.
    pub original: &'a Document,
    pub model_nodes: usize,
}
#[derive(Debug)]
pub enum SelectionFailure<E> {
    Resource(E),
    Limit(&'static str),
    Snapshot { identity: String, error: E },
}

/// Sealed metadata decision retained beside the exact captured entry. It grants
/// no compilation, activation, study or publication capability on its own.
pub struct EntrySelection {
    producer: bool,
}
impl EntrySelection {
    pub fn is_producer(&self) -> bool {
        self.producer
    }
}

/// Complete admission and whole original-input inspection precede selection.
/// Opaque refusal and changed-snapshot payloads retain their original identity.
pub fn select_entry<P: MetadataAdmissionPort>(
    view: EntryView<'_>,
    model_node_limit: usize,
    port: &mut P,
) -> Result<EntrySelection, SelectionFailure<P::Error>> {
    let document = view.original;
    port.charge_work(document.nodes().len())
        .map_err(SelectionFailure::Resource)?;
    if view.model_nodes > model_node_limit {
        return Err(SelectionFailure::Limit("producer_graph_document_nodes"));
    }
    let mut producer = false;
    if let Some(input) = document.field(document.root(), "input") {
        if let DocumentNode::Mapping(entries) = &document.nodes()[input] {
            for &(_, declaration) in entries {
                producer |=
                    yamaa_core::producer_admission::is_producer_input(document, declaration);
            }
        }
    }
    port.verify_metadata()
        .map_err(|(identity, error)| SelectionFailure::Snapshot { identity, error })?;
    Ok(EntrySelection { producer })
}

/// Complete metadata capabilities. Construction and compilation accept no study
/// or publication authority; the engine selects a route from the held entry.
pub enum Prepared<S, P, G> {
    Plain(S),
    Ordinary(P),
    Producer(G),
}

pub trait PreparationPort {
    type Entry;
    type Plain;
    type Project;
    type Producer;
    type Failure;

    /// Capture the optional environment and original entry through one resource
    /// reader. Root limits precede environment IO; absence captures nothing.
    fn capture(&mut self, request: Request<'_>) -> Result<Self::Entry, Self::Failure>;
    fn selection(entry: &Self::Entry) -> &EntrySelection;
    fn plain(&mut self, entry: Self::Entry) -> Result<Self::Plain, Self::Failure>;
    fn project(&mut self, entry: Self::Entry) -> Result<Self::Project, Self::Failure>;
    fn producer(&mut self, entry: Self::Entry) -> Result<Self::Producer, Self::Failure>;
}

pub type Preparation<P> = Result<
    Prepared<
        <P as PreparationPort>::Plain,
        <P as PreparationPort>::Project,
        <P as PreparationPort>::Producer,
    >,
    <P as PreparationPort>::Failure,
>;

pub fn prepare<P: PreparationPort>(request: Request<'_>, port: &mut P) -> Preparation<P> {
    let explicit_environment = request.environment.is_some();
    let entry = port.capture(request)?;
    select(entry, explicit_environment, port)
}

/// Resource-owning callers use the same sealed selection without reopening the
/// entry or reconstructing a request from captured canonical identities.
pub fn select<P: PreparationPort>(
    entry: P::Entry,
    explicit_environment: bool,
    port: &mut P,
) -> Preparation<P> {
    if P::selection(&entry).is_producer() {
        port.producer(entry).map(Prepared::Producer)
    } else if explicit_environment {
        port.project(entry).map(Prepared::Ordinary)
    } else {
        port.plain(entry).map(Prepared::Plain)
    }
}

/// Host ports move checked capabilities into their existing engine execution,
/// output and report services. Original host failures remain owned by the result.
/// A build never accepts a publisher: publication uses only a completed result's
/// explicit Save capability.
pub trait Ports {
    type Plain;
    type Project;
    type Producer;
    type CheckResult;
    type BuildResult;
    type Failure;

    fn prepare(&mut self, request: Request<'_>) -> PreparedRequest<Self>;
    fn check_plain(&mut self, prepared: Self::Plain) -> Result<Self::CheckResult, Self::Failure>;
    fn check_project(
        &mut self,
        prepared: Self::Project,
    ) -> Result<Self::CheckResult, Self::Failure>;
    fn check_producer(
        &mut self,
        prepared: Self::Producer,
    ) -> Result<Self::CheckResult, Self::Failure>;
    fn build_plain(&mut self, prepared: Self::Plain) -> Result<Self::BuildResult, Self::Failure>;
    fn build_project(
        &mut self,
        prepared: Self::Project,
    ) -> Result<Self::BuildResult, Self::Failure>;
    fn build_producer(
        &mut self,
        prepared: Self::Producer,
    ) -> Result<Self::BuildResult, Self::Failure>;

    fn build_plain_with_observer(
        &mut self,
        prepared: Self::Plain,
        _scope: crate::application_observer::Scope<'_>,
    ) -> Result<Self::BuildResult, Self::Failure> {
        self.build_plain(prepared)
    }
    fn build_project_with_observer(
        &mut self,
        prepared: Self::Project,
        _scope: crate::application_observer::Scope<'_>,
    ) -> Result<Self::BuildResult, Self::Failure> {
        self.build_project(prepared)
    }
    fn build_producer_with_observer(
        &mut self,
        prepared: Self::Producer,
        _scope: crate::application_observer::Scope<'_>,
    ) -> Result<Self::BuildResult, Self::Failure> {
        self.build_producer(prepared)
    }
}

pub type PreparedRequest<P> = Result<
    Prepared<<P as Ports>::Plain, <P as Ports>::Project, <P as Ports>::Producer>,
    <P as Ports>::Failure,
>;

pub fn check<P: Ports>(request: Request<'_>, port: &mut P) -> Result<P::CheckResult, P::Failure> {
    match port.prepare(request)? {
        Prepared::Plain(prepared) => port.check_plain(prepared),
        Prepared::Ordinary(prepared) => port.check_project(prepared),
        Prepared::Producer(prepared) => port.check_producer(prepared),
    }
}

pub fn domain<P: Ports>(request: Request<'_>, port: &mut P) -> Result<P::BuildResult, P::Failure> {
    domain_with_observer(
        request,
        port,
        crate::application_observer::Scope::new(&crate::application_observer::Noop),
    )
}

/// Preparation includes the actual port's original file/environment admission.
/// Detailed execution events come from the selected checked service, never an
/// invented second run. Returned failures close this outer attempt; panic fences
/// keep the entered event prefix and may close their adapter clock separately.
pub fn domain_with_observer<P: Ports>(
    request: Request<'_>,
    port: &mut P,
    scope: crate::application_observer::Scope<'_>,
) -> Result<P::BuildResult, P::Failure> {
    use crate::application_observer::{Outcome, Phase};
    scope.phase(Phase::Preparation);
    let prepared = port.prepare(request);
    scope.exit(Phase::Preparation, Outcome::from_result(&prepared));
    let result = match prepared {
        Ok(prepared) => {
            // Close actual metadata preparation before entering the selected
            // runtime port, including older ports without detailed events.
            scope.phase(Phase::Build);
            let result = match prepared {
                Prepared::Plain(prepared) => port.build_plain_with_observer(prepared, scope),
                Prepared::Ordinary(prepared) => port.build_project_with_observer(prepared, scope),
                Prepared::Producer(prepared) => port.build_producer_with_observer(prepared, scope),
            };
            scope.exit(Phase::Build, Outcome::from_result(&result));
            result
        }
        Err(error) => Err(error),
    };
    scope.phase(Phase::Finished);
    result
}
