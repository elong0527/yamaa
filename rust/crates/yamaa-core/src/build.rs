//! Shared build request and original metadata admission. Study and publication
//! authority enter only after the selected checked capability is complete.
pub struct Request<'a> {
    pub specification: &'a str,
    pub environment: Option<&'a str>,
}

/// An absent environment has no declarations, captures, lock or activation
/// requirements. It does not manufacture a source file or discovery location.
pub fn without_environment(
    host: crate::project_function::Language,
) -> crate::project_environment::ExecutionEnvironment {
    use crate::project_environment::{Draft, Environment};
    Environment::admit(
        host,
        Draft {
            language: None,
            lock: None,
            functions: None,
            codelists: std::vec::Vec::new(),
            has_study: false,
            submissions: std::vec::Vec::new(),
        },
    )
    .expect("empty environment admission")
    .into_execution()
}

/// Native quota and snapshot observations; these methods neither decode nor
/// activate code, read study data, or publish artifacts.
pub trait MetadataAdmissionPort {
    type Error;
    fn verify_metadata(&mut self) -> Result<(), (String, Self::Error)>;
}

/// Every build YAML resource uses the same native reader and original error
/// carrier. Existing document and environment applications own parsing and
/// dependency traversal; the reader owns physical capture and alias equality.
///
/// Root metadata follows the same resolve/capture/copy order as original
/// file preparation. Relative dependency requests retain their declaring file.
fn capture_root<R>(
    reader: &mut R,
    written: &str,
) -> Result<crate::project_preparation::Source, <R as MetadataAdmissionPort>::Error>
where
    R: MetadataAdmissionPort
        + crate::file_preparation::NativePort<Error = <R as MetadataAdmissionPort>::Error>
        + crate::project_preparation::ProjectCapturePort<Error = <R as MetadataAdmissionPort>::Error>,
{
    let identity = crate::file_preparation::NativePort::resolve_entry(reader, None, written)?;
    let bytes = crate::file_preparation::NativePort::capture_entry(reader, None, written)?;
    Ok(crate::project_preparation::Source { identity, bytes })
}

/// Root capture and environment preparation keep separate original failure
/// families. A projection refusal cannot replace either retained native payload.
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentFailure<E, D, X> {
    #[error("build resource: {0:?}")]
    Resource(E),
    #[error("build preparation: {0:?}")]
    Preparation(crate::project_preparation::Failure<E, D, X>),
}

pub(crate) type EnvironmentPreparation<R, D> = Result<
    crate::project_preparation::PreparedEnvironment<
        <D as crate::project_preparation::ProjectDecodePort>::Document,
    >,
    EnvironmentFailure<
        <R as MetadataAdmissionPort>::Error,
        <D as crate::project_preparation::ProjectDecodePort>::Document,
        <D as crate::project_preparation::ProjectDecodePort>::Error,
    >,
>;

/// Compose the shared native reader and structural metadata codec with the
/// existing environment application. No activation, study, output or publisher
/// port is accepted at this boundary.
pub fn prepare_environment<R, D: crate::project_preparation::ProjectDecodePort>(
    reader: &mut R,
    decoder: &mut D,
    written: &str,
    host: crate::project_function::Language,
) -> EnvironmentPreparation<R, D>
where
    R: MetadataAdmissionPort
        + crate::file_preparation::NativePort<Error = <R as MetadataAdmissionPort>::Error>
        + crate::project_preparation::ProjectCapturePort<Error = <R as MetadataAdmissionPort>::Error>,
{
    let source = capture_root(reader, written).map_err(EnvironmentFailure::Resource)?;
    crate::project_preparation::prepare(decoder, source, host, reader)
        .map_err(EnvironmentFailure::Preparation)
}

#[derive(Debug, thiserror::Error)]
pub enum SelectionFailure<E> {
    #[error("build snapshot: identity={identity:?}, error={error:?}")]
    Snapshot { identity: String, error: E },
}

/// Complete admission and whole original-input inspection precede selection.
/// Opaque refusal and changed-snapshot payloads retain their original identity.
pub fn verify_entry<P: MetadataAdmissionPort>(
    port: &mut P,
) -> Result<(), SelectionFailure<P::Error>> {
    port.verify_metadata()
        .map_err(|(identity, error)| SelectionFailure::Snapshot { identity, error })
}

/// Complete metadata capabilities. Construction and compilation accept no study
/// or publication authority; an explicit environment selects the project route.
pub enum Prepared<S, P> {
    Plain(S),
    Ordinary(P),
}

pub trait PreparationPort {
    type Entry;
    type Plain;
    type Project;
    type Failure;

