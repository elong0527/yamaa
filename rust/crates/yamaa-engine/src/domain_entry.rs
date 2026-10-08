//! Host-neutral file entry points. Preparation and activation precede study authority.
//! Checks may inspect declarations and locks but never execute project code.

pub struct Request<'a> {
    pub specification: &'a str,
    pub environment: Option<&'a str>,
}

/// Native adapters own file/codec access and result conversion. Bindings supply
/// only the request; neither Python nor R orchestrates these checkpoints.
pub trait Ports {
    type Environment;
    type Prepared;
    type CheckResult;
    type BuildResult;
    type Failure;

    fn environment(&mut self, path: Option<&str>) -> Result<Self::Environment, Self::Failure>;
    fn prepare(&mut self, path: &str) -> Result<Self::Prepared, Self::Failure>;
    fn check(
        &mut self,
        prepared: Self::Prepared,
        environment: &Self::Environment,
    ) -> Result<Self::CheckResult, Self::Failure>;
    /// Admit called packages/locks and run called-function tests for every build.
    fn activate(
        &mut self,
        prepared: &Self::Prepared,
        environment: &Self::Environment,
    ) -> Result<(), Self::Failure>;
    fn build(
        &mut self,
        prepared: Self::Prepared,
        environment: &Self::Environment,
    ) -> Result<Self::BuildResult, Self::Failure>;
}

pub fn check<P: Ports>(request: Request<'_>, ports: &mut P) -> Result<P::CheckResult, P::Failure> {
    let environment = ports.environment(request.environment)?;
    let prepared = ports.prepare(request.specification)?;
    ports.check(prepared, &environment)
}

pub fn domain<P: Ports>(request: Request<'_>, ports: &mut P) -> Result<P::BuildResult, P::Failure> {
    let environment = ports.environment(request.environment)?;
    let prepared = ports.prepare(request.specification)?;
    ports.activate(&prepared, &environment)?;
    ports.build(prepared, &environment)
}
