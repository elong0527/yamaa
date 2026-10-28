//! Shared build request and original metadata admission. Study and publication
//! authority enter only after the selected checked capability is complete.
pub struct Request<'a> {
    pub specification: &'a str,
    pub environment: Option<&'a str>,
}

/// An absent project has no declarations, captures, lock or activation
/// requirements. It does not manufacture a source file or discovery location.
pub fn without_environment(
    host: Option<crate::project_function::Language>,
) -> crate::project_environment::ExecutionEnvironment {
    crate::project_environment::ExecutionEnvironment::without_project(host)
}

/// Every build YAML resource uses the same resolver and original error
/// carrier. Existing document and environment applications own parsing and
/// dependency traversal; the resolver owns physical capture and alias equality.
///
/// Root metadata follows the same resolve/capture order as original file
/// preparation. Relative dependency requests retain their declaring file.
fn capture_root<R: crate::resolver::Resolver>(
    resolver: &mut R,
    written: &str,
) -> Result<crate::project_preparation::Source, R::Error> {
    let entry = crate::resolver::Name::Entry {
        declaring: None,
        written,
    };
    let identity = resolver.inspect(entry)?.identity;
    let bytes = resolver.capture(entry)?.bytes.to_vec();
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
        <D as crate::decoder::Decoder>::Admitted,
        <D as crate::decoder::Decoder>::Raw,
    >,
    EnvironmentFailure<
        <R as crate::resolver::Resolver>::Error,
        <D as crate::decoder::Decoder>::Admitted,
        <D as crate::decoder::Decoder>::DocumentError,
    >,
>;

/// Compose the resolver and structural metadata codec with the existing
/// environment application. No activation, study, output or publisher port
/// is accepted at this boundary.
pub fn prepare_environment<R: crate::resolver::Resolver, D: crate::decoder::Decoder>(
    resolver: &mut R,
    decoder: &mut D,
    written: &str,
    host: Option<crate::project_function::Language>,
) -> EnvironmentPreparation<R, D> {
    let source = capture_root(resolver, written).map_err(EnvironmentFailure::Resource)?;
    crate::project_preparation::prepare(decoder, source, host, resolver)
        .map_err(EnvironmentFailure::Preparation)
}

/// The Preparation half of a build: metadata capture, admission and
/// compilation, with no study, activation or publication authority. Its
/// events are a build's prefix. Every build of a specification, alone or as a
/// member of a project run, prepares through this one phase.
pub fn compile<T, F>(
    scope: crate::application_observer::Scope<'_>,
    prepare: impl FnOnce() -> Result<T, F>,
) -> Result<T, F> {
    use crate::application_observer::{Outcome, Phase};
    scope.phase(Phase::Preparation);
    let prepared = prepare();
    scope.exit(Phase::Preparation, Outcome::from_result(&prepared));
    prepared
}

/// The Build and Finished half of a build: one run of a prepared member. A
/// build never accepts a publisher: publication uses only a completed
/// result's explicit Save capability. Builds borrow the prepared value, so one
/// preparation can serve many runs; each run's study state lives in that run
/// alone. Returned failures close this outer attempt; panic fences keep the
/// entered event prefix and may close their adapter clock separately.
pub fn run<T, F>(
    scope: crate::application_observer::Scope<'_>,
    build: impl FnOnce() -> Result<T, F>,
) -> Result<T, F> {
    use crate::application_observer::{Outcome, Phase};
    // Close actual metadata preparation before entering the runtime.
    scope.phase(Phase::Build);
    let result = build();
    scope.exit(Phase::Build, Outcome::from_result(&result));
    finish(scope);
    result
}

/// A build is [`compile`] followed by one [`run`]: one admission and
/// evaluation route whether a prepared value serves one run or many. A
/// refused compilation only finishes.
pub fn domain<P, T, F>(
    scope: crate::application_observer::Scope<'_>,
    prepare: impl FnOnce() -> Result<P, F>,
    build: impl FnOnce(&P) -> Result<T, F>,
) -> Result<T, F> {
    match compile(scope, prepare) {
        Ok(prepared) => run(scope, || build(&prepared)),
        Err(error) => {
            finish(scope);
            Err(error)
        }
    }
}

/// Close a run. A refused compilation's runs only finish: they perform no
/// resource, activation or study work.
pub fn finish(scope: crate::application_observer::Scope<'_>) {
    scope.phase(crate::application_observer::Phase::Finished);
}