    /// Capture the optional environment and original entry through one resource
    /// reader. Root limits precede environment IO; absence captures nothing.
    fn capture(&mut self, request: Request<'_>) -> Result<Self::Entry, Self::Failure>;
    fn plain(&mut self, entry: Self::Entry) -> Result<Self::Plain, Self::Failure>;
    fn project(&mut self, entry: Self::Entry) -> Result<Self::Project, Self::Failure>;
}

pub type Preparation<P> = Result<
    Prepared<<P as PreparationPort>::Plain, <P as PreparationPort>::Project>,
    <P as PreparationPort>::Failure,
>;

pub fn prepare<P: PreparationPort>(request: Request<'_>, port: &mut P) -> Preparation<P> {
    let explicit_environment = request.environment.is_some();
    let entry = port.capture(request)?;
    select(entry, explicit_environment, port)
}

/// Resource-owning callers use the same selection without reopening the entry
/// or reconstructing a request from captured canonical identities.
pub fn select<P: PreparationPort>(
    entry: P::Entry,
    explicit_environment: bool,
    port: &mut P,
) -> Preparation<P> {
    if explicit_environment {
        port.project(entry).map(Prepared::Ordinary)
    } else {
        port.plain(entry).map(Prepared::Plain)
    }
}

/// Host ports lend checked capabilities to their existing engine execution,
/// output and report services. Original host failures remain owned by the result.
/// A build never accepts a publisher: publication uses only a completed result's
/// explicit Save capability. Builds borrow the prepared value, so one preparation
/// can serve many runs; each run's study state lives in that run alone.
pub trait Ports {
    type Plain;
    type Project;
    type CheckResult;
    type BuildResult;
    type Failure;

    fn prepare(&mut self, request: Request<'_>) -> PreparedRequest<Self>;
    fn check_plain(&mut self, prepared: Self::Plain) -> Result<Self::CheckResult, Self::Failure>;
    fn check_project(
        &mut self,
        prepared: Self::Project,
    ) -> Result<Self::CheckResult, Self::Failure>;
    fn build_plain(
        &mut self,
        prepared: &Self::Plain,
        scope: crate::application_observer::Scope<'_>,
    ) -> Result<Self::BuildResult, Self::Failure>;
    fn build_project(
        &mut self,
        prepared: &Self::Project,
        scope: crate::application_observer::Scope<'_>,
    ) -> Result<Self::BuildResult, Self::Failure>;
}

pub type PreparedRequest<P> =
    Result<Prepared<<P as Ports>::Plain, <P as Ports>::Project>, <P as Ports>::Failure>;

pub fn check<P: Ports>(request: Request<'_>, port: &mut P) -> Result<P::CheckResult, P::Failure> {
    match port.prepare(request)? {
        Prepared::Plain(prepared) => port.check_plain(prepared),
        Prepared::Ordinary(prepared) => port.check_project(prepared),
    }
}

/// A build is [`compile`] followed by one [`run`]: one admission and
/// evaluation route whether a prepared value serves one run or many.
/// Preparation includes the actual port's original file/environment admission.
/// Detailed execution events come from the selected checked service, never an
/// invented second run. Returned failures close this outer attempt; panic fences
/// keep the entered event prefix and may close their adapter clock separately.
pub fn domain<P: Ports>(
    request: Request<'_>,
    port: &mut P,
    scope: crate::application_observer::Scope<'_>,
) -> Result<P::BuildResult, P::Failure> {
    match compile(request, port, scope) {
        Ok(prepared) => run(&prepared, port, scope),
        Err(error) => {
            finish(scope);
            Err(error)
        }
    }
}

/// The Preparation half of a build: metadata capture, admission and
/// compilation, with no study, activation or publication authority. Its
/// events are a build's prefix.
pub fn compile<P: Ports>(
    request: Request<'_>,
    port: &mut P,
    scope: crate::application_observer::Scope<'_>,
) -> PreparedRequest<P> {
    use crate::application_observer::{Outcome, Phase};
    scope.phase(Phase::Preparation);
    let prepared = port.prepare(request);
    scope.exit(Phase::Preparation, Outcome::from_result(&prepared));
    prepared
}

/// The Build and Finished half of a build: one run that borrows the prepared
/// value, so the same value can serve the next run unchanged.
pub fn run<P: Ports>(
    prepared: &Prepared<P::Plain, P::Project>,
    port: &mut P,
    scope: crate::application_observer::Scope<'_>,
) -> Result<P::BuildResult, P::Failure> {
    use crate::application_observer::{Outcome, Phase};
    // Close actual metadata preparation before entering the selected
    // runtime port.
    scope.phase(Phase::Build);
    let result = match prepared {
        Prepared::Plain(prepared) => port.build_plain(prepared, scope),
        Prepared::Ordinary(prepared) => port.build_project(prepared, scope),
    };
    scope.exit(Phase::Build, Outcome::from_result(&result));
    finish(scope);
    result
}

/// Close a run. A refused compilation's runs only finish: they perform no
/// resource, activation or study work.
pub fn finish(scope: crate::application_observer::Scope<'_>) {
    scope.phase(crate::application_observer::Phase::Finished);
}
